// Intercepted component protocol test: no real DB/quorum or native custody proof.
import { accessSync, constants, statSync } from "node:fs";
import { createHash } from "node:crypto";
import { chmod } from "node:fs/promises";
import { chromium, type Browser, type BrowserContext } from "playwright";
import { describe, expect, it } from "vitest";
import { executeBrowserRun } from "../src/browser-run-execution.js";
import { accountPurgeSubjectHash } from "../src/account-residency.js";
import { profilePaths } from "../src/profile-store.js";
import {
  CURRENT_CHECKPOINT_VERSION, scanManagedRunCheckpoints, writeManagedRunCheckpoint,
  type CloudRunCheckpoint,
} from "../src/run-checkpoint-store.js";
import type { CloudRunRequest, RunEvent } from "../src/server.js";
import { createBrowserRunLeaseFixture, type BrowserRunLeaseMode } from "./browser-run-lease-fixture.js";
import {
  createBrowserRunFixture, createBrowserRunStorageFixture,
  greenhouseApplicationHtml, greenhouseConfirmationHtml, GREENHOUSE_JOB_URL,
} from "./browser-run-fixture.js";

const CHROMIUM = chromium.executablePath();
type Scenario = "success" | "phase-b-reject" | "phase-b-loss" | "employer-loss"
  | "stale-dom" | "stale-document" | "stopping";

describe("browser run execution integration", () => {
  it("checkpoints one intercepted submission and replays its bound workflow receipt", async () => {
    const result = await executeFixture("success", 60_000);
    expect(result.error).toBeUndefined();
    expect(result.execution?.receipt, JSON.stringify({
      receipt: result.execution?.receipt,
      finalUrl: result.execution?.receiptBundle.finalUrl,
      posts: result.posts,
      checkpoint: result.checkpoint?.phase,
    })).toMatchObject({ status: "submitted", issues: [] });
    expect(result.posts).toBe(1);
    expect(result.postResponseStatuses).toEqual([200]);
    expect(result.unexpectedFixtureRequests).toEqual([]);
    expect(result.execution?.receiptBundle.finalUrl).toBe(`${GREENHOUSE_JOB_URL}/confirmation`);
    expect(result.postAfterCheckpointAndAuthority).toBe(true);
    expect(result.postContainsExactDocument).toBe(true);
    expect(result.calls.filter((call) => call.operation === "irreversible")).toHaveLength(1);
    expect(result.checkpoint?.phase).toBe("final_submit_activated");
    expect(result.receiptPersisted).toBe(true);
    expect(result.screenshotPersisted).toBe(true);
    expect(result.execution?.receiptBundle.accountId).toBe("account-browser-execution-fixture");
    expect(result.execution?.receiptBundle.runId).toBe("run-browser-execution-fixture");
    expect(result.execution?.receiptBundle.packet.approvedPacketChecksum).toBe(result.checksum);
    expect(result.execution?.receiptBundle.documents[0]?.sha256).toBe(result.resumeSha256);
    expect(result.execution?.receiptBundle.atsCertifiedReceiptAuthority?.runId)
      .toBe("run-browser-execution-fixture");
    const execution = result.execution;
    if (!execution || execution.receipt.status !== "submitted") {
      throw new Error("Expected a submitted synthetic browser result");
    }
    const bundle = execution.receiptBundle;
    expect(Date.parse(bundle.generatedAt)).toBe(bundle.atsCertifiedReceiptAuthority?.bindingConsumedAtMs);
    expect(Date.parse(bundle.result.submittedAt!)).toBeLessThan(Date.parse(bundle.generatedAt));
    expect(bundle.result.submittedAt).toBe(execution.receipt.submittedAt);
    const { verifyBrowserReceiptWorkflowReplay } =
      await import("./browser-receipt-workflow-fixture.js");
    await verifyBrowserReceiptWorkflowReplay(result.request, execution, result.profileScope);
  }, 45_000);

  it.each(["phase-b-reject", "phase-b-loss"] as const)(
    "%s retains the pre-effect checkpoint and never posts to the employer",
    async (scenario) => {
      const result = await executeFixture(scenario);
      expect(result.posts).toBe(0);
      expect(result.calls.filter((call) => call.operation === "irreversible")).toHaveLength(1);
      expect(result.checkpoint?.phase).toBe("final_submit_started");
      expect(result.execution?.receipt.status).not.toBe("submitted");
      expect(result.receiptPersisted).toBe(false);
    }, 30_000,
  );

  it("keeps employer response loss uncertain after exactly one intercepted POST", async () => {
    const result = await executeFixture("employer-loss");
    expect(result.posts).toBe(1);
    expect(result.postAfterCheckpointAndAuthority).toBe(true);
    expect(result.postContainsExactDocument).toBe(true);
    expect(result.calls.filter((call) => call.operation === "irreversible")).toHaveLength(1);
    expect(["final_submit_started", "side_effect_unknown"]).toContain(result.checkpoint?.phase);
    expect(result.checkpoint?.workflow.status).toBe("side_effect_unknown");
    expect(result.execution?.receipt.status).not.toBe("submitted");
    expect(result.execution?.receipt.submittedAt).toBeUndefined();
  }, 30_000);

  it.each(["stale-dom", "stale-document", "stopping"] as const)(
    "%s refuses before any employer POST", async (scenario) => {
      const result = await executeFixture(scenario);
      expect(result.posts).toBe(0);
      expect(result.execution?.receipt.status).not.toBe("submitted");
      if (scenario === "stopping") {
        expect(result.stoppingObserved).toBe(true);
        expect(result.calls.filter((call) => call.operation === "irreversible")).toHaveLength(0);
      } else if (scenario === "stale-dom") {
        expect(result.domMutationObserved).toBe(true);
        expect(result.checkpoint?.phase).toBe("final_submit_started");
        expect(result.calls.filter((call) => call.operation === "irreversible")).toHaveLength(1);
      } else {
        expect(result.documentMutationObserved).toBe(true);
        expect(result.calls.filter((call) => call.operation === "irreversible")).toHaveLength(0);
        expect(result.checkpoint).toBeUndefined();
      }
    }, 30_000,
  );
});

async function executeFixture(scenario: Scenario, serverClockLeadMs = 0) {
  expect(configuredChromium()).toBe(true);
  const fixture = await createBrowserRunFixture();
  let storage: Awaited<ReturnType<typeof createBrowserRunStorageFixture>> | undefined;
  let browser: Browser | undefined;
  let context: BrowserContext | undefined;
  let lease: Awaited<ReturnType<typeof createBrowserRunLeaseFixture>> | undefined;
  try {
    const key = Buffer.alloc(32, 42);
    const runId = "run-browser-execution-fixture";
    const input: CloudRunRequest = {
      accountId: "account-browser-execution-fixture",
      applicationId: fixture.packet.applicationId,
      applicationIdentityId: fixture.packet.applicationIdentityId!,
      browserProfileId: fixture.packet.browserProfileId!,
      browserSessionId: "session-browser-execution-fixture",
      runId, requestId: `${runId}:initial`, url: GREENHOUSE_JOB_URL,
      packet: fixture.packet, job: fixture.job,
    };
    const mode: BrowserRunLeaseMode = scenario === "phase-b-reject" ? "reject"
      : scenario === "phase-b-loss" ? "response-loss" : "success";
    lease = await createBrowserRunLeaseFixture(input, mode, serverClockLeadMs);
    const activeLease = lease;
    const scope = profilePaths("/synthetic", input.accountId, input.applicationIdentityId).scope;
    storage = await createBrowserRunStorageFixture(activeLease.lease.purgeSubject, scope);
    const managed = storage.storage;
    browser = await chromium.launch({ executablePath: CHROMIUM, headless: true });
    context = await browser.newContext({ serviceWorkers: "block", offline: true });
    const activeContext = context;
    let posts = 0;
    const postResponseStatuses: number[] = [];
    const unexpectedFixtureRequests: string[] = [];
    let postAfterCheckpointAndAuthority = false;
    let postContainsExactDocument = false;
    let stoppingObserved = false;
    let domMutationObserved = false;
    let documentMutationObserved = false;
    let residencyCalls = 0;
    const events: RunEvent[] = [];
    const checkpointCreatedAtMs = Date.now();
    context.on("response", (response) => {
      if (response.url() === GREENHOUSE_JOB_URL && response.request().method() === "POST") {
        postResponseStatuses.push(response.status());
      }
    });
    await context.route("**/*", async (route) => {
      const request = route.request();
      if (request.url() !== GREENHOUSE_JOB_URL) {
        unexpectedFixtureRequests.push(`${request.method()} ${request.url()}`);
        return route.abort("blockedbyclient");
      }
      if (request.method() === "GET") {
        return route.fulfill({ status: 200, contentType: "text/html", body: greenhouseApplicationHtml() });
      }
      if (request.method() === "POST") {
        posts += 1;
        const scan = await scanManagedRunCheckpoints<CloudRunRequest, RunEvent>(managed, key);
        postAfterCheckpointAndAuthority = scan.failures.length === 0
          && scan.checkpoints.length === 1
          && scan.checkpoints[0]?.checkpoint.phase === "final_submit_started"
          && activeLease.calls.filter((call) => call.operation === "irreversible").length === 1
          && activeLease.lease.finalSubmitAuthorized;
        const body = request.postDataBuffer();
        postContainsExactDocument = body !== null
          && body.includes(Buffer.from(fixture.resumeBytes))
          && body.includes(Buffer.from("synthetic@example.test"));
        if (scenario === "employer-loss") return route.abort("failed");
        // This component fixture proves a 200 response and explicit same-document
        // confirmation. Actual HTTP redirect confinement needs its own evidence.
        return route.fulfill({ status: 200, contentType: "text/html", body: greenhouseConfirmationHtml });
      }
      unexpectedFixtureRequests.push(`${request.method()} ${request.url()}`);
      return route.abort("blockedbyclient");
    });
    let execution: Awaited<ReturnType<typeof executeBrowserRun>> | undefined;
    let error: unknown;
    try {
      execution = await executeBrowserRun(input, context, events, activeLease.lease, true, {
        withProfileResidency: async (operation) => {
          expect(managed.subjectSha256).toBe(accountPurgeSubjectHash(activeLease.lease.purgeSubject));
          expect(managed.scope).toBe(scope);
          residencyCalls += 1;
          if (scenario === "stale-document" && residencyCalls === 2) {
            const run = await managed.receipts.openChildDirectory(runId);
            const documents = await run.openChildDirectory("documents");
            const filename = `resume-${fixture.resumeSha256}.pdf`;
            await chmod(`${documents.canonicalPath}/${filename}`, 0o600);
            await documents.replaceFile(filename, Buffer.from("synthetic stale document"));
            documentMutationObserved = true;
          }
          return operation(managed);
        },
        assertWorkMayContinue: () => {
          if (scenario === "stopping") {
            stoppingObserved = true;
            throw new Error("Synthetic stopping work");
          }
        },
        writeCheckpoint: async (part) => {
          const now = Date.now();
          const checkpoint: CloudRunCheckpoint<CloudRunRequest, RunEvent> = {
            version: CURRENT_CHECKPOINT_VERSION, phase: part.phase,
            createdAtMs: checkpointCreatedAtMs, updatedAtMs: now, expiresAtMs: now + 86_400_000,
            profileScope: managed.scope, browserSessionId: input.browserSessionId, request: input,
            browser: { url: part.browserUrl },
            workflow: { status: part.status, requestId: input.requestId,
              ...(part.providerReview ? { providerReview: part.providerReview } : {}) },
            events: [...events],
            lease: { ...activeLease.lease.checkpointMetadata(), purgeSubject: activeLease.lease.purgeSubject },
          };
          await writeManagedRunCheckpoint(managed, checkpoint, key);
          if (scenario === "stale-dom" && part.phase === "final_submit_started") {
            await activeContext.pages()[0]!.locator("#email").fill("changed@example.test");
            domMutationObserved = true;
          }
        },
        writeReceiptArtifact: async (requestedRunId, name, contents) => {
          expect(requestedRunId).toBe(runId);
          const directory = await managed.receipts.openChildDirectory(runId);
          await directory.replaceFile(name, contents);
          const entries = await directory.inventory();
          const entry = entries.entries.find((candidate) => candidate.relativePath === name);
          const persisted = await directory.readFileBounded(name, 8 * 1024 * 1024);
          expect(persisted.equals(contents)).toBe(true);
          expect(entry?.sha256).toBe(createHash("sha256").update(contents).digest("hex"));
          return `${directory.canonicalPath}/${name}`;
        },
        takeoverOrigin: () => undefined,
      });
    } catch (caught) {
      error = caught;
    }
    const scan = await scanManagedRunCheckpoints<CloudRunRequest, RunEvent>(managed, key);
    expect(scan.failures).toEqual([]);
    const checkpoint = scan.checkpoints[0]?.checkpoint;
    let receiptPersisted = false;
    let screenshotPersisted = false;
    if (execution) {
      const directory = await managed.receipts.openChildDirectory(runId);
      const receipt = await directory.readFileBounded("receipt.json", 8 * 1024 * 1024);
      const screenshot = await directory.readFileBounded("final.png", 8 * 1024 * 1024);
      receiptPersisted = JSON.stringify(JSON.parse(receipt.toString("utf8")))
        === JSON.stringify(execution.receiptBundle);
      screenshotPersisted = screenshot.length > 0
        && execution.evidenceObjects.some((entry) => entry.kind === "screenshot"
          && entry.sha256 === createHash("sha256").update(screenshot).digest("hex"));
    }
    return { execution, error, posts, request: input, profileScope: scope,
      postResponseStatuses, unexpectedFixtureRequests,
      postAfterCheckpointAndAuthority, postContainsExactDocument,
      stoppingObserved, domMutationObserved, documentMutationObserved,
      calls: [...activeLease.calls], checkpoint, receiptPersisted, screenshotPersisted,
      checksum: fixture.packet.approvedPacketChecksum, resumeSha256: fixture.resumeSha256 };
  } finally {
    const shutdownErrors: unknown[] = [];
    for (const close of [
      () => lease?.close(),
      () => context?.close(),
      () => browser?.close(),
    ]) {
      try {
        await close();
      } catch (error) {
        shutdownErrors.push(error);
      }
    }
    if (shutdownErrors.length > 0) {
      // The canonical supervisor reaps its group before removing the owned root.
      // A direct invocation must retain data until browser shutdown is resolved.
      throw new AggregateError(shutdownErrors, "Browser fixture shutdown failed; temporary data retained");
    }
    await storage?.dispose();
    await fixture.dispose();
  }
}

function configuredChromium(): boolean {
  try {
    accessSync(CHROMIUM, constants.X_OK);
    return statSync(CHROMIUM).isFile();
  } catch {
    return false;
  }
}

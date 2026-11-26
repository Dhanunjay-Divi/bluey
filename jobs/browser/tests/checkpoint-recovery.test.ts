import { mkdir, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { BrowserContext, Page } from "playwright";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  approvedExecutionChecksum,
  type NormalizedJob,
} from "@bluey/jobs-automation";
import {
  prepareRecoveredLocalPage,
  reconcileLocalRunCheckpoints,
} from "../src/checkpoint-recovery.js";
import { acquireFinalSubmitAuthority } from "../src/irreversible-submit.js";
import { LocalCheckpointStore, type LocalRunCheckpoint } from "../src/local-checkpoint-store.js";
import { localRunDirectory, type StartRunRequest } from "../src/local-run-contracts.js";
import {
  legacyLocalRunCapabilitiesFixture,
  localRunCapabilitiesFixture,
} from "./fixtures/local-run-capability.js";

const temporaryDirectories: string[] = [];

vi.mock("node:dns/promises", () => ({
  lookup: vi.fn(async () => [{ address: "93.184.216.34" }]),
}));

afterEach(async () => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  await Promise.all(temporaryDirectories.splice(0).map((path) => rm(path, { recursive: true, force: true })));
});

describe("local checkpoint reconciliation", () => {
  it("reconciles an encrypted pre-authority v1 crash checkpoint within the late window", async () => {
    const root = await temporaryDirectory();
    const store = await LocalCheckpointStore.open(root);
    const checkpoint = fixture();
    const expiredAtMs = Date.now() - 1_000;
    checkpoint.expiresAtMs = expiredAtMs;
    checkpoint.delivery.capabilities = legacyLocalRunCapabilitiesFixture(expiredAtMs);
    await store.write(checkpoint);
    const contextFor = vi.fn();
    const unknown = vi.fn();
    const fetchMock = vi.fn(async () => applicationResponse(checkpoint, "side_effect_unknown"));
    vi.stubGlobal("fetch", fetchMock);

    const reopened = await LocalCheckpointStore.open(root);
    await reconcileLocalRunCheckpoints({
      store: reopened,
      userDataDirectory: root,
      activeRuns: new Map(),
      contextFor,
      onRecovered: vi.fn(),
      onUnknown: unknown,
    });

    expect(contextFor).not.toHaveBeenCalled();
    expect(unknown).toHaveBeenCalledOnce();
    expect(fetchMock).toHaveBeenCalledOnce();
    const body = JSON.parse(String(fetchMock.mock.calls[0]?.[1]?.body)) as {
      capability: string;
    };
    expect(body.capability).toBe(checkpoint.delivery.capabilities.result);
    expect(JSON.parse(Buffer.from(body.capability.split(".")[0]!, "base64url").toString("utf8")))
      .toMatchObject({ version: 1, operation: "result", run_id: checkpoint.request.runId });
  });

  it("reports an interrupted final submit as unknown and never restores or retries it", async () => {
    const root = await temporaryDirectory();
    const store = await LocalCheckpointStore.open(root);
    const checkpoint = fixture();
    const runDirectory = localRunDirectory(root, checkpoint.request);
    await mkdir(runDirectory, { recursive: true });
    await acquireFinalSubmitAuthority(runDirectory);
    await store.write(checkpoint);
    const unknown = vi.fn();
    const recovered = vi.fn();
    const contextFor = vi.fn();
    const fetchMock = vi.fn(async () => applicationResponse(checkpoint, "side_effect_unknown"));
    vi.stubGlobal("fetch", fetchMock);

    await reconcileLocalRunCheckpoints({
      store,
      userDataDirectory: root,
      activeRuns: new Map(),
      contextFor,
      onRecovered: recovered,
      onUnknown: unknown,
    });

    expect(contextFor).not.toHaveBeenCalled();
    expect(recovered).not.toHaveBeenCalled();
    expect(unknown).toHaveBeenCalledOnce();
    expect(fetchMock).toHaveBeenCalledOnce();
    const persisted = await store.read<StartRunRequest>(store.scopeFor(checkpoint.request));
    expect(persisted?.phase).toBe("side_effect_unknown");
    expect(persisted?.workflow.status).toBe("side_effect_unknown");
  });

  it("retains the manual-submission reason across restart and never restores the run", async () => {
    const root = await temporaryDirectory();
    const store = await LocalCheckpointStore.open(root);
    const checkpoint: LocalRunCheckpoint<StartRunRequest> = {
      ...fixture(),
      phase: "side_effect_unknown",
      workflow: {
        status: "side_effect_unknown",
        approvedSubmitActionConsumed: true,
        sideEffectReason: "manual_submission_observed",
      },
    };
    await store.write(checkpoint);
    const contextFor = vi.fn();
    const fetchMock = vi.fn(async () => applicationResponse(checkpoint, "side_effect_unknown"));
    vi.stubGlobal("fetch", fetchMock);

    await reconcileLocalRunCheckpoints({
      store,
      userDataDirectory: root,
      activeRuns: new Map(),
      contextFor,
      onRecovered: vi.fn(),
      onUnknown: vi.fn(),
    });

    expect(contextFor).not.toHaveBeenCalled();
    const resultRequest = JSON.parse(String(fetchMock.mock.calls[0]?.[1]?.body)) as {
      receipt: { errorCode: string; issues: Array<{ message: string }> };
    };
    expect(resultRequest.receipt.errorCode).toBe("manual_submission_observed");
    expect(resultRequest.receipt.issues[0]?.message).toMatch(/will not retry automatically/i);
    const persisted = await store.read<StartRunRequest>(store.scopeFor(checkpoint.request));
    expect(persisted?.workflow.sideEffectReason).toBe("manual_submission_observed");
  });

  it("removes a crash checkpoint only after an exact submitted acknowledgement", async () => {
    const root = await temporaryDirectory();
    const store = await LocalCheckpointStore.open(root);
    const checkpoint = fixture();
    const runDirectory = localRunDirectory(root, checkpoint.request);
    await mkdir(runDirectory, { recursive: true });
    await acquireFinalSubmitAuthority(runDirectory);
    await store.write(checkpoint);
    const unknown = vi.fn();
    const recovered = vi.fn();
    const contextFor = vi.fn();
    const fetchMock = vi.fn(async () => applicationResponse(checkpoint, "submitted"));
    vi.stubGlobal("fetch", fetchMock);

    await reconcileLocalRunCheckpoints({
      store,
      userDataDirectory: root,
      activeRuns: new Map(),
      contextFor,
      onRecovered: recovered,
      onUnknown: unknown,
    });

    expect(fetchMock).toHaveBeenCalledOnce();
    expect(contextFor).not.toHaveBeenCalled();
    expect(recovered).not.toHaveBeenCalled();
    expect(unknown).not.toHaveBeenCalled();
    await expect(store.read(store.scopeFor(checkpoint.request))).resolves.toBeUndefined();
  });

  it.each([
    { id: "application-other" },
    { run_id: "run-other" },
    { submitted_at_ms: undefined },
    { submitted_at_ms: Number.MAX_SAFE_INTEGER + 1 },
  ])("retains a checkpoint for a malformed submitted acknowledgement: %o", async (override) => {
    const root = await temporaryDirectory();
    const store = await LocalCheckpointStore.open(root);
    const checkpoint = fixture();
    const runDirectory = localRunDirectory(root, checkpoint.request);
    await mkdir(runDirectory, { recursive: true });
    await acquireFinalSubmitAuthority(runDirectory);
    await store.write(checkpoint);
    const unknown = vi.fn();
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify({
      id: checkpoint.request.applicationId,
      run_id: checkpoint.request.runId,
      state: "submitted",
      submitted_at_ms: Date.now(),
      ...override,
    }), {
      status: 200,
      headers: { "Content-Type": "application/json" },
    })));

    await reconcileLocalRunCheckpoints({
      store,
      userDataDirectory: root,
      activeRuns: new Map(),
      contextFor: vi.fn(),
      onRecovered: vi.fn(),
      onUnknown: unknown,
    });

    expect(unknown).toHaveBeenCalledOnce();
    await expect(store.read(store.scopeFor(checkpoint.request))).resolves.toMatchObject({
      phase: "side_effect_unknown",
    });
  });

  it.each([
    ["checkpoint URL", "checkpoint"],
    ["existing page URL", "page"],
  ] as const)("quarantines a safe recovery with a cross-job %s", async (_label, drift) => {
    const root = await temporaryDirectory();
    const store = await LocalCheckpointStore.open(root);
    const checkpoint = fixture();
    checkpoint.phase = "prepared";
    checkpoint.workflow = { status: "prepared" };
    checkpoint.browser.url = drift === "checkpoint"
      ? "https://boards.greenhouse.io/acme/jobs/456"
      : checkpoint.request.url;
    await store.write(checkpoint);
    const close = vi.fn(async () => undefined);
    const context = {
      close,
      pages: () => drift === "page" ? [{
        url: () => "https://boards.greenhouse.io/acme/jobs/456",
      }] : [],
    };
    const contextFor = vi.fn(async () => context as never);

    await reconcileLocalRunCheckpoints({
      store,
      userDataDirectory: root,
      activeRuns: new Map(),
      contextFor,
      onRecovered: vi.fn(),
      onUnknown: vi.fn(),
    });

    if (drift === "checkpoint") {
      expect(contextFor).not.toHaveBeenCalled();
      expect(close).not.toHaveBeenCalled();
    } else {
      expect(contextFor).toHaveBeenCalledOnce();
      expect(close).toHaveBeenCalledOnce();
    }
    await expect(store.read(store.scopeFor(checkpoint.request))).resolves.toBeDefined();
  });

  it("keeps a restored exact-job page offline until the local guard is installed", async () => {
    const checkpoint = fixture();
    const harness = fakeBrowserContext([
      "about:blank",
      checkpoint.request.url,
    ]);
    const installGuard = vi.fn(async (page: Page) => {
      harness.events.push(`guard:${page.url()}`);
    });

    const selected = await prepareRecoveredLocalPage(
      harness.context,
      checkpoint.request,
      checkpoint.request.url,
      installGuard,
    );

    expect(selected.url()).toBe(checkpoint.request.url);
    expect(harness.events).toEqual([
      "offline:true",
      `guard:${checkpoint.request.url}`,
      "close:about:blank",
      "offline:false",
    ]);
    expect(harness.context.pages()).toEqual([selected]);
  });

  it("does not enable local recovery network while a service worker remains", async () => {
    const checkpoint = fixture();
    const harness = fakeBrowserContext([checkpoint.request.url], [{}]);

    await expect(prepareRecoveredLocalPage(
      harness.context,
      checkpoint.request,
      checkpoint.request.url,
      async () => undefined,
    )).rejects.toThrow();

    expect(harness.events).toEqual(["offline:true"]);
  });
});

function fakeBrowserContext(
  urls: string[],
  serviceWorkers: object[] = [],
): { context: BrowserContext; events: string[] } {
  const events: string[] = [];
  const pages: Page[] = [];
  const makePage = (initialUrl: string): Page => {
    let currentUrl = initialUrl;
    const page = {
      url: () => currentUrl,
      goto: vi.fn(async (url: string) => {
        events.push(`goto:${url}`);
        currentUrl = url;
        return null;
      }),
      close: vi.fn(async () => {
        events.push(`close:${currentUrl}`);
        const index = pages.indexOf(page as Page);
        if (index >= 0) pages.splice(index, 1);
      }),
    } as unknown as Page;
    return page;
  };
  for (const url of urls) pages.push(makePage(url));
  const context = {
    pages: () => [...pages],
    serviceWorkers: () => serviceWorkers,
    setOffline: vi.fn(async (offline: boolean) => {
      events.push(`offline:${offline}`);
    }),
    newPage: vi.fn(async () => {
      const page = makePage("about:blank");
      pages.push(page);
      return page;
    }),
  } as unknown as BrowserContext;
  return { context, events };
}

function applicationResponse(
  checkpoint: LocalRunCheckpoint<StartRunRequest>,
  state: "side_effect_unknown" | "submitted",
): Response {
  return new Response(JSON.stringify({
    id: checkpoint.request.applicationId,
    run_id: checkpoint.request.runId,
    state,
    submitted_at_ms: state === "submitted" ? Date.now() : null,
  }), {
    status: 200,
    headers: { "Content-Type": "application/json" },
  });
}

function fixture(): LocalRunCheckpoint<StartRunRequest> {
  const expiresAtMs = Date.now() + 60_000;
  const job: NormalizedJob = {
    externalId: "job-123",
    canonicalUrl: "https://boards.greenhouse.io/acme/jobs/job-123",
    company: "Acme",
    title: "Engineer",
    location: "Remote",
    workplace: "remote",
    description: "Build reliable systems.",
    source: "greenhouse",
  };
  const packet: StartRunRequest["packet"] = {
    applicationId: "application-123",
    jobId: "job-123",
    resumeVersionId: "resume-123",
    approvedPacketChecksum: "",
    answers: {},
    verifiedClaimIds: [],
    applicationIdentityId: "identity-123",
  };
  packet.approvedPacketChecksum = approvedExecutionChecksum(packet, job);
  const request: StartRunRequest = {
    accountId: "account-123",
    applicationIdentityId: "identity-123",
    runId: "run-123",
    applicationId: "application-123",
    browserProfileId: "profile-123",
    url: "https://boards.greenhouse.io/acme/jobs/job-123",
    packet,
    job,
  };
  return {
    version: 1,
    phase: "final_submit_started",
    createdAtMs: Date.now() - 1_000,
    updatedAtMs: Date.now(),
    expiresAtMs,
    request,
    delivery: {
      apiOrigin: "https://bluey.example.test",
      capabilities: localRunCapabilitiesFixture(expiresAtMs),
    },
    browser: { url: request.url },
    workflow: { status: "side_effect_unknown" },
    events: [],
  };
}

async function temporaryDirectory(): Promise<string> {
  const path = await mkdtemp(join(tmpdir(), "bluey-browser-recovery-"));
  temporaryDirectories.push(path);
  return path;
}

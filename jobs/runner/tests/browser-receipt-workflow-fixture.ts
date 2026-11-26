import { createHash, createHmac, randomBytes } from "node:crypto";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { expect, vi } from "vitest";
import type { executeBrowserRun } from "../src/browser-run-execution.js";
import { readResult, stageResult, writeResult } from "../src/result-store.js";
import type { CloudRunRequest } from "../src/server.js";
import type { ApplicationWorkflowInput } from "../../workflows/src/contracts.js";

type BrowserResult = Awaited<ReturnType<typeof executeBrowserRun>>;

// Uses the real encrypted result codec and workflow receipt activity. Both
// service transports are test doubles; no HTTP server or tracker DB is used.
export async function verifyBrowserReceiptWorkflowReplay(
  request: CloudRunRequest,
  execution: BrowserResult,
  profileScope: string,
): Promise<void> {
  const authority = execution.receiptAuthority;
  if (!authority) throw new Error("Synthetic submission must retain receipt authority");
  const root = await mkdtemp(join(tmpdir(), "bluey-browser-workflow-receipt-"));
  const key = randomBytes(32);
  const context = { requestId: request.requestId, profileScope };
  const runnerOrigin = "https://runner-receipt-fixture.example.test";
  const apiOrigin = "https://api-receipt-fixture.example.test";
  const runnerToken = "synthetic-runner-token";
  const workerId = "synthetic-receipt-workflow";
  const signingKey = randomBytes(32).toString("hex");
  const receiptPath = `/api/jobs/internal/applications/${encodeURIComponent(request.applicationId)}/receipt`;
  const headers = {
    "content-type": "application/json",
    "cache-control": "no-store",
    "x-content-type-options": "nosniff",
  };
  const lookup = {
    accountId: request.accountId,
    applicationId: request.applicationId,
    applicationIdentityId: request.applicationIdentityId,
    browserSessionId: request.browserSessionId,
    runId: request.runId,
    requestId: request.requestId,
  };
  const receiptBodies: string[] = [];
  const nonces = new Set<string>();
  let reads = 0;
  let unexpectedRequests = 0;
  try {
    await stageResult(root, context, execution, key);
    expect(await readResult(root, context, key)).toBeUndefined();
    await writeResult(root, context, execution, key);

    vi.resetModules();
    vi.stubEnv("BLUEY_JOBS_API_ORIGIN", apiOrigin);
    vi.stubEnv("BLUEY_JOBS_RUNNER_ORIGIN", runnerOrigin);
    vi.stubEnv("BLUEY_JOBS_RUNNER_TOKEN", runnerToken);
    vi.stubEnv("BLUEY_JOBS_WORKFLOW_WORKER_ID", workerId);
    vi.stubEnv("BLUEY_JOBS_WORKER_SIGNING_KEY", signingKey);
    vi.stubGlobal("fetch", async (url: string | URL | Request, init?: RequestInit) => {
      const actualHeaders = new Headers(init?.headers);
      const body = init?.body;
      if (init?.method !== "POST" || init.redirect !== "error" || typeof body !== "string") {
        unexpectedRequests += 1;
        throw new Error("Unexpected synthetic request shape");
      }
      if (String(url) === `${runnerOrigin}/results`) {
        expect(actualHeaders.get("authorization")).toBe(`Bearer ${runnerToken}`);
        expect(JSON.parse(body)).toEqual(lookup);
        const committed = await readResult<BrowserResult>(root, context, key);
        if (!committed) throw new Error("Synthetic committed result is missing");
        reads += 1;
        return Response.json({ ...committed, requestId: lookup.requestId }, { headers });
      }
      if (String(url) !== `${apiOrigin}${receiptPath}`) {
        unexpectedRequests += 1;
        throw new Error("Unexpected synthetic destination");
      }
      const digest = createHash("sha256").update(body).digest("hex");
      const timestamp = actualHeaders.get("x-bluey-jobs-worker-timestamp");
      const nonce = actualHeaders.get("x-bluey-jobs-worker-nonce") ?? "";
      expect(timestamp).toMatch(/^\d+$/);
      expect(nonce).toMatch(/^[a-f0-9]{32}$/);
      expect(nonces.has(nonce)).toBe(false);
      nonces.add(nonce);
      expect(actualHeaders.get("x-bluey-jobs-worker-id")).toBe(workerId);
      expect(actualHeaders.get("x-bluey-jobs-worker-scope")).toBe("receipt");
      expect(actualHeaders.get("x-bluey-jobs-worker-audience")).toBe("bluey-jobs-api");
      expect(actualHeaders.get("x-bluey-jobs-worker-content-sha256")).toBe(digest);
      const canonical = [
        "bluey-jobs-worker-v1", timestamp, nonce, workerId, "bluey-jobs-api",
        "receipt", "POST", receiptPath, digest,
      ].join("\n");
      expect(actualHeaders.get("x-bluey-jobs-worker-signature"))
        .toBe(createHmac("sha256", signingKey).update(canonical).digest("hex"));
      expect(JSON.parse(body)).toEqual({
        account_id: request.accountId, lease_token: authority.leaseToken, fence: authority.fence,
        receipt: execution.receiptBundle, evidence_objects: execution.evidenceObjects,
      });
      receiptBodies.push(body);
      if (receiptBodies.length === 1) throw new TypeError("Synthetic receipt acknowledgement lost");
      return Response.json({
        id: request.applicationId, run_id: request.runId, state: "submitted",
      }, { headers });
    });
    const { persistSubmissionReceipt } = await import("../../workflows/src/activities.js");
    const input: ApplicationWorkflowInput & { browserSessionId: string; resultRequestId: string } = {
      accountId: request.accountId,
      applicationId: request.applicationId,
      applicationIdentityId: request.applicationIdentityId,
      browserProfileId: request.browserProfileId,
      browserSessionId: request.browserSessionId,
      idempotencyKey: request.runId,
      resultRequestId: request.requestId,
      jobId: request.packet.jobId,
      canonicalJobKey: request.job.canonicalUrl,
      packetId: request.packet.resumeVersionId,
      packet: request.packet,
      job: request.job,
      runner: "cloud",
      url: request.url,
    };
    await expect(persistSubmissionReceipt(input)).rejects.toThrow("Synthetic receipt acknowledgement lost");
    await expect(persistSubmissionReceipt(input)).resolves.toBeUndefined();
    expect(reads).toBe(2);
    expect(receiptBodies).toHaveLength(2);
    expect(receiptBodies[1]).toBe(receiptBodies[0]);
    expect(nonces.size).toBe(2);
    expect(unexpectedRequests).toBe(0);
  } finally {
    vi.unstubAllGlobals();
    vi.unstubAllEnvs();
    vi.resetModules();
    key.fill(0);
    await rm(root, { recursive: true, force: true });
  }
}

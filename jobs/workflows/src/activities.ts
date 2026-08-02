import { createHash } from "node:crypto";
import type { ApplicationReceiptBundle, ApplicationState, EvidenceObjectUpload, SubmissionReceipt } from "@bluey/jobs-automation";
import type { ApplicationWorkflowInput, InterventionResolution, RunnerExecutionResult } from "./contracts.js";
import { workerAuthHeaders } from "./worker-auth.js";

const apiOrigin = process.env.BLUEY_JOBS_API_ORIGIN || "http://127.0.0.1:8080";
const serviceSigningKey = process.env.BLUEY_JOBS_WORKER_SIGNING_KEY || "";
const serviceWorkerId = process.env.BLUEY_JOBS_WORKER_ID || `workflow-${process.pid}`;
const runnerOrigin = process.env.BLUEY_JOBS_RUNNER_ORIGIN || "http://127.0.0.1:8091";
const runnerToken = process.env.BLUEY_JOBS_RUNNER_TOKEN || "";

export async function assertEntitlement(input: ApplicationWorkflowInput): Promise<void> {
  if (input.runner !== "cloud") throw new Error("Cloud workflows require the cloud runner");
  await event(input, "entitlement_checked", { runner: input.runner });
}

export async function loadPacket(input: ApplicationWorkflowInput): Promise<void> {
  if (input.packet.applicationId !== input.applicationId
    || input.packet.resumeVersionId !== input.packetId
    || input.packet.applicationIdentityId !== input.applicationIdentityId) {
    throw new Error("Application execution bundle is inconsistent");
  }
  await event(input, "application_loaded", {
    resume_version_id: input.packetId,
    application_identity_id: input.applicationIdentityId,
  });
}

export async function allocateBrowser(input: ApplicationWorkflowInput): Promise<{ browserSessionId: string }> {
  const browserSessionId = `${input.runner}-${input.applicationId}`;
  await event(input, "browser_allocated", { browser_session_id: browserSessionId });
  return { browserSessionId };
}

export async function runApplication(input: ApplicationWorkflowInput & { browserSessionId: string }): Promise<RunnerExecutionResult> {
  if (!runnerToken) throw new Error("BLUEY_JOBS_RUNNER_TOKEN is required");
  await event(input, "runner_requested", {
    runner: input.runner,
    browser_session_id: input.browserSessionId,
    url: input.url,
  });
  const response = await fetch(`${runnerOrigin}/runs`, {
    method: "POST",
    headers: { Authorization: `Bearer ${runnerToken}`, "Content-Type": "application/json" },
    body: JSON.stringify({ ...input, runId: input.idempotencyKey }),
  });
  if (!response.ok) throw new Error(`Bluey Jobs runner returned ${response.status}`);
  const result = await response.json() as RunnerExecutionResult;
  if (!result.receipt || !["submitted", "needs_input", "failed"].includes(result.receipt.status)) {
    throw new Error("Bluey Jobs runner returned an invalid receipt");
  }
  return result;
}

export async function resumeApplication(input: ApplicationWorkflowInput & {
  browserSessionId: string;
  requestId: string;
  resolution: InterventionResolution;
}): Promise<RunnerExecutionResult> {
  if (!runnerToken) throw new Error("BLUEY_JOBS_RUNNER_TOKEN is required");
  await event(input, "runner_resuming", {
    browser_session_id: input.browserSessionId,
    action: input.resolution.action,
    field: input.resolution.field,
  });
  const response = await fetch(
    `${runnerOrigin}/runs/${encodeURIComponent(input.browserSessionId)}/resume`,
    {
      method: "POST",
      headers: { Authorization: `Bearer ${runnerToken}`, "Content-Type": "application/json" },
      body: JSON.stringify({
        ...input.resolution,
        requestId: input.requestId,
        profileScope: runnerProfileScope(input.accountId, input.applicationIdentityId),
      }),
    },
  );
  if (!response.ok) throw new Error(`Bluey Jobs runner resume returned ${response.status}`);
  return response.json() as Promise<RunnerExecutionResult>;
}

function runnerProfileScope(accountId: string, applicationIdentityId: string): string {
  return createHash("sha256")
    .update(accountId)
    .update("\0")
    .update(applicationIdentityId)
    .digest("hex")
    .slice(0, 40);
}

export async function persistReceipt(input: ApplicationWorkflowInput & {
  receiptBundle: ApplicationReceiptBundle;
  evidenceObjects: EvidenceObjectUpload[];
}): Promise<void> {
  await workerRequest(`/api/jobs/internal/applications/${encodeURIComponent(input.applicationId)}/receipt`, {
    account_id: input.accountId,
    receipt: input.receiptBundle,
    evidence_objects: input.evidenceObjects,
  });
  await event(input, "receipt_persisted", { receipt_id: input.receiptBundle.receiptId });
}

export async function releaseBrowser(browserSessionId: string): Promise<void> {
  if (!browserSessionId) throw new Error("browser session ID is required");
  if (!runnerToken) throw new Error("BLUEY_JOBS_RUNNER_TOKEN is required");
  const response = await fetch(`${runnerOrigin}/runs/${encodeURIComponent(browserSessionId)}`, {
    method: "DELETE",
    headers: { Authorization: `Bearer ${runnerToken}` },
  });
  if (!response.ok && response.status !== 404) {
    throw new Error(`Bluey Jobs runner release returned ${response.status}`);
  }
}

export async function recordState(input: ApplicationWorkflowInput, state: ApplicationState): Promise<void> {
  await workerRequest(`/api/jobs/internal/applications/${encodeURIComponent(input.applicationId)}/state`, {
    account_id: input.accountId,
    state,
  });
}

export async function createIntervention(input: ApplicationWorkflowInput, receipt: SubmissionReceipt): Promise<string> {
  const result = await workerRequest<{ id: string }>(
    `/api/jobs/internal/applications/${encodeURIComponent(input.applicationId)}/interventions`,
    { account_id: input.accountId, receipt },
  );
  return result.id;
}

async function event(input: ApplicationWorkflowInput, type: string, body: unknown): Promise<void> {
  await workerRequest(`/api/jobs/internal/runs/${encodeURIComponent(input.idempotencyKey)}/events`, {
    account_id: input.accountId,
    application_id: input.applicationId,
    type,
    body,
  });
}

async function workerRequest<T = unknown>(path: string, body: unknown): Promise<T> {
  if (!serviceSigningKey) throw new Error("BLUEY_JOBS_WORKER_SIGNING_KEY is required");
  const encodedBody = JSON.stringify(body);
  const response = await fetch(`${apiOrigin}${path}`, {
    method: "POST",
    headers: {
      ...workerAuthHeaders({
        signingKey: serviceSigningKey,
        workerId: serviceWorkerId,
        method: "POST",
        path,
        body: encodedBody,
      }),
      "Content-Type": "application/json",
    },
    body: encodedBody,
  });
  if (!response.ok) throw new Error(`Jobs API returned ${response.status}`);
  if (response.status === 204) return undefined as T;
  return response.json() as Promise<T>;
}

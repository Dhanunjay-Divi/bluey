import { createHash, randomUUID } from "node:crypto";
import {
  isSuccessfulExactSubmitHttpStatus,
  type ApplicationReceiptBundle,
  type ApplicationState,
  type EvidenceObjectUpload,
  type InterventionRequest,
  type SubmissionReceipt,
  type ValidationIssue,
} from "@bluey/jobs-automation";
import { createJobsWorkerAuthHeaders } from "@bluey/jobs-automation/worker-auth";
import {
  isSubmissionReceiptAuthority,
  type ApplicationWorkflowInput,
  type InterventionResolution,
  type RunnerExecutionResult,
  type SubmissionReceiptAuthority,
} from "./contracts.js";

const apiOrigin = serviceOrigin(
  process.env.BLUEY_JOBS_API_ORIGIN || "http://127.0.0.1:8080",
  "BLUEY_JOBS_API_ORIGIN",
);
const workerSigningKey = process.env.BLUEY_JOBS_WORKER_SIGNING_KEY || "";
const workerId = process.env.BLUEY_JOBS_WORKFLOW_WORKER_ID
  || `workflow-${process.pid}-${randomUUID()}`;
const runnerOrigin = serviceOrigin(
  process.env.BLUEY_JOBS_RUNNER_ORIGIN || "http://127.0.0.1:8091",
  "BLUEY_JOBS_RUNNER_ORIGIN",
);
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

export async function runApplication(
  input: ApplicationWorkflowInput & { browserSessionId: string },
): Promise<RunnerExecutionResult> {
  if (!runnerToken) throw new Error("BLUEY_JOBS_RUNNER_TOKEN is required");
  const resultRequestId = `${input.idempotencyKey}:initial`;
  const completed = await loadDurableRunnerExecution(input, resultRequestId);
  if (completed) return workflowExecutionResult(completed);
  await event(input, "runner_requested", {
    runner: input.runner,
    browser_session_id: input.browserSessionId,
    url: input.url,
  });
  const response = await fetch(`${runnerOrigin}/runs`, {
    method: "POST",
    headers: { Authorization: `Bearer ${runnerToken}`, "Content-Type": "application/json" },
    body: JSON.stringify({ ...input, runId: input.idempotencyKey }),
    redirect: "error",
  });
  if (!response.ok) throw new Error(`Bluey Jobs runner returned ${response.status}`);
  return workflowExecutionResult(parseRunnerExecutionResult(await response.json()));
}

export async function resumeApplication(input: ApplicationWorkflowInput & {
  browserSessionId: string;
  requestId: string;
  resolution: InterventionResolution;
}): Promise<RunnerExecutionResult> {
  if (!runnerToken) throw new Error("BLUEY_JOBS_RUNNER_TOKEN is required");
  const completed = await loadDurableRunnerExecution(input, input.requestId);
  if (completed) return workflowExecutionResult(completed);
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
        accountId: input.accountId,
        applicationId: input.applicationId,
        applicationIdentityId: input.applicationIdentityId,
        runId: input.idempotencyKey,
        requestId: input.requestId,
        profileScope: runnerProfileScope(input.accountId, input.applicationIdentityId),
      }),
      redirect: "error",
    },
  );
  if (!response.ok) throw new Error(`Bluey Jobs runner resume returned ${response.status}`);
  return workflowExecutionResult(parseRunnerExecutionResult(await response.json()));
}

export async function persistSubmissionReceipt(input: ApplicationWorkflowInput & {
  browserSessionId: string;
  resultRequestId: string;
}): Promise<void> {
  if (!runnerToken) throw new Error("BLUEY_JOBS_RUNNER_TOKEN is required");
  const execution = await loadDurableRunnerExecution(input, input.resultRequestId);
  if (!execution) throw new Error("The runner's committed submission result is not available");
  const evidence = submittedExecutionEvidence(execution);
  await persistReceipt({
    ...input,
    receiptBundle: evidence.receiptBundle,
    evidenceObjects: evidence.evidenceObjects,
    receiptAuthority: evidence.receiptAuthority,
  });
}

function runnerProfileScope(accountId: string, applicationIdentityId: string): string {
  return createHash("sha256")
    .update(accountId)
    .update("\0")
    .update(applicationIdentityId)
    .digest("hex")
    .slice(0, 40);
}

async function persistReceipt(input: ApplicationWorkflowInput & {
  receiptBundle: ApplicationReceiptBundle;
  evidenceObjects: EvidenceObjectUpload[];
  receiptAuthority: SubmissionReceiptAuthority;
}): Promise<void> {
  if (!isSubmissionReceiptAuthority(input.receiptAuthority)) {
    throw new Error("Submitted run has invalid fenced receipt authority");
  }
  await workerRequest(`/api/jobs/internal/applications/${encodeURIComponent(input.applicationId)}/receipt`, {
    account_id: input.accountId,
    lease_token: input.receiptAuthority.leaseToken,
    fence: input.receiptAuthority.fence,
    receipt: input.receiptBundle,
    evidence_objects: input.evidenceObjects,
  });
}

export async function releaseBrowser(browserSessionId: string): Promise<void> {
  if (!browserSessionId) throw new Error("browser session ID is required");
  if (!runnerToken) throw new Error("BLUEY_JOBS_RUNNER_TOKEN is required");
  const response = await fetch(`${runnerOrigin}/runs/${encodeURIComponent(browserSessionId)}`, {
    method: "DELETE",
    headers: { Authorization: `Bearer ${runnerToken}` },
    redirect: "error",
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
  if (!workerSigningKey) throw new Error("BLUEY_JOBS_WORKER_SIGNING_KEY is required");
  const serializedBody = JSON.stringify(body);
  const response = await fetch(`${apiOrigin}${path}`, {
    method: "POST",
    headers: {
      ...createJobsWorkerAuthHeaders({
        signingKey: workerSigningKey,
        workerId,
        method: "POST",
        path,
        body: serializedBody,
      }),
      "Content-Type": "application/json",
    },
    body: serializedBody,
    redirect: "error",
  });
  if (!response.ok) throw new Error(`Jobs API returned ${response.status}`);
  if (response.status === 204) return undefined as T;
  return response.json() as Promise<T>;
}

interface RawRunnerExecutionResult {
  receipt: SubmissionReceipt;
  receiptBundle?: ApplicationReceiptBundle;
  evidenceObjects?: EvidenceObjectUpload[];
  receiptAuthority?: SubmissionReceiptAuthority;
}

function workflowExecutionResult(execution: RawRunnerExecutionResult): RunnerExecutionResult {
  if (execution.receipt.status === "submitted") submittedExecutionEvidence(execution);
  return { receipt: execution.receipt };
}

function submittedExecutionEvidence(execution: RawRunnerExecutionResult): {
  receiptBundle: ApplicationReceiptBundle;
  evidenceObjects: EvidenceObjectUpload[];
  receiptAuthority: SubmissionReceiptAuthority;
} {
  if (execution.receipt.status !== "submitted") {
    throw new Error("The runner's committed result is not a submission");
  }
  if (!execution.receiptBundle
    || typeof execution.receiptBundle !== "object"
    || Array.isArray(execution.receiptBundle)) {
    throw new Error("Submitted runner result is missing its receipt bundle");
  }
  if (!Array.isArray(execution.evidenceObjects) || execution.evidenceObjects.length === 0) {
    throw new Error("Submitted runner result is missing evidence bytes");
  }
  if (!isSubmissionReceiptAuthority(execution.receiptAuthority)) {
    throw new Error("Submitted runner result is missing fenced receipt authority");
  }
  return {
    receiptBundle: execution.receiptBundle,
    evidenceObjects: execution.evidenceObjects,
    receiptAuthority: execution.receiptAuthority,
  };
}

async function loadDurableRunnerExecution(
  input: Pick<
    ApplicationWorkflowInput,
    | "accountId"
    | "applicationId"
    | "applicationIdentityId"
    | "idempotencyKey"
  > & {
    browserSessionId: string;
  },
  requestId: string,
): Promise<RawRunnerExecutionResult | undefined> {
  const response = await fetch(`${runnerOrigin}/results`, {
    method: "POST",
    headers: { Authorization: `Bearer ${runnerToken}`, "Content-Type": "application/json" },
    body: JSON.stringify({
      accountId: input.accountId,
      applicationId: input.applicationId,
      applicationIdentityId: input.applicationIdentityId,
      browserSessionId: input.browserSessionId,
      runId: input.idempotencyKey,
      requestId,
    }),
    redirect: "error",
  });
  if (response.status === 404 || response.status === 204) return undefined;
  if (!response.ok) throw new Error(`Bluey Jobs runner result lookup returned ${response.status}`);
  return parseRunnerExecutionResult(await response.json());
}

function parseRunnerExecutionResult(value: unknown): RawRunnerExecutionResult {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("Bluey Jobs runner returned an invalid receipt");
  }
  const result = value as Record<string, unknown>;
  const receipt = parseWorkflowReceipt(result.receipt);
  const hasAuthority = Object.prototype.hasOwnProperty.call(result, "receiptAuthority");
  if (receipt.status === "submitted") {
    if (!isSubmissionReceiptAuthority(result.receiptAuthority)) {
      throw new Error("Submitted runner result is missing fenced receipt authority");
    }
  } else if (hasAuthority) {
    throw new Error("Non-submitted runner result included receipt authority");
  }
  return {
    receipt,
    ...(Object.prototype.hasOwnProperty.call(result, "receiptBundle")
      ? { receiptBundle: result.receiptBundle as ApplicationReceiptBundle }
      : {}),
    ...(Object.prototype.hasOwnProperty.call(result, "evidenceObjects")
      ? { evidenceObjects: result.evidenceObjects as EvidenceObjectUpload[] }
      : {}),
    ...(hasAuthority
      ? { receiptAuthority: result.receiptAuthority as SubmissionReceiptAuthority }
      : {}),
  };
}

function parseWorkflowReceipt(value: unknown): SubmissionReceipt {
  const receipt = objectRecord(value, "Bluey Jobs runner returned an invalid receipt");
  const status = receipt.status;
  if (status !== "submitted" && status !== "needs_input" && status !== "failed") {
    throw new Error("Bluey Jobs runner returned an invalid receipt");
  }
  if (!Array.isArray(receipt.issues) || receipt.issues.length > 100) {
    throw new Error("Bluey Jobs runner returned invalid receipt issues");
  }
  const parsed: SubmissionReceipt = {
    status,
    issues: receipt.issues.map(parseValidationIssue),
  };
  const confirmationText = optionalString(receipt, "confirmationText", 8_000);
  if (confirmationText !== undefined) parsed.confirmationText = confirmationText;
  const confirmationUrl = optionalWebUrl(receipt, "confirmationUrl");
  if (confirmationUrl !== undefined) parsed.confirmationUrl = confirmationUrl;
  if (Object.prototype.hasOwnProperty.call(receipt, "submitHttpStatus")) {
    const submitHttpStatus = receipt.submitHttpStatus;
    if (!isSuccessfulExactSubmitHttpStatus(submitHttpStatus)) {
      throw new Error("Bluey Jobs runner returned an invalid receipt");
    }
    parsed.submitHttpStatus = submitHttpStatus;
  }
  const submittedAt = optionalTimestamp(receipt, "submittedAt");
  if (submittedAt !== undefined) parsed.submittedAt = submittedAt;
  if (Object.prototype.hasOwnProperty.call(receipt, "intervention")) {
    parsed.intervention = parseIntervention(receipt.intervention);
  }
  return parsed;
}

function parseValidationIssue(value: unknown): ValidationIssue {
  const issue = objectRecord(value, "Bluey Jobs runner returned an invalid receipt issue");
  const severity = issue.severity;
  if (severity !== "blocking" && severity !== "warning") {
    throw new Error("Bluey Jobs runner returned an invalid receipt issue");
  }
  return {
    field: requiredString(issue, "field", 256),
    message: requiredString(issue, "message", 4_000),
    severity,
  };
}

function parseIntervention(value: unknown): InterventionRequest {
  const intervention = objectRecord(
    value,
    "Bluey Jobs runner returned an invalid receipt intervention",
  );
  const kind = intervention.kind;
  if (kind !== "captcha"
    && kind !== "two_factor"
    && kind !== "assessment"
    && kind !== "unknown_question"
    && kind !== "missing_fact"
    && kind !== "sensitive_question"
    && kind !== "browser_takeover") {
    throw new Error("Bluey Jobs runner returned an invalid receipt intervention");
  }
  const parsed: InterventionRequest = {
    kind,
    title: requiredString(intervention, "title", 1_000),
    detail: requiredString(intervention, "detail", 8_000),
  };
  const field = optionalString(intervention, "field", 256);
  if (field !== undefined) parsed.field = field;
  if (Object.prototype.hasOwnProperty.call(intervention, "choices")) {
    if (!Array.isArray(intervention.choices) || intervention.choices.length > 100) {
      throw new Error("Bluey Jobs runner returned invalid intervention choices");
    }
    parsed.choices = intervention.choices.map((choice) => {
      if (typeof choice !== "string" || choice.length > 1_000) {
        throw new Error("Bluey Jobs runner returned invalid intervention choices");
      }
      return choice;
    });
  }
  const takeoverUrl = optionalWebUrl(intervention, "takeoverUrl");
  if (takeoverUrl !== undefined) parsed.takeoverUrl = takeoverUrl;
  if (Object.prototype.hasOwnProperty.call(intervention, "resolution")) {
    parsed.resolution = parseInterventionResolution(intervention.resolution);
  }
  return parsed;
}

function parseInterventionResolution(value: unknown): NonNullable<InterventionRequest["resolution"]> {
  const resolution = objectRecord(
    value,
    "Bluey Jobs runner returned an invalid intervention resolution",
  );
  const kind = resolution.kind;
  if ((kind !== "browser_takeover" && kind !== "email_otp_approval" && kind !== "answer")
    || typeof resolution.resumeAfter !== "boolean") {
    throw new Error("Bluey Jobs runner returned an invalid intervention resolution");
  }
  const parsed: NonNullable<InterventionRequest["resolution"]> = {
    kind,
    resumeAfter: resolution.resumeAfter,
  };
  const expiresAt = optionalTimestamp(resolution, "expiresAt");
  if (expiresAt !== undefined) parsed.expiresAt = expiresAt;
  if (Object.prototype.hasOwnProperty.call(resolution, "provider")) {
    if (resolution.provider !== "gmail" && resolution.provider !== "outlook_email") {
      throw new Error("Bluey Jobs runner returned an invalid intervention provider");
    }
    parsed.provider = resolution.provider;
  }
  const messageId = optionalString(resolution, "messageId", 1_000);
  if (messageId !== undefined) parsed.messageId = messageId;
  return parsed;
}

function objectRecord(value: unknown, message: string): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error(message);
  return value as Record<string, unknown>;
}

function requiredString(
  record: Record<string, unknown>,
  key: string,
  maximumLength: number,
): string {
  const value = record[key];
  if (typeof value !== "string" || value.length === 0 || value.length > maximumLength) {
    throw new Error(`Bluey Jobs runner returned an invalid ${key}`);
  }
  return value;
}

function optionalString(
  record: Record<string, unknown>,
  key: string,
  maximumLength: number,
): string | undefined {
  if (!Object.prototype.hasOwnProperty.call(record, key)) return undefined;
  const value = record[key];
  if (typeof value !== "string" || value.length > maximumLength) {
    throw new Error(`Bluey Jobs runner returned an invalid ${key}`);
  }
  return value;
}

function optionalTimestamp(record: Record<string, unknown>, key: string): string | undefined {
  const value = optionalString(record, key, 64);
  if (value === undefined) return undefined;
  if (!Number.isFinite(Date.parse(value))) {
    throw new Error(`Bluey Jobs runner returned an invalid ${key}`);
  }
  return value;
}

function optionalWebUrl(record: Record<string, unknown>, key: string): string | undefined {
  const value = optionalString(record, key, 4_096);
  if (value === undefined) return undefined;
  try {
    const url = new URL(value);
    const loopback = new Set(["localhost", "127.0.0.1", "::1", "[::1]"]).has(url.hostname);
    if ((url.protocol !== "https:" && !(url.protocol === "http:" && loopback))
      || url.username
      || url.password) {
      throw new Error("invalid URL");
    }
    return url.toString();
  } catch {
    throw new Error(`Bluey Jobs runner returned an invalid ${key}`);
  }
}

function serviceOrigin(rawOrigin: string, name: string): string {
  try {
    const url = new URL(rawOrigin);
    const loopback = new Set(["localhost", "127.0.0.1", "::1", "[::1]"]).has(url.hostname);
    if ((url.protocol !== "https:" && !(url.protocol === "http:" && loopback))
      || url.username
      || url.password
      || url.pathname !== "/"
      || url.search
      || url.hash) {
      throw new Error("invalid origin");
    }
    return url.origin;
  } catch {
    throw new Error(`${name} must be an HTTPS origin or a loopback HTTP origin`);
  }
}

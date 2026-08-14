import { createHash, randomUUID } from "node:crypto";
import { ApplicationFailure } from "@temporalio/common";
import {
  assertApprovedExecutionChecksum,
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
  managedCloudReleaseMemoBytes,
  parseManagedCloudReleaseMemo,
  type ManagedCloudReleaseMemoAuthority,
} from "@bluey/jobs-automation/managed-cloud-execution";
import {
  isSubmissionReceiptAuthority,
  type ApplicationWorkflowInput,
  type InterventionResolution,
  type ManagedWorkflowCommandInput,
  type ManagedWorkflowResumeCommandInput,
  type WorkflowCommandAuthority,
  type WorkflowCommandOperation,
  type WorkflowCommandStep,
  type WorkflowInterventionAuthority,
  type WorkflowPublishedIntervention,
  type WorkflowResumeCommandAuthority,
  type WorkflowTerminalCommand,
  type WorkflowTerminalReasonCode,
  type WorkflowTerminalState,
  type RunnerExecutionResult,
  type SubmissionReceiptAuthority,
} from "./contracts.js";
import { parseInterventionResolution as parseResolution } from "./intervention-policy.js";

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
const API_REQUEST_TIMEOUT_MS = 30_000;
const RUNNER_LOOKUP_TIMEOUT_MS = 30_000;
const RUNNER_EXECUTION_TIMEOUT_MS = 9 * 60 * 1_000;

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
  return runApplicationWithRequestId(input, `${input.idempotencyKey}:initial`);
}

async function runApplicationWithRequestId(
  input: ApplicationWorkflowInput & { browserSessionId: string },
  resultRequestId: string,
): Promise<RunnerExecutionResult> {
  if (!runnerToken) throw new Error("BLUEY_JOBS_RUNNER_TOKEN is required");
  const completed = await loadDurableRunnerExecution(input, resultRequestId);
  if (completed) return workflowExecutionResult(completed);
  await event(input, "runner_requested", {
    runner: input.runner,
    browser_session_id: input.browserSessionId,
    url: input.url,
  });
  return requestApplicationRun(input, resultRequestId);
}

async function requestApplicationRun(
  input: ApplicationWorkflowInput & { browserSessionId: string },
  resultRequestId: string,
): Promise<RunnerExecutionResult> {
  const response = await fetch(`${runnerOrigin}/runs`, {
    method: "POST",
    headers: { Authorization: `Bearer ${runnerToken}`, "Content-Type": "application/json" },
    body: JSON.stringify({
      accountId: input.accountId,
      applicationId: input.applicationId,
      applicationIdentityId: input.applicationIdentityId,
      browserProfileId: input.browserProfileId,
      browserSessionId: input.browserSessionId,
      runId: input.idempotencyKey,
      requestId: resultRequestId,
      url: input.url,
      packet: input.packet,
      job: input.job,
    }),
    redirect: "error",
    signal: AbortSignal.timeout(RUNNER_EXECUTION_TIMEOUT_MS),
  });
  await assertRunnerResponse(response, resultRequestId);
  return workflowExecutionResult(
    await readRunnerExecutionResult(response, { ...input, requestId: resultRequestId }),
  );
}

async function requestManagedApplicationRun(
  input: ApplicationWorkflowInput & { browserSessionId: string },
  resultRequestId: string,
  managedCloudRelease: ManagedCloudReleaseMemoAuthority,
): Promise<RunnerExecutionResult> {
  const response = await fetch(`${runnerOrigin}/runs`, {
    method: "POST",
    headers: { Authorization: `Bearer ${runnerToken}`, "Content-Type": "application/json" },
    body: JSON.stringify({
      accountId: input.accountId,
      applicationId: input.applicationId,
      applicationIdentityId: input.applicationIdentityId,
      browserProfileId: input.browserProfileId,
      browserSessionId: input.browserSessionId,
      runId: input.idempotencyKey,
      requestId: resultRequestId,
      url: input.url,
      packet: input.packet,
      job: input.job,
      managedCloudRelease,
    }),
    redirect: "error",
    signal: AbortSignal.timeout(RUNNER_EXECUTION_TIMEOUT_MS),
  });
  await assertRunnerResponse(response, resultRequestId);
  return workflowExecutionResult(
    await readRunnerExecutionResult(response, { ...input, requestId: resultRequestId }),
  );
}

export async function resumeApplication(input: ApplicationWorkflowInput & {
  browserSessionId: string;
  requestId: string;
  resolution: InterventionResolution;
}): Promise<RunnerExecutionResult> {
  return resumeApplicationWithOptions(input);
}

async function resumeApplicationWithOptions(input: ApplicationWorkflowInput & {
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
  return requestApplicationResume(input);
}

async function requestApplicationResume(input: ApplicationWorkflowInput & {
  browserSessionId: string;
  requestId: string;
  resolution: InterventionResolution;
}): Promise<RunnerExecutionResult> {
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
      signal: AbortSignal.timeout(RUNNER_EXECUTION_TIMEOUT_MS),
    },
  );
  await assertRunnerResponse(response, input.requestId);
  return workflowExecutionResult(await readRunnerExecutionResult(response, input));
}

async function requestManagedApplicationResume(input: ApplicationWorkflowInput & {
  browserSessionId: string;
  requestId: string;
  resolution: InterventionResolution;
}, managedCloudRelease: ManagedCloudReleaseMemoAuthority): Promise<RunnerExecutionResult> {
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
        managedCloudRelease,
      }),
      redirect: "error",
      signal: AbortSignal.timeout(RUNNER_EXECUTION_TIMEOUT_MS),
    },
  );
  await assertRunnerResponse(response, input.requestId);
  return workflowExecutionResult(await readRunnerExecutionResult(response, input));
}

/**
 * Protocol-v2 start activity. Its argument and return value are the complete
 * Temporal-visible contract; private command material is loaded and consumed
 * inside this activity only.
 */
export async function executeApplicationCommand(
  authority: WorkflowCommandAuthority,
): Promise<WorkflowCommandStep> {
  const materialized = await materializeWorkflowCommand(authority, "start");
  const input = materialized.workflowInput;
  const runnerInput = { ...input, browserSessionId: materialized.browserSessionId };
  let completed: RawRunnerExecutionResult | undefined;
  try {
    completed = await loadDurableRunnerExecution(
      runnerInput,
      materialized.resultRequestId,
    );
  } catch (error) {
    if (error instanceof RunnerSideEffectUnknown) return { state: "side_effect_unknown" };
    throw error;
  }
  let execution: RunnerExecutionResult;
  if (completed) {
    execution = workflowExecutionResult(completed);
  } else {
    await recordState(input, "running");
    try {
      execution = await requestApplicationRun(runnerInput, materialized.resultRequestId);
    } catch (error) {
      if (error instanceof RunnerSideEffectUnknown) return { state: "side_effect_unknown" };
      throw error;
    }
  }
  return finishOpaqueCommand(materialized, execution);
}

/** Managed-cloud start activity. Release memo A stays opaque in Temporal history. */
export async function executeManagedApplicationCommand(
  input: ManagedWorkflowCommandInput,
): Promise<WorkflowCommandStep> {
  const parsed = parseManagedWorkflowCommandInput(input);
  const materialized = await materializeManagedWorkflowCommand(
    parsed.command,
    "start",
    parsed.managedCloudRelease,
  );
  const workflowInput = materialized.workflowInput;
  const runnerInput = {
    ...workflowInput,
    browserSessionId: materialized.browserSessionId,
  };
  let completed: RawRunnerExecutionResult | undefined;
  try {
    completed = await loadDurableRunnerExecution(
      runnerInput,
      materialized.resultRequestId,
    );
  } catch (error) {
    if (error instanceof RunnerSideEffectUnknown) return { state: "side_effect_unknown" };
    throw error;
  }
  let execution: RunnerExecutionResult;
  if (completed) {
    execution = workflowExecutionResult(completed);
  } else {
    await recordState(workflowInput, "running");
    try {
      execution = await requestManagedApplicationRun(
        runnerInput,
        materialized.resultRequestId,
        parsed.managedCloudRelease,
      );
    } catch (error) {
      if (error instanceof RunnerSideEffectUnknown) return { state: "side_effect_unknown" };
      throw error;
    }
  }
  return finishOpaqueCommand(materialized, execution);
}

/** Protocol-v2 resume activity, bound to one exact open intervention. */
export async function resumeApplicationCommand(input: {
  workflow: WorkflowCommandAuthority;
  command: WorkflowResumeCommandAuthority;
}): Promise<WorkflowCommandStep> {
  assertResumeMatchesWorkflow(input.workflow, input.command);
  const materialized = await materializeWorkflowCommand(input.command, "resume");
  if (!materialized.resolution) throw new Error("Opaque resolution authority is missing");
  const runnerInput = {
    ...materialized.workflowInput,
    browserSessionId: materialized.browserSessionId,
    requestId: materialized.resultRequestId,
    resolution: materialized.resolution,
  };
  let completed: RawRunnerExecutionResult | undefined;
  try {
    completed = await loadDurableRunnerExecution(
      runnerInput,
      materialized.resultRequestId,
    );
  } catch (error) {
    if (error instanceof RunnerSideEffectUnknown) return { state: "side_effect_unknown" };
    throw error;
  }
  let execution: RunnerExecutionResult;
  if (completed) {
    execution = workflowExecutionResult(completed);
  } else {
    await recordState(materialized.workflowInput, "running");
    try {
      execution = await requestApplicationResume(runnerInput);
    } catch (error) {
      if (error instanceof RunnerSideEffectUnknown) return { state: "side_effect_unknown" };
      throw error;
    }
  }
  return finishOpaqueCommand(materialized, execution);
}

/** Managed-cloud resume activity, bound to the workflow's immutable release memo A. */
export async function resumeManagedApplicationCommand(
  input: ManagedWorkflowResumeCommandInput,
): Promise<WorkflowCommandStep> {
  const parsed = parseManagedWorkflowResumeCommandInput(input);
  assertResumeMatchesWorkflow(parsed.workflow, parsed.command);
  const materialized = await materializeManagedWorkflowCommand(
    parsed.command,
    "resume",
    parsed.managedCloudRelease,
  );
  if (!materialized.resolution) throw new Error("Opaque resolution authority is missing");
  const runnerInput = {
    ...materialized.workflowInput,
    browserSessionId: materialized.browserSessionId,
    requestId: materialized.resultRequestId,
    resolution: materialized.resolution,
  };
  let completed: RawRunnerExecutionResult | undefined;
  try {
    completed = await loadDurableRunnerExecution(
      runnerInput,
      materialized.resultRequestId,
    );
  } catch (error) {
    if (error instanceof RunnerSideEffectUnknown) return { state: "side_effect_unknown" };
    throw error;
  }
  let execution: RunnerExecutionResult;
  if (completed) {
    execution = workflowExecutionResult(completed);
  } else {
    await recordState(materialized.workflowInput, "running");
    try {
      execution = await requestManagedApplicationResume(
        runnerInput,
        parsed.managedCloudRelease,
      );
    } catch (error) {
      if (error instanceof RunnerSideEffectUnknown) return { state: "side_effect_unknown" };
      throw error;
    }
  }
  return finishOpaqueCommand(materialized, execution);
}

/**
 * Publish only after the workflow has recorded the prepared opaque ID as its
 * exact Update authority. Retrying this activity replays the same command and
 * prepared intervention; it never creates a second public prompt.
 */
export async function publishApplicationIntervention(
  input: WorkflowInterventionAuthority,
): Promise<WorkflowPublishedIntervention> {
  const operation = commandOperation(input.command);
  assertOpaqueAuthority(input.command, operation);
  if (!opaqueIdentifier(input.interventionId, 128)) {
    throwClosedActivityFailure("invalid_authority");
  }
  const path = workflowCommandPath(
    input.command.requestId,
    `/intervention/${encodeURIComponent(input.interventionId)}/publish`,
  );
  const response = await workerRequest<unknown>(path, commandAuthorityBody(input.command));
  try {
    parseWorkflowInterventionMutation(
      response,
      input.command,
      input.interventionId,
      "publish",
    );
  } catch {
    throwClosedActivityFailure("identity_conflict");
  }
  return { state: "needs_input", interventionId: input.interventionId };
}

/** Persist an exact terminal command and close its runner before completion. */
export async function finalizeApplicationCommand(
  input: WorkflowTerminalCommand,
): Promise<{ state: WorkflowTerminalState }> {
  assertTerminalCommand(input);
  const operation = commandOperation(input.command);
  const materialized = await materializeWorkflowCommand(input.command, operation);
  await finalizeMaterializedCommand(
    materialized,
    input.terminalState,
    input.reasonCode,
    input.openInterventionId,
  );
  return { state: input.terminalState };
}

interface MaterializedWorkflowCommand {
  authority: WorkflowCommandAuthority;
  operation: WorkflowCommandOperation;
  interventionId?: string;
  workflowInput: ApplicationWorkflowInput;
  browserSessionId: string;
  resultRequestId: string;
  resolution?: InterventionResolution;
}

async function materializeWorkflowCommand(
  authority: WorkflowCommandAuthority | WorkflowResumeCommandAuthority,
  operation: WorkflowCommandOperation,
): Promise<MaterializedWorkflowCommand> {
  assertOpaqueAuthority(authority, operation);
  const path = `/api/jobs/internal/workflow-commands/${encodeURIComponent(
    authority.requestId,
  )}/materialize`;
  const response = await workerRequest<unknown>(path, {
    schema_version: 2,
    workflow_id: authority.workflowId,
    payload_digest: authority.payloadDigest,
    operation,
    ...(operation === "resume"
      ? { intervention_id: (authority as WorkflowResumeCommandAuthority).interventionId }
      : {}),
  });
  try {
    return parseMaterializedWorkflowCommand(response, authority, operation);
  } catch {
    throwClosedActivityFailure("identity_conflict");
  }
}

async function materializeManagedWorkflowCommand(
  authority: WorkflowCommandAuthority | WorkflowResumeCommandAuthority,
  operation: WorkflowCommandOperation,
  managedCloudRelease: ManagedCloudReleaseMemoAuthority,
): Promise<MaterializedWorkflowCommand> {
  assertOpaqueAuthority(authority, operation);
  const path = `/api/jobs/internal/workflow-commands/${encodeURIComponent(
    authority.requestId,
  )}/materialize`;
  const response = await workerRequest<unknown>(path, {
    schema_version: 2,
    workflow_id: authority.workflowId,
    payload_digest: authority.payloadDigest,
    operation,
    ...(operation === "resume"
      ? { intervention_id: (authority as WorkflowResumeCommandAuthority).interventionId }
      : {}),
    managed_cloud_release: managedCloudRelease,
  });
  try {
    return parseManagedMaterializedWorkflowCommand(
      response,
      authority,
      operation,
      managedCloudRelease,
    );
  } catch {
    throwClosedActivityFailure("identity_conflict");
  }
}

function parseMaterializedWorkflowCommand(
  value: unknown,
  authority: WorkflowCommandAuthority | WorkflowResumeCommandAuthority,
  operation: WorkflowCommandOperation,
): MaterializedWorkflowCommand {
  const record = objectRecord(value, "Jobs API returned invalid workflow command material");
  const expectedKeys = operation === "start"
    ? [
      "browser_session_id",
      "operation",
      "payload_digest",
      "request_id",
      "result_request_id",
      "schema_version",
      "workflow_id",
      "workflow_input",
    ]
    : [
      "browser_session_id",
      "intervention_id",
      "operation",
      "payload_digest",
      "request_id",
      "resolution",
      "result_request_id",
      "schema_version",
      "workflow_id",
      "workflow_input",
    ];
  if (!sameKeys(record, expectedKeys)
    || record.schema_version !== 2
    || record.operation !== operation
    || record.request_id !== authority.requestId
    || record.workflow_id !== authority.workflowId
    || record.payload_digest !== authority.payloadDigest
    || record.result_request_id !== authority.requestId
    || !privateIdentifier(record.browser_session_id, 200)) {
    throw new Error("Jobs API returned mismatched workflow command material");
  }
  const workflowInput = parsePrivateWorkflowInput(record.workflow_input);
  if (record.browser_session_id !== `${workflowInput.runner}-${workflowInput.applicationId}`) {
    throw new Error("Jobs API returned mismatched browser authority");
  }
  if (operation === "resume") {
    const resume = authority as WorkflowResumeCommandAuthority;
    if (record.intervention_id !== resume.interventionId) {
      throw new Error("Jobs API returned mismatched intervention authority");
    }
    return {
      authority: workflowAuthority(authority),
      operation,
      interventionId: resume.interventionId,
      workflowInput,
      browserSessionId: record.browser_session_id,
      resultRequestId: record.result_request_id,
      resolution: parseResolution(record.resolution),
    };
  }
  return {
    authority: workflowAuthority(authority),
    operation,
    workflowInput,
    browserSessionId: record.browser_session_id,
    resultRequestId: record.result_request_id,
  };
}

function parseManagedMaterializedWorkflowCommand(
  value: unknown,
  authority: WorkflowCommandAuthority | WorkflowResumeCommandAuthority,
  operation: WorkflowCommandOperation,
  expectedManagedCloudRelease: ManagedCloudReleaseMemoAuthority,
): MaterializedWorkflowCommand {
  const record = objectRecord(value, "Jobs API returned invalid managed workflow command material");
  const expectedKeys = operation === "start"
    ? [
      "browser_session_id",
      "managed_cloud_release",
      "operation",
      "payload_digest",
      "request_id",
      "result_request_id",
      "schema_version",
      "workflow_id",
      "workflow_input",
    ]
    : [
      "browser_session_id",
      "intervention_id",
      "managed_cloud_release",
      "operation",
      "payload_digest",
      "request_id",
      "resolution",
      "result_request_id",
      "schema_version",
      "workflow_id",
      "workflow_input",
    ];
  if (!sameKeys(record, expectedKeys)
    || !sameManagedCloudReleaseMemo(
      record.managed_cloud_release,
      expectedManagedCloudRelease,
    )) {
    throw new Error("Jobs API returned mismatched managed-cloud release authority");
  }
  const { managed_cloud_release: _, ...legacyResponse } = record;
  return parseMaterializedWorkflowCommand(legacyResponse, authority, operation);
}

function parsePrivateWorkflowInput(value: unknown): ApplicationWorkflowInput {
  const input = objectRecord(value, "Jobs API returned invalid private workflow input");
  if (!sameKeys(input, [
    "accountId",
    "applicationId",
    "applicationIdentityId",
    "browserProfileId",
    "canonicalJobKey",
    "idempotencyKey",
    "job",
    "jobId",
    "packet",
    "packetId",
    "runner",
    "url",
  ])) {
    throw new Error("Jobs API returned invalid private workflow input");
  }
  for (const key of [
    "accountId",
    "applicationId",
    "applicationIdentityId",
    "browserProfileId",
    "canonicalJobKey",
    "idempotencyKey",
    "jobId",
    "packetId",
  ] as const) {
    if (!privateIdentifier(input[key], 200)) {
      throw new Error("Jobs API returned invalid private workflow input");
    }
  }
  if (input.runner !== "cloud") throw new Error("Jobs API returned invalid private runner");
  const url = requiredHttpsUrl(input.url, "Jobs API returned invalid private workflow URL");
  const packet = objectRecord(input.packet, "Jobs API returned invalid private workflow packet");
  const job = objectRecord(input.job, "Jobs API returned invalid private workflow job");
  const parsed = {
    accountId: input.accountId,
    applicationId: input.applicationId,
    jobId: input.jobId,
    canonicalJobKey: input.canonicalJobKey,
    packetId: input.packetId,
    applicationIdentityId: input.applicationIdentityId,
    browserProfileId: input.browserProfileId,
    packet,
    job,
    runner: "cloud" as const,
    url,
    idempotencyKey: input.idempotencyKey,
  } as unknown as ApplicationWorkflowInput;
  assertApprovedExecutionChecksum(parsed.packet, parsed.job);
  if (parsed.packet.applicationId !== parsed.applicationId
    || parsed.packet.jobId !== parsed.jobId
    || parsed.packet.resumeVersionId !== parsed.packetId
    || parsed.packet.applicationIdentityId !== parsed.applicationIdentityId
    || parsed.packet.browserProfileId !== parsed.browserProfileId) {
    throw new Error("Jobs API returned inconsistent private workflow input");
  }
  return parsed;
}

async function finishOpaqueCommand(
  materialized: MaterializedWorkflowCommand,
  execution: RunnerExecutionResult,
): Promise<WorkflowCommandStep> {
  const { workflowInput, browserSessionId, resultRequestId } = materialized;
  const receipt = execution.receipt;
  if (receipt.status === "needs_input") {
    const interventionId = await prepareApplicationIntervention(materialized, receipt);
    return { state: "intervention_prepared", interventionId };
  }
  if (receipt.status === "submitted") {
    await persistSubmissionReceipt({
      ...workflowInput,
      browserSessionId,
      resultRequestId,
    });
    await releaseBrowserAfterCanonicalCommit(browserSessionId);
    return { state: "submitted" };
  }
  return { state: "failed" };
}

async function prepareApplicationIntervention(
  materialized: MaterializedWorkflowCommand,
  receipt: SubmissionReceipt,
): Promise<string> {
  const command = materializedCommandAuthority(materialized);
  const path = workflowCommandPath(command.requestId, "/intervention/prepare");
  const response = await workerRequest<unknown>(path, {
    ...commandAuthorityBody(command),
    receipt,
  });
  try {
    return parseWorkflowInterventionMutation(response, command, undefined, "prepare");
  } catch {
    throwClosedActivityFailure("identity_conflict");
  }
}

async function finalizeMaterializedCommand(
  materialized: MaterializedWorkflowCommand,
  terminalState: WorkflowTerminalState,
  reasonCode: WorkflowTerminalReasonCode,
  openInterventionId?: string,
): Promise<void> {
  const command = materializedCommandAuthority(materialized);
  assertTerminalStateAndReason(terminalState, reasonCode);
  if (openInterventionId !== undefined && !opaqueIdentifier(openInterventionId, 128)) {
    throwClosedActivityFailure("invalid_authority");
  }
  const path = workflowCommandPath(command.requestId, "/finalize");
  const response = await workerRequest<unknown>(path, {
    ...commandAuthorityBody(command),
    terminal_state: terminalState,
    reason_code: reasonCode,
    ...(openInterventionId === undefined ? {} : { open_intervention_id: openInterventionId }),
  });
  try {
    parseWorkflowFinalization(
      response,
      command,
      terminalState,
      reasonCode,
      openInterventionId,
    );
  } catch {
    throwClosedActivityFailure("identity_conflict");
  }
  await releaseBrowserAfterCanonicalCommit(materialized.browserSessionId);
}

function parseWorkflowInterventionMutation(
  value: unknown,
  command: WorkflowCommandAuthority | WorkflowResumeCommandAuthority,
  expectedInterventionId: string | undefined,
  mutation: "prepare" | "publish",
): string {
  const response = objectRecord(value, "Jobs API returned invalid intervention authority");
  const operation = commandOperation(command);
  const expectedKeys = [
    ...(operation === "resume" ? ["command_intervention_id"] : []),
    "intervention_id",
    "operation",
    "payload_digest",
    "replayed",
    "request_id",
    "schema_version",
    "workflow_id",
  ];
  if (!sameKeys(response, expectedKeys)
    || response.schema_version !== 2
    || response.request_id !== command.requestId
    || response.workflow_id !== command.workflowId
    || response.payload_digest !== command.payloadDigest
    || response.operation !== operation
    || typeof response.replayed !== "boolean"
    || !opaqueIdentifier(response.intervention_id, 128)
    || (operation === "resume"
      && response.command_intervention_id
        !== (command as WorkflowResumeCommandAuthority).interventionId)
    || (expectedInterventionId !== undefined
      && response.intervention_id !== expectedInterventionId)) {
    throw new Error(`Jobs API returned mismatched intervention ${mutation}`);
  }
  return response.intervention_id;
}

function parseWorkflowFinalization(
  value: unknown,
  command: WorkflowCommandAuthority | WorkflowResumeCommandAuthority,
  terminalState: WorkflowTerminalState,
  reasonCode: WorkflowTerminalReasonCode,
  openInterventionId?: string,
): void {
  const response = objectRecord(value, "Jobs API returned invalid workflow finalization");
  const operation = commandOperation(command);
  const expectedKeys = [
    ...(operation === "resume" ? ["command_intervention_id"] : []),
    ...(openInterventionId === undefined ? [] : ["open_intervention_id"]),
    "operation",
    "payload_digest",
    "reason_code",
    "replayed",
    "request_id",
    "schema_version",
    "terminal_state",
    "workflow_id",
  ];
  if (!sameKeys(response, expectedKeys)
    || response.schema_version !== 2
    || response.request_id !== command.requestId
    || response.workflow_id !== command.workflowId
    || response.payload_digest !== command.payloadDigest
    || response.operation !== operation
    || response.terminal_state !== terminalState
    || response.reason_code !== reasonCode
    || typeof response.replayed !== "boolean"
    || (operation === "resume"
      && response.command_intervention_id
        !== (command as WorkflowResumeCommandAuthority).interventionId)
    || (openInterventionId !== undefined
      && response.open_intervention_id !== openInterventionId)) {
    throw new Error("Jobs API returned mismatched workflow finalization");
  }
}

function assertTerminalCommand(input: WorkflowTerminalCommand): void {
  const expectedKeys = [
    "command",
    ...(input.openInterventionId === undefined ? [] : ["openInterventionId"]),
    "reasonCode",
    "terminalState",
  ];
  if (!sameKeys(input as unknown as Record<string, unknown>, expectedKeys)) {
    throwClosedActivityFailure("invalid_authority");
  }
  const operation = commandOperation(input.command);
  assertOpaqueAuthority(input.command, operation);
  assertTerminalStateAndReason(input.terminalState, input.reasonCode);
  if (input.openInterventionId !== undefined
    && !opaqueIdentifier(input.openInterventionId, 128)) {
    throwClosedActivityFailure("invalid_authority");
  }
}

function assertTerminalStateAndReason(
  terminalState: WorkflowTerminalState,
  reasonCode: WorkflowTerminalReasonCode,
): void {
  if ((terminalState === "failed"
      && (reasonCode === "runner_failed"
        || reasonCode === "intervention_timeout"
        || reasonCode === "intervention_limit"))
    || (terminalState === "side_effect_unknown" && reasonCode === "runner_ambiguous")) {
    return;
  }
  throwClosedActivityFailure("invalid_authority");
}

function materializedCommandAuthority(
  materialized: MaterializedWorkflowCommand,
): WorkflowCommandAuthority | WorkflowResumeCommandAuthority {
  if (materialized.operation === "resume") {
    if (!materialized.interventionId) {
      throwClosedActivityFailure("invalid_authority");
    }
    return { ...materialized.authority, interventionId: materialized.interventionId };
  }
  return materialized.authority;
}

function commandOperation(
  command: WorkflowCommandAuthority | WorkflowResumeCommandAuthority,
): WorkflowCommandOperation {
  return Object.prototype.hasOwnProperty.call(command, "interventionId") ? "resume" : "start";
}

function commandAuthorityBody(
  command: WorkflowCommandAuthority | WorkflowResumeCommandAuthority,
): Record<string, unknown> {
  const operation = commandOperation(command);
  return {
    schema_version: 2,
    workflow_id: command.workflowId,
    payload_digest: command.payloadDigest,
    operation,
    ...(operation === "resume"
      ? { intervention_id: (command as WorkflowResumeCommandAuthority).interventionId }
      : {}),
  };
}

function workflowCommandPath(requestId: string, suffix: string): string {
  return `/api/jobs/internal/workflow-commands/${encodeURIComponent(requestId)}${suffix}`;
}

function parseManagedWorkflowCommandInput(
  value: ManagedWorkflowCommandInput,
): ManagedWorkflowCommandInput {
  const record = managedActivityInputRecord(value);
  if (!sameKeys(record, ["command", "managedCloudRelease"])) {
    throwClosedActivityFailure("invalid_authority");
  }
  const command = record.command as WorkflowCommandAuthority;
  try {
    assertOpaqueAuthority(command, "start");
  } catch {
    throwClosedActivityFailure("invalid_authority");
  }
  return {
    command,
    managedCloudRelease: parseManagedCloudReleaseAuthority(record.managedCloudRelease),
  };
}

function parseManagedWorkflowResumeCommandInput(
  value: ManagedWorkflowResumeCommandInput,
): ManagedWorkflowResumeCommandInput {
  const record = managedActivityInputRecord(value);
  if (!sameKeys(record, ["command", "managedCloudRelease", "workflow"])) {
    throwClosedActivityFailure("invalid_authority");
  }
  const workflow = record.workflow as WorkflowCommandAuthority;
  const command = record.command as WorkflowResumeCommandAuthority;
  try {
    assertOpaqueAuthority(workflow, "start");
    assertOpaqueAuthority(command, "resume");
  } catch {
    throwClosedActivityFailure("invalid_authority");
  }
  return {
    workflow,
    command,
    managedCloudRelease: parseManagedCloudReleaseAuthority(record.managedCloudRelease),
  };
}

function managedActivityInputRecord(value: unknown): Record<string, unknown> {
  try {
    return objectRecord(value, "Invalid managed workflow activity input");
  } catch {
    throwClosedActivityFailure("invalid_authority");
  }
}

function parseManagedCloudReleaseAuthority(
  value: unknown,
): ManagedCloudReleaseMemoAuthority {
  try {
    return parseManagedCloudReleaseMemo(value);
  } catch {
    throwClosedActivityFailure("invalid_authority");
  }
}

function sameManagedCloudReleaseMemo(
  value: unknown,
  expected: ManagedCloudReleaseMemoAuthority,
): boolean {
  try {
    const actualBytes = managedCloudReleaseMemoBytes(value);
    const expectedBytes = managedCloudReleaseMemoBytes(expected);
    return actualBytes.length === expectedBytes.length
      && actualBytes.every((byte, index) => byte === expectedBytes[index]);
  } catch {
    return false;
  }
}

function assertResumeMatchesWorkflow(
  workflow: WorkflowCommandAuthority,
  resume: WorkflowResumeCommandAuthority,
): void {
  assertOpaqueAuthority(workflow, "start");
  assertOpaqueAuthority(resume, "resume");
  if (workflow.workflowId !== resume.workflowId) {
    throwClosedActivityFailure("identity_conflict");
  }
}

function assertOpaqueAuthority(
  authority: WorkflowCommandAuthority | WorkflowResumeCommandAuthority,
  operation: WorkflowCommandOperation,
): void {
  const record = authority as unknown as Record<string, unknown>;
  const keys = operation === "start"
    ? ["payloadDigest", "requestId", "schemaVersion", "workflowId"]
    : ["interventionId", "payloadDigest", "requestId", "schemaVersion", "workflowId"];
  if (!sameKeys(record, keys)
    || authority.schemaVersion !== 2
    || !opaqueIdentifier(authority.requestId, 128)
    || !opaqueIdentifier(authority.workflowId, 192)
    || !/^[a-f0-9]{64}$/.test(authority.payloadDigest)
    || (operation === "resume"
      && !opaqueIdentifier((authority as WorkflowResumeCommandAuthority).interventionId, 128))) {
    throwClosedActivityFailure("invalid_authority");
  }
}

function workflowAuthority(
  authority: WorkflowCommandAuthority | WorkflowResumeCommandAuthority,
): WorkflowCommandAuthority {
  return {
    schemaVersion: 2,
    requestId: authority.requestId,
    workflowId: authority.workflowId,
    payloadDigest: authority.payloadDigest,
  };
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
    signal: AbortSignal.timeout(API_REQUEST_TIMEOUT_MS),
  });
  if (response.ok || response.status === 404) return;
  if (response.status === 401 || response.status === 403) {
    // Credential drift is repairable and must not silently strand an active
    // browser after canonical state commits. Keep the idempotent cleanup
    // activity retrying until operator configuration is restored.
    throw new Error("Bluey Jobs runner transient failure");
  }
  if (isPermanentRunnerStatus(response.status)) {
    throwClosedActivityFailure("runner_release_rejected");
  }
  throw new Error("Bluey Jobs runner transient failure");
}

async function releaseBrowserAfterCanonicalCommit(browserSessionId: string): Promise<void> {
  try {
    await releaseBrowser(browserSessionId);
  } catch (error) {
    if (!isNonRetryableApplicationFailure(error)) throw error;
    // Canonical submitted/terminal state is already committed. A permanent
    // cleanup rejection cannot downgrade it or fail the deterministic workflow;
    // transient/ambiguous cleanup still retries the idempotent activity.
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
    signal: AbortSignal.timeout(API_REQUEST_TIMEOUT_MS),
  });
  // An intermediary-generated 5xx/timeout response may not carry the private
  // service headers. Keep that class retryable; only trust and classify a
  // permanent response after the exact authenticated-response headers exist.
  if (!response.ok && !isPermanentHttpStatus(response.status)) {
    throw new Error("Jobs API transient failure");
  }
  const workflowCommandPath = isWorkflowCommandPath(path);
  if (workflowCommandPath && !hasExactPrivateJsonHeaders(response.headers)) {
    // Missing private-service headers can be an intermediary response or a
    // corrupted success after an idempotent mutation committed. Replay the
    // exact command instead of terminalizing an accepted workflow.
    throw new Error("Jobs API ambiguous response");
  }
  if (!response.ok) {
    if (workflowCommandPath) {
      const failureType = await exactWorkflowCommandFailure(response);
      if (failureType) throwClosedActivityFailure(failureType);
      // A proxy or rollout mismatch can produce a permanent-looking status
      // after an idempotent mutation committed. Only the server's exact closed
      // error contract may stop retries of accepted command authority.
      throw new Error("Jobs API ambiguous response");
    }
    if (response.status === 400) throwClosedActivityFailure("invalid_request");
    if (response.status === 404) throwClosedActivityFailure("not_found");
    if (response.status === 409) throwClosedActivityFailure("identity_conflict");
    if (isPermanentHttpStatus(response.status)) {
      throwClosedActivityFailure("api_request_rejected");
    }
    throw new Error("Jobs API transient failure");
  }
  if (response.status === 204) return undefined as T;
  try {
    return await response.json() as T;
  } catch {
    if (workflowCommandPath) throw new Error("Jobs API ambiguous response");
    throwClosedActivityFailure("invalid_response");
  }
}

async function assertRunnerResponse(
  response: Response,
  requestId: string,
): Promise<void> {
  if (response.ok) return;
  if (await isExactRunnerAmbiguityResponse(response, requestId)) {
    throw new RunnerSideEffectUnknown();
  }
  // A status-only runner rejection does not prove that an irreversible submit
  // was never attempted. Keep it retryable until either a canonical durable
  // result or the exact request-bound ambiguity receipt can be recovered.
  throw new Error("Bluey Jobs runner transient failure");
}

async function isExactRunnerAmbiguityResponse(
  response: Response,
  requestId: string,
): Promise<boolean> {
  if (response.status !== 500
    || response.headers.get("content-type") !== "application/json"
    || response.headers.get("cache-control") !== "no-store"
    || response.headers.get("x-content-type-options") !== "nosniff") {
    return false;
  }
  try {
    const value = await response.json() as unknown;
    const record = objectRecord(value, "Invalid runner ambiguity response");
    return sameKeys(record, ["outcome", "requestId", "schemaVersion"])
      && record.schemaVersion === 2
      && record.outcome === "side_effect_unknown"
      && record.requestId === requestId;
  } catch {
    return false;
  }
}

class RunnerSideEffectUnknown extends Error {
  constructor() {
    super("opaque_runner_ambiguity");
    this.name = "RunnerSideEffectUnknown";
  }
}

function throwClosedActivityFailure(type: string): never {
  throw ApplicationFailure.nonRetryable("opaque_failure", type);
}

function isNonRetryableApplicationFailure(error: unknown): boolean {
  return error instanceof ApplicationFailure && error.nonRetryable === true;
}

function isWorkflowCommandPath(path: string): boolean {
  return path.startsWith("/api/jobs/internal/workflow-commands/");
}

function hasExactPrivateJsonHeaders(headers: Headers): boolean {
  return headers.get("content-type") === "application/json"
    && headers.get("cache-control") === "no-store"
    && headers.get("x-content-type-options") === "nosniff";
}

async function exactWorkflowCommandFailure(response: Response): Promise<string | undefined> {
  let record: Record<string, unknown>;
  try {
    record = objectRecord(await response.json(), "Invalid workflow command error");
  } catch {
    return undefined;
  }
  if (!sameKeys(record, ["outcome", "reason", "schema_version"])
    || record.schema_version !== 2) {
    return undefined;
  }
  if ((response.status === 400 || response.status === 413)
    && record.outcome === "rejected"
    && record.reason === "invalid_request") {
    return "invalid_request";
  }
  if (response.status === 404
    && record.outcome === "rejected"
    && record.reason === "not_found") {
    return "not_found";
  }
  if (response.status === 409
    && record.outcome === "identity_conflict"
    && record.reason === "identity_conflict") {
    return "identity_conflict";
  }
  return undefined;
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
    signal: AbortSignal.timeout(RUNNER_LOOKUP_TIMEOUT_MS),
  });
  if (await isExactRunnerResultAbsentResponse(response)) return undefined;
  if (!response.ok) {
    if (await isExactRunnerAmbiguityResponse(response, requestId)) {
      throw new RunnerSideEffectUnknown();
    }
    // Generic lookup failures, including 4xx responses, carry no durable
    // side-effect authority and therefore cannot terminalize the workflow.
    throw new Error("Bluey Jobs runner transient failure");
  }
  return readRunnerExecutionResult(response, { ...input, requestId });
}

async function readRunnerExecutionResult(
  response: Response,
  binding: RunnerResultBinding,
): Promise<RawRunnerExecutionResult> {
  try {
    if (response.status !== 200 || !hasExactPrivateJsonHeaders(response.headers)) {
      throw new Error("Runner result response is not authoritative");
    }
    const result = parseRunnerExecutionResult(await response.json(), binding);
    if (result.receipt.status === "submitted") submittedExecutionEvidence(result);
    return result;
  } catch {
    // A malformed success may be a rollout/proxy mismatch after the runner
    // committed work. Retrying the exact request is safer than inventing a
    // terminal failure that could mask an employer-side submit.
    throw new Error("Bluey Jobs runner returned an invalid response");
  }
}

async function isExactRunnerResultAbsentResponse(response: Response): Promise<boolean> {
  if (response.status !== 404 || !hasExactPrivateJsonHeaders(response.headers)) {
    return false;
  }
  try {
    const value = objectRecord(
      await response.json(),
      "Invalid runner result absence response",
    );
    return sameKeys(value, ["error"])
      && value.error === "Durable result not found";
  } catch {
    return false;
  }
}

function isPermanentRunnerStatus(status: number): boolean {
  return isPermanentHttpStatus(status);
}

function isPermanentHttpStatus(status: number): boolean {
  return status >= 400 && status < 500 && status !== 408 && status !== 429;
}

function parseRunnerExecutionResult(
  value: unknown,
  binding: RunnerResultBinding,
): RawRunnerExecutionResult {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("Bluey Jobs runner returned an invalid receipt");
  }
  const result = value as Record<string, unknown>;
  const optionalKeys = [
    "evidenceObjects",
    "receiptAuthority",
    "receiptBundle",
    "receiptPath",
  ].filter((key) => Object.prototype.hasOwnProperty.call(result, key));
  if (!sameKeys(result, [
    "accountId",
    "applicationId",
    "applicationIdentityId",
    "browserSessionId",
    "receipt",
    "requestId",
    "runId",
    ...optionalKeys,
  ].sort())) {
    throw new Error("Bluey Jobs runner returned an invalid result shape");
  }
  for (const key of [
    "accountId",
    "applicationId",
    "applicationIdentityId",
    "browserSessionId",
  ] as const) {
    if (result[key] !== binding[key]) {
      throw new Error("Bluey Jobs runner returned mismatched result authority");
    }
  }
  if (result.runId !== binding.idempotencyKey) {
    throw new Error("Bluey Jobs runner returned mismatched result authority");
  }
  if (result.requestId !== binding.requestId) {
    throw new Error("Bluey Jobs runner returned mismatched result authority");
  }
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

type RunnerResultBinding = Pick<
  ApplicationWorkflowInput,
  "accountId" | "applicationId" | "applicationIdentityId" | "idempotencyKey"
> & { browserSessionId: string; requestId: string };

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

function sameKeys(record: Record<string, unknown>, expected: readonly string[]): boolean {
  const keys = Object.keys(record).sort();
  return keys.length === expected.length && keys.every((key, index) => key === expected[index]);
}

function opaqueIdentifier(value: unknown, maximumLength: number): value is string {
  return typeof value === "string"
    && value.length >= 20
    && value.length <= maximumLength
    && /^[A-Za-z0-9_-]+$/.test(value);
}

function privateIdentifier(value: unknown, maximumLength: number): value is string {
  return typeof value === "string"
    && value.length >= 1
    && value.length <= maximumLength
    && /^[A-Za-z0-9._:-]+$/.test(value);
}

function requiredHttpsUrl(value: unknown, message: string): string {
  if (typeof value !== "string" || value.length > 4_096) throw new Error(message);
  try {
    const url = new URL(value);
    if (url.protocol !== "https:" || url.username || url.password) throw new Error(message);
    return url.toString();
  } catch {
    throw new Error(message);
  }
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

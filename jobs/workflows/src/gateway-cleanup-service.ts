import { createHash } from "node:crypto";
import {
  WorkflowNotFoundError,
  isGrpcServiceError,
  type WorkflowClient,
  type WorkflowExecutionDescription,
} from "@temporalio/client";
import type {
  WorkflowCleanupAuthority,
  WorkflowCleanupPendingReason,
  WorkflowCleanupReceipt,
  WorkflowGatewayError,
} from "./contracts.js";
import {
  WORKFLOW_PROTOCOL_MEMO_KEY,
  WORKFLOW_TYPE_V2,
} from "./gateway-service.js";

export const WORKFLOW_CLEANUP_PATH = "/workflow-cleanup";

const DEFAULT_CONFIRMATION_AGE_MS = 30_000;
const DEFAULT_CACHE_LIMIT = 1_024;
const DEFAULT_PAGE_SIZE = 100;
const DEFAULT_MAX_PAGES = 64;
const DEFAULT_MAX_EXECUTIONS = 4_096;
const MAX_PAGE_TOKEN_BYTES = 4_096;
const GRPC_NOT_FOUND = 5;
const EVIDENCE_DOMAIN = "bluey-jobs-workflow-cleanup-evidence-v2\0";

type CleanupProofState = "found" | "not_found" | "not_checked" | "unavailable";

interface CleanupProof {
  describe: CleanupProofState;
  history: CleanupProofState;
  visibility: CleanupProofState;
}

export interface WorkflowCleanupServiceResult {
  status: number;
  body: WorkflowCleanupReceipt | WorkflowGatewayError;
}

export interface TemporalCleanupVisibilityExecution {
  workflowId: unknown;
  runId: unknown;
}

export interface TemporalCleanupVisibilityPage {
  executions: readonly TemporalCleanupVisibilityExecution[];
  nextPageToken?: Uint8Array;
}

export interface TemporalCleanupClient {
  withDeadline<T>(deadline: number | Date, operation: () => Promise<T>): Promise<T>;
  describe(workflowId: string, runId?: string): Promise<WorkflowExecutionDescription>;
  terminate(workflowId: string, runId: string, firstExecutionRunId: string): Promise<void>;
  delete(workflowId: string, runId: string): Promise<void>;
  historyProbe(workflowId: string, runId?: string): Promise<void>;
  listPage(
    workflowId: string,
    nextPageToken: Uint8Array,
    pageSize: number,
  ): Promise<TemporalCleanupVisibilityPage>;
}

export interface WorkflowCleanupServiceOptions {
  client: TemporalCleanupClient;
  rpcTimeoutMs?: number;
  visibilityConfirmationAgeMs?: number;
  authorityCacheLimit?: number;
  visibilityPageSize?: number;
  visibilityMaxPages?: number;
  visibilityMaxExecutions?: number;
  now?: () => number;
}

interface CleanupObservation {
  baseAuthority: string;
  initialFirstExecutionRunId?: string;
  boundFirstExecutionRunId?: string;
  observedRunIds: Set<string>;
  validatedRunIds: Set<string>;
  deletionConfirmedRunIds: Set<string>;
  unvalidatedRunIds: Set<string>;
  sawValidatedExecution: boolean;
  absenceFirstObservedAtMs?: number;
  lastAccess: number;
  inFlight?: Promise<WorkflowCleanupServiceResult>;
}

interface LoadedCleanupOptions {
  client: TemporalCleanupClient;
  rpcTimeoutMs: number;
  visibilityConfirmationAgeMs: number;
  authorityCacheLimit: number;
  visibilityPageSize: number;
  visibilityMaxPages: number;
  visibilityMaxExecutions: number;
  now: () => number;
}

interface ValidatedExecution {
  runId: string;
  firstExecutionRunId: string;
  running: boolean;
}

type TemporalLookup<T> =
  | { state: "found"; value: T }
  | { state: "not_found" }
  | { state: "unavailable" };

type VisibilityLookup =
  | { state: "found"; runIds: Set<string> }
  | { state: "unavailable" };

export function createTemporalCleanupClient(client: WorkflowClient): TemporalCleanupClient {
  return {
    withDeadline: (deadline, operation) => client.withDeadline(deadline, operation),
    describe: async (workflowId, runId) => client.getHandle(workflowId, runId).describe(),
    terminate: async (workflowId, runId, firstExecutionRunId) => {
      await client.getHandle(workflowId, runId, { firstExecutionRunId })
        .terminate("bluey_jobs_cleanup_v2");
    },
    delete: async (workflowId, runId) => {
      await client.workflowService.deleteWorkflowExecution({
        namespace: client.options.namespace,
        workflowExecution: { workflowId, runId },
      });
    },
    historyProbe: async (workflowId, runId) => {
      await client.workflowService.getWorkflowExecutionHistory({
        namespace: client.options.namespace,
        execution: { workflowId, ...(runId ? { runId } : {}) },
        maximumPageSize: 1,
        waitNewEvent: false,
        skipArchival: false,
      });
    },
    listPage: async (workflowId, nextPageToken, pageSize) => {
      const response = await client.workflowService.listWorkflowExecutions({
        namespace: client.options.namespace,
        query: `WorkflowId = "${workflowId}"`,
        pageSize,
        nextPageToken,
      });
      return {
        executions: (response.executions ?? []).map((execution) => ({
          workflowId: execution.execution?.workflowId,
          runId: execution.execution?.runId,
        })),
        ...(response.nextPageToken && response.nextPageToken.length > 0
          ? { nextPageToken: response.nextPageToken }
          : {}),
      };
    },
  };
}

export function createWorkflowCleanupService(input: WorkflowCleanupServiceOptions) {
  const options = loadOptions(input);
  const observations = new Map<string, CleanupObservation>();
  let accessSequence = 0;

  return {
    async executeCleanup(value: unknown): Promise<WorkflowCleanupServiceResult> {
      let authority: WorkflowCleanupAuthority;
      try {
        authority = parseWorkflowCleanupAuthority(value);
      } catch {
        return cleanupError(400, "rejected", "invalid_request");
      }

      const binding = bindObservation(
        observations,
        authority,
        options.authorityCacheLimit,
        accessSequence += 1,
      );
      if (binding.state === "conflict") {
        return cleanupError(409, "identity_conflict", "identity_conflict");
      }
      if (binding.state === "unavailable") {
        return cleanupReceipt(authority, authority.firstExecutionRunId, "pending", {
          reason: "temporal_unavailable",
          proof: unavailableProof(),
        });
      }
      const observation = binding.observation;
      if (observation.inFlight) return observation.inFlight;

      const operation = reconcileCleanup(authority, observation, options)
        .catch(() => cleanupReceipt(
          authority,
          observation.boundFirstExecutionRunId,
          "pending",
          { reason: "temporal_unavailable", proof: unavailableProof() },
        ))
        .finally(() => {
          if (observation.inFlight === operation) observation.inFlight = undefined;
        });
      observation.inFlight = operation;
      return operation;
    },
  };
}

export function parseWorkflowCleanupAuthority(value: unknown): WorkflowCleanupAuthority {
  const record = exactRecord(value);
  const hasFirstExecutionRunId = Object.prototype.hasOwnProperty.call(
    record,
    "firstExecutionRunId",
  );
  const expectedKeys = [
    "cleanupFence",
    "cleanupRequestId",
    ...(hasFirstExecutionRunId ? ["firstExecutionRunId"] : []),
    "generation",
    "schemaVersion",
    "startPayloadDigest",
    "startRequestId",
    "targetSetDigest",
    "workflowId",
  ];
  if (!sameKeys(record, expectedKeys)
    || record.schemaVersion !== 2
    || !opaqueId(record.cleanupRequestId, 128)
    || !positiveSafeInteger(record.generation)
    || !digest(record.targetSetDigest)
    || !positiveSafeInteger(record.cleanupFence)
    || !opaqueId(record.workflowId, 192)
    || !opaqueId(record.startRequestId, 128)
    || !digest(record.startPayloadDigest)
    || (hasFirstExecutionRunId && !opaqueId(record.firstExecutionRunId, 128))) {
    throw new Error("Invalid workflow cleanup authority");
  }
  return {
    schemaVersion: 2,
    cleanupRequestId: record.cleanupRequestId,
    generation: record.generation,
    targetSetDigest: record.targetSetDigest,
    cleanupFence: record.cleanupFence,
    workflowId: record.workflowId,
    startRequestId: record.startRequestId,
    startPayloadDigest: record.startPayloadDigest,
    ...(hasFirstExecutionRunId
      ? { firstExecutionRunId: record.firstExecutionRunId as string }
      : {}),
  };
}

async function reconcileCleanup(
  authority: WorkflowCleanupAuthority,
  observation: CleanupObservation,
  options: LoadedCleanupOptions,
): Promise<WorkflowCleanupServiceResult> {
  const liveExecutions = new Map<string, ValidatedExecution>();
  const latest = await describeExecution(options, authority.workflowId);
  if (latest.state === "unavailable") {
    return pending(authority, observation, "temporal_unavailable", {
      describe: "unavailable",
      history: "not_checked",
      visibility: "not_checked",
    });
  }
  if (latest.state === "found") {
    const recorded = recordValidatedExecution(
      latest.value,
      authority,
      observation,
      liveExecutions,
    );
    if (!recorded) return identityConflict();
  }

  const visibility = await listWorkflowExecutions(options, authority.workflowId);
  if (visibility.state === "unavailable") {
    return pending(authority, observation, "temporal_unavailable", {
      describe: latest.state,
      history: "not_checked",
      visibility: "unavailable",
    });
  }
  if (visibility.runIds.size > 0) observation.absenceFirstObservedAtMs = undefined;
  for (const runId of visibility.runIds) observation.observedRunIds.add(runId);
  if (authority.firstExecutionRunId) {
    observation.observedRunIds.add(authority.firstExecutionRunId);
  }
  if (observation.boundFirstExecutionRunId) {
    observation.observedRunIds.add(observation.boundFirstExecutionRunId);
  }

  const candidates = [...observation.observedRunIds].sort();
  for (const runId of candidates) {
    if (liveExecutions.has(runId)) continue;
    const described = await describeExecution(options, authority.workflowId, runId);
    if (described.state === "unavailable") {
      return pending(authority, observation, "temporal_unavailable", {
        describe: latest.state,
        history: "not_checked",
        visibility: visibility.runIds.size > 0 ? "found" : "not_found",
      });
    }
    if (described.state === "found") {
      const recorded = recordValidatedExecution(
        described.value,
        authority,
        observation,
        liveExecutions,
        runId,
      );
      if (!recorded) return identityConflict();
      continue;
    }
    if (visibility.runIds.has(runId) && !observation.validatedRunIds.has(runId)) {
      observation.unvalidatedRunIds.add(runId);
    }
  }

  if ([...visibility.runIds].some((runId) => observation.unvalidatedRunIds.has(runId))) {
    return pending(authority, observation, "visibility_pending", {
      describe: latest.state,
      history: "not_checked",
      visibility: "found",
    });
  }

  for (const execution of [...liveExecutions.values()].sort(compareExecutions)) {
    if (!execution.running) continue;
    const terminated = await mutateExecution(options, () => options.client.terminate(
      authority.workflowId,
      execution.runId,
      execution.firstExecutionRunId,
    ));
    if (terminated === "unavailable") {
      return pending(authority, observation, "termination_pending", {
        describe: "found",
        history: "not_checked",
        visibility: visibility.runIds.size > 0 ? "found" : "not_found",
      });
    }
  }

  for (const execution of [...liveExecutions.values()].sort(compareExecutions)) {
    const deleted = await mutateExecution(
      options,
      () => options.client.delete(authority.workflowId, execution.runId),
    );
    if (deleted === "unavailable") {
      return pending(authority, observation, "history_delete_pending", {
        describe: "found",
        history: "not_checked",
        visibility: visibility.runIds.size > 0 ? "found" : "not_found",
      });
    }
    observation.deletionConfirmedRunIds.add(execution.runId);
  }

  return proveCleanupAbsence(authority, observation, options);
}

async function proveCleanupAbsence(
  authority: WorkflowCleanupAuthority,
  observation: CleanupObservation,
  options: LoadedCleanupOptions,
): Promise<WorkflowCleanupServiceResult> {
  const liveExecutions = new Map<string, ValidatedExecution>();
  const latest = await describeExecution(options, authority.workflowId);
  if (latest.state === "unavailable") {
    return pending(authority, observation, "temporal_unavailable", {
      describe: "unavailable",
      history: "not_checked",
      visibility: "not_checked",
    });
  }
  if (latest.state === "found") {
    const recorded = recordValidatedExecution(
      latest.value,
      authority,
      observation,
      liveExecutions,
    );
    if (!recorded) return identityConflict();
  }

  for (const runId of [...observation.observedRunIds].sort()) {
    const described = await describeExecution(options, authority.workflowId, runId);
    if (described.state === "unavailable") {
      return pending(authority, observation, "temporal_unavailable", {
        describe: "unavailable",
        history: "not_checked",
        visibility: "not_checked",
      });
    }
    if (described.state === "found") {
      const recorded = recordValidatedExecution(
        described.value,
        authority,
        observation,
        liveExecutions,
        runId,
      );
      if (!recorded) return identityConflict();
    }
  }

  let history: CleanupProofState = "not_found";
  const historyRunIds: Array<string | undefined> = observation.observedRunIds.size > 0
    ? [...observation.observedRunIds].sort()
    : [undefined];
  for (const runId of historyRunIds) {
    const probe = await historyProbe(options, authority.workflowId, runId);
    if (probe === "unavailable") {
      return pending(authority, observation, "temporal_unavailable", {
        describe: latest.state,
        history: "unavailable",
        visibility: "not_checked",
      });
    }
    if (probe === "found") history = "found";
  }

  const visibility = await listWorkflowExecutions(options, authority.workflowId);
  if (visibility.state === "unavailable") {
    return pending(authority, observation, "temporal_unavailable", {
      describe: latest.state,
      history,
      visibility: "unavailable",
    });
  }
  for (const runId of visibility.runIds) {
    observation.observedRunIds.add(runId);
    if (!observation.validatedRunIds.has(runId)) {
      observation.unvalidatedRunIds.add(runId);
    }
  }
  const visibilityState = visibility.runIds.size > 0 ? "found" : "not_found";
  if (liveExecutions.size > 0) {
    observation.absenceFirstObservedAtMs = undefined;
    const running = [...liveExecutions.values()].some((execution) => execution.running);
    return pending(
      authority,
      observation,
      running ? "termination_pending" : "history_delete_pending",
      { describe: "found", history, visibility: visibilityState },
    );
  }
  if (history === "found") {
    observation.absenceFirstObservedAtMs = undefined;
    return pending(authority, observation, "history_delete_pending", {
      describe: "not_found",
      history,
      visibility: visibilityState,
    });
  }
  if (visibilityState === "found") {
    observation.absenceFirstObservedAtMs = undefined;
    return pending(authority, observation, "visibility_pending", {
      describe: "not_found",
      history: "not_found",
      visibility: "found",
    });
  }

  const proof: CleanupProof = {
    describe: "not_found",
    history: "not_found",
    visibility: "not_found",
  };
  const deletionConfirmed = observation.sawValidatedExecution
    && [...observation.validatedRunIds].every(
      (runId) => observation.deletionConfirmedRunIds.has(runId),
    );
  if (observation.unvalidatedRunIds.size === 0 && deletionConfirmed) {
    return cleanupReceipt(
      authority,
      observation.boundFirstExecutionRunId,
      "complete",
      { reason: "absence_proved", proof },
    );
  }
  if (observation.unvalidatedRunIds.size > 0) {
    return pending(authority, observation, "history_delete_pending", proof);
  }

  const now = options.now();
  if (observation.absenceFirstObservedAtMs === undefined) {
    observation.absenceFirstObservedAtMs = now;
    return pending(authority, observation, "visibility_pending", proof);
  }
  if (now - observation.absenceFirstObservedAtMs < options.visibilityConfirmationAgeMs) {
    return pending(authority, observation, "visibility_pending", proof);
  }
  return cleanupReceipt(authority, observation.boundFirstExecutionRunId, "complete", {
    reason: "absence_proved",
    proof,
  });
}

function recordValidatedExecution(
  description: WorkflowExecutionDescription,
  authority: WorkflowCleanupAuthority,
  observation: CleanupObservation,
  liveExecutions: Map<string, ValidatedExecution>,
  expectedRunId?: string,
): boolean {
  const memo = description.memo;
  const firstExecutionRunId = description.raw.workflowExecutionInfo?.firstRunId;
  if (description.type !== WORKFLOW_TYPE_V2
    || description.workflowId !== authority.workflowId
    || !opaqueId(description.runId, 128)
    || (expectedRunId !== undefined && description.runId !== expectedRunId)
    || !opaqueId(firstExecutionRunId, 128)
    || !memo
    || !sameKeys(memo, [WORKFLOW_PROTOCOL_MEMO_KEY])
    || !exactStartMemo(memo[WORKFLOW_PROTOCOL_MEMO_KEY], authority)
    || !validWorkflowStatus(description.status.name)) {
    return false;
  }
  if (authority.firstExecutionRunId
    && authority.firstExecutionRunId !== firstExecutionRunId) {
    return false;
  }
  if (observation.boundFirstExecutionRunId
    && observation.boundFirstExecutionRunId !== firstExecutionRunId) {
    return false;
  }
  observation.boundFirstExecutionRunId = firstExecutionRunId;
  observation.observedRunIds.add(description.runId);
  observation.observedRunIds.add(firstExecutionRunId);
  observation.validatedRunIds.add(description.runId);
  observation.unvalidatedRunIds.delete(description.runId);
  observation.sawValidatedExecution = true;
  observation.absenceFirstObservedAtMs = undefined;
  liveExecutions.set(description.runId, {
    runId: description.runId,
    firstExecutionRunId,
    running: description.status.name === "RUNNING",
  });
  return true;
}

function exactStartMemo(value: unknown, authority: WorkflowCleanupAuthority): boolean {
  const memo = recordOrUndefined(value);
  return memo !== undefined
    && sameKeys(memo, ["payloadDigest", "requestId", "schemaVersion", "workflowId"])
    && memo.schemaVersion === 2
    && memo.requestId === authority.startRequestId
    && memo.workflowId === authority.workflowId
    && memo.payloadDigest === authority.startPayloadDigest;
}

async function describeExecution(
  options: LoadedCleanupOptions,
  workflowId: string,
  runId?: string,
): Promise<TemporalLookup<WorkflowExecutionDescription>> {
  try {
    return {
      state: "found",
      value: await temporalRpc(options, () => options.client.describe(workflowId, runId)),
    };
  } catch (error) {
    return temporalNotFound(error) ? { state: "not_found" } : { state: "unavailable" };
  }
}

async function historyProbe(
  options: LoadedCleanupOptions,
  workflowId: string,
  runId?: string,
): Promise<"found" | "not_found" | "unavailable"> {
  try {
    await temporalRpc(options, () => options.client.historyProbe(workflowId, runId));
    // Even an empty successful History response proves that a history still
    // exists. Only exact NotFound is erasure evidence.
    return "found";
  } catch (error) {
    return temporalNotFound(error) ? "not_found" : "unavailable";
  }
}

async function mutateExecution(
  options: LoadedCleanupOptions,
  operation: () => Promise<void>,
): Promise<"complete" | "not_found" | "unavailable"> {
  try {
    await temporalRpc(options, operation);
    return "complete";
  } catch (error) {
    return temporalNotFound(error) ? "not_found" : "unavailable";
  }
}

async function listWorkflowExecutions(
  options: LoadedCleanupOptions,
  workflowId: string,
): Promise<VisibilityLookup> {
  let token: Uint8Array = new Uint8Array();
  const seenTokens = new Set<string>();
  const runIds = new Set<string>();
  for (let page = 0; page < options.visibilityMaxPages; page += 1) {
    let response: TemporalCleanupVisibilityPage;
    try {
      response = await temporalRpc(
        options,
        () => options.client.listPage(workflowId, token, options.visibilityPageSize),
      );
    } catch {
      return { state: "unavailable" };
    }
    if (!Array.isArray(response.executions)) return { state: "unavailable" };
    for (const execution of response.executions) {
      if (execution.workflowId !== workflowId || !opaqueId(execution.runId, 128)) {
        return { state: "unavailable" };
      }
      runIds.add(execution.runId);
      if (runIds.size > options.visibilityMaxExecutions) {
        return { state: "unavailable" };
      }
    }
    const next = response.nextPageToken;
    if (next === undefined || next.length === 0) return { state: "found", runIds };
    if (!(next instanceof Uint8Array) || next.length > MAX_PAGE_TOKEN_BYTES) {
      return { state: "unavailable" };
    }
    const encoded = Buffer.from(next).toString("base64");
    if (seenTokens.has(encoded)) return { state: "unavailable" };
    seenTokens.add(encoded);
    token = next;
  }
  return { state: "unavailable" };
}

function bindObservation(
  observations: Map<string, CleanupObservation>,
  authority: WorkflowCleanupAuthority,
  cacheLimit: number,
  access: number,
):
  | { state: "bound"; observation: CleanupObservation }
  | { state: "conflict" }
  | { state: "unavailable" } {
  const baseAuthority = canonicalizeCleanupEvidence(cleanupBaseAuthority(authority));
  const existing = observations.get(authority.cleanupRequestId);
  if (existing) {
    if (existing.baseAuthority !== baseAuthority) return { state: "conflict" };
    if (existing.initialFirstExecutionRunId) {
      if (authority.firstExecutionRunId !== existing.initialFirstExecutionRunId) {
        return { state: "conflict" };
      }
    } else if (authority.firstExecutionRunId
      && authority.firstExecutionRunId !== existing.boundFirstExecutionRunId) {
      return { state: "conflict" };
    }
    existing.lastAccess = access;
    return { state: "bound", observation: existing };
  }

  if (observations.size >= cacheLimit) {
    const evictable = [...observations.entries()]
      .filter(([, observation]) => !observation.inFlight)
      .sort((left, right) => left[1].lastAccess - right[1].lastAccess)[0];
    if (!evictable) return { state: "unavailable" };
    observations.delete(evictable[0]);
  }
  const observation: CleanupObservation = {
    baseAuthority,
    initialFirstExecutionRunId: authority.firstExecutionRunId,
    boundFirstExecutionRunId: authority.firstExecutionRunId,
    observedRunIds: new Set(authority.firstExecutionRunId
      ? [authority.firstExecutionRunId]
      : []),
    validatedRunIds: new Set(),
    deletionConfirmedRunIds: new Set(),
    unvalidatedRunIds: new Set(),
    sawValidatedExecution: false,
    lastAccess: access,
  };
  observations.set(authority.cleanupRequestId, observation);
  return { state: "bound", observation };
}

function cleanupBaseAuthority(authority: WorkflowCleanupAuthority): Omit<
  WorkflowCleanupAuthority,
  "firstExecutionRunId"
> {
  return {
    schemaVersion: 2,
    cleanupRequestId: authority.cleanupRequestId,
    generation: authority.generation,
    targetSetDigest: authority.targetSetDigest,
    cleanupFence: authority.cleanupFence,
    workflowId: authority.workflowId,
    startRequestId: authority.startRequestId,
    startPayloadDigest: authority.startPayloadDigest,
  };
}

function pending(
  authority: WorkflowCleanupAuthority,
  observation: CleanupObservation,
  reason: WorkflowCleanupPendingReason,
  proof: CleanupProof,
): WorkflowCleanupServiceResult {
  return cleanupReceipt(
    authority,
    observation.boundFirstExecutionRunId,
    "pending",
    { reason, proof },
  );
}

function cleanupReceipt(
  authority: WorkflowCleanupAuthority,
  firstExecutionRunId: string | undefined,
  outcome: "complete" | "pending",
  input: {
    reason: "absence_proved" | WorkflowCleanupPendingReason;
    proof: CleanupProof;
  },
): WorkflowCleanupServiceResult {
  const responseAuthority: WorkflowCleanupAuthority = {
    ...cleanupBaseAuthority(authority),
    ...(firstExecutionRunId ? { firstExecutionRunId } : {}),
  };
  const evidenceDigest = cleanupEvidenceDigest(
    responseAuthority,
    firstExecutionRunId,
    outcome,
    input.reason,
    input.proof,
  );
  if (outcome === "complete" && input.reason === "absence_proved") {
    return {
      status: 202,
      body: { ...responseAuthority, outcome, reason: input.reason, evidenceDigest },
    };
  }
  return {
    status: 202,
    body: {
      ...responseAuthority,
      outcome: "pending",
      reason: input.reason as WorkflowCleanupPendingReason,
      evidenceDigest,
    },
  };
}

export function cleanupEvidenceDigest(
  authority: WorkflowCleanupAuthority,
  firstExecutionRunId: string | undefined,
  outcome: "complete" | "pending",
  reason: "absence_proved" | WorkflowCleanupPendingReason,
  proof: CleanupProof,
): string {
  const canonical = canonicalizeCleanupEvidence({
    schemaVersion: 2,
    cleanupRequestId: authority.cleanupRequestId,
    generation: authority.generation,
    targetSetDigest: authority.targetSetDigest,
    cleanupFence: authority.cleanupFence,
    workflowId: authority.workflowId,
    startRequestId: authority.startRequestId,
    startPayloadDigest: authority.startPayloadDigest,
    firstExecutionRunId: firstExecutionRunId ?? null,
    outcome,
    reason,
    describe: proof.describe,
    history: proof.history,
    visibility: proof.visibility,
  });
  return createHash("sha256")
    .update(EVIDENCE_DOMAIN, "utf8")
    .update(canonical, "utf8")
    .digest("hex");
}

export function canonicalizeCleanupEvidence(
  value: Record<string, string | number | null>,
): string {
  return `{${Object.keys(value).sort().map((key) =>
    `${JSON.stringify(key)}:${JSON.stringify(value[key])}`).join(",")}}`;
}

function cleanupError(
  status: number,
  outcome: WorkflowGatewayError["outcome"],
  reason: WorkflowGatewayError["reason"],
): WorkflowCleanupServiceResult {
  return { status, body: { schemaVersion: 2, outcome, reason } };
}

function identityConflict(): WorkflowCleanupServiceResult {
  return cleanupError(409, "identity_conflict", "identity_conflict");
}

function unavailableProof(): CleanupProof {
  return {
    describe: "unavailable",
    history: "not_checked",
    visibility: "unavailable",
  };
}

function temporalRpc<T>(
  options: LoadedCleanupOptions,
  operation: () => Promise<T>,
): Promise<T> {
  return options.client.withDeadline(options.now() + options.rpcTimeoutMs, operation);
}

function temporalNotFound(error: unknown): boolean {
  return error instanceof WorkflowNotFoundError
    || (isGrpcServiceError(error) && error.code === GRPC_NOT_FOUND);
}

function compareExecutions(left: ValidatedExecution, right: ValidatedExecution): number {
  return left.runId.localeCompare(right.runId);
}

function validWorkflowStatus(value: string): boolean {
  return WORKFLOW_STATUSES.has(value);
}

const WORKFLOW_STATUSES = new Set([
  "RUNNING",
  "COMPLETED",
  "FAILED",
  "CANCELLED",
  "TERMINATED",
  "CONTINUED_AS_NEW",
  "TIMED_OUT",
]);

function loadOptions(input: WorkflowCleanupServiceOptions): LoadedCleanupOptions {
  const options: LoadedCleanupOptions = {
    client: input.client,
    rpcTimeoutMs: input.rpcTimeoutMs ?? 5_000,
    visibilityConfirmationAgeMs:
      input.visibilityConfirmationAgeMs ?? DEFAULT_CONFIRMATION_AGE_MS,
    authorityCacheLimit: input.authorityCacheLimit ?? DEFAULT_CACHE_LIMIT,
    visibilityPageSize: input.visibilityPageSize ?? DEFAULT_PAGE_SIZE,
    visibilityMaxPages: input.visibilityMaxPages ?? DEFAULT_MAX_PAGES,
    visibilityMaxExecutions:
      input.visibilityMaxExecutions ?? DEFAULT_MAX_EXECUTIONS,
    now: input.now ?? Date.now,
  };
  assertInteger(options.rpcTimeoutMs, 100, 30_000, "cleanup RPC timeout");
  assertInteger(
    options.visibilityConfirmationAgeMs,
    1,
    10 * 60 * 1_000,
    "cleanup visibility confirmation age",
  );
  assertInteger(options.authorityCacheLimit, 1, 10_000, "cleanup authority cache limit");
  assertInteger(options.visibilityPageSize, 1, 1_000, "cleanup visibility page size");
  assertInteger(options.visibilityMaxPages, 1, 1_000, "cleanup visibility page limit");
  assertInteger(
    options.visibilityMaxExecutions,
    1,
    100_000,
    "cleanup visibility execution limit",
  );
  return options;
}

function assertInteger(value: number, minimum: number, maximum: number, label: string): void {
  if (!Number.isSafeInteger(value) || value < minimum || value > maximum) {
    throw new Error(`Invalid ${label}`);
  }
}

function exactRecord(value: unknown): Record<string, unknown> {
  const record = recordOrUndefined(value);
  if (!record) throw new Error("Invalid workflow cleanup authority");
  return record;
}

function recordOrUndefined(value: unknown): Record<string, unknown> | undefined {
  return value && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : undefined;
}

function sameKeys(record: Record<string, unknown>, expected: readonly string[]): boolean {
  const keys = Object.keys(record).sort();
  const sortedExpected = [...expected].sort();
  return keys.length === sortedExpected.length
    && keys.every((key, index) => key === sortedExpected[index]);
}

function opaqueId(value: unknown, maximum: number): value is string {
  return typeof value === "string"
    && value.length >= 20
    && value.length <= maximum
    && /^[A-Za-z0-9_-]+$/.test(value);
}

function digest(value: unknown): value is string {
  return typeof value === "string" && /^[a-f0-9]{64}$/.test(value);
}

function positiveSafeInteger(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value > 0;
}

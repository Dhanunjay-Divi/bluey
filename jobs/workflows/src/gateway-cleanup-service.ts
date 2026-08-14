import { createHash } from "node:crypto";
import {
  WorkflowNotFoundError,
  isGrpcServiceError,
  type WorkflowClient,
} from "@temporalio/client";
import type {
  LegacyWorkflowInventoryPageReceipt,
  LegacyWorkflowInventoryPageRequest,
  LegacyWorkflowInventoryTarget,
  ReconcileLegacyWorkflowTargetRequest,
  ReconcileV2WorkflowTargetRequest,
  WorkflowCleanupError,
  WorkflowCleanupReconcileReason,
  WorkflowCleanupReconcileReceipt,
  WorkflowCleanupRequest,
  WorkflowCleanupResponse,
} from "./contracts.js";
import {
  WORKFLOW_PROTOCOL_MEMO_KEY,
  WORKFLOW_TYPE_V2,
} from "./gateway-service.js";

export const WORKFLOW_CLEANUP_PATH = "/workflow-cleanup";
export const LEGACY_WORKFLOW_TYPE = "applicationWorkflow";
export const WORKFLOW_CLEANUP_SCHEMA_VERSION = 3;
export const WORKFLOW_CLEANUP_PAGE_SIZE = 100;
export const WORKFLOW_CLEANUP_MAX_RUN_IDS = 32;

const DEFAULT_RPC_TIMEOUT_MS = 5_000;
const DEFAULT_MAX_VISIBILITY_PAGES = 64;
const DEFAULT_REQUEST_TIMEOUT_MS = 12_000;
const MAX_PAGE_TOKEN_BYTES = 4_096;
const MAX_MEMO_BYTES = 4_096;
const MAX_CUTOFF_MS = 253_402_300_799_999;
const MAX_PAGE_INDEX = 4_095;
const GRPC_NOT_FOUND = 5;
const QUERY_DIGEST_DOMAIN = "bluey-jobs-legacy-inventory-query-v3\0";
const TARGETS_DIGEST_DOMAIN = "bluey-jobs-legacy-inventory-targets-v3\0";
const PAGE_DIGEST_DOMAIN = "bluey-jobs-legacy-inventory-page-v3\0";
const LEGACY_TARGET_DIGEST_DOMAIN = "bluey-jobs-legacy-target-v3\0";
const V2_TARGET_DIGEST_DOMAIN = "bluey-jobs-workflow-v2-target-v3\0";
const EVIDENCE_DIGEST_DOMAIN = "bluey-jobs-workflow-cleanup-evidence-v3\0";

type WorkflowStatus = LegacyWorkflowInventoryTarget["status"];

export interface WorkflowCleanupServiceResult {
  status: number;
  body: WorkflowCleanupResponse;
}

export interface TemporalCleanupExecution {
  workflowId: unknown;
  runId: unknown;
  firstExecutionRunId: unknown;
  workflowType: unknown;
  status: unknown;
  startTime: unknown;
  memoFields: unknown;
}

export interface TemporalCleanupVisibilityPage {
  executions: readonly TemporalCleanupExecution[];
  nextPageToken?: Uint8Array;
}

export interface TemporalCleanupClient {
  readonly namespace: string;
  withDeadline<T>(deadline: number | Date, operation: () => Promise<T>): Promise<T>;
  describe(workflowId: string, runId?: string): Promise<TemporalCleanupExecution>;
  terminate(workflowId: string, runId: string, firstExecutionRunId: string): Promise<void>;
  delete(workflowId: string, runId: string): Promise<void>;
  historyProbe(workflowId: string, runId?: string): Promise<void>;
  listPage(
    query: string,
    nextPageToken: Uint8Array,
    pageSize: number,
  ): Promise<TemporalCleanupVisibilityPage>;
}

export interface WorkflowCleanupServiceOptions {
  client: TemporalCleanupClient;
  namespace: string;
  rpcTimeoutMs?: number;
  requestTimeoutMs?: number;
  visibilityMaxPages?: number;
  visibilityMaxExecutions?: number;
  now?: () => number;
}

interface LoadedCleanupOptions {
  client: TemporalCleanupClient;
  namespace: string;
  rpcTimeoutMs: number;
  requestTimeoutMs: number;
  visibilityMaxPages: number;
  visibilityMaxExecutions: number;
  now: () => number;
}

interface ValidatedExecution {
  workflowId: string;
  runId: string;
  firstExecutionRunId: string | null;
  workflowType: typeof LEGACY_WORKFLOW_TYPE | typeof WORKFLOW_TYPE_V2;
  status: WorkflowStatus;
  startTimeMs: number;
  memoFields: unknown;
}

interface OperationBudget {
  deadlineMs: number;
}

type TemporalLookup<T> =
  | { state: "found"; value: T }
  | { state: "not_found" }
  | { state: "unavailable" };

type MutationResult = "complete" | "not_found" | "unavailable";

type ExhaustiveVisibility =
  | { state: "found"; executions: ValidatedExecution[] }
  | { state: "identity_conflict" }
  | { state: "unavailable" };

export function createTemporalCleanupClient(client: WorkflowClient): TemporalCleanupClient {
  const namespace = client.options.namespace;
  return {
    namespace,
    withDeadline: (deadline, operation) => client.withDeadline(deadline, operation),
    describe: async (workflowId, runId) => {
      const response = await client.workflowService.describeWorkflowExecution({
        namespace,
        execution: { workflowId, ...(runId ? { runId } : {}) },
      });
      return rawExecution(response.workflowExecutionInfo);
    },
    terminate: async (workflowId, runId, firstExecutionRunId) => {
      await client.workflowService.terminateWorkflowExecution({
        namespace,
        workflowExecution: { workflowId, runId },
        firstExecutionRunId,
        reason: "bluey_jobs_cleanup_v3",
      });
    },
    delete: async (workflowId, runId) => {
      await client.workflowService.deleteWorkflowExecution({
        namespace,
        workflowExecution: { workflowId, runId },
      });
    },
    historyProbe: async (workflowId, runId) => {
      await client.workflowService.getWorkflowExecutionHistory({
        namespace,
        execution: { workflowId, ...(runId ? { runId } : {}) },
        maximumPageSize: 1,
        waitNewEvent: false,
        skipArchival: false,
      });
    },
    listPage: async (query, nextPageToken, pageSize) => {
      const response = await client.workflowService.listWorkflowExecutions({
        namespace,
        query,
        pageSize,
        nextPageToken,
      });
      return {
        executions: (response.executions ?? []).map(rawExecution),
        ...(response.nextPageToken && response.nextPageToken.length > 0
          ? { nextPageToken: response.nextPageToken }
          : {}),
      };
    },
  };
}

export function createWorkflowCleanupService(input: WorkflowCleanupServiceOptions) {
  const options = loadOptions(input);
  return {
    async executeCleanup(value: unknown): Promise<WorkflowCleanupServiceResult> {
      let request: WorkflowCleanupRequest;
      try {
        request = parseWorkflowCleanupRequest(value);
      } catch {
        return cleanupError(400, "rejected", "invalid_request");
      }
      if (request.namespace !== options.namespace) return identityConflict();
      const requestStartedAtMs = options.now();
      if (!Number.isSafeInteger(requestStartedAtMs)
        || requestStartedAtMs < 0
        || requestStartedAtMs > Number.MAX_SAFE_INTEGER - options.requestTimeoutMs) {
        return cleanupError(503, "rejected", "temporal_unavailable");
      }
      const budget = { deadlineMs: requestStartedAtMs + options.requestTimeoutMs };
      try {
        switch (request.operation) {
          case "legacy_inventory_page":
            return await inventoryLegacyPage(request, options, budget);
          case "reconcile_legacy_target":
            return await reconcileLegacyTarget(request, options, budget);
          case "reconcile_v2_target":
            return await reconcileV2Target(request, options, budget);
        }
      } catch {
        return cleanupError(503, "rejected", "temporal_unavailable");
      }
    },
  };
}

export function parseWorkflowCleanupRequest(value: unknown): WorkflowCleanupRequest {
  const record = exactRecord(value);
  switch (record.operation) {
    case "legacy_inventory_page":
      return parseLegacyInventoryPage(record);
    case "reconcile_legacy_target":
      return parseLegacyTarget(record);
    case "reconcile_v2_target":
      return parseV2Target(record);
    default:
      throw new Error("Invalid workflow cleanup request");
  }
}

export function legacyInventoryQuery(visibilityCutoffMs: number): string {
  if (!cutoffMilliseconds(visibilityCutoffMs)) {
    throw new Error("Invalid workflow visibility cutoff");
  }
  return `WorkflowType = "${LEGACY_WORKFLOW_TYPE}"`;
}

export function legacyInventoryQueryDigest(input: {
  namespace: string;
  workflowType: typeof LEGACY_WORKFLOW_TYPE;
  visibilityCutoffMs: number;
}): string {
  const query = legacyInventoryQuery(input.visibilityCutoffMs);
  return domainDigest(QUERY_DIGEST_DOMAIN, {
    namespace: input.namespace,
    workflowType: input.workflowType,
    visibilityCutoffMs: input.visibilityCutoffMs,
    query,
  });
}

export function legacyInventoryTargetsDigest(
  targets: readonly LegacyWorkflowInventoryTarget[],
): string {
  return domainDigest(TARGETS_DIGEST_DOMAIN, targets);
}

export function legacyInventoryPageDigest(
  receipt: Omit<LegacyWorkflowInventoryPageReceipt, "pageDigest">,
): string {
  return domainDigest(PAGE_DIGEST_DOMAIN, receipt);
}

export function legacyWorkflowTargetDigest(
  request: Omit<
    ReconcileLegacyWorkflowTargetRequest,
    "schemaVersion" | "operation" | "cleanupRequestId" | "targetDigest" |
    "cleanupFence" | "observationPass"
  >,
): string {
  return domainDigest(LEGACY_TARGET_DIGEST_DOMAIN, {
    inventoryGenerationId: request.inventoryGenerationId,
    namespace: request.namespace,
    workflowType: request.workflowType,
    visibilityCutoffMs: request.visibilityCutoffMs,
    queryDigest: request.queryDigest,
    scanPass: request.scanPass,
    workflowId: request.workflowId,
    runId: request.runId,
    firstExecutionRunId: request.firstExecutionRunId,
  });
}

export function v2WorkflowTargetDigest(
  request: Omit<
    ReconcileV2WorkflowTargetRequest,
    "schemaVersion" | "operation" | "cleanupRequestId" | "targetDigest" |
    "cleanupFence" | "observationPass"
  >,
): string {
  return domainDigest(V2_TARGET_DIGEST_DOMAIN, {
    cleanupGenerationId: request.cleanupGenerationId,
    targetSetDigest: request.targetSetDigest,
    namespace: request.namespace,
    workflowType: request.workflowType,
    workflowId: request.workflowId,
    firstExecutionRunId: request.firstExecutionRunId,
    startRequestId: request.startRequestId,
    startPayloadDigest: request.startPayloadDigest,
  });
}

export function canonicalizeWorkflowCleanupEvidence(value: unknown): string {
  if (value === null) return "null";
  if (typeof value === "string" || typeof value === "boolean") {
    return JSON.stringify(value);
  }
  if (typeof value === "number" && Number.isSafeInteger(value)) return String(value);
  if (Array.isArray(value)) {
    return `[${value.map(canonicalizeWorkflowCleanupEvidence).join(",")}]`;
  }
  const record = recordOrUndefined(value);
  if (!record) throw new Error("Invalid workflow cleanup evidence");
  return `{${Object.keys(record).sort().map((key) =>
    `${JSON.stringify(key)}:${canonicalizeWorkflowCleanupEvidence(record[key])}`
  ).join(",")}}`;
}

async function inventoryLegacyPage(
  request: LegacyWorkflowInventoryPageRequest,
  options: LoadedCleanupOptions,
  budget: OperationBudget,
): Promise<WorkflowCleanupServiceResult> {
  if (request.queryDigest !== legacyInventoryQueryDigest(request)) {
    return identityConflict();
  }
  const query = legacyInventoryQuery(request.visibilityCutoffMs);
  const token = decodePageToken(request.pageToken);
  let page: TemporalCleanupVisibilityPage;
  try {
    page = await temporalRpc(
      options,
      budget,
      () => options.client.listPage(query, token, WORKFLOW_CLEANUP_PAGE_SIZE),
    );
  } catch {
    return cleanupError(503, "rejected", "temporal_unavailable");
  }
  if (!Array.isArray(page.executions) || page.executions.length > WORKFLOW_CLEANUP_PAGE_SIZE) {
    return cleanupError(503, "rejected", "temporal_unavailable");
  }
  const targets: LegacyWorkflowInventoryTarget[] = [];
  const seen = new Set<string>();
  for (const raw of page.executions) {
    const execution = validateExecution(raw, LEGACY_WORKFLOW_TYPE);
    if (!execution || execution.firstExecutionRunId === null) return identityConflict();
    const identity = `${execution.workflowId}\0${execution.runId}`;
    if (seen.has(identity)) return cleanupError(503, "rejected", "temporal_unavailable");
    seen.add(identity);
    targets.push(inventoryTarget({
      ...execution,
      firstExecutionRunId: execution.firstExecutionRunId,
    }));
  }
  targets.sort(compareTargets);
  const nextPageToken = encodeProviderPageToken(page.nextPageToken);
  if (nextPageToken === undefined) {
    return cleanupError(503, "rejected", "temporal_unavailable");
  }
  if (nextPageToken !== null && nextPageToken === request.pageToken) {
    return cleanupError(503, "rejected", "temporal_unavailable");
  }
  if (request.pageIndex === MAX_PAGE_INDEX && nextPageToken !== null) {
    return cleanupError(503, "rejected", "temporal_unavailable");
  }
  const targetsDigest = legacyInventoryTargetsDigest(targets);
  const withoutPageDigest: Omit<LegacyWorkflowInventoryPageReceipt, "pageDigest"> = {
    ...request,
    outcome: "page",
    targetsDigest,
    targets,
    nextPageToken,
    exhausted: nextPageToken === null,
  };
  return {
    status: 202,
    body: {
      ...withoutPageDigest,
      pageDigest: legacyInventoryPageDigest(withoutPageDigest),
    },
  };
}

async function reconcileLegacyTarget(
  request: ReconcileLegacyWorkflowTargetRequest,
  options: LoadedCleanupOptions,
  budget: OperationBudget,
): Promise<WorkflowCleanupServiceResult> {
  if (request.queryDigest !== legacyInventoryQueryDigest(request)
    || request.targetDigest !== legacyWorkflowTargetDigest(request)) {
    return identityConflict();
  }
  const firstExecutionRunId = request.firstExecutionRunId;
  const initial = await describeExecution(options, budget, request.workflowId, request.runId);
  if (initial.state === "unavailable") {
    return reconcileReceipt(request, firstExecutionRunId, [request.runId],
      "pending", "temporal_unavailable");
  }
  if (initial.state === "found") {
    const execution = validateExecution(initial.value, LEGACY_WORKFLOW_TYPE, {
      workflowId: request.workflowId,
      runId: request.runId,
    });
    if (!execution || execution.firstExecutionRunId !== firstExecutionRunId) {
      return identityConflict();
    }
    if (execution.status === "RUNNING") {
      return reconcileReceipt(request, firstExecutionRunId, [request.runId],
        "pending", "workflow_running");
    }
    const deleted = await mutateExecution(
      options,
      budget,
      () => options.client.delete(request.workflowId, request.runId),
    );
    if (deleted === "unavailable") {
      return reconcileReceipt(request, firstExecutionRunId, [request.runId],
        "pending", "history_delete_pending");
    }
  }
  return proveLegacyTargetAbsence(request, firstExecutionRunId, options, budget);
}

async function proveLegacyTargetAbsence(
  request: ReconcileLegacyWorkflowTargetRequest,
  firstExecutionRunId: string,
  options: LoadedCleanupOptions,
  budget: OperationBudget,
): Promise<WorkflowCleanupServiceResult> {
  const described = await describeExecution(options, budget, request.workflowId, request.runId);
  if (described.state === "unavailable") {
    return reconcileReceipt(request, firstExecutionRunId, [request.runId],
      "pending", "temporal_unavailable");
  }
  if (described.state === "found") {
    const execution = validateExecution(described.value, LEGACY_WORKFLOW_TYPE, {
      workflowId: request.workflowId,
      runId: request.runId,
    });
    if (!execution || execution.firstExecutionRunId !== firstExecutionRunId) {
      return identityConflict();
    }
    return reconcileReceipt(request, firstExecutionRunId, [request.runId], "pending",
      execution.status === "RUNNING" ? "workflow_running" : "history_delete_pending");
  }
  const history = await historyProbe(options, budget, request.workflowId, request.runId);
  if (history === "unavailable") {
    return reconcileReceipt(request, firstExecutionRunId, [request.runId],
      "pending", "temporal_unavailable");
  }
  if (history === "found") {
    return reconcileReceipt(request, firstExecutionRunId, [request.runId],
      "pending", "history_delete_pending");
  }
  const visibility = await listAllExecutions(
    options,
    budget,
    exactRunVisibilityQuery(request.workflowId, request.runId),
    LEGACY_WORKFLOW_TYPE,
    {
      workflowId: request.workflowId,
      runId: request.runId,
    },
  );
  if (visibility.state === "identity_conflict") return identityConflict();
  if (visibility.state === "unavailable") {
    return reconcileReceipt(request, firstExecutionRunId, [request.runId],
      "pending", "temporal_unavailable");
  }
  if (visibility.executions.length > 0) {
    const execution = visibility.executions[0];
    if (!execution || execution.firstExecutionRunId !== firstExecutionRunId) {
      return identityConflict();
    }
    return reconcileReceipt(request, firstExecutionRunId, [request.runId], "pending",
      execution.status === "RUNNING" ? "workflow_running" : "visibility_pending");
  }
  return reconcileReceipt(request, firstExecutionRunId, [request.runId],
    "absence_observed", "absence_observed");
}

async function reconcileV2Target(
  request: ReconcileV2WorkflowTargetRequest,
  options: LoadedCleanupOptions,
  budget: OperationBudget,
): Promise<WorkflowCleanupServiceResult> {
  if (request.targetDigest !== v2WorkflowTargetDigest(request)) return identityConflict();
  let firstExecutionRunId = request.firstExecutionRunId;
  const executions = new Map<string, ValidatedExecution>();
  const runIds = new Set(request.knownRunIds);

  const latest = await describeExecution(options, budget, request.workflowId);
  if (latest.state === "unavailable") {
    return v2UnavailableReceipt(request);
  }
  if (latest.state === "found") {
    const validated = validateV2Execution(latest.value, request);
    if (!validated || !bindFirstRun(validated, firstExecutionRunId)) return identityConflict();
    firstExecutionRunId ??= validated.firstExecutionRunId;
    executions.set(validated.runId, validated);
    runIds.add(validated.firstExecutionRunId);
    runIds.add(validated.runId);
  }
  if (runIds.size > options.visibilityMaxExecutions) {
    return v2UnavailableReceipt(request);
  }

  const visible = await listAllExecutions(
    options,
    budget,
    exactWorkflowVisibilityQuery(request.workflowId),
    WORKFLOW_TYPE_V2,
    { workflowId: request.workflowId },
  );
  if (visible.state === "identity_conflict") return identityConflict();
  if (visible.state === "unavailable") {
    return v2UnavailableReceipt(request);
  }
  const visibleRunIds = new Set<string>();
  for (const visibleExecution of visible.executions) {
    if (!exactV2Memo(visibleExecution.memoFields, request)
      || !bindFirstRun(visibleExecution, firstExecutionRunId)) {
      return identityConflict();
    }
    firstExecutionRunId ??= visibleExecution.firstExecutionRunId;
    visibleRunIds.add(visibleExecution.runId);
    runIds.add(visibleExecution.firstExecutionRunId);
    runIds.add(visibleExecution.runId);
  }
  if (runIds.size > options.visibilityMaxExecutions) {
    return v2UnavailableReceipt(request);
  }

  for (const runId of [...runIds].sort()) {
    if (executions.has(runId)) continue;
    const described = await describeExecution(options, budget, request.workflowId, runId);
    if (described.state === "unavailable") {
      return v2UnavailableReceipt(request);
    }
    if (described.state === "not_found") {
      if (visibleRunIds.has(runId)) {
        return reconcileReceipt(request, firstExecutionRunId, [...runIds],
          "pending", "visibility_pending");
      }
      continue;
    }
    const validated = validateV2Execution(described.value, request, runId);
    if (!validated || !bindFirstRun(validated, firstExecutionRunId)) return identityConflict();
    firstExecutionRunId ??= validated.firstExecutionRunId;
    runIds.add(validated.firstExecutionRunId);
    if (runIds.size > options.visibilityMaxExecutions) {
      return v2UnavailableReceipt(request);
    }
    executions.set(runId, validated);
  }

  const allRunIds = [...runIds].sort();
  if (allRunIds.some((runId) => !request.knownRunIds.includes(runId))) {
    return reconcileReceipt(
      request,
      firstExecutionRunId,
      allRunIds,
      "pending",
      "visibility_pending",
    );
  }
  if (executions.size === 0) {
    return proveV2RunAbsence(
      request,
      firstExecutionRunId,
      allRunIds,
      options,
      budget,
    );
  }
  if (firstExecutionRunId === null) return identityConflict();

  for (const execution of [...executions.values()].sort(compareExecutions)) {
    if (execution.status !== "RUNNING") continue;
    const terminated = await mutateExecution(
      options,
      budget,
      () => options.client.terminate(
        request.workflowId,
        execution.runId,
        firstExecutionRunId as string,
      ),
    );
    if (terminated === "unavailable") {
      return reconcileReceipt(request, firstExecutionRunId, allRunIds,
        "pending", "termination_pending");
    }
  }
  for (const execution of [...executions.values()].sort(compareExecutions)) {
    const deleted = await mutateExecution(
      options,
      budget,
      () => options.client.delete(request.workflowId, execution.runId),
    );
    if (deleted === "unavailable") {
      return reconcileReceipt(request, firstExecutionRunId, allRunIds,
        "pending", "history_delete_pending");
    }
  }
  return proveV2RunAbsence(request, firstExecutionRunId, allRunIds, options, budget);
}

async function proveV2RunAbsence(
  request: ReconcileV2WorkflowTargetRequest,
  firstExecutionRunId: string | null,
  runIds: string[],
  options: LoadedCleanupOptions,
  budget: OperationBudget,
): Promise<WorkflowCleanupServiceResult> {
  if (runIds.length === 0) {
    const history = await historyProbe(options, budget, request.workflowId);
    if (history === "unavailable") {
      return v2UnavailableReceipt(request);
    }
    if (history === "found") {
      return reconcileReceipt(request, firstExecutionRunId, [],
        "pending", "history_delete_pending");
    }
  }
  for (const runId of runIds) {
    const described = await describeExecution(options, budget, request.workflowId, runId);
    if (described.state === "unavailable") {
      return v2UnavailableReceipt(request);
    }
    if (described.state === "found") {
      const execution = validateV2Execution(described.value, request, runId);
      if (!execution || !bindFirstRun(execution, firstExecutionRunId)) {
        return identityConflict();
      }
      const observedFirstRunId = firstExecutionRunId ?? execution.firstExecutionRunId;
      return reconcileReceipt(request, observedFirstRunId, runIds, "pending",
        execution.status === "RUNNING" ? "termination_pending" : "history_delete_pending");
    }
    const history = await historyProbe(options, budget, request.workflowId, runId);
    if (history === "unavailable") {
      return v2UnavailableReceipt(request);
    }
    if (history === "found") {
      return reconcileReceipt(request, firstExecutionRunId, runIds,
        "pending", "history_delete_pending");
    }
  }
  const visible = await listAllExecutions(
    options,
    budget,
    exactWorkflowVisibilityQuery(request.workflowId),
    WORKFLOW_TYPE_V2,
    { workflowId: request.workflowId },
  );
  if (visible.state === "identity_conflict") return identityConflict();
  if (visible.state === "unavailable") {
    return v2UnavailableReceipt(request);
  }
  if (visible.executions.length > 0) {
    const observedRunIds = new Set(runIds);
    for (const execution of visible.executions) {
      if (!exactV2Memo(execution.memoFields, request)
        || !bindFirstRun(execution, firstExecutionRunId)) {
        return identityConflict();
      }
      firstExecutionRunId ??= execution.firstExecutionRunId;
      observedRunIds.add(execution.firstExecutionRunId);
      observedRunIds.add(execution.runId);
    }
    const sortedObservedRunIds = [...observedRunIds].sort();
    if (sortedObservedRunIds.length > options.visibilityMaxExecutions) {
      return v2UnavailableReceipt(request);
    }
    return reconcileReceipt(
      request,
      firstExecutionRunId,
      sortedObservedRunIds,
      "pending",
      "visibility_pending",
    );
  }
  return reconcileReceipt(request, firstExecutionRunId, runIds,
    "absence_observed", "absence_observed");
}

function v2UnavailableReceipt(
  request: ReconcileV2WorkflowTargetRequest,
): WorkflowCleanupServiceResult {
  return reconcileReceipt(
    request,
    request.firstExecutionRunId,
    request.knownRunIds,
    "pending",
    "temporal_unavailable",
  );
}

function reconcileReceipt(
  request: ReconcileLegacyWorkflowTargetRequest | ReconcileV2WorkflowTargetRequest,
  firstExecutionRunId: string | null,
  runIds: readonly string[],
  outcome: "pending" | "absence_observed",
  reason: WorkflowCleanupReconcileReason,
): WorkflowCleanupServiceResult {
  const authority = { ...request, firstExecutionRunId };
  const receiptWithoutDigest = {
    ...authority,
    outcome,
    reason,
    runIds: [...new Set(runIds)].sort(),
  };
  const body: WorkflowCleanupReconcileReceipt = {
    ...receiptWithoutDigest,
    evidenceDigest: domainDigest(EVIDENCE_DIGEST_DOMAIN, receiptWithoutDigest),
  };
  return { status: 202, body };
}

async function describeExecution(
  options: LoadedCleanupOptions,
  budget: OperationBudget,
  workflowId: string,
  runId?: string,
): Promise<TemporalLookup<TemporalCleanupExecution>> {
  try {
    return {
      state: "found",
      value: await temporalRpc(options, budget, () => options.client.describe(workflowId, runId)),
    };
  } catch (error) {
    return temporalNotFound(error) ? { state: "not_found" } : { state: "unavailable" };
  }
}

async function historyProbe(
  options: LoadedCleanupOptions,
  budget: OperationBudget,
  workflowId: string,
  runId?: string,
): Promise<"found" | "not_found" | "unavailable"> {
  try {
    await temporalRpc(options, budget, () => options.client.historyProbe(workflowId, runId));
    return "found";
  } catch (error) {
    return temporalNotFound(error) ? "not_found" : "unavailable";
  }
}

async function mutateExecution(
  options: LoadedCleanupOptions,
  budget: OperationBudget,
  operation: () => Promise<void>,
): Promise<MutationResult> {
  try {
    await temporalRpc(options, budget, operation);
    return "complete";
  } catch (error) {
    return temporalNotFound(error) ? "not_found" : "unavailable";
  }
}

async function listAllExecutions(
  options: LoadedCleanupOptions,
  budget: OperationBudget,
  query: string,
  workflowType: typeof LEGACY_WORKFLOW_TYPE | typeof WORKFLOW_TYPE_V2,
  expected: { workflowId: string; runId?: string },
): Promise<ExhaustiveVisibility> {
  let token = new Uint8Array();
  const seenTokens = new Set<string>();
  const seenExecutions = new Set<string>();
  const executions: ValidatedExecution[] = [];
  for (let pageIndex = 0; pageIndex < options.visibilityMaxPages; pageIndex += 1) {
    let page: TemporalCleanupVisibilityPage;
    try {
      page = await temporalRpc(
        options,
        budget,
        () => options.client.listPage(query, token, WORKFLOW_CLEANUP_PAGE_SIZE),
      );
    } catch {
      return { state: "unavailable" };
    }
    if (!Array.isArray(page.executions) || page.executions.length > WORKFLOW_CLEANUP_PAGE_SIZE) {
      return { state: "unavailable" };
    }
    for (const raw of page.executions) {
      const execution = validateExecution(raw, workflowType, expected);
      if (!execution) return { state: "identity_conflict" };
      const identity = `${execution.workflowId}\0${execution.runId}`;
      if (seenExecutions.has(identity)) return { state: "unavailable" };
      seenExecutions.add(identity);
      executions.push(execution);
      if (executions.length > options.visibilityMaxExecutions) {
        return { state: "unavailable" };
      }
    }
    if (page.nextPageToken === undefined) {
      executions.sort(compareExecutions);
      return { state: "found", executions };
    }
    if (!(page.nextPageToken instanceof Uint8Array)) return { state: "unavailable" };
    if (page.nextPageToken.length === 0) {
      executions.sort(compareExecutions);
      return { state: "found", executions };
    }
    const encoded = encodeProviderPageToken(page.nextPageToken);
    if (!encoded || seenTokens.has(encoded)) return { state: "unavailable" };
    seenTokens.add(encoded);
    token = new Uint8Array(page.nextPageToken);
  }
  return { state: "unavailable" };
}

function validateExecution(
  raw: TemporalCleanupExecution,
  workflowType: typeof LEGACY_WORKFLOW_TYPE | typeof WORKFLOW_TYPE_V2,
  expected: { workflowId?: string; runId?: string } = {},
): ValidatedExecution | undefined {
  const workflowId = raw.workflowId;
  const runId = raw.runId;
  const firstExecutionRunId = raw.firstExecutionRunId === null
    || raw.firstExecutionRunId === undefined
    || raw.firstExecutionRunId === ""
    ? null
    : raw.firstExecutionRunId;
  const status = workflowStatus(raw.status);
  const startTimeMs = timestampMilliseconds(raw.startTime);
  if (!workflowIdForType(workflowId, workflowType)
    || !opaqueId(runId, 128)
    || (firstExecutionRunId !== null && !opaqueId(firstExecutionRunId, 128))
    || raw.workflowType !== workflowType
    || !status
    || startTimeMs === undefined
    || (expected.workflowId !== undefined && workflowId !== expected.workflowId)
    || (expected.runId !== undefined && runId !== expected.runId)) {
    return undefined;
  }
  return {
    workflowId,
    runId,
    firstExecutionRunId,
    workflowType,
    status,
    startTimeMs,
    memoFields: raw.memoFields,
  };
}

function validateV2Execution(
  raw: TemporalCleanupExecution,
  request: ReconcileV2WorkflowTargetRequest,
  runId?: string,
): (ValidatedExecution & { firstExecutionRunId: string }) | undefined {
  const execution = validateExecution(raw, WORKFLOW_TYPE_V2, {
    workflowId: request.workflowId,
    ...(runId ? { runId } : {}),
  });
  if (!execution || execution.firstExecutionRunId === null
    || !exactV2Memo(execution.memoFields, request)) {
    return undefined;
  }
  return { ...execution, firstExecutionRunId: execution.firstExecutionRunId };
}

function exactV2Memo(
  value: unknown,
  request: ReconcileV2WorkflowTargetRequest,
): boolean {
  const fields = recordOrUndefined(value);
  if (!fields || !sameKeys(fields, [WORKFLOW_PROTOCOL_MEMO_KEY])) return false;
  const payload = recordOrUndefined(fields[WORKFLOW_PROTOCOL_MEMO_KEY]);
  const metadata = recordOrUndefined(payload?.metadata);
  const data = payload?.data;
  const payloadKeysAreExact = payload !== undefined
    && (sameKeys(payload, ["data", "metadata"])
      || sameKeys(payload, ["data", "externalPayloads", "metadata"]));
  if (!payload || !payloadKeysAreExact
    || ("externalPayloads" in payload
      && (!Array.isArray(payload.externalPayloads) || payload.externalPayloads.length !== 0))
    || !metadata || !sameKeys(metadata, ["encoding"])
    || !(metadata.encoding instanceof Uint8Array)
    || Buffer.from(metadata.encoding).toString("utf8") !== "json/plain"
    || !(data instanceof Uint8Array)
    || data.length < 2
    || data.length > MAX_MEMO_BYTES) {
    return false;
  }
  const expected = JSON.stringify({
    schemaVersion: 2,
    requestId: request.startRequestId,
    workflowId: request.workflowId,
    payloadDigest: request.startPayloadDigest,
  });
  return Buffer.from(data).equals(Buffer.from(expected, "utf8"));
}

function rawExecution(value: unknown): TemporalCleanupExecution {
  const record = recordOrUndefined(value);
  const execution = recordOrUndefined(record?.execution);
  const type = recordOrUndefined(record?.type);
  const memo = recordOrUndefined(record?.memo);
  return {
    workflowId: execution?.workflowId,
    runId: execution?.runId,
    firstExecutionRunId: record?.firstRunId,
    workflowType: type?.name,
    status: record?.status,
    startTime: record?.startTime,
    memoFields: memo?.fields,
  };
}

function inventoryTarget(
  execution: ValidatedExecution & { firstExecutionRunId: string },
): LegacyWorkflowInventoryTarget {
  return {
    workflowId: execution.workflowId,
    runId: execution.runId,
    firstExecutionRunId: execution.firstExecutionRunId,
    status: execution.status,
  };
}

function bindFirstRun(
  execution: ValidatedExecution,
  expected: string | null,
): execution is ValidatedExecution & { firstExecutionRunId: string } {
  return execution.firstExecutionRunId !== null
    && (expected === null || execution.firstExecutionRunId === expected);
}

function workflowIdForType(
  value: unknown,
  workflowType: typeof LEGACY_WORKFLOW_TYPE | typeof WORKFLOW_TYPE_V2,
): value is string {
  return workflowType === LEGACY_WORKFLOW_TYPE
    ? legacyWorkflowId(value)
    : opaqueId(value, 192);
}

function compareTargets(
  left: LegacyWorkflowInventoryTarget,
  right: LegacyWorkflowInventoryTarget,
): number {
  return compareOpaqueIds(left.workflowId, right.workflowId)
    || compareOpaqueIds(left.runId, right.runId);
}

function compareExecutions(left: ValidatedExecution, right: ValidatedExecution): number {
  return compareOpaqueIds(left.runId, right.runId);
}

function compareOpaqueIds(left: string, right: string): number {
  return left < right ? -1 : left > right ? 1 : 0;
}

function exactWorkflowVisibilityQuery(workflowId: string): string {
  return `WorkflowId = "${workflowId}"`;
}

function exactRunVisibilityQuery(workflowId: string, runId: string): string {
  return `WorkflowId = "${workflowId}" AND RunId = "${runId}"`;
}

function temporalRpc<T>(
  options: LoadedCleanupOptions,
  budget: OperationBudget,
  operation: () => Promise<T>,
): Promise<T> {
  const now = options.now();
  if (!Number.isSafeInteger(now) || now >= budget.deadlineMs) {
    throw new Error("Workflow cleanup request deadline exhausted");
  }
  return options.client.withDeadline(
    Math.min(now + options.rpcTimeoutMs, budget.deadlineMs),
    operation,
  );
}

function temporalNotFound(error: unknown): boolean {
  return error instanceof WorkflowNotFoundError
    || (isGrpcServiceError(error) && error.code === GRPC_NOT_FOUND);
}

function cleanupError(
  status: number,
  outcome: WorkflowCleanupError["outcome"],
  reason: WorkflowCleanupError["reason"],
): WorkflowCleanupServiceResult {
  return {
    status,
    body: { schemaVersion: WORKFLOW_CLEANUP_SCHEMA_VERSION, outcome, reason },
  };
}

function identityConflict(): WorkflowCleanupServiceResult {
  return cleanupError(409, "identity_conflict", "identity_conflict");
}

function parseLegacyInventoryPage(
  record: Record<string, unknown>,
): LegacyWorkflowInventoryPageRequest {
  const expectedKeys = [
    "schemaVersion",
    "operation",
    "cleanupRequestId",
    "inventoryGenerationId",
    "namespace",
    "workflowType",
    "visibilityCutoffMs",
    "queryDigest",
    "scanPass",
    "pageIndex",
    "predecessorPageDigest",
    "pageToken",
    "cleanupFence",
  ];
  if (!sameKeys(record, expectedKeys)
    || !commonLegacyInventoryAuthority(record)
    || !nonNegativeSafeInteger(record.pageIndex)
    || record.pageIndex > MAX_PAGE_INDEX
    || !(record.predecessorPageDigest === null || digest(record.predecessorPageDigest))
    || !(record.pageToken === null || canonicalPageToken(record.pageToken))
    || (record.pageIndex === 0
      ? record.predecessorPageDigest !== null || record.pageToken !== null
      : record.predecessorPageDigest === null || record.pageToken === null)) {
    throw new Error("Invalid workflow cleanup request");
  }
  return record as unknown as LegacyWorkflowInventoryPageRequest;
}

function parseLegacyTarget(
  record: Record<string, unknown>,
): ReconcileLegacyWorkflowTargetRequest {
  const expectedKeys = [
    "schemaVersion",
    "operation",
    "cleanupRequestId",
    "inventoryGenerationId",
    "namespace",
    "workflowType",
    "visibilityCutoffMs",
    "queryDigest",
    "scanPass",
    "workflowId",
    "runId",
    "firstExecutionRunId",
    "targetDigest",
    "cleanupFence",
    "observationPass",
  ];
  if (!sameKeys(record, expectedKeys)
    || !commonLegacyInventoryAuthority(record)
    || !legacyWorkflowId(record.workflowId)
    || !opaqueId(record.runId, 128)
    || !opaqueId(record.firstExecutionRunId, 128)
    || !digest(record.targetDigest)
    || !cleanupPass(record.observationPass)) {
    throw new Error("Invalid workflow cleanup request");
  }
  return record as unknown as ReconcileLegacyWorkflowTargetRequest;
}

function parseV2Target(record: Record<string, unknown>): ReconcileV2WorkflowTargetRequest {
  const expectedKeys = [
    "schemaVersion",
    "operation",
    "cleanupRequestId",
    "cleanupGenerationId",
    "targetSetDigest",
    "namespace",
    "workflowType",
    "workflowId",
    "firstExecutionRunId",
    "startRequestId",
    "startPayloadDigest",
    "knownRunIds",
    "targetDigest",
    "cleanupFence",
    "observationPass",
  ];
  if (!sameKeys(record, expectedKeys)
    || record.schemaVersion !== WORKFLOW_CLEANUP_SCHEMA_VERSION
    || record.operation !== "reconcile_v2_target"
    || !opaqueId(record.cleanupRequestId, 128)
    || !opaqueId(record.cleanupGenerationId, 128)
    || !digest(record.targetSetDigest)
    || !validNamespace(record.namespace)
    || record.workflowType !== WORKFLOW_TYPE_V2
    || !opaqueId(record.workflowId, 192)
    || !(record.firstExecutionRunId === null
      || opaqueId(record.firstExecutionRunId, 128))
    || !opaqueId(record.startRequestId, 128)
    || !digest(record.startPayloadDigest)
    || !sortedUniqueOpaqueIds(record.knownRunIds, WORKFLOW_CLEANUP_MAX_RUN_IDS)
    || ((record.firstExecutionRunId === null)
      !== ((record.knownRunIds as string[]).length === 0))
    || (typeof record.firstExecutionRunId === "string"
      && !(record.knownRunIds as string[]).includes(record.firstExecutionRunId))
    || !digest(record.targetDigest)
    || !positiveSafeInteger(record.cleanupFence)
    || !cleanupPass(record.observationPass)) {
    throw new Error("Invalid workflow cleanup request");
  }
  return record as unknown as ReconcileV2WorkflowTargetRequest;
}

function commonLegacyInventoryAuthority(record: Record<string, unknown>): boolean {
  return record.schemaVersion === WORKFLOW_CLEANUP_SCHEMA_VERSION
    && (record.operation === "legacy_inventory_page"
      || record.operation === "reconcile_legacy_target")
    && opaqueId(record.cleanupRequestId, 128)
    && opaqueId(record.inventoryGenerationId, 128)
    && validNamespace(record.namespace)
    && record.workflowType === LEGACY_WORKFLOW_TYPE
    && cutoffMilliseconds(record.visibilityCutoffMs)
    && digest(record.queryDigest)
    && cleanupPass(record.scanPass)
    && positiveSafeInteger(record.cleanupFence);
}

function loadOptions(input: WorkflowCleanupServiceOptions): LoadedCleanupOptions {
  if (!validNamespace(input.namespace) || input.client.namespace !== input.namespace) {
    throw new Error("Invalid workflow cleanup namespace");
  }
  const options: LoadedCleanupOptions = {
    client: input.client,
    namespace: input.namespace,
    rpcTimeoutMs: input.rpcTimeoutMs ?? DEFAULT_RPC_TIMEOUT_MS,
    requestTimeoutMs: input.requestTimeoutMs ?? DEFAULT_REQUEST_TIMEOUT_MS,
    visibilityMaxPages: input.visibilityMaxPages ?? DEFAULT_MAX_VISIBILITY_PAGES,
    visibilityMaxExecutions: input.visibilityMaxExecutions ?? WORKFLOW_CLEANUP_MAX_RUN_IDS,
    now: input.now ?? Date.now,
  };
  assertInteger(options.rpcTimeoutMs, 100, 30_000, "cleanup RPC timeout");
  assertInteger(
    options.requestTimeoutMs,
    1_000,
    DEFAULT_REQUEST_TIMEOUT_MS,
    "cleanup request timeout",
  );
  assertInteger(options.visibilityMaxPages, 1, 1_000, "cleanup visibility page limit");
  assertInteger(
    options.visibilityMaxExecutions,
    1,
    WORKFLOW_CLEANUP_MAX_RUN_IDS,
    "cleanup visibility execution limit",
  );
  return options;
}

function domainDigest(domain: string, value: unknown): string {
  return createHash("sha256")
    .update(domain, "utf8")
    .update(canonicalizeWorkflowCleanupEvidence(value), "utf8")
    .digest("hex");
}

function encodeProviderPageToken(value: Uint8Array | undefined): string | null | undefined {
  if (value === undefined || value.length === 0) return null;
  if (!(value instanceof Uint8Array) || value.length > MAX_PAGE_TOKEN_BYTES) return undefined;
  return Buffer.from(value).toString("base64url");
}

function decodePageToken(value: string | null): Uint8Array {
  if (value === null) return new Uint8Array();
  if (!canonicalPageToken(value)) throw new Error("Invalid workflow cleanup page token");
  return new Uint8Array(Buffer.from(value, "base64url"));
}

function canonicalPageToken(value: unknown): value is string {
  if (typeof value !== "string" || value.length === 0 || !/^[A-Za-z0-9_-]+$/.test(value)) {
    return false;
  }
  const decoded = Buffer.from(value, "base64url");
  return decoded.length > 0
    && decoded.length <= MAX_PAGE_TOKEN_BYTES
    && decoded.toString("base64url") === value;
}

function workflowStatus(value: unknown): WorkflowStatus | undefined {
  if (typeof value === "string" && WORKFLOW_STATUSES.has(value as WorkflowStatus)) {
    return value as WorkflowStatus;
  }
  return typeof value === "number" ? WORKFLOW_STATUS_BY_CODE[value] : undefined;
}

const WORKFLOW_STATUS_BY_CODE: Readonly<Record<number, WorkflowStatus>> = {
  1: "RUNNING",
  2: "COMPLETED",
  3: "FAILED",
  4: "CANCELED",
  5: "TERMINATED",
  6: "CONTINUED_AS_NEW",
  7: "TIMED_OUT",
};

const WORKFLOW_STATUSES = new Set<WorkflowStatus>(Object.values(WORKFLOW_STATUS_BY_CODE));

function timestampMilliseconds(value: unknown): number | undefined {
  const timestamp = recordOrUndefined(value);
  if (!timestamp || !Number.isInteger(timestamp.nanos)
    || (timestamp.nanos as number) < 0
    || (timestamp.nanos as number) > 999_999_999) {
    return undefined;
  }
  const secondsText = longText(timestamp.seconds);
  if (!secondsText || !/^-?(?:0|[1-9][0-9]*)$/.test(secondsText)) return undefined;
  const seconds = Number(secondsText);
  if (!Number.isSafeInteger(seconds)) return undefined;
  const milliseconds = seconds * 1_000 + Math.floor((timestamp.nanos as number) / 1_000_000);
  return Number.isSafeInteger(milliseconds) && cutoffMilliseconds(milliseconds)
    ? milliseconds
    : undefined;
}

function longText(value: unknown): string | undefined {
  if (typeof value === "number" && Number.isSafeInteger(value)) return String(value);
  if (typeof value === "string") return value;
  if (value && typeof value === "object" && "toString" in value
    && typeof value.toString === "function") {
    return value.toString();
  }
  return undefined;
}

function cutoffMilliseconds(value: unknown): value is number {
  return typeof value === "number"
    && Number.isSafeInteger(value)
    && !Object.is(value, -0)
    && value > 0
    && value <= MAX_CUTOFF_MS;
}

function validNamespace(value: unknown): value is string {
  return typeof value === "string"
    && value.length >= 1
    && value.length <= 255
    && /^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(value);
}

function cleanupPass(value: unknown): value is 1 | 2 {
  return value === 1 || value === 2;
}

function digest(value: unknown): value is string {
  return typeof value === "string" && /^[a-f0-9]{64}$/.test(value);
}

function opaqueId(value: unknown, maximum: number): value is string {
  return typeof value === "string"
    && value.length >= 20
    && value.length <= maximum
    && /^[A-Za-z0-9_-]+$/.test(value);
}

export function legacyWorkflowId(value: unknown): value is string {
  return typeof value === "string"
    && value.length >= 18
    && value.length <= 412
    && /^bluey-jobs:[A-Za-z0-9:_-]{3,200}:[A-Za-z0-9:_-]{3,200}$/.test(value);
}

function positiveSafeInteger(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value > 0;
}

function nonNegativeSafeInteger(value: unknown): value is number {
  return typeof value === "number"
    && Number.isSafeInteger(value)
    && !Object.is(value, -0)
    && value >= 0;
}

function sortedUniqueOpaqueIds(value: unknown, maximumItems: number): value is string[] {
  if (!Array.isArray(value) || value.length > maximumItems) return false;
  let previous: string | undefined;
  for (const item of value) {
    if (!opaqueId(item, 128) || (previous !== undefined && previous >= item)) return false;
    previous = item;
  }
  return true;
}

function assertInteger(value: number, minimum: number, maximum: number, label: string): void {
  if (!Number.isSafeInteger(value) || value < minimum || value > maximum) {
    throw new Error(`Invalid ${label}`);
  }
}

function exactRecord(value: unknown): Record<string, unknown> {
  const record = recordOrUndefined(value);
  if (!record) throw new Error("Invalid workflow cleanup request");
  return record;
}

function recordOrUndefined(value: unknown): Record<string, unknown> | undefined {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : undefined;
}

function sameKeys(record: Record<string, unknown>, expected: readonly string[]): boolean {
  const keys = Object.keys(record).sort();
  const sortedExpected = [...expected].sort();
  return keys.length === sortedExpected.length
    && keys.every((key, index) => key === sortedExpected[index]);
}

import {
  WorkflowExecutionAlreadyStartedError,
  WorkflowNotFoundError,
  WorkflowUpdateFailedError,
  WorkflowUpdateRPCTimeoutOrCancelledError,
  isGrpcDeadlineError,
  type WorkflowClient,
  type WorkflowExecutionDescription,
} from "@temporalio/client";
import type {
  WorkflowCommandAuthority,
  WorkflowGatewayCommand,
  WorkflowGatewayError,
  WorkflowGatewayErrorReason,
  WorkflowGatewayReceipt,
  WorkflowResumeCommandAuthority,
  WorkflowUpdateReceipt,
} from "./contracts.js";
import {
  applicationWorkflowV2,
  resolveInterventionUpdate,
} from "./workflows.js";

export const WORKFLOW_COMMAND_PATH = "/workflow-commands";
export const WORKFLOW_PROTOCOL_MEMO_KEY = "bluey_jobs_command_v2";
export const WORKFLOW_TYPE_V2 = "applicationWorkflowV2";

export interface GatewayServiceOptions {
  client: Pick<WorkflowClient, "start" | "getHandle" | "withDeadline">;
  taskQueue: string;
  describeAttempts?: number;
  rpcTimeoutMs?: number;
}

export interface GatewayServiceResult {
  status: number;
  body: WorkflowGatewayReceipt | WorkflowGatewayError;
}

export function createGatewayService(options: GatewayServiceOptions) {
  const describeAttempts = options.describeAttempts ?? 3;
  const rpcTimeoutMs = options.rpcTimeoutMs ?? 5_000;
  if (!Number.isSafeInteger(describeAttempts) || describeAttempts < 1 || describeAttempts > 8) {
    throw new Error("Invalid gateway describe retry limit");
  }
  if (!Number.isSafeInteger(rpcTimeoutMs) || rpcTimeoutMs < 100 || rpcTimeoutMs > 30_000) {
    throw new Error("Invalid gateway RPC timeout");
  }

  return {
    async execute(value: unknown): Promise<GatewayServiceResult> {
      let command: WorkflowGatewayCommand;
      try {
        command = parseWorkflowGatewayCommand(value);
      } catch {
        return gatewayError(400, "rejected", "invalid_request");
      }

      if (command.operation === "start") {
        return startWorkflow(options, command, describeAttempts, rpcTimeoutMs);
      }
      return resumeWorkflow(options, command, rpcTimeoutMs);
    },
  };
}

async function startWorkflow(
  options: GatewayServiceOptions,
  command: WorkflowGatewayCommand & { operation: "start" },
  describeAttempts: number,
  rpcTimeoutMs: number,
): Promise<GatewayServiceResult> {
  const authority = workflowAuthority(command);
  try {
    const handle = await temporalRpc(options, rpcTimeoutMs, () =>
      options.client.start(applicationWorkflowV2, {
        taskQueue: options.taskQueue,
        workflowId: command.workflowId,
        args: [authority],
        workflowIdConflictPolicy: "FAIL",
        workflowIdReusePolicy: "REJECT_DUPLICATE",
        memo: {
          [WORKFLOW_PROTOCOL_MEMO_KEY]: authority,
        },
      }));
    return gatewayReceipt(command, "accepted", handle.firstExecutionRunId);
  } catch (error) {
    if (!(error instanceof WorkflowExecutionAlreadyStartedError)) {
      return gatewayError(503, "delivery_unknown", "temporal_unavailable");
    }
  }

  let description: WorkflowExecutionDescription | undefined;
  for (let attempt = 0; attempt < describeAttempts; attempt += 1) {
    try {
      description = await temporalRpc(options, rpcTimeoutMs, () =>
        options.client.getHandle(command.workflowId).describe());
      break;
    } catch {
      // A start conflict proves an execution exists, but a failed Describe does
      // not prove which execution. Keep the dispatcher retrying the same bytes.
    }
  }
  if (!description) return gatewayError(503, "delivery_unknown", "describe_ambiguous");
  if (!exactDescriptionMatches(description, authority)) {
    return gatewayError(409, "identity_conflict", "identity_conflict");
  }
  const firstExecutionRunId = firstRunId(description);
  if (!firstExecutionRunId) {
    return gatewayError(503, "delivery_unknown", "describe_ambiguous");
  }
  return gatewayReceipt(command, "already_accepted", firstExecutionRunId);
}

async function resumeWorkflow(
  options: GatewayServiceOptions,
  command: WorkflowGatewayCommand & { operation: "resume"; interventionId: string },
  rpcTimeoutMs: number,
): Promise<GatewayServiceResult> {
  const resumeAuthority: WorkflowResumeCommandAuthority = {
    ...workflowAuthority(command),
    interventionId: command.interventionId,
  };
  try {
    const description = await temporalRpc(options, rpcTimeoutMs, () =>
      options.client.getHandle(command.workflowId).describe());
    if (!validV2Description(description, command.workflowId)) {
      return gatewayError(409, "identity_conflict", "identity_conflict");
    }
    const firstExecutionRunId = firstRunId(description);
    if (!firstExecutionRunId) {
      return gatewayError(503, "delivery_unknown", "describe_ambiguous");
    }
    const handle = options.client.getHandle(command.workflowId, undefined, {
      firstExecutionRunId,
    });
    if (description.status.name !== "RUNNING") {
      // A response-lost Update may have completed this workflow before the
      // dispatcher retries. Never send a new Update to a closed execution;
      // recover only the exact durable Update ID and require its full echo.
      try {
        const receipt = await temporalRpc(options, rpcTimeoutMs, () =>
          handle.getUpdateHandle<WorkflowUpdateReceipt>(command.requestId).result());
        if (!exactUpdateReceipt(receipt, resumeAuthority)) {
          return gatewayError(409, "identity_conflict", "identity_conflict");
        }
        return gatewayReceipt(command, "already_accepted", firstExecutionRunId);
      } catch (error) {
        if (error instanceof WorkflowNotFoundError) {
          return gatewayError(409, "rejected", "workflow_closed");
        }
        if (error instanceof WorkflowUpdateFailedError && updateRejectedByIdentity(error)) {
          return gatewayError(409, "identity_conflict", "identity_conflict");
        }
        return gatewayError(503, "delivery_unknown", "temporal_unavailable");
      }
    }
    let receipt: WorkflowUpdateReceipt;
    try {
      receipt = await temporalRpc(options, rpcTimeoutMs, () =>
        handle.executeUpdate(resolveInterventionUpdate, {
          updateId: command.requestId,
          args: [resumeAuthority],
        }));
    } catch (error) {
      if (error instanceof WorkflowUpdateFailedError && updateRejectedByIdentity(error)) {
        return gatewayError(409, "identity_conflict", "identity_conflict");
      }
      const workflowClosedDuringUpdate = error instanceof WorkflowNotFoundError;
      if (!workflowClosedDuringUpdate
        && !(error instanceof WorkflowUpdateRPCTimeoutOrCancelledError)
        && !isGrpcDeadlineError(error)) throw error;
      try {
        receipt = await temporalRpc(options, rpcTimeoutMs, () =>
          handle.getUpdateHandle<WorkflowUpdateReceipt>(command.requestId).result());
      } catch (recoveryError) {
        if (recoveryError instanceof WorkflowUpdateFailedError
          && updateRejectedByIdentity(recoveryError)) {
          return gatewayError(409, "identity_conflict", "identity_conflict");
        }
        if (recoveryError instanceof WorkflowNotFoundError && workflowClosedDuringUpdate) {
          // The target may have closed between Describe and executeUpdate. Only
          // an exact durable Update receipt proves acceptance; an exact absent
          // handle proves the closed execution cannot accept this command.
          return gatewayError(409, "rejected", "workflow_closed");
        }
        return gatewayError(503, "delivery_unknown", "temporal_unavailable");
      }
    }
    if (!exactUpdateReceipt(receipt, resumeAuthority)) {
      return gatewayError(409, "identity_conflict", "identity_conflict");
    }
    return gatewayReceipt(command, "accepted", firstExecutionRunId);
  } catch (error) {
    if (error instanceof WorkflowNotFoundError) {
      return gatewayError(404, "rejected", "workflow_not_found");
    }
    return gatewayError(503, "delivery_unknown", "temporal_unavailable");
  }
}

function temporalRpc<T>(
  options: GatewayServiceOptions,
  rpcTimeoutMs: number,
  operation: () => Promise<T>,
): Promise<T> {
  return options.client.withDeadline(Date.now() + rpcTimeoutMs, operation);
}

function updateRejectedByIdentity(error: WorkflowUpdateFailedError): boolean {
  const cause = error.cause as { type?: unknown } | undefined;
  return cause?.type === "identity_conflict" || cause?.type === "invalid_authority";
}

export function parseWorkflowGatewayCommand(value: unknown): WorkflowGatewayCommand {
  const record = exactRecord(value, "Invalid workflow command");
  const operation = record.operation;
  const expectedKeys = operation === "start"
    ? ["operation", "payloadDigest", "requestId", "schemaVersion", "workflowId"]
    : operation === "resume"
      ? [
        "interventionId",
        "operation",
        "payloadDigest",
        "requestId",
        "schemaVersion",
        "workflowId",
      ]
      : [];
  if (!sameKeys(record, expectedKeys)
    || record.schemaVersion !== 2
    || !opaqueId(record.requestId, 128)
    || !opaqueId(record.workflowId, 192)
    || !digest(record.payloadDigest)) {
    throw new Error("Invalid workflow command");
  }
  if (operation === "resume") {
    if (!opaqueId(record.interventionId, 128)) throw new Error("Invalid workflow command");
    return {
      schemaVersion: 2,
      operation,
      requestId: record.requestId,
      workflowId: record.workflowId,
      payloadDigest: record.payloadDigest,
      interventionId: record.interventionId,
    };
  }
  if (operation !== "start") throw new Error("Invalid workflow command");
  return {
    schemaVersion: 2,
    operation,
    requestId: record.requestId,
    workflowId: record.workflowId,
    payloadDigest: record.payloadDigest,
  };
}

function exactDescriptionMatches(
  description: WorkflowExecutionDescription,
  authority: WorkflowCommandAuthority,
): boolean {
  return description.type === WORKFLOW_TYPE_V2
    && description.workflowId === authority.workflowId
    && exactWorkflowAuthority(description.memo?.[WORKFLOW_PROTOCOL_MEMO_KEY], authority);
}

function validV2Description(
  description: WorkflowExecutionDescription,
  workflowId: string,
): boolean {
  const memo = recordOrUndefined(description.memo?.[WORKFLOW_PROTOCOL_MEMO_KEY]);
  return description.type === WORKFLOW_TYPE_V2
    && description.workflowId === workflowId
    && memo !== undefined
    && sameKeys(memo, ["payloadDigest", "requestId", "schemaVersion", "workflowId"])
    && memo.schemaVersion === 2
    && opaqueId(memo.requestId, 128)
    && memo.workflowId === workflowId
    && digest(memo.payloadDigest);
}

function exactWorkflowAuthority(value: unknown, expected: WorkflowCommandAuthority): boolean {
  const record = recordOrUndefined(value);
  return record !== undefined
    && sameKeys(record, ["payloadDigest", "requestId", "schemaVersion", "workflowId"])
    && record.schemaVersion === 2
    && record.requestId === expected.requestId
    && record.workflowId === expected.workflowId
    && record.payloadDigest === expected.payloadDigest;
}

function exactUpdateReceipt(
  value: WorkflowUpdateReceipt,
  expected: WorkflowResumeCommandAuthority,
): boolean {
  const record = recordOrUndefined(value);
  return record !== undefined
    && sameKeys(
      record,
      ["interventionId", "outcome", "payloadDigest", "requestId", "schemaVersion", "workflowId"],
    )
    && record.schemaVersion === 2
    && record.outcome === "accepted"
    && record.requestId === expected.requestId
    && record.workflowId === expected.workflowId
    && record.payloadDigest === expected.payloadDigest
    && record.interventionId === expected.interventionId;
}

function workflowAuthority(command: WorkflowGatewayCommand): WorkflowCommandAuthority {
  return {
    schemaVersion: 2,
    requestId: command.requestId,
    workflowId: command.workflowId,
    payloadDigest: command.payloadDigest,
  };
}

function gatewayReceipt(
  command: WorkflowGatewayCommand,
  outcome: WorkflowGatewayReceipt["outcome"],
  temporalRunId: string,
): GatewayServiceResult {
  if (!opaqueId(temporalRunId, 128)) {
    return gatewayError(503, "delivery_unknown", "describe_ambiguous");
  }
  return {
    status: 202,
    body: {
      schemaVersion: 2,
      outcome,
      requestId: command.requestId,
      workflowId: command.workflowId,
      payloadDigest: command.payloadDigest,
      temporalRunId,
      ...(command.operation === "resume" ? { interventionId: command.interventionId } : {}),
    },
  };
}

function gatewayError(
  status: number,
  outcome: WorkflowGatewayError["outcome"],
  reason: WorkflowGatewayErrorReason,
): GatewayServiceResult {
  return { status, body: { schemaVersion: 2, outcome, reason } };
}

function firstRunId(description: WorkflowExecutionDescription): string | undefined {
  const raw = description.raw.workflowExecutionInfo?.firstRunId;
  return opaqueId(raw, 128) ? raw : undefined;
}

function exactRecord(value: unknown, message: string): Record<string, unknown> {
  const record = recordOrUndefined(value);
  if (!record) throw new Error(message);
  return record;
}

function recordOrUndefined(value: unknown): Record<string, unknown> | undefined {
  return value && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : undefined;
}

function sameKeys(record: Record<string, unknown>, expected: readonly string[]): boolean {
  const keys = Object.keys(record).sort();
  return keys.length === expected.length && keys.every((key, index) => key === expected[index]);
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

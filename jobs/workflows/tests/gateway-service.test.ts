import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  ApplicationFailure,
  WorkflowExecutionAlreadyStartedError,
  WorkflowNotFoundError,
  WorkflowUpdateFailedError,
  WorkflowUpdateRPCTimeoutOrCancelledError,
  type WorkflowClient,
  type WorkflowExecutionDescription,
} from "@temporalio/client";
import {
  createGatewayService,
  parseWorkflowGatewayCommand,
  WORKFLOW_PROTOCOL_MEMO_KEY,
} from "../src/gateway-service.js";
import type {
  WorkflowGatewayCommand,
  WorkflowResumeCommandAuthority,
} from "../src/contracts.js";

const start = vi.fn();
const describeExecution = vi.fn();
const executeUpdate = vi.fn();
const updateResult = vi.fn();
const getUpdateHandle = vi.fn(() => ({ result: updateResult }));
const withDeadline = vi.fn(async (
  _deadline: number | Date,
  operation: () => Promise<unknown>,
) => operation());
const getHandle = vi.fn(() => ({
  describe: describeExecution,
  executeUpdate,
  getUpdateHandle,
}));

function service() {
  return createGatewayService({
    client: { start, getHandle, withDeadline } as unknown as Pick<
      WorkflowClient,
      "start" | "getHandle" | "withDeadline"
    >,
    taskQueue: "jobs-v2",
  });
}

function startCommand(): WorkflowGatewayCommand & { operation: "start" } {
  return {
    schemaVersion: 2,
    operation: "start",
    requestId: `wfreq-v2-${"a".repeat(32)}`,
    workflowId: `bluey-jobs-v2-${"b".repeat(32)}`,
    payloadDigest: "c".repeat(64),
  };
}

function resumeCommand(): WorkflowGatewayCommand & {
  operation: "resume";
  interventionId: string;
} {
  return {
    schemaVersion: 2,
    operation: "resume",
    requestId: `wfreq-v2-${"d".repeat(32)}`,
    workflowId: startCommand().workflowId,
    payloadDigest: "e".repeat(64),
    interventionId: `intervention-${"f".repeat(32)}`,
  };
}

function description(
  command = startCommand(),
  override: Partial<WorkflowExecutionDescription> = {},
): WorkflowExecutionDescription {
  return {
    type: "applicationWorkflowV2",
    workflowId: command.workflowId,
    runId: `run-${"a".repeat(32)}`,
    taskQueue: "jobs-v2",
    status: { code: 1, name: "RUNNING" },
    historyLength: 1,
    startTime: new Date(0),
    memo: {
      [WORKFLOW_PROTOCOL_MEMO_KEY]: {
        schemaVersion: 2,
        requestId: command.requestId,
        workflowId: command.workflowId,
        payloadDigest: command.payloadDigest,
      },
    },
    searchAttributes: {},
    typedSearchAttributes: {} as never,
    raw: {
      workflowExecutionInfo: {
        firstRunId: `first-run-${"a".repeat(32)}`,
      },
    },
    staticDetails: async () => undefined,
    staticSummary: async () => undefined,
    ...override,
  };
}

beforeEach(() => {
  start.mockReset();
  getHandle.mockClear();
  describeExecution.mockReset();
  executeUpdate.mockReset();
  getUpdateHandle.mockClear();
  updateResult.mockReset();
  withDeadline.mockClear();
});

describe("protocol-v2 workflow gateway", () => {
  it("starts a fresh opaque workflow with closed conflict and reuse policies", async () => {
    start.mockResolvedValueOnce({ firstExecutionRunId: `first-run-${"a".repeat(32)}` });
    const command = startCommand();

    await expect(service().execute(command)).resolves.toEqual({
      status: 202,
      body: {
        schemaVersion: 2,
        outcome: "accepted",
        requestId: command.requestId,
        workflowId: command.workflowId,
        payloadDigest: command.payloadDigest,
        temporalRunId: `first-run-${"a".repeat(32)}`,
      },
    });

    expect(start).toHaveBeenCalledWith(expect.any(Function), {
      taskQueue: "jobs-v2",
      workflowId: command.workflowId,
      args: [{
        schemaVersion: 2,
        requestId: command.requestId,
        workflowId: command.workflowId,
        payloadDigest: command.payloadDigest,
      }],
      workflowIdConflictPolicy: "FAIL",
      workflowIdReusePolicy: "REJECT_DUPLICATE",
      memo: {
        [WORKFLOW_PROTOCOL_MEMO_KEY]: {
          schemaVersion: 2,
          requestId: command.requestId,
          workflowId: command.workflowId,
          payloadDigest: command.payloadDigest,
        },
      },
    });
    expect(withDeadline).toHaveBeenCalledTimes(1);
    expect(withDeadline.mock.calls[0]?.[0]).toEqual(expect.any(Number));
    expect(Number(withDeadline.mock.calls[0]?.[0])).toBeLessThanOrEqual(Date.now() + 5_000);
  });

  it("accepts an exact already-started identity only after Describe", async () => {
    const command = startCommand();
    start.mockRejectedValueOnce(new WorkflowExecutionAlreadyStartedError(
      "already started",
      command.workflowId,
      "applicationWorkflowV2",
    ));
    describeExecution.mockResolvedValueOnce(description(command));

    const result = await service().execute(command);

    expect(result.status).toBe(202);
    expect(result.body).toMatchObject({
      outcome: "already_accepted",
      temporalRunId: `first-run-${"a".repeat(32)}`,
    });
    expect(describeExecution).toHaveBeenCalledTimes(1);
    expect(withDeadline).toHaveBeenCalledTimes(2);
  });

  it("rejects a same-ID execution whose type or memo identity differs", async () => {
    const command = startCommand();
    start.mockRejectedValueOnce(new WorkflowExecutionAlreadyStartedError(
      "already started",
      command.workflowId,
      "applicationWorkflowV2",
    ));
    describeExecution.mockResolvedValueOnce(description(command, {
      memo: {
        [WORKFLOW_PROTOCOL_MEMO_KEY]: {
          schemaVersion: 2,
          requestId: command.requestId,
          workflowId: command.workflowId,
          payloadDigest: "f".repeat(64),
        },
      },
    }));

    await expect(service().execute(command)).resolves.toEqual({
      status: 409,
      body: { schemaVersion: 2, outcome: "identity_conflict", reason: "identity_conflict" },
    });
  });

  it("keeps a conflict retryable when exact Describe remains ambiguous", async () => {
    const command = startCommand();
    start.mockRejectedValueOnce(new WorkflowExecutionAlreadyStartedError(
      "already started",
      command.workflowId,
      "applicationWorkflowV2",
    ));
    describeExecution.mockRejectedValue(new Error("private transport detail"));

    await expect(service().execute(command)).resolves.toEqual({
      status: 503,
      body: { schemaVersion: 2, outcome: "delivery_unknown", reason: "describe_ambiguous" },
    });
    expect(describeExecution).toHaveBeenCalledTimes(3);
    expect(withDeadline).toHaveBeenCalledTimes(4);
  });

  it("keeps an exact conflict ambiguous when first execution run ID is unavailable", async () => {
    const command = startCommand();
    start.mockRejectedValueOnce(new WorkflowExecutionAlreadyStartedError(
      "already started",
      command.workflowId,
      "applicationWorkflowV2",
    ));
    describeExecution.mockResolvedValueOnce(description(command, { raw: {} }));

    await expect(service().execute(command)).resolves.toEqual({
      status: 503,
      body: { schemaVersion: 2, outcome: "delivery_unknown", reason: "describe_ambiguous" },
    });
  });

  it("treats an exact closed workflow as already accepted rather than reusing its ID", async () => {
    const command = startCommand();
    start.mockRejectedValueOnce(new WorkflowExecutionAlreadyStartedError(
      "closed duplicate",
      command.workflowId,
      "applicationWorkflowV2",
    ));
    describeExecution.mockResolvedValueOnce(description(command, {
      status: { code: 2, name: "COMPLETED" },
    }));

    const result = await service().execute(command);

    expect(result.status).toBe(202);
    expect(result.body).toMatchObject({ outcome: "already_accepted" });
    expect(start).toHaveBeenCalledTimes(1);
  });

  it("executes a resume as an intervention-bound Update with request ID dedupe", async () => {
    const command = resumeCommand();
    describeExecution.mockResolvedValueOnce(description(startCommand()));
    executeUpdate.mockResolvedValueOnce({
      schemaVersion: 2,
      outcome: "accepted",
      requestId: command.requestId,
      workflowId: command.workflowId,
      payloadDigest: command.payloadDigest,
      interventionId: command.interventionId,
    });

    const result = await service().execute(command);

    expect(result).toEqual({
      status: 202,
      body: {
        schemaVersion: 2,
        outcome: "accepted",
        requestId: command.requestId,
        workflowId: command.workflowId,
        payloadDigest: command.payloadDigest,
        temporalRunId: `first-run-${"a".repeat(32)}`,
        interventionId: command.interventionId,
      },
    });
    expect(executeUpdate).toHaveBeenCalledWith(expect.anything(), {
      updateId: command.requestId,
      args: [{
        schemaVersion: 2,
        requestId: command.requestId,
        workflowId: command.workflowId,
        payloadDigest: command.payloadDigest,
        interventionId: command.interventionId,
      } satisfies WorkflowResumeCommandAuthority],
    });
    expect(withDeadline).toHaveBeenCalledTimes(2);
  });

  it("rejects a duplicate Update receipt whose exact echo differs", async () => {
    const command = resumeCommand();
    describeExecution.mockResolvedValueOnce(description(startCommand()));
    executeUpdate.mockResolvedValueOnce({
      schemaVersion: 2,
      outcome: "accepted",
      requestId: command.requestId,
      workflowId: command.workflowId,
      payloadDigest: "0".repeat(64),
      interventionId: command.interventionId,
    });

    await expect(service().execute(command)).resolves.toEqual({
      status: 409,
      body: { schemaVersion: 2, outcome: "identity_conflict", reason: "identity_conflict" },
    });
  });

  it("recovers the exact prior Update receipt after an RPC timeout", async () => {
    const command = resumeCommand();
    const exactReceipt = {
      schemaVersion: 2,
      outcome: "accepted",
      requestId: command.requestId,
      workflowId: command.workflowId,
      payloadDigest: command.payloadDigest,
      interventionId: command.interventionId,
    };
    describeExecution.mockResolvedValueOnce(description(startCommand()));
    executeUpdate.mockRejectedValueOnce(new WorkflowUpdateRPCTimeoutOrCancelledError("timeout"));
    updateResult.mockResolvedValueOnce(exactReceipt);

    const result = await service().execute(command);

    expect(result.status).toBe(202);
    expect(result.body).toMatchObject({ outcome: "accepted", requestId: command.requestId });
    expect(getUpdateHandle).toHaveBeenCalledWith(command.requestId);
    expect(updateResult).toHaveBeenCalledTimes(1);
    expect(withDeadline).toHaveBeenCalledTimes(3);
  });

  it("keeps an RPC-timeout resume retryable when no Update handle is visible yet", async () => {
    const command = resumeCommand();
    describeExecution.mockResolvedValueOnce(description(startCommand()));
    executeUpdate.mockRejectedValueOnce(new WorkflowUpdateRPCTimeoutOrCancelledError("timeout"));
    updateResult.mockRejectedValueOnce(new WorkflowNotFoundError(
      "update not visible",
      command.workflowId,
      undefined,
    ));

    await expect(service().execute(command)).resolves.toEqual({
      status: 503,
      body: {
        schemaVersion: 2,
        outcome: "delivery_unknown",
        reason: "temporal_unavailable",
      },
    });
    expect(getUpdateHandle).toHaveBeenCalledWith(command.requestId);
    expect(updateResult).toHaveBeenCalledTimes(1);
  });

  it("recovers an Update after the explicit gRPC deadline expires", async () => {
    const command = resumeCommand();
    const exactReceipt = {
      schemaVersion: 2,
      outcome: "accepted",
      requestId: command.requestId,
      workflowId: command.workflowId,
      payloadDigest: command.payloadDigest,
      interventionId: command.interventionId,
    };
    describeExecution.mockResolvedValueOnce(description(startCommand()));
    executeUpdate.mockRejectedValueOnce(Object.assign(new Error("deadline"), {
      code: 4,
      details: "deadline exceeded",
      metadata: {},
    }));
    updateResult.mockResolvedValueOnce(exactReceipt);

    await expect(service().execute(command)).resolves.toMatchObject({
      status: 202,
      body: { outcome: "accepted", requestId: command.requestId },
    });

    expect(getUpdateHandle).toHaveBeenCalledWith(command.requestId);
    expect(withDeadline).toHaveBeenCalledTimes(3);
  });

  it("rejects a recovered Update receipt with mismatched semantics", async () => {
    const command = resumeCommand();
    describeExecution.mockResolvedValueOnce(description(startCommand()));
    executeUpdate.mockRejectedValueOnce(new WorkflowUpdateRPCTimeoutOrCancelledError("timeout"));
    updateResult.mockResolvedValueOnce({
      schemaVersion: 2,
      outcome: "accepted",
      requestId: command.requestId,
      workflowId: command.workflowId,
      payloadDigest: "0".repeat(64),
      interventionId: command.interventionId,
    });

    await expect(service().execute(command)).resolves.toEqual({
      status: 409,
      body: { schemaVersion: 2, outcome: "identity_conflict", reason: "identity_conflict" },
    });
  });

  it("maps a workflow validator identity rejection to an exact conflict", async () => {
    const command = resumeCommand();
    describeExecution.mockResolvedValueOnce(description(startCommand()));
    executeUpdate.mockRejectedValueOnce(new WorkflowUpdateFailedError(
      "update rejected",
      ApplicationFailure.nonRetryable("identity_conflict", "identity_conflict"),
    ));

    await expect(service().execute(command)).resolves.toEqual({
      status: 409,
      body: { schemaVersion: 2, outcome: "identity_conflict", reason: "identity_conflict" },
    });
  });

  it("rejects a closed workflow only after proving the exact Update is absent", async () => {
    const command = resumeCommand();
    describeExecution.mockResolvedValueOnce(description(startCommand(), {
      status: { code: 2, name: "COMPLETED" },
    }));
    updateResult.mockRejectedValueOnce(new WorkflowNotFoundError(
      "update not found",
      command.workflowId,
      undefined,
    ));

    await expect(service().execute(command)).resolves.toEqual({
      status: 409,
      body: { schemaVersion: 2, outcome: "rejected", reason: "workflow_closed" },
    });
    expect(executeUpdate).not.toHaveBeenCalled();
    expect(getUpdateHandle).toHaveBeenCalledWith(command.requestId);
  });

  it("recovers an accepted Update after response loss and rapid workflow completion", async () => {
    const command = resumeCommand();
    describeExecution.mockResolvedValueOnce(description(startCommand(), {
      status: { code: 2, name: "COMPLETED" },
    }));
    updateResult.mockResolvedValueOnce({
      schemaVersion: 2,
      outcome: "accepted",
      requestId: command.requestId,
      workflowId: command.workflowId,
      payloadDigest: command.payloadDigest,
      interventionId: command.interventionId,
    });

    await expect(service().execute(command)).resolves.toEqual({
      status: 202,
      body: {
        schemaVersion: 2,
        outcome: "already_accepted",
        requestId: command.requestId,
        workflowId: command.workflowId,
        payloadDigest: command.payloadDigest,
        temporalRunId: `first-run-${"a".repeat(32)}`,
        interventionId: command.interventionId,
      },
    });
    expect(executeUpdate).not.toHaveBeenCalled();
    expect(getUpdateHandle).toHaveBeenCalledWith(command.requestId);
  });

  it("recovers an exact Update when the workflow closes after a running Describe", async () => {
    const command = resumeCommand();
    describeExecution.mockResolvedValueOnce(description(startCommand()));
    executeUpdate.mockRejectedValueOnce(new WorkflowNotFoundError(
      "workflow closed during update delivery",
      command.workflowId,
      undefined,
    ));
    updateResult.mockResolvedValueOnce({
      schemaVersion: 2,
      outcome: "accepted",
      requestId: command.requestId,
      workflowId: command.workflowId,
      payloadDigest: command.payloadDigest,
      interventionId: command.interventionId,
    });

    await expect(service().execute(command)).resolves.toEqual({
      status: 202,
      body: {
        schemaVersion: 2,
        outcome: "accepted",
        requestId: command.requestId,
        workflowId: command.workflowId,
        payloadDigest: command.payloadDigest,
        temporalRunId: `first-run-${"a".repeat(32)}`,
        interventionId: command.interventionId,
      },
    });
    expect(executeUpdate).toHaveBeenCalledTimes(1);
    expect(getUpdateHandle).toHaveBeenCalledWith(command.requestId);
    expect(updateResult).toHaveBeenCalledTimes(1);
  });

  it("fails closed when a closed workflow replays a mismatched Update receipt", async () => {
    const command = resumeCommand();
    describeExecution.mockResolvedValueOnce(description(startCommand(), {
      status: { code: 2, name: "COMPLETED" },
    }));
    updateResult.mockResolvedValueOnce({
      schemaVersion: 2,
      outcome: "accepted",
      requestId: command.requestId,
      workflowId: command.workflowId,
      payloadDigest: "0".repeat(64),
      interventionId: command.interventionId,
    });

    await expect(service().execute(command)).resolves.toEqual({
      status: 409,
      body: { schemaVersion: 2, outcome: "identity_conflict", reason: "identity_conflict" },
    });
    expect(executeUpdate).not.toHaveBeenCalled();
  });

  it("returns a closed not-found outcome for a missing resume target", async () => {
    const command = resumeCommand();
    describeExecution.mockRejectedValueOnce(new WorkflowNotFoundError(
      "not found",
      command.workflowId,
      undefined,
    ));

    await expect(service().execute(command)).resolves.toEqual({
      status: 404,
      body: { schemaVersion: 2, outcome: "rejected", reason: "workflow_not_found" },
    });
  });

  it.each([
    ["unknown field", { ...startCommand(), accountId: "private-account" }],
    ["missing field", { ...startCommand(), payloadDigest: undefined }],
    ["short request ID", { ...startCommand(), requestId: "wfreq-v2-short" }],
    ["uppercase digest", { ...startCommand(), payloadDigest: "A".repeat(64) }],
    ["legacy operation", { ...startCommand(), operation: "signal" }],
    ["start intervention", { ...startCommand(), interventionId: `intervention-${"a".repeat(32)}` }],
    ["resume without intervention", { ...resumeCommand(), interventionId: undefined }],
  ])("rejects malformed command: %s", async (_label, command) => {
    await expect(service().execute(command)).resolves.toEqual({
      status: 400,
      body: { schemaVersion: 2, outcome: "rejected", reason: "invalid_request" },
    });
    expect(start).not.toHaveBeenCalled();
    expect(getHandle).not.toHaveBeenCalled();
  });

  it("has a parser with no protocol-v1 or extra-field fallback", () => {
    expect(() => parseWorkflowGatewayCommand({
      ...startCommand(),
      schemaVersion: 1,
    })).toThrow("Invalid workflow command");
    expect(() => parseWorkflowGatewayCommand({
      ...startCommand(),
      packet: { secret: true },
    })).toThrow("Invalid workflow command");
  });

  it("requires a finite bounded Temporal RPC timeout", () => {
    expect(() => createGatewayService({
      client: { start, getHandle, withDeadline } as unknown as Pick<
        WorkflowClient,
        "start" | "getHandle" | "withDeadline"
      >,
      taskQueue: "jobs-v2",
      rpcTimeoutMs: 0,
    })).toThrow("Invalid gateway RPC timeout");
  });
});

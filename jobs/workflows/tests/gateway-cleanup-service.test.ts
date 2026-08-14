import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  WorkflowNotFoundError,
  type WorkflowClient,
  type WorkflowExecutionDescription,
} from "@temporalio/client";
import {
  cleanupEvidenceDigest,
  canonicalizeCleanupEvidence,
  createTemporalCleanupClient,
  createWorkflowCleanupService,
  parseWorkflowCleanupAuthority,
  type TemporalCleanupClient,
} from "../src/gateway-cleanup-service.js";
import { WORKFLOW_PROTOCOL_MEMO_KEY } from "../src/gateway-service.js";
import type { WorkflowCleanupAuthority } from "../src/contracts.js";

const RUN_A = `temporal-run-${"a".repeat(32)}`;
const RUN_B = `temporal-run-${"b".repeat(32)}`;
const describeExecution = vi.fn();
const terminateExecution = vi.fn();
const deleteExecution = vi.fn();
const historyProbe = vi.fn();
const listPage = vi.fn();
const withDeadline = vi.fn(async (
  _deadline: number | Date,
  operation: () => Promise<unknown>,
) => operation());
let now = 1_000;

const client: TemporalCleanupClient = {
  describe: describeExecution,
  terminate: terminateExecution,
  delete: deleteExecution,
  historyProbe,
  listPage,
  withDeadline,
};

function service(
  override: Partial<Parameters<typeof createWorkflowCleanupService>[0]> = {},
) {
  return createWorkflowCleanupService({
    client,
    rpcTimeoutMs: 500,
    visibilityConfirmationAgeMs: 100,
    now: () => now,
    ...override,
  });
}

function authority(firstExecutionRunId?: string): WorkflowCleanupAuthority {
  return {
    schemaVersion: 2,
    cleanupRequestId: `wfclean-v2-${"c".repeat(32)}`,
    generation: 7,
    targetSetDigest: "d".repeat(64),
    cleanupFence: 11,
    workflowId: `bluey-jobs-v2-${"e".repeat(32)}`,
    startRequestId: `wfreq-v2-${"f".repeat(32)}`,
    startPayloadDigest: "1".repeat(64),
    ...(firstExecutionRunId ? { firstExecutionRunId } : {}),
  };
}

function executionDescription(
  runId: string,
  firstExecutionRunId: string,
  status = "RUNNING",
  override: Partial<WorkflowExecutionDescription> = {},
): WorkflowExecutionDescription {
  const input = authority();
  return {
    type: "applicationWorkflowV2",
    workflowId: input.workflowId,
    runId,
    taskQueue: "jobs-v2",
    status: { code: 1, name: status },
    historyLength: 1,
    startTime: new Date(0),
    memo: {
      [WORKFLOW_PROTOCOL_MEMO_KEY]: {
        schemaVersion: 2,
        requestId: input.startRequestId,
        workflowId: input.workflowId,
        payloadDigest: input.startPayloadDigest,
      },
    },
    searchAttributes: {},
    typedSearchAttributes: {} as never,
    raw: { workflowExecutionInfo: { firstRunId: firstExecutionRunId } },
    staticDetails: async () => undefined,
    staticSummary: async () => undefined,
    ...override,
  };
}

function notFound(runId?: string): WorkflowNotFoundError {
  return new WorkflowNotFoundError("private not-found detail", authority().workflowId, runId);
}

beforeEach(() => {
  now = 1_000;
  describeExecution.mockReset().mockRejectedValue(notFound());
  terminateExecution.mockReset().mockResolvedValue(undefined);
  deleteExecution.mockReset().mockResolvedValue(undefined);
  historyProbe.mockReset().mockRejectedValue(notFound());
  listPage.mockReset().mockResolvedValue({ executions: [] });
  withDeadline.mockClear();
});

describe("protocol-v2 Temporal cleanup gateway", () => {
  it("keeps an initially absent target pending until an aged exact confirmation", async () => {
    const cleanup = service();
    const input = authority();

    const first = await cleanup.executeCleanup(input);
    now += 99;
    const early = await cleanup.executeCleanup(input);
    now += 1;
    const complete = await cleanup.executeCleanup(input);

    expect(first).toMatchObject({
      status: 202,
      body: { outcome: "pending", reason: "visibility_pending" },
    });
    expect(early).toMatchObject({
      status: 202,
      body: { outcome: "pending", reason: "visibility_pending" },
    });
    expect(complete).toMatchObject({
      status: 202,
      body: { ...input, outcome: "complete", reason: "absence_proved" },
    });
    expect(terminateExecution).not.toHaveBeenCalled();
    expect(deleteExecution).not.toHaveBeenCalled();
    expect(historyProbe.mock.calls).toEqual([
      [input.workflowId, undefined],
      [input.workflowId, undefined],
      [input.workflowId, undefined],
    ]);
  });

  it("terminates, deletes, and proves one exact running execution absent", async () => {
    const input = authority();
    describeExecution
      .mockResolvedValueOnce(executionDescription(RUN_A, RUN_A))
      .mockRejectedValueOnce(notFound());
    listPage
      .mockResolvedValueOnce({
        executions: [{ workflowId: input.workflowId, runId: RUN_A }],
      })
      .mockResolvedValueOnce({ executions: [] });

    const result = await service().executeCleanup(input);

    expect(result).toMatchObject({
      status: 202,
      body: {
        ...input,
        firstExecutionRunId: RUN_A,
        outcome: "complete",
        reason: "absence_proved",
        evidenceDigest: expect.stringMatching(/^[a-f0-9]{64}$/),
      },
    });
    expect(terminateExecution).toHaveBeenCalledWith(input.workflowId, RUN_A, RUN_A);
    expect(deleteExecution).toHaveBeenCalledWith(input.workflowId, RUN_A);
    expect(historyProbe).toHaveBeenCalledWith(input.workflowId, RUN_A);
    expect(describeExecution.mock.calls).toEqual([
      [input.workflowId, undefined],
      [input.workflowId, undefined],
      [input.workflowId, RUN_A],
    ]);
    expect(withDeadline).toHaveBeenCalledTimes(8);
    for (const [deadline] of withDeadline.mock.calls) {
      expect(Number(deadline)).toBe(1_500);
    }
  });

  it("exhausts visibility pages and deletes every specific run in one chain", async () => {
    const input = authority();
    const token = Uint8Array.from([1, 2, 3]);
    describeExecution
      .mockResolvedValueOnce(executionDescription(RUN_B, RUN_A))
      .mockResolvedValueOnce(executionDescription(RUN_A, RUN_A, "COMPLETED"))
      .mockRejectedValueOnce(notFound());
    listPage
      .mockResolvedValueOnce({
        executions: [{ workflowId: input.workflowId, runId: RUN_A }],
        nextPageToken: token,
      })
      .mockResolvedValueOnce({
        executions: [{ workflowId: input.workflowId, runId: RUN_B }],
      })
      .mockResolvedValueOnce({ executions: [] });

    const result = await service().executeCleanup(input);

    expect(result.body).toMatchObject({
      firstExecutionRunId: RUN_A,
      outcome: "complete",
      reason: "absence_proved",
    });
    expect(listPage.mock.calls[0]?.[1]).toEqual(new Uint8Array());
    expect(listPage.mock.calls[1]?.[1]).toEqual(token);
    expect(terminateExecution).toHaveBeenCalledTimes(1);
    expect(terminateExecution).toHaveBeenCalledWith(input.workflowId, RUN_B, RUN_A);
    expect(deleteExecution.mock.calls).toEqual([
      [input.workflowId, RUN_A],
      [input.workflowId, RUN_B],
    ]);
    expect(historyProbe.mock.calls).toEqual([
      [input.workflowId, RUN_A],
      [input.workflowId, RUN_B],
    ]);
  });

  it.each([
    ["workflow type", { type: "applicationWorkflow" }],
    ["workflow ID", { workflowId: `bluey-jobs-v2-${"9".repeat(32)}` }],
    ["run ID", { runId: "short" }],
    ["first run ID", { raw: { workflowExecutionInfo: { firstRunId: "short" } } }],
    ["status", { status: { code: 0, name: "UNSPECIFIED" } }],
    ["extra memo", {
      memo: {
        [WORKFLOW_PROTOCOL_MEMO_KEY]: {
          schemaVersion: 2,
          requestId: authority().startRequestId,
          workflowId: authority().workflowId,
          payloadDigest: authority().startPayloadDigest,
        },
        private: "forbidden",
      },
    }],
    ["memo digest", {
      memo: {
        [WORKFLOW_PROTOCOL_MEMO_KEY]: {
          schemaVersion: 2,
          requestId: authority().startRequestId,
          workflowId: authority().workflowId,
          payloadDigest: "0".repeat(64),
        },
      },
    }],
  ])("returns identity conflict for mismatched %s before mutation", async (_label, override) => {
    describeExecution.mockResolvedValueOnce(executionDescription(
      RUN_A,
      RUN_A,
      "RUNNING",
      override as Partial<WorkflowExecutionDescription>,
    ));

    const result = await service().executeCleanup(authority());

    expect(result).toEqual({
      status: 409,
      body: { schemaVersion: 2, outcome: "identity_conflict", reason: "identity_conflict" },
    });
    expect(terminateExecution).not.toHaveBeenCalled();
    expect(deleteExecution).not.toHaveBeenCalled();
  });

  it("rejects a supplied first-run mismatch before mutation", async () => {
    describeExecution.mockResolvedValueOnce(executionDescription(RUN_B, RUN_A));

    const result = await service().executeCleanup(authority(RUN_B));

    expect(result.status).toBe(409);
    expect(terminateExecution).not.toHaveBeenCalled();
    expect(deleteExecution).not.toHaveBeenCalled();
  });

  it("does not mutate when a newly visible run cannot be identity-validated", async () => {
    const input = authority();
    describeExecution
      .mockResolvedValueOnce(executionDescription(RUN_A, RUN_A))
      .mockRejectedValueOnce(notFound(RUN_B));
    listPage.mockResolvedValueOnce({
      executions: [
        { workflowId: input.workflowId, runId: RUN_A },
        { workflowId: input.workflowId, runId: RUN_B },
      ],
    });

    const result = await service().executeCleanup(input);

    expect(result.body).toMatchObject({ outcome: "pending", reason: "visibility_pending" });
    expect(terminateExecution).not.toHaveBeenCalled();
    expect(deleteExecution).not.toHaveBeenCalled();
  });

  it("rejects a visible execution from a different first-run chain before mutation", async () => {
    const input = authority();
    describeExecution
      .mockResolvedValueOnce(executionDescription(RUN_A, RUN_A))
      .mockResolvedValueOnce(executionDescription(RUN_B, RUN_B));
    listPage.mockResolvedValueOnce({
      executions: [
        { workflowId: input.workflowId, runId: RUN_A },
        { workflowId: input.workflowId, runId: RUN_B },
      ],
    });

    const result = await service().executeCleanup(input);

    expect(result.status).toBe(409);
    expect(terminateExecution).not.toHaveBeenCalled();
    expect(deleteExecution).not.toHaveBeenCalled();
  });

  it("fails a repeated cleanup request ID with changed authority closed", async () => {
    const cleanup = service();
    await cleanup.executeCleanup(authority());

    const result = await cleanup.executeCleanup({ ...authority(), generation: 8 });

    expect(result).toEqual({
      status: 409,
      body: { schemaVersion: 2, outcome: "identity_conflict", reason: "identity_conflict" },
    });
  });

  it("accepts the exact first-run binding echoed by an earlier request", async () => {
    const input = authority();
    const cleanup = service();
    describeExecution
      .mockResolvedValueOnce(executionDescription(RUN_A, RUN_A))
      .mockRejectedValue(notFound());
    listPage.mockResolvedValue({ executions: [] });

    const first = await cleanup.executeCleanup(input);
    const replay = await cleanup.executeCleanup({ ...input, firstExecutionRunId: RUN_A });

    expect(first.body).toMatchObject({ firstExecutionRunId: RUN_A, outcome: "complete" });
    expect(replay.body).toMatchObject({ firstExecutionRunId: RUN_A, outcome: "complete" });
  });

  it("never treats an empty successful History response as NotFound proof", async () => {
    const input = authority();
    describeExecution
      .mockResolvedValueOnce(executionDescription(RUN_A, RUN_A, "COMPLETED"))
      .mockRejectedValueOnce(notFound());
    listPage.mockResolvedValue({ executions: [] });
    historyProbe.mockResolvedValueOnce(undefined);

    const result = await service().executeCleanup(input);

    expect(result.body).toMatchObject({
      firstExecutionRunId: RUN_A,
      outcome: "pending",
      reason: "history_delete_pending",
    });
  });

  it("waits for eventual visibility after exact Describe and History absence", async () => {
    const input = authority();
    const cleanup = service();
    describeExecution
      .mockResolvedValueOnce(executionDescription(RUN_A, RUN_A))
      .mockRejectedValue(notFound());
    listPage
      .mockResolvedValueOnce({
        executions: [{ workflowId: input.workflowId, runId: RUN_A }],
      })
      .mockResolvedValueOnce({
        executions: [{ workflowId: input.workflowId, runId: RUN_A }],
      })
      .mockResolvedValueOnce({ executions: [] })
      .mockResolvedValueOnce({ executions: [] });

    const pending = await cleanup.executeCleanup(input);
    const complete = await cleanup.executeCleanup(input);

    expect(pending.body).toMatchObject({ outcome: "pending", reason: "visibility_pending" });
    expect(complete.body).toMatchObject({ outcome: "complete", reason: "absence_proved" });
    expect(deleteExecution).toHaveBeenCalledTimes(1);
  });

  it("returns termination_pending without deleting after a termination outage", async () => {
    const input = authority();
    describeExecution.mockResolvedValueOnce(executionDescription(RUN_A, RUN_A));
    listPage.mockResolvedValueOnce({ executions: [] });
    terminateExecution.mockRejectedValueOnce(new Error("private Temporal outage"));

    const result = await service().executeCleanup(input);

    expect(result.body).toMatchObject({
      firstExecutionRunId: RUN_A,
      outcome: "pending",
      reason: "termination_pending",
    });
    expect(deleteExecution).not.toHaveBeenCalled();
    expect(JSON.stringify(result)).not.toContain("private");
  });

  it("returns history_delete_pending after a bounded deletion outage", async () => {
    const input = authority();
    describeExecution.mockResolvedValueOnce(executionDescription(RUN_A, RUN_A, "COMPLETED"));
    listPage.mockResolvedValueOnce({ executions: [] });
    deleteExecution.mockRejectedValueOnce(new Error("private Temporal outage"));

    const result = await service().executeCleanup(input);

    expect(result.body).toMatchObject({ outcome: "pending", reason: "history_delete_pending" });
  });

  it("treats terminate and delete NotFound as idempotent before proving absence", async () => {
    const input = authority();
    describeExecution
      .mockResolvedValueOnce(executionDescription(RUN_A, RUN_A))
      .mockRejectedValue(notFound());
    terminateExecution.mockRejectedValueOnce(notFound(RUN_A));
    deleteExecution.mockRejectedValueOnce(notFound(RUN_A));

    const result = await service().executeCleanup(input);

    expect(result.body).toMatchObject({
      firstExecutionRunId: RUN_A,
      outcome: "complete",
      reason: "absence_proved",
    });
    expect(historyProbe).toHaveBeenCalledWith(input.workflowId, RUN_A);
  });

  it("does not complete when a bound run reappears during per-run Describe proof", async () => {
    const input = authority();
    describeExecution
      .mockResolvedValueOnce(executionDescription(RUN_A, RUN_A, "COMPLETED"))
      .mockRejectedValueOnce(notFound())
      .mockResolvedValueOnce(executionDescription(RUN_A, RUN_A, "COMPLETED"));

    const result = await service().executeCleanup(input);

    expect(result.body).toMatchObject({
      firstExecutionRunId: RUN_A,
      outcome: "pending",
      reason: "history_delete_pending",
    });
  });

  it("detects a repeated visibility page token and performs no mutation", async () => {
    const input = authority();
    const repeated = Uint8Array.from([7, 7, 7]);
    describeExecution.mockResolvedValueOnce(executionDescription(RUN_A, RUN_A));
    listPage
      .mockResolvedValueOnce({ executions: [], nextPageToken: repeated })
      .mockResolvedValueOnce({ executions: [], nextPageToken: repeated });

    const result = await service().executeCleanup(input);

    expect(result.body).toMatchObject({ outcome: "pending", reason: "temporal_unavailable" });
    expect(terminateExecution).not.toHaveBeenCalled();
    expect(deleteExecution).not.toHaveBeenCalled();
  });

  it("stops at the strict visibility page cap", async () => {
    const input = authority();
    describeExecution.mockRejectedValue(notFound());
    listPage.mockResolvedValue({
      executions: [],
      nextPageToken: Uint8Array.from([1]),
    });

    const result = await service({ visibilityMaxPages: 1 }).executeCleanup(input);

    expect(result.body).toMatchObject({ outcome: "pending", reason: "temporal_unavailable" });
    expect(listPage).toHaveBeenCalledTimes(1);
  });

  it("stops at the strict visibility execution cap before mutation", async () => {
    const input = authority();
    listPage.mockResolvedValueOnce({
      executions: [
        { workflowId: input.workflowId, runId: RUN_A },
        { workflowId: input.workflowId, runId: RUN_B },
      ],
    });

    const result = await service({ visibilityMaxExecutions: 1 }).executeCleanup(input);

    expect(result.body).toMatchObject({ outcome: "pending", reason: "temporal_unavailable" });
    expect(terminateExecution).not.toHaveBeenCalled();
    expect(deleteExecution).not.toHaveBeenCalled();
  });

  it.each([
    ["wrong workflow", { workflowId: `bluey-jobs-v2-${"9".repeat(32)}`, runId: RUN_A }],
    ["malformed run", { workflowId: authority().workflowId, runId: "short" }],
  ])("rejects a malformed visibility row: %s", async (_label, row) => {
    listPage.mockResolvedValueOnce({ executions: [row] });

    const result = await service().executeCleanup(authority());

    expect(result.body).toMatchObject({ outcome: "pending", reason: "temporal_unavailable" });
    expect(terminateExecution).not.toHaveBeenCalled();
    expect(deleteExecution).not.toHaveBeenCalled();
  });

  it("rejects an oversized visibility page token", async () => {
    listPage.mockResolvedValueOnce({
      executions: [],
      nextPageToken: new Uint8Array(4_097),
    });

    const result = await service().executeCleanup(authority());

    expect(result.body).toMatchObject({ outcome: "pending", reason: "temporal_unavailable" });
  });

  it("bounds a deadline failure and never invokes the underlying RPC", async () => {
    withDeadline.mockImplementationOnce(async () => {
      throw new Error("private deadline detail");
    });

    const result = await service().executeCleanup(authority());

    expect(result.body).toMatchObject({ outcome: "pending", reason: "temporal_unavailable" });
    expect(describeExecution).not.toHaveBeenCalled();
    expect(JSON.stringify(result)).not.toContain("private");
  });

  it("requires an aged second absence pass after restart with a DB-bound first-run", async () => {
    const input = authority(RUN_A);
    const firstProcess = service();
    const secondProcess = service();

    const firstObservation = await firstProcess.executeCleanup(input);
    now += 100;
    const firstComplete = await firstProcess.executeCleanup(input);
    const afterRestart = await secondProcess.executeCleanup(input);
    now += 100;
    const afterRestartComplete = await secondProcess.executeCleanup(input);

    expect(firstObservation.body).toMatchObject({
      firstExecutionRunId: RUN_A,
      outcome: "pending",
      reason: "visibility_pending",
    });
    expect(firstComplete.body).toMatchObject({ outcome: "complete", reason: "absence_proved" });
    expect(afterRestart.body).toMatchObject({
      firstExecutionRunId: RUN_A,
      outcome: "pending",
      reason: "visibility_pending",
    });
    expect(afterRestartComplete.body).toMatchObject({
      outcome: "complete",
      reason: "absence_proved",
    });
  });

  it("loses absence age conservatively after bounded-cache eviction", async () => {
    const cleanup = service({ authorityCacheLimit: 1 });
    const first = authority();
    const other = {
      ...authority(),
      cleanupRequestId: `wfclean-v2-${"8".repeat(32)}`,
      workflowId: `bluey-jobs-v2-${"7".repeat(32)}`,
      startRequestId: `wfreq-v2-${"6".repeat(32)}`,
      startPayloadDigest: "5".repeat(64),
    };

    await cleanup.executeCleanup(first);
    await cleanup.executeCleanup(other);
    now += 100;
    const afterEviction = await cleanup.executeCleanup(first);

    expect(afterEviction.body).toMatchObject({
      outcome: "pending",
      reason: "visibility_pending",
    });
  });

  it("computes the exact lexicographic canonical evidence digest", () => {
    const input = authority(RUN_A);
    const proof = {
      describe: "not_found" as const,
      history: "not_found" as const,
      visibility: "not_found" as const,
    };
    const evidence = {
      schemaVersion: 2,
      cleanupRequestId: input.cleanupRequestId,
      generation: input.generation,
      targetSetDigest: input.targetSetDigest,
      cleanupFence: input.cleanupFence,
      workflowId: input.workflowId,
      startRequestId: input.startRequestId,
      startPayloadDigest: input.startPayloadDigest,
      firstExecutionRunId: RUN_A,
      outcome: "complete",
      reason: "absence_proved",
      describe: "not_found",
      history: "not_found",
      visibility: "not_found",
    };
    const canonical = [
      `{"cleanupFence":11`,
      `"cleanupRequestId":"${input.cleanupRequestId}"`,
      `"describe":"not_found"`,
      `"firstExecutionRunId":"${RUN_A}"`,
      `"generation":7`,
      `"history":"not_found"`,
      `"outcome":"complete"`,
      `"reason":"absence_proved"`,
      `"schemaVersion":2`,
      `"startPayloadDigest":"${input.startPayloadDigest}"`,
      `"startRequestId":"${input.startRequestId}"`,
      `"targetSetDigest":"${input.targetSetDigest}"`,
      `"visibility":"not_found"`,
      `"workflowId":"${input.workflowId}"}`,
    ].join(",");

    expect(canonicalizeCleanupEvidence(evidence)).toBe(canonical);
    expect(cleanupEvidenceDigest(
      input,
      RUN_A,
      "complete",
      "absence_proved",
      proof,
    )).toBe("86feaf8ad74dfa80251f1ad3b337d8642e201ac6ae2b1da93abc4096a93fafac");
  });

  it.each([
    ["null", null],
    ["array", []],
    ["unknown field", { ...authority(), private: true }],
    ["explicit null run", { ...authority(), firstExecutionRunId: null }],
    ["short cleanup ID", { ...authority(), cleanupRequestId: "short" }],
    ["short workflow ID", { ...authority(), workflowId: "short" }],
    ["short start request", { ...authority(), startRequestId: "short" }],
    ["uppercase target digest", { ...authority(), targetSetDigest: "A".repeat(64) }],
    ["uppercase start digest", { ...authority(), startPayloadDigest: "A".repeat(64) }],
    ["zero generation", { ...authority(), generation: 0 }],
    ["fractional generation", { ...authority(), generation: 1.5 }],
    ["unsafe generation", { ...authority(), generation: Number.MAX_SAFE_INTEGER + 1 }],
    ["zero fence", { ...authority(), cleanupFence: 0 }],
    ["protocol v1", { ...authority(), schemaVersion: 1 }],
  ])("rejects malformed cleanup authority: %s", async (_label, value) => {
    const result = await service().executeCleanup(value);

    expect(result).toEqual({
      status: 400,
      body: { schemaVersion: 2, outcome: "rejected", reason: "invalid_request" },
    });
    expect(withDeadline).not.toHaveBeenCalled();
  });

  it("has a strict parser with no explicit-null optional fallback", () => {
    expect(parseWorkflowCleanupAuthority(authority())).toEqual(authority());
    expect(parseWorkflowCleanupAuthority(authority(RUN_A))).toEqual(authority(RUN_A));
    expect(() => parseWorkflowCleanupAuthority({
      ...authority(),
      firstExecutionRunId: null,
    })).toThrow("Invalid workflow cleanup authority");
  });

  it("uses exact per-run raw Temporal requests in the production adapter", async () => {
    const rawList = vi.fn(async () => ({
      executions: [{ execution: { workflowId: authority().workflowId, runId: RUN_A } }],
      nextPageToken: Uint8Array.from([9]),
    }));
    const rawDelete = vi.fn(async () => ({}));
    const rawHistory = vi.fn(async () => ({ history: { events: [] } }));
    const terminate = vi.fn(async () => ({}));
    const describe = vi.fn(async () => executionDescription(RUN_A, RUN_A));
    const getHandle = vi.fn(() => ({ terminate, describe }));
    const workflowClient = {
      options: { namespace: "jobs-namespace" },
      workflowService: {
        listWorkflowExecutions: rawList,
        deleteWorkflowExecution: rawDelete,
        getWorkflowExecutionHistory: rawHistory,
      },
      getHandle,
      withDeadline,
    } as unknown as WorkflowClient;
    const adapter = createTemporalCleanupClient(workflowClient);
    const token = Uint8Array.from([4]);

    await adapter.listPage(authority().workflowId, token, 25);
    await adapter.delete(authority().workflowId, RUN_A);
    await adapter.historyProbe(authority().workflowId, RUN_A);
    await adapter.terminate(authority().workflowId, RUN_A, RUN_A);

    expect(rawList).toHaveBeenCalledWith({
      namespace: "jobs-namespace",
      query: `WorkflowId = "${authority().workflowId}"`,
      pageSize: 25,
      nextPageToken: token,
    });
    expect(rawDelete).toHaveBeenCalledWith({
      namespace: "jobs-namespace",
      workflowExecution: { workflowId: authority().workflowId, runId: RUN_A },
    });
    expect(rawHistory).toHaveBeenCalledWith({
      namespace: "jobs-namespace",
      execution: { workflowId: authority().workflowId, runId: RUN_A },
      maximumPageSize: 1,
      waitNewEvent: false,
      skipArchival: false,
    });
    expect(getHandle).toHaveBeenLastCalledWith(
      authority().workflowId,
      RUN_A,
      { firstExecutionRunId: RUN_A },
    );
    expect(terminate).toHaveBeenCalledWith("bluey_jobs_cleanup_v2");
  });

  it("requires bounded cleanup configuration", () => {
    expect(() => service({ rpcTimeoutMs: 0 })).toThrow("Invalid cleanup RPC timeout");
    expect(() => service({ visibilityConfirmationAgeMs: 0 }))
      .toThrow("Invalid cleanup visibility confirmation age");
    expect(() => service({ authorityCacheLimit: 0 }))
      .toThrow("Invalid cleanup authority cache limit");
  });
});

import { readFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  WorkflowNotFoundError,
  type WorkflowClient,
} from "@temporalio/client";
import {
  MANAGED_CLOUD_RELEASE_MEMO_KEY,
  managedCloudReleaseMemoBytes,
} from "@bluey/jobs-automation/managed-cloud-execution";
import {
  WORKFLOW_CLEANUP_PAGE_SIZE,
  WORKFLOW_CLEANUP_MAX_RUN_IDS,
  canonicalizeWorkflowCleanupEvidence,
  createTemporalCleanupClient,
  createWorkflowCleanupService,
  legacyInventoryPageDigest,
  legacyInventoryQuery,
  legacyInventoryQueryDigest,
  legacyInventoryTargetsDigest,
  legacyWorkflowId,
  legacyWorkflowTargetDigest,
  parseWorkflowCleanupRequest,
  v2WorkflowTargetDigest,
  type TemporalCleanupClient,
  type TemporalCleanupExecution,
} from "../src/gateway-cleanup-service.js";
import { WORKFLOW_PROTOCOL_MEMO_KEY } from "../src/gateway-service.js";
import type {
  LegacyWorkflowInventoryPageRequest,
  ReconcileLegacyWorkflowTargetRequest,
  ReconcileV2WorkflowTargetRequest,
} from "../src/contracts.js";

const NAMESPACE = "bluey-jobs";
const CUTOFF_MS = 1_783_900_800_000;
const LEGACY_WORKFLOW_ID = "bluey-jobs:account_123:application-456";
const WORKFLOW_ID = `bluey-jobs-v2-${"a".repeat(32)}`;
const RUN_A = `temporal-run-${"b".repeat(32)}`;
const RUN_B = `temporal-run-${"c".repeat(32)}`;
const RUN_C = `temporal-run-${"d".repeat(32)}`;
const REQUEST_ID = `wfreq-v2-${"e".repeat(32)}`;
const PAYLOAD_DIGEST = "f".repeat(64);
const MANAGED_BINDING_DIGEST = "1".repeat(64);

const describeExecution = vi.fn();
const terminateExecution = vi.fn();
const deleteExecution = vi.fn();
const historyProbe = vi.fn();
const listPage = vi.fn();
const withDeadline = vi.fn(async (
  _deadline: number | Date,
  operation: () => Promise<unknown>,
) => operation());

const client: TemporalCleanupClient = {
  namespace: NAMESPACE,
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
    namespace: NAMESPACE,
    rpcTimeoutMs: 500,
    now: () => 1_000,
    ...override,
  });
}

function inventoryRequest(
  override: Partial<LegacyWorkflowInventoryPageRequest> = {},
): LegacyWorkflowInventoryPageRequest {
  const base: LegacyWorkflowInventoryPageRequest = {
    schemaVersion: 3,
    operation: "legacy_inventory_page",
    cleanupRequestId: `wfclean-v3-${"1".repeat(32)}`,
    inventoryGenerationId: `wfinventory-v3-${"2".repeat(32)}`,
    namespace: NAMESPACE,
    workflowType: "applicationWorkflow",
    visibilityCutoffMs: CUTOFF_MS,
    queryDigest: "0".repeat(64),
    scanPass: 1,
    pageIndex: 0,
    predecessorPageDigest: null,
    pageToken: null,
    cleanupFence: 7,
  };
  const merged = { ...base, ...override };
  if (!Object.prototype.hasOwnProperty.call(override, "queryDigest")) {
    merged.queryDigest = legacyInventoryQueryDigest(merged);
  }
  return merged;
}

function legacyRequest(
  override: Partial<ReconcileLegacyWorkflowTargetRequest> = {},
): ReconcileLegacyWorkflowTargetRequest {
  const base: ReconcileLegacyWorkflowTargetRequest = {
    schemaVersion: 3,
    operation: "reconcile_legacy_target",
    cleanupRequestId: `wfclean-v3-${"3".repeat(32)}`,
    inventoryGenerationId: `wfinventory-v3-${"2".repeat(32)}`,
    namespace: NAMESPACE,
    workflowType: "applicationWorkflow",
    visibilityCutoffMs: CUTOFF_MS,
    queryDigest: legacyInventoryQueryDigest({
      namespace: NAMESPACE,
      workflowType: "applicationWorkflow",
      visibilityCutoffMs: CUTOFF_MS,
    }),
    scanPass: 1,
    workflowId: LEGACY_WORKFLOW_ID,
    runId: RUN_A,
    firstExecutionRunId: RUN_A,
    targetDigest: "0".repeat(64),
    cleanupFence: 9,
    observationPass: 1,
  };
  const merged = { ...base, ...override };
  if (!Object.prototype.hasOwnProperty.call(override, "targetDigest")) {
    merged.targetDigest = legacyWorkflowTargetDigest(merged);
  }
  return merged;
}

function v2Request(
  override: Partial<ReconcileV2WorkflowTargetRequest> = {},
): ReconcileV2WorkflowTargetRequest {
  const base: ReconcileV2WorkflowTargetRequest = {
    schemaVersion: 3,
    operation: "reconcile_v2_target",
    cleanupRequestId: `wfclean-v3-${"4".repeat(32)}`,
    cleanupGenerationId: `wfgeneration-v3-${"5".repeat(32)}`,
    targetSetDigest: "6".repeat(64),
    namespace: NAMESPACE,
    workflowType: "applicationWorkflowV2",
    workflowId: WORKFLOW_ID,
    firstExecutionRunId: RUN_A,
    startRequestId: REQUEST_ID,
    startPayloadDigest: PAYLOAD_DIGEST,
    knownRunIds: [RUN_A],
    targetDigest: "0".repeat(64),
    cleanupFence: 11,
    observationPass: 1,
  };
  const merged = { ...base, ...override };
  if (!Object.prototype.hasOwnProperty.call(override, "targetDigest")) {
    merged.targetDigest = v2WorkflowTargetDigest(merged);
  }
  return merged;
}

function managedCloudCleanupAuthority(): Pick<
  ReconcileV2WorkflowTargetRequest,
  "managedCloudBindingSha256" | "managedCloudReleaseMemoBase64url" |
  "managedCloudReleaseMemoSha256"
> & { memoBytes: Buffer } {
  const memoBytes = Buffer.from(managedCloudReleaseMemoBytes({
    version: 1,
    bindingSha256: MANAGED_BINDING_DIGEST,
    scope: { environment: "staging", region: "us-east-1", channel: "canary" },
    headRevision: 7,
    transitionSha256: "2".repeat(64),
    activationSha256: "3".repeat(64),
    manifestSha256: "4".repeat(64),
    cohortSha256: "5".repeat(64),
    trustGeneration: 2,
    channelSequence: 8,
    releaseId: "managed-cloud-release-1234",
    releaseSequence: 4,
    taskQueueSha256: "6".repeat(64),
    failureConverterSha256: "7".repeat(64),
    readinessSha256: "8".repeat(64),
    activationExpiresAtMs: 1_800_000_000_000,
    resolvedAtMs: 1_750_000_000_000,
  }));
  return {
    managedCloudBindingSha256: MANAGED_BINDING_DIGEST,
    managedCloudReleaseMemoBase64url: memoBytes.toString("base64url"),
    managedCloudReleaseMemoSha256: createHash("sha256").update(memoBytes).digest("hex"),
    memoBytes,
  };
}

function rawMemo(
  override: Partial<Record<"schemaVersion" | "requestId" | "workflowId" | "payloadDigest", unknown>> = {},
  payloadOverride: Record<string, unknown> = {},
  managedCloudMemo?: Uint8Array,
): Record<string, unknown> {
  const value = {
    schemaVersion: 2,
    requestId: REQUEST_ID,
    workflowId: WORKFLOW_ID,
    payloadDigest: PAYLOAD_DIGEST,
    ...override,
  };
  const fields: Record<string, unknown> = {
    [WORKFLOW_PROTOCOL_MEMO_KEY]: {
      metadata: { encoding: Buffer.from("json/plain") },
      data: Buffer.from(JSON.stringify(value)),
      ...payloadOverride,
    },
  };
  if (managedCloudMemo) {
    fields[MANAGED_CLOUD_RELEASE_MEMO_KEY] = {
      metadata: { encoding: Buffer.from("json/plain") },
      data: Buffer.from(managedCloudMemo),
    };
  }
  return fields;
}

function execution(
  workflowType: "applicationWorkflow" | "applicationWorkflowV2",
  runId = RUN_A,
  status: unknown = "COMPLETED",
  override: Partial<TemporalCleanupExecution> = {},
): TemporalCleanupExecution {
  return {
    workflowId: workflowType === "applicationWorkflow" ? LEGACY_WORKFLOW_ID : WORKFLOW_ID,
    runId,
    firstExecutionRunId: RUN_A,
    workflowType,
    status,
    startTime: { seconds: Math.floor((CUTOFF_MS - 1_000) / 1_000), nanos: 0 },
    memoFields: workflowType === "applicationWorkflowV2" ? rawMemo() : undefined,
    ...override,
  };
}

function notFound(runId?: string): WorkflowNotFoundError {
  return new WorkflowNotFoundError("private not-found detail", WORKFLOW_ID, runId);
}

beforeEach(() => {
  describeExecution.mockReset().mockRejectedValue(notFound());
  terminateExecution.mockReset().mockResolvedValue(undefined);
  deleteExecution.mockReset().mockResolvedValue(undefined);
  historyProbe.mockReset().mockRejectedValue(notFound());
  listPage.mockReset().mockResolvedValue({ executions: [] });
  withDeadline.mockReset().mockImplementation(async (
    _deadline: number | Date,
    operation: () => Promise<unknown>,
  ) => operation());
});

describe("stateless Temporal cleanup protocol v3", () => {
  it("matches the cross-language canonical legacy query digest fixture", () => {
    expect(legacyInventoryQuery(CUTOFF_MS)).toBe('WorkflowType = "applicationWorkflow"');
    expect(legacyInventoryQueryDigest(inventoryRequest())).toBe(
      "3c9d936edbb8c1f09a56b2278079f97d40731bf0d3d51a939bf2670598a32bf2",
    );
  });

  it("admits only the exact historical legacy workflow ID envelope", async () => {
    const minimum = "bluey-jobs:abc:def";
    const maximum = `bluey-jobs:${"a".repeat(200)}:${"b".repeat(200)}`;
    expect(legacyWorkflowId(LEGACY_WORKFLOW_ID)).toBe(true);
    expect(legacyWorkflowId(minimum)).toBe(true);
    expect(minimum).toHaveLength(18);
    expect(legacyWorkflowId(maximum)).toBe(true);
    expect(maximum).toHaveLength(412);

    const invalid = [
      "bluey-jobs:ab:def",
      "bluey-jobs:abc:de",
      `bluey-jobs:${"a".repeat(201)}:def`,
      `bluey-jobs:abc:${"b".repeat(201)}`,
      "bluey-jobs:abc:def\"",
      "bluey-jobs:abc:def\\",
      `bluey-jobs-v2-${"a".repeat(32)}`,
    ];
    for (const workflowId of invalid) {
      expect(legacyWorkflowId(workflowId)).toBe(false);
      expect((await service().executeCleanup(legacyRequest({ workflowId }))).status).toBe(400);
    }

    for (const workflowId of ["bluey-jobs:abc:def\"", "bluey-jobs:abc:def\\"]) {
      listPage.mockReset().mockResolvedValueOnce({
        executions: [execution("applicationWorkflow", RUN_A, "COMPLETED", { workflowId })],
      });
      expect((await service().executeCleanup(inventoryRequest())).status).toBe(409);
    }
  });

  it("returns one exact bounded legacy page with sorted opaque targets", async () => {
    const request = inventoryRequest();
    listPage.mockResolvedValueOnce({
      executions: [
        execution("applicationWorkflow", RUN_B, 3),
        execution("applicationWorkflow", RUN_A, 1),
      ],
      nextPageToken: Uint8Array.from([1, 2, 3]),
    });

    const result = await service().executeCleanup(request);

    expect(result.status).toBe(202);
    expect(result.body).toMatchObject({
      ...request,
      outcome: "page",
      targets: [
        {
          workflowId: LEGACY_WORKFLOW_ID,
          runId: RUN_A,
          firstExecutionRunId: RUN_A,
          status: "RUNNING",
        },
        {
          workflowId: LEGACY_WORKFLOW_ID,
          runId: RUN_B,
          firstExecutionRunId: RUN_A,
          status: "FAILED",
        },
      ],
      nextPageToken: "AQID",
      exhausted: false,
      targetsDigest: expect.stringMatching(/^[a-f0-9]{64}$/),
      pageDigest: expect.stringMatching(/^[a-f0-9]{64}$/),
    });
    expect(listPage).toHaveBeenCalledWith(
      legacyInventoryQuery(CUTOFF_MS),
      new Uint8Array(),
      WORKFLOW_CLEANUP_PAGE_SIZE,
    );
    const body = result.body;
    if ("pageDigest" in body) {
      const { pageDigest, ...withoutDigest } = body;
      expect(pageDigest).toBe(legacyInventoryPageDigest(withoutDigest));
      expect(body.targetsDigest).toBe(legacyInventoryTargetsDigest(body.targets));
    }
  });

  it("decodes and echoes exact canonical page and predecessor authority", async () => {
    const request = inventoryRequest({
      pageIndex: 4,
      predecessorPageDigest: "7".repeat(64),
      pageToken: Buffer.from([9, 8]).toString("base64url"),
      scanPass: 2,
    });

    const result = await service().executeCleanup(request);

    expect(result.body).toMatchObject({
      ...request,
      outcome: "page",
      nextPageToken: null,
      exhausted: true,
    });
    expect(listPage.mock.calls[0]?.[1]).toEqual(Uint8Array.from([9, 8]));
  });

  it("rejects a repeated, malformed, or oversized provider page token", async () => {
    const request = inventoryRequest({
      pageIndex: 1,
      predecessorPageDigest: "8".repeat(64),
      pageToken: "AQID",
    });
    for (const nextPageToken of [
      Uint8Array.from([1, 2, 3]),
      new Uint8Array(4_097),
      "not-bytes",
    ]) {
      listPage.mockReset().mockResolvedValueOnce({ executions: [], nextPageToken });
      const result = await service().executeCleanup(request);
      expect(result).toEqual({
        status: 503,
        body: { schemaVersion: 3, outcome: "rejected", reason: "temporal_unavailable" },
      });
    }
  });

  it("rejects provider rows beyond the fixed page bound or duplicated in one page", async () => {
    listPage.mockResolvedValueOnce({
      executions: Array.from(
        { length: WORKFLOW_CLEANUP_PAGE_SIZE + 1 },
        (_, index) => execution("applicationWorkflow", `temporal-run-${String(index).padStart(32, "0")}`),
      ),
    });
    expect((await service().executeCleanup(inventoryRequest())).status).toBe(503);

    listPage.mockResolvedValueOnce({
      executions: [
        execution("applicationWorkflow", RUN_A),
        execution("applicationWorkflow", RUN_A),
      ],
    });
    expect((await service().executeCleanup(inventoryRequest())).status).toBe(503);
  });

  it.each([
    ["type", { workflowType: "applicationWorkflowV2" }],
    ["workflow ID", { workflowId: "short" }],
    ["run ID", { runId: "short" }],
    ["first run ID", { firstExecutionRunId: "short" }],
    ["missing first run ID", { firstExecutionRunId: null }],
    ["status", { status: 0 }],
    ["paused status", { status: 8 }],
    ["malformed timestamp", { startTime: { seconds: "private", nanos: 0 } }],
  ])("rejects a legacy inventory row with invalid %s", async (_label, override) => {
    listPage.mockResolvedValueOnce({
      executions: [execution("applicationWorkflow", RUN_A, "COMPLETED", override)],
    });

    const result = await service().executeCleanup(inventoryRequest());

    expect(result.status).toBe(409);
    expect(result.body).toEqual({
      schemaVersion: 3,
      outcome: "identity_conflict",
      reason: "identity_conflict",
    });
  });

  it("never inspects a legacy workflow memo or history payload", async () => {
    const privateMemo = Object.defineProperty({}, "privatePayload", {
      get: () => { throw new Error("private memo decoded"); },
    });
    listPage.mockResolvedValueOnce({
      executions: [execution("applicationWorkflow", RUN_A, "COMPLETED", {
        memoFields: privateMemo,
      })],
    });

    const result = await service().executeCleanup(inventoryRequest());

    expect(result.status).toBe(202);
    expect(JSON.stringify(result)).not.toContain("private");
  });

  it("includes a fresh post-cutoff legacy execution instead of manufacturing global zero", async () => {
    listPage.mockResolvedValueOnce({
      executions: [execution("applicationWorkflow", RUN_A, "RUNNING", {
        startTime: { seconds: CUTOFF_MS / 1_000 + 3_600, nanos: 0 },
      })],
    });

    const result = await service().executeCleanup(inventoryRequest());

    expect(result.body).toMatchObject({
      outcome: "page",
      exhausted: true,
      targets: [{ workflowId: LEGACY_WORKFLOW_ID, runId: RUN_A, status: "RUNNING" }],
    });
  });

  it("discovers a late continued-as-new v1 run on the second global scan", async () => {
    listPage
      .mockResolvedValueOnce({ executions: [] })
      .mockResolvedValueOnce({
        executions: [execution("applicationWorkflow", RUN_B, "RUNNING", {
          firstExecutionRunId: RUN_A,
          startTime: { seconds: CUTOFF_MS / 1_000 + 7_200, nanos: 0 },
        })],
      });

    const first = await service().executeCleanup(inventoryRequest({ scanPass: 1 }));
    const second = await service().executeCleanup(inventoryRequest({
      cleanupRequestId: `wfclean-v3-${"8".repeat(32)}`,
      scanPass: 2,
      cleanupFence: 8,
    }));

    expect(first.body).toMatchObject({ outcome: "page", targets: [], exhausted: true });
    expect(second.body).toMatchObject({
      outcome: "page",
      targets: [{
        workflowId: LEGACY_WORKFLOW_ID,
        runId: RUN_B,
        firstExecutionRunId: RUN_A,
        status: "RUNNING",
      }],
      exhausted: true,
    });
  });

  it("fails namespace, query, and target binding conflicts before Temporal I/O", async () => {
    for (const request of [
      inventoryRequest({ namespace: "other-namespace" }),
      inventoryRequest({ queryDigest: "9".repeat(64) }),
      legacyRequest({ targetDigest: "8".repeat(64) }),
      v2Request({ targetDigest: "7".repeat(64) }),
    ]) {
      const result = await service().executeCleanup(request);
      expect(result.status).toBe(409);
    }
    expect(withDeadline).not.toHaveBeenCalled();
  });

  it("returns a closed 400 for malformed union members and page chains", async () => {
    const overBoundRunIds = Array.from(
      { length: WORKFLOW_CLEANUP_MAX_RUN_IDS + 1 },
      (_, index) => `run-${String(index).padStart(4, "0")}-${"a".repeat(32)}`,
    );
    for (const request of [
      null,
      [],
      { ...inventoryRequest(), extra: "private" },
      { ...inventoryRequest(), schemaVersion: 2 },
      { ...inventoryRequest(), pageIndex: 1 },
      inventoryRequest({
        pageIndex: 4_096,
        predecessorPageDigest: "a".repeat(64),
        pageToken: "AQID",
      }),
      { ...inventoryRequest(), pageToken: "=" },
      { ...inventoryRequest(), cleanupFence: 0 },
      { ...inventoryRequest(), visibilityCutoffMs: 0 },
      { ...legacyRequest(), observationPass: 3 },
      { ...legacyRequest(), firstExecutionRunId: null },
      { ...v2Request(), firstExecutionRunId: undefined },
      { ...v2Request(), firstExecutionRunId: null },
      { ...v2Request(), knownRunIds: [RUN_B, RUN_A] },
      { ...v2Request(), knownRunIds: [RUN_A, RUN_A] },
      { ...v2Request(), knownRunIds: [] },
      v2Request({
        firstExecutionRunId: overBoundRunIds[0],
        knownRunIds: overBoundRunIds,
      }),
    ]) {
      const result = await service().executeCleanup(request);
      expect(result).toEqual({
        status: 400,
        body: { schemaVersion: 3, outcome: "rejected", reason: "invalid_request" },
      });
    }
    expect(withDeadline).not.toHaveBeenCalled();
  });

  it("accepts the final bounded legacy inventory page index", () => {
    const request = inventoryRequest({
      pageIndex: 4_095,
      predecessorPageDigest: "a".repeat(64),
      pageToken: "AQID",
    });

    expect(parseWorkflowCleanupRequest(request)).toEqual(request);
  });

  it("refuses a continuation beyond the final bounded inventory page", async () => {
    listPage.mockResolvedValueOnce({
      executions: [],
      nextPageToken: new Uint8Array([1, 2, 3]),
    });
    const request = inventoryRequest({
      pageIndex: 4_095,
      predecessorPageDigest: "a".repeat(64),
      pageToken: "BAUG",
    });

    expect(await service().executeCleanup(request)).toEqual({
      status: 503,
      body: { schemaVersion: 3, outcome: "rejected", reason: "temporal_unavailable" },
    });
  });

  it("keeps a running legacy execution pending and never terminates or deletes it", async () => {
    describeExecution.mockResolvedValueOnce(execution("applicationWorkflow", RUN_A, "RUNNING"));

    const result = await service().executeCleanup(legacyRequest());

    expect(result.body).toMatchObject({
      outcome: "pending",
      reason: "workflow_running",
      firstExecutionRunId: RUN_A,
      runIds: [RUN_A],
    });
    expect(terminateExecution).not.toHaveBeenCalled();
    expect(deleteExecution).not.toHaveBeenCalled();
  });

  it("deletes an exact closed legacy run then proves describe/history/visibility absence", async () => {
    describeExecution
      .mockResolvedValueOnce(execution("applicationWorkflow", RUN_A, "COMPLETED"))
      .mockRejectedValueOnce(notFound(RUN_A));

    const result = await service().executeCleanup(legacyRequest());

    expect(result.body).toMatchObject({
      outcome: "absence_observed",
      reason: "absence_observed",
      firstExecutionRunId: RUN_A,
      runIds: [RUN_A],
      evidenceDigest: expect.stringMatching(/^[a-f0-9]{64}$/),
    });
    expect(terminateExecution).not.toHaveBeenCalled();
    expect(deleteExecution).toHaveBeenCalledWith(LEGACY_WORKFLOW_ID, RUN_A);
    expect(historyProbe).toHaveBeenCalledWith(LEGACY_WORKFLOW_ID, RUN_A);
    expect(listPage).toHaveBeenCalledWith(
      `WorkflowId = "${LEGACY_WORKFLOW_ID}" AND RunId = "${RUN_A}"`,
      new Uint8Array(),
      WORKFLOW_CLEANUP_PAGE_SIZE,
    );
  });

  it("sorts opaque inventory identities by exact ASCII bytes", async () => {
    const upperWorkflowId = "bluey-jobs:AAA:component";
    const lowerWorkflowId = "bluey-jobs:aaa:component";
    listPage.mockResolvedValueOnce({
      executions: [
        execution("applicationWorkflow", RUN_A, "COMPLETED", {
          workflowId: lowerWorkflowId,
        }),
        execution("applicationWorkflow", RUN_B, "COMPLETED", {
          workflowId: upperWorkflowId,
        }),
      ],
    });

    const result = await service().executeCleanup(inventoryRequest());

    expect(result.body).toMatchObject({
      targets: [
        { workflowId: upperWorkflowId, runId: RUN_B },
        { workflowId: lowerWorkflowId, runId: RUN_A },
      ],
    });
  });

  it("requires exact legacy type, timestamp shape, status, and first-run binding before deletion", async () => {
    for (const override of [
      { workflowType: "applicationWorkflowV2" },
      { firstExecutionRunId: RUN_B },
      { status: "UNKNOWN" },
      { startTime: { seconds: "malformed", nanos: 0 } },
    ]) {
      describeExecution.mockReset().mockResolvedValueOnce(
        execution("applicationWorkflow", RUN_A, "COMPLETED", override),
      );
      expect((await service().executeCleanup(legacyRequest())).status).toBe(409);
    }
    expect(deleteExecution).not.toHaveBeenCalled();
  });

  it("keeps legacy history and visibility lag pending", async () => {
    historyProbe.mockResolvedValueOnce(undefined);
    let result = await service().executeCleanup(legacyRequest());
    expect(result.body).toMatchObject({ outcome: "pending", reason: "history_delete_pending" });

    historyProbe.mockRejectedValue(notFound());
    listPage.mockResolvedValueOnce({
      executions: [execution("applicationWorkflow", RUN_A, "COMPLETED")],
    });
    result = await service().executeCleanup(legacyRequest());
    expect(result.body).toMatchObject({ outcome: "pending", reason: "visibility_pending" });
  });

  it("is stateless across requests and leaves two-pass age authority to the database", async () => {
    const cleanup = service();
    const first = await cleanup.executeCleanup(legacyRequest({ observationPass: 1 }));
    const second = await cleanup.executeCleanup(legacyRequest({
      cleanupRequestId: `wfclean-v3-${"9".repeat(32)}`,
      cleanupFence: 10,
      observationPass: 2,
    }));

    expect(first.body).toMatchObject({ outcome: "absence_observed", observationPass: 1 });
    expect(second.body).toMatchObject({ outcome: "absence_observed", observationPass: 2 });
    expect(describeExecution).toHaveBeenCalledTimes(4);
    expect(historyProbe).toHaveBeenCalledTimes(2);
    expect(listPage).toHaveBeenCalledTimes(2);
  });

  it("keeps target digests stable across request leases and observation passes", () => {
    const legacy = legacyRequest();
    expect(legacyWorkflowTargetDigest({
      ...legacy,
      cleanupRequestId: `wfclean-v3-${"9".repeat(32)}`,
      cleanupFence: 99,
      observationPass: 2,
    })).toBe(legacy.targetDigest);
    const v2 = v2Request();
    expect(v2.targetDigest).toBe(
      "a0367cabb234f15fbc3089323607ae0e245b299cef72d86f6e04b7d42ea82d2b",
    );
    expect(v2WorkflowTargetDigest({
      ...v2,
      cleanupRequestId: `wfclean-v3-${"8".repeat(32)}`,
      cleanupFence: 100,
      observationPass: 2,
    })).toBe(v2.targetDigest);
  });

  it("terminates and deletes every exact verified run in one v2 chain", async () => {
    describeExecution
      .mockResolvedValueOnce(execution("applicationWorkflowV2", RUN_B, "RUNNING", {
        memoFields: rawMemo({}, { externalPayloads: [] }),
      }))
      .mockResolvedValueOnce(execution("applicationWorkflowV2", RUN_A, "COMPLETED"))
      .mockRejectedValueOnce(notFound(RUN_A))
      .mockRejectedValueOnce(notFound(RUN_B));
    listPage
      .mockResolvedValueOnce({
        executions: [
          execution("applicationWorkflowV2", RUN_A, "COMPLETED"),
          execution("applicationWorkflowV2", RUN_B, "RUNNING", {
            memoFields: rawMemo({}, { externalPayloads: [] }),
          }),
        ],
      })
      .mockResolvedValueOnce({ executions: [] });

    const result = await service().executeCleanup(v2Request({ knownRunIds: [RUN_A, RUN_B] }));

    expect(result.body).toMatchObject({
      outcome: "absence_observed",
      reason: "absence_observed",
      firstExecutionRunId: RUN_A,
      runIds: [RUN_A, RUN_B],
    });
    expect(terminateExecution).toHaveBeenCalledWith(WORKFLOW_ID, RUN_B, RUN_A);
    expect(deleteExecution.mock.calls).toEqual([
      [WORKFLOW_ID, RUN_A],
      [WORKFLOW_ID, RUN_B],
    ]);
    expect(historyProbe.mock.calls).toEqual([
      [WORKFLOW_ID, RUN_A],
      [WORKFLOW_ID, RUN_B],
    ]);
  });

  it("verifies the sole raw opaque v2 memo before every mutation", async () => {
    const exactMemo = {
      schemaVersion: 2,
      requestId: REQUEST_ID,
      workflowId: WORKFLOW_ID,
      payloadDigest: PAYLOAD_DIGEST,
    };
    const duplicateKeyMemo = [
      `{"schemaVersion":2,"requestId":${JSON.stringify(REQUEST_ID)}`,
      `,"requestId":${JSON.stringify(REQUEST_ID)}`,
      `,"workflowId":${JSON.stringify(WORKFLOW_ID)}`,
      `,"payloadDigest":${JSON.stringify(PAYLOAD_DIGEST)}}`,
    ].join("");
    for (const memoFields of [
      rawMemo({ payloadDigest: "0".repeat(64) }),
      { ...rawMemo(), private: { data: Buffer.from("private") } },
      rawMemo({}, { private: true }),
      rawMemo({}, { externalPayloads: [{ uri: "private" }] }),
      rawMemo({}, { metadata: {
        encoding: Buffer.from("json/plain"),
        private: Buffer.from("private"),
      } }),
      rawMemo({}, { data: Buffer.from(` ${JSON.stringify(exactMemo)}`) }),
      rawMemo({}, { data: Buffer.from(JSON.stringify({
        requestId: REQUEST_ID,
        schemaVersion: 2,
        workflowId: WORKFLOW_ID,
        payloadDigest: PAYLOAD_DIGEST,
      })) }),
      rawMemo({}, { data: Buffer.from(JSON.stringify({ ...exactMemo, extra: true })) }),
      rawMemo({}, { data: Buffer.from(duplicateKeyMemo) }),
      {
        [WORKFLOW_PROTOCOL_MEMO_KEY]: {
          metadata: { encoding: Buffer.from("binary/plain") },
          data: Buffer.from("private"),
        },
      },
      undefined,
    ]) {
      describeExecution.mockReset().mockResolvedValueOnce(
        execution("applicationWorkflowV2", RUN_A, "RUNNING", { memoFields }),
      );
      const result = await service().executeCleanup(v2Request());
      expect(result.status).toBe(409);
    }
    expect(terminateExecution).not.toHaveBeenCalled();
    expect(deleteExecution).not.toHaveBeenCalled();
  });

  it("binds the exact managed-cloud release memo without weakening historical cleanup", async () => {
    const { memoBytes, ...managedAuthority } = managedCloudCleanupAuthority();
    const request = v2Request(managedAuthority);
    describeExecution.mockResolvedValueOnce(
      execution("applicationWorkflowV2", RUN_A, "RUNNING", {
        memoFields: rawMemo({}, {}, memoBytes),
      }),
    );

    const exact = await service().executeCleanup(request);

    expect(exact.status).toBe(202);
    expect(terminateExecution).toHaveBeenCalledWith(WORKFLOW_ID, RUN_A, RUN_A);
    expect(deleteExecution).toHaveBeenCalledWith(WORKFLOW_ID, RUN_A);
    const parsedMemo = JSON.parse(memoBytes.toString("utf8")) as Record<string, unknown>;
    const reorderedMemo = Buffer.from(JSON.stringify({
      version: parsedMemo.version,
      ...parsedMemo,
    }));

    for (const [cleanupRequest, memoFields] of [
      [request, rawMemo()],
      [v2Request(), rawMemo({}, {}, memoBytes)],
      [request, rawMemo({}, {}, reorderedMemo)],
    ] as const) {
      describeExecution.mockReset().mockResolvedValueOnce(
        execution("applicationWorkflowV2", RUN_A, "RUNNING", { memoFields }),
      );
      terminateExecution.mockClear();
      deleteExecution.mockClear();
      const rejected = await service().executeCleanup(cleanupRequest);
      expect(rejected.status).toBe(409);
      expect(terminateExecution).not.toHaveBeenCalled();
      expect(deleteExecution).not.toHaveBeenCalled();
    }
  });

  it("does not mutate when a visible v2 run cannot be exactly described", async () => {
    describeExecution
      .mockRejectedValueOnce(notFound())
      .mockRejectedValueOnce(notFound(RUN_B));
    listPage.mockResolvedValueOnce({
      executions: [execution("applicationWorkflowV2", RUN_B, "COMPLETED")],
    });

    const result = await service().executeCleanup(v2Request());

    expect(result.body).toMatchObject({ outcome: "pending", reason: "visibility_pending" });
    expect(terminateExecution).not.toHaveBeenCalled();
    expect(deleteExecution).not.toHaveBeenCalled();
  });

  it("rejects a visible v2 memo mismatch even when per-run Describe is not found", async () => {
    describeExecution
      .mockRejectedValueOnce(notFound())
      .mockRejectedValueOnce(notFound(RUN_B));
    listPage.mockResolvedValueOnce({
      executions: [execution("applicationWorkflowV2", RUN_B, "COMPLETED", {
        memoFields: rawMemo({ requestId: `wfreq-v2-${"9".repeat(32)}` }),
      })],
    });

    const result = await service().executeCleanup(v2Request());

    expect(result.status).toBe(409);
    expect(terminateExecution).not.toHaveBeenCalled();
    expect(deleteExecution).not.toHaveBeenCalled();
  });

  it("rejects a newly visible v2 memo mismatch during final absence proof", async () => {
    describeExecution
      .mockResolvedValueOnce(execution("applicationWorkflowV2", RUN_A, "COMPLETED"))
      .mockRejectedValueOnce(notFound(RUN_A));
    listPage
      .mockResolvedValueOnce({
        executions: [execution("applicationWorkflowV2", RUN_A, "COMPLETED")],
      })
      .mockResolvedValueOnce({
        executions: [execution("applicationWorkflowV2", RUN_B, "COMPLETED", {
          memoFields: rawMemo({ requestId: `wfreq-v2-${"9".repeat(32)}` }),
        })],
      });

    const result = await service().executeCleanup(v2Request());

    expect(result.status).toBe(409);
    expect(terminateExecution).not.toHaveBeenCalled();
    expect(deleteExecution).toHaveBeenCalledOnce();
  });

  it("persists a null-to-bound v2 first run before permitting any mutation", async () => {
    describeExecution.mockResolvedValueOnce(
      execution("applicationWorkflowV2", RUN_B, "RUNNING"),
    );
    listPage.mockResolvedValueOnce({
      executions: [execution("applicationWorkflowV2", RUN_B, "RUNNING")],
    });

    const result = await service().executeCleanup(v2Request({
      firstExecutionRunId: null,
      knownRunIds: [],
    }));

    expect(result.body).toMatchObject({
      firstExecutionRunId: RUN_A,
      knownRunIds: [],
      runIds: [RUN_A, RUN_B],
      outcome: "pending",
      reason: "visibility_pending",
    });
    expect(terminateExecution).not.toHaveBeenCalled();
    expect(deleteExecution).not.toHaveBeenCalled();
  });

  it("does not introduce response-loss identities with a visibility outage receipt", async () => {
    describeExecution.mockResolvedValueOnce(
      execution("applicationWorkflowV2", RUN_B, "RUNNING"),
    );
    listPage.mockRejectedValueOnce(new Error("private visibility outage"));

    const result = await service().executeCleanup(v2Request());

    expect(result.body).toMatchObject({
      firstExecutionRunId: RUN_A,
      knownRunIds: [RUN_A],
      runIds: [RUN_A],
      outcome: "pending",
      reason: "temporal_unavailable",
    });
    expect(terminateExecution).not.toHaveBeenCalled();
    expect(deleteExecution).not.toHaveBeenCalled();
  });

  it("rolls back visible identity introductions when per-run Describe is unavailable", async () => {
    describeExecution
      .mockRejectedValueOnce(notFound())
      .mockRejectedValueOnce(new Error("private describe outage"));
    listPage.mockResolvedValueOnce({
      executions: [execution("applicationWorkflowV2", RUN_B, "COMPLETED")],
    });

    const result = await service().executeCleanup(v2Request({
      firstExecutionRunId: null,
      knownRunIds: [],
    }));

    expect(result.body).toMatchObject({
      firstExecutionRunId: null,
      knownRunIds: [],
      runIds: [],
      outcome: "pending",
      reason: "temporal_unavailable",
    });
    expect(terminateExecution).not.toHaveBeenCalled();
    expect(deleteExecution).not.toHaveBeenCalled();
  });

  it("proves a never-created v2 workflow absent without mutation", async () => {
    const result = await service().executeCleanup(v2Request({
      firstExecutionRunId: null,
      knownRunIds: [],
    }));

    expect(result.body).toMatchObject({
      outcome: "absence_observed",
      reason: "absence_observed",
      firstExecutionRunId: null,
      runIds: [],
    });
    expect(historyProbe).toHaveBeenCalledWith(WORKFLOW_ID, undefined);
    expect(terminateExecution).not.toHaveBeenCalled();
    expect(deleteExecution).not.toHaveBeenCalled();
  });

  it("re-proves every DB-bound v2 run after earlier deletion and gateway restart", async () => {
    const result = await service().executeCleanup(v2Request({
      knownRunIds: [RUN_A, RUN_B],
    }));

    expect(result.body).toMatchObject({
      knownRunIds: [RUN_A, RUN_B],
      runIds: [RUN_A, RUN_B],
      outcome: "absence_observed",
      reason: "absence_observed",
    });
    expect(describeExecution.mock.calls).toEqual([
      [WORKFLOW_ID, undefined],
      [WORKFLOW_ID, RUN_A],
      [WORKFLOW_ID, RUN_B],
      [WORKFLOW_ID, RUN_A],
      [WORKFLOW_ID, RUN_B],
    ]);
    expect(historyProbe.mock.calls).toEqual([
      [WORKFLOW_ID, RUN_A],
      [WORKFLOW_ID, RUN_B],
    ]);
  });

  it("returns the monotonic union of DB-known and newly visible v2 run IDs", async () => {
    describeExecution
      .mockResolvedValueOnce(execution("applicationWorkflowV2", RUN_C, "COMPLETED"))
      .mockRejectedValue(notFound());
    listPage.mockResolvedValueOnce({
      executions: [execution("applicationWorkflowV2", RUN_C, "COMPLETED")],
    });

    const result = await service().executeCleanup(v2Request({ knownRunIds: [RUN_A, RUN_B] }));

    expect(result.body).toMatchObject({
      knownRunIds: [RUN_A, RUN_B],
      runIds: [RUN_A, RUN_B, RUN_C],
      outcome: "pending",
      reason: "visibility_pending",
    });
    expect(terminateExecution).not.toHaveBeenCalled();
    expect(deleteExecution).not.toHaveBeenCalled();
  });

  it("keeps over-bound v2 discovery pending without emitting an invalid run set", async () => {
    const maximumKnownRunIds = Array.from(
      { length: WORKFLOW_CLEANUP_MAX_RUN_IDS },
      (_, index) => `run-${String(index).padStart(4, "0")}-${"a".repeat(32)}`,
    );
    describeExecution.mockResolvedValueOnce(execution(
      "applicationWorkflowV2",
      `run-z-${"b".repeat(32)}`,
      "COMPLETED",
      { firstExecutionRunId: maximumKnownRunIds[0] },
    ));

    const result = await service().executeCleanup(v2Request({
      firstExecutionRunId: maximumKnownRunIds[0],
      knownRunIds: maximumKnownRunIds,
    }));

    expect(result.body).toMatchObject({
      firstExecutionRunId: maximumKnownRunIds[0],
      knownRunIds: maximumKnownRunIds,
      runIds: maximumKnownRunIds,
      outcome: "pending",
      reason: "temporal_unavailable",
    });
    expect(listPage).not.toHaveBeenCalled();
    expect(terminateExecution).not.toHaveBeenCalled();
    expect(deleteExecution).not.toHaveBeenCalled();
  });

  it("keeps v2 termination and history deletion failures pending", async () => {
    describeExecution.mockResolvedValue(execution("applicationWorkflowV2", RUN_A, "RUNNING"));
    listPage.mockResolvedValueOnce({
      executions: [execution("applicationWorkflowV2", RUN_A, "RUNNING")],
    });
    terminateExecution.mockRejectedValueOnce(new Error("private outage"));
    let result = await service().executeCleanup(v2Request());
    expect(result.body).toMatchObject({ outcome: "pending", reason: "termination_pending" });
    expect(deleteExecution).not.toHaveBeenCalled();

    describeExecution.mockReset().mockResolvedValue(
      execution("applicationWorkflowV2", RUN_A, "COMPLETED"),
    );
    listPage.mockReset().mockResolvedValueOnce({
      executions: [execution("applicationWorkflowV2", RUN_A, "COMPLETED")],
    });
    deleteExecution.mockRejectedValueOnce(new Error("private outage"));
    result = await service().executeCleanup(v2Request());
    expect(result.body).toMatchObject({ outcome: "pending", reason: "history_delete_pending" });
  });

  it("rejects repeated target visibility tokens without mutation", async () => {
    const token = Uint8Array.from([5, 5]);
    describeExecution.mockRejectedValueOnce(notFound());
    listPage
      .mockResolvedValueOnce({ executions: [], nextPageToken: token })
      .mockResolvedValueOnce({ executions: [], nextPageToken: token });

    const result = await service().executeCleanup(v2Request());

    expect(result.body).toMatchObject({ outcome: "pending", reason: "temporal_unavailable" });
    expect(terminateExecution).not.toHaveBeenCalled();
    expect(deleteExecution).not.toHaveBeenCalled();
  });

  it("bounds every raw Temporal RPC with the configured deadline", async () => {
    await service().executeCleanup(inventoryRequest());

    expect(withDeadline).toHaveBeenCalledTimes(1);
    expect(withDeadline.mock.calls[0]?.[0]).toBe(1_500);
  });

  it("stops before mutation when the total request deadline is exhausted", async () => {
    let now = 0;
    const boundedClient: TemporalCleanupClient = {
      ...client,
      withDeadline: async (_deadline, operation) => {
        const result = await operation();
        now = 1_000;
        return result;
      },
    };
    describeExecution.mockResolvedValueOnce(
      execution("applicationWorkflowV2", RUN_A, "RUNNING"),
    );

    const result = await service({
      client: boundedClient,
      now: () => now,
      requestTimeoutMs: 1_000,
    }).executeCleanup(v2Request());

    expect(result.body).toMatchObject({ outcome: "pending", reason: "temporal_unavailable" });
    expect(terminateExecution).not.toHaveBeenCalled();
    expect(deleteExecution).not.toHaveBeenCalled();
  });

  it("uses raw Temporal service calls and never a decoding workflow handle", async () => {
    const info = {
      execution: { workflowId: WORKFLOW_ID, runId: RUN_A },
      firstRunId: RUN_A,
      type: { name: "applicationWorkflowV2" },
      status: 1,
      startTime: { seconds: 1, nanos: 0 },
      memo: { fields: rawMemo() },
    };
    const rawDescribe = vi.fn(async () => ({ workflowExecutionInfo: info }));
    const rawList = vi.fn(async () => ({ executions: [info], nextPageToken: Uint8Array.from([1]) }));
    const rawTerminate = vi.fn(async () => ({}));
    const rawDelete = vi.fn(async () => ({}));
    const rawHistory = vi.fn(async () => ({ history: { events: [] } }));
    const workflowClient = {
      options: { namespace: NAMESPACE },
      workflowService: {
        describeWorkflowExecution: rawDescribe,
        listWorkflowExecutions: rawList,
        terminateWorkflowExecution: rawTerminate,
        deleteWorkflowExecution: rawDelete,
        getWorkflowExecutionHistory: rawHistory,
      },
      withDeadline,
    } as unknown as WorkflowClient;
    const adapter = createTemporalCleanupClient(workflowClient);

    await adapter.describe(WORKFLOW_ID, RUN_A);
    await adapter.listPage("exact query", Uint8Array.from([2]), 100);
    await adapter.terminate(WORKFLOW_ID, RUN_A, RUN_A);
    await adapter.delete(WORKFLOW_ID, RUN_A);
    await adapter.historyProbe(WORKFLOW_ID, RUN_A);

    expect(rawDescribe).toHaveBeenCalledWith({
      namespace: NAMESPACE,
      execution: { workflowId: WORKFLOW_ID, runId: RUN_A },
    });
    expect(rawList).toHaveBeenCalledWith({
      namespace: NAMESPACE,
      query: "exact query",
      pageSize: 100,
      nextPageToken: Uint8Array.from([2]),
    });
    expect(rawTerminate).toHaveBeenCalledWith({
      namespace: NAMESPACE,
      workflowExecution: { workflowId: WORKFLOW_ID, runId: RUN_A },
      firstExecutionRunId: RUN_A,
      reason: "bluey_jobs_cleanup_v3",
    });
    expect(rawHistory).toHaveBeenCalledWith({
      namespace: NAMESPACE,
      execution: { workflowId: WORKFLOW_ID, runId: RUN_A },
      maximumPageSize: 1,
      waitNewEvent: false,
      skipArchival: false,
    });
  });

  it("contains no cleanup logger, process cache, or high-level payload-decoding handle", () => {
    const source = readFileSync(
      new URL("../src/gateway-cleanup-service.ts", import.meta.url),
      "utf8",
    );
    expect(source).not.toContain("console.");
    expect(source).not.toContain("getHandle(");
    expect(source).not.toContain("new Map<string, CleanupObservation>");
    expect(source).toContain("workflowService.getWorkflowExecutionHistory");
  });

  it("has a strict parser and lexicographic canonicalizer", () => {
    expect(parseWorkflowCleanupRequest(inventoryRequest())).toEqual(inventoryRequest());
    expect(parseWorkflowCleanupRequest(legacyRequest())).toEqual(legacyRequest());
    expect(parseWorkflowCleanupRequest(v2Request())).toEqual(v2Request());
    const { memoBytes: _, ...managedAuthority } = managedCloudCleanupAuthority();
    const managedRequest = v2Request(managedAuthority);
    expect(parseWorkflowCleanupRequest(managedRequest)).toEqual(managedRequest);
    expect(() => parseWorkflowCleanupRequest({
      ...v2Request(),
      managedCloudBindingSha256: MANAGED_BINDING_DIGEST,
    })).toThrow("Invalid workflow cleanup request");
    expect(canonicalizeWorkflowCleanupEvidence({ z: [2, { b: true, a: null }], a: "x" }))
      .toBe('{"a":"x","z":[2,{"a":null,"b":true}]}');
    const maximumKnownRunIds = Array.from(
      { length: WORKFLOW_CLEANUP_MAX_RUN_IDS },
      (_, index) => `run-${String(index).padStart(4, "0")}-${"a".repeat(32)}`,
    );
    expect(parseWorkflowCleanupRequest(v2Request({
      firstExecutionRunId: maximumKnownRunIds[0],
      knownRunIds: maximumKnownRunIds,
    }))).toMatchObject({ knownRunIds: maximumKnownRunIds });
  });

  it("keeps a worst-case maximum-run v2 receipt within 128 KiB", async () => {
    const maximumKnownRunIds = Array.from(
      { length: WORKFLOW_CLEANUP_MAX_RUN_IDS },
      (_, index) => `run-${String(index).padStart(4, "0")}-${"a".repeat(119)}`,
    );

    const result = await service().executeCleanup(v2Request({
      firstExecutionRunId: maximumKnownRunIds[0],
      knownRunIds: maximumKnownRunIds,
    }));
    const serializedReceipt = JSON.stringify(result.body);

    expect(result.body).toMatchObject({
      outcome: "absence_observed",
      knownRunIds: maximumKnownRunIds,
      runIds: maximumKnownRunIds,
    });
    expect(Buffer.byteLength(serializedReceipt)).toBe(9_376);
    expect(Buffer.byteLength(serializedReceipt)).toBeLessThanOrEqual(128 * 1024);
  });

  it("requires exact namespace and bounded service configuration", () => {
    expect(() => service({ namespace: "other" })).toThrow("namespace");
    expect(() => service({ rpcTimeoutMs: 0 })).toThrow("RPC timeout");
    expect(() => service({ requestTimeoutMs: 999 })).toThrow("request timeout");
    expect(() => service({ requestTimeoutMs: 12_001 })).toThrow("request timeout");
    expect(() => service({ visibilityMaxPages: 0 })).toThrow("page limit");
    expect(() => service({ visibilityMaxExecutions: 0 })).toThrow("execution limit");
  });
});

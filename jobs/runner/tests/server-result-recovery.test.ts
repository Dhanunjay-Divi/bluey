import { randomBytes } from "node:crypto";
import { mkdtemp, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it, vi } from "vitest";
import {
  approvedExecutionChecksum,
  type ApplicationPacket,
  type NormalizedJob,
} from "@bluey/jobs-automation";
import { profilePaths } from "../src/profile-store.js";
import type { ActiveExecutionLease } from "../src/execution-lease.js";
import { ExecutionLeaseError } from "../src/execution-lease.js";
import { LeasedRunError } from "../src/leased-run.js";
import {
  readResultState,
  ResultStoreError,
  stageResult,
  writeResult,
} from "../src/result-store.js";
import {
  cloudCheckpointScope,
  removeRunCheckpoint,
  writeRunCheckpoint,
  type CloudRunCheckpoint,
} from "../src/run-checkpoint-store.js";
import {
  assertWorkflowCommandCheckpointProfileScope,
  authoritativeRunnerResult,
  createDurableSideEffectUnknownAuthority,
  createDurableFailedRunResult,
  DurableFailedRunResultError,
  DurableSideEffectUnknownError,
  leaveLeasedRunRetryable,
  persistAndAbortSideEffectUnknown,
  persistAndFinishFailedRun,
  publicRunnerFailureResponse,
  recoverDurableSideEffectUnknown,
  recoverDurableRunResult,
  requestIdMatchesRun,
  restoreSafeWorkflowCommandCheckpoint,
  stabilizeSafeWorkflowCommandCheckpoint,
  stabilizeSideEffectUnknownCheckpoint,
  TerminalResultPersistenceError,
} from "../src/server.js";

describe("runner durable result recovery", () => {
  it("reconciles only an expired restore rejected by current lease authority", async () => {
    const expired = { expiresAtMs: Date.now() - 1 };
    const current = { expiresAtMs: Date.now() + 60_000 };
    const rejected = new ExecutionLeaseError("claim", "lease_unavailable", 409);
    const reconcile = vi.fn(async () => {});

    await expect(restoreSafeWorkflowCommandCheckpoint(expired, {
      restore: async () => { throw rejected; },
      reconcile,
    })).resolves.toBeUndefined();
    expect(reconcile).toHaveBeenCalledOnce();

    for (const [checkpoint, error] of [
      [current, rejected],
      [expired, new ExecutionLeaseError("claim", "timed_out")],
      [expired, new ExecutionLeaseError("heartbeat", "lease_unavailable", 409)],
    ] as const) {
      const forbiddenReconcile = vi.fn(async () => {});
      await expect(restoreSafeWorkflowCommandCheckpoint(checkpoint, {
        restore: async () => { throw error; },
        reconcile: forbiddenReconcile,
      })).rejects.toBe(error);
      expect(forbiddenReconcile).not.toHaveBeenCalled();
    }
  });

  it("rejects a workflow checkpoint placed in another profile scope", () => {
    const requestId = "wfreq-v2-12345678-1234-5678-9234-123456789abc";
    const lookup = ambiguityLookup(requestId);
    const exactScope = profilePaths(
      "/tmp/bluey-runner-profile-scope-test",
      lookup.accountId,
      lookup.applicationIdentityId,
    ).scope;
    const checkpoint = ambiguityCheckpoint(exactScope, lookup);

    expect(() => assertWorkflowCommandCheckpointProfileScope(checkpoint)).not.toThrow();
    expect(() => assertWorkflowCommandCheckpointProfileScope({
      ...checkpoint,
      profileScope: "0".repeat(40),
    })).toThrow(ResultStoreError);
  });

  it("echoes only the exact requested V2 authority on canonical 200 results", () => {
    const requestId = "wfreq-v2-12345678-1234-5678-9234-123456789abc";
    const staleRequestId = "wfreq-v2-22345678-1234-5678-9234-123456789abc";
    const stored = createDurableFailedRunResult(ambiguityLookup(requestId));

    expect(authoritativeRunnerResult(stored, requestId)).toEqual({
      ...stored,
      requestId,
    });
    expect(stored).not.toHaveProperty("requestId");
    expect(() =>
      authoritativeRunnerResult(
        { ...stored, requestId: staleRequestId },
        requestId,
      ),
    ).toThrow("does not match");
  });

  it("returns a closed exact ambiguity result only for a v2 command request", () => {
    const requestId = "wfreq-v2-12345678-1234-5678-9234-123456789abc";

    expect(
      publicRunnerFailureResponse(
        new DurableSideEffectUnknownError(),
        requestId,
      ),
    ).toEqual({
      status: 500,
      code: "side_effect_unknown",
      body: { schemaVersion: 2, outcome: "side_effect_unknown", requestId },
    });
    expect(
      publicRunnerFailureResponse(
        new LeasedRunError("side_effect_unknown"),
        requestId,
      ),
    ).toMatchObject({
      status: 500,
      code: "side_effect_unknown",
      body: { error: expect.any(String) },
    });
    expect(
      publicRunnerFailureResponse(
        new DurableSideEffectUnknownError(),
        "run-123:initial",
      ),
    ).toMatchObject({
      status: 500,
      code: "side_effect_unknown",
      body: { error: expect.any(String) },
    });
    expect(
      publicRunnerFailureResponse(new LeasedRunError("failed"), requestId),
    ).toMatchObject({
      status: 500,
      code: "failed",
      body: { error: expect.any(String) },
    });
    expect(
      publicRunnerFailureResponse(
        new ResultStoreError("result_promotion_conflict"),
        requestId,
      ),
    ).toMatchObject({
      status: 500,
      code: "result_store_result_promotion_conflict",
      body: { error: expect.any(String) },
    });
  });

  it("accepts the frozen v2 request authority without deriving it from the private run ID", () => {
    expect(
      requestIdMatchesRun(
        "run-private-123",
        "wfreq-v2-12345678-1234-5678-9234-123456789abc",
      ),
    ).toBe(true);
    expect(
      requestIdMatchesRun(
        "run-private-123",
        "wfreq-v2-12345678-1234-5678-9234-123456789abc",
        true,
      ),
    ).toBe(true);
    expect(
      requestIdMatchesRun("run-private-123", "wfreq:v2:unsafe-authority"),
    ).toBe(false);
    expect(
      requestIdMatchesRun(
        "run-private-123",
        "wfreq-v2-12345678-1234-4678-9234-123456789abc",
      ),
    ).toBe(false);
    expect(
      requestIdMatchesRun(
        "run-private-123",
        "wfreq-v2-12345678-1234-5678-7234-123456789abc",
      ),
    ).toBe(false);
    expect(
      requestIdMatchesRun(
        "run-private-123",
        "wfreq-v2-12345678-1234-5678-9234-123456789abc-extra",
      ),
    ).toBe(false);
    expect(
      requestIdMatchesRun(
        "run-private-123",
        "WFREQ-v2-12345678-1234-5678-9234-123456789abc",
      ),
    ).toBe(false);
    expect(
      requestIdMatchesRun("run-private-123", "another-command-authority"),
    ).toBe(false);
  });

  it("recovers the exact v2 ambiguity authority from an encrypted checkpoint", async () => {
    const dataRoot = await mkdtemp(join(tmpdir(), "bluey-runner-ambiguity-"));
    const encryptionKey = randomBytes(32);
    const requestId = "wfreq-v2-12345678-1234-5678-9234-123456789abc";
    const lookup = ambiguityLookup(requestId);
    const profileScope = profilePaths(
      dataRoot,
      lookup.accountId,
      lookup.applicationIdentityId,
    ).scope;
    await writeRunCheckpoint(
      dataRoot,
      ambiguityCheckpoint(profileScope, lookup),
      encryptionKey,
    );

    const recoveredRequestId = await recoverDurableSideEffectUnknown(
      dataRoot,
      encryptionKey,
      lookup,
    );
    expect(recoveredRequestId).toBe(requestId);
    const response = publicRunnerFailureResponse(
      new DurableSideEffectUnknownError(),
      recoveredRequestId,
    );
    expect(response).toEqual({
      status: 500,
      code: "side_effect_unknown",
      body: { schemaVersion: 2, outcome: "side_effect_unknown", requestId },
    });
    expect(Object.keys(response.body).sort()).toEqual([
      "outcome",
      "requestId",
      "schemaVersion",
    ]);
    expect(JSON.stringify(response.body)).not.toContain(lookup.accountId);
    expect(JSON.stringify(response.body)).not.toContain(lookup.applicationId);
  });

  it("converts startup ambiguity into a minimal purgeable result tombstone", async () => {
    const dataRoot = await mkdtemp(
      join(tmpdir(), "bluey-runner-ambiguity-restart-"),
    );
    const encryptionKey = randomBytes(32);
    const requestId = "wfreq-v2-12345678-1234-5678-9234-123456789abc";
    const lookup = ambiguityLookup(requestId);
    const profileScope = profilePaths(
      dataRoot,
      lookup.accountId,
      lookup.applicationIdentityId,
    ).scope;
    const checkpoint = ambiguityCheckpoint(profileScope, lookup);
    await writeRunCheckpoint(dataRoot, checkpoint, encryptionKey);
    const order: string[] = [];

    await expect(
      stabilizeSideEffectUnknownCheckpoint(checkpoint, {
        async persist() {
          order.push("persist");
          await writeResult(
            dataRoot,
            { profileScope, requestId },
            createDurableSideEffectUnknownAuthority(
              checkpoint.request,
              requestId,
            ),
            encryptionKey,
          );
        },
        async reconcile() {
          order.push("reconcile");
        },
        async remove() {
          order.push("remove");
          await removeRunCheckpoint(
            dataRoot,
            profileScope,
            lookup.browserSessionId,
          );
        },
      }),
    ).resolves.toBe(true);

    expect(order).toEqual(["persist", "reconcile", "remove"]);
    await expect(
      recoverDurableSideEffectUnknown(dataRoot, encryptionKey, lookup),
    ).resolves.toBe(requestId);
    const tombstone = await recoverDurableRunResult(
      dataRoot,
      encryptionKey,
      lookup,
    );
    expect(tombstone).toEqual({
      schemaVersion: 2,
      outcome: "side_effect_unknown",
      ...lookup,
    });
    expect(Object.keys(tombstone as object).sort()).toEqual(
      [
        "accountId",
        "applicationId",
        "applicationIdentityId",
        "browserSessionId",
        "outcome",
        "requestId",
        "runId",
        "schemaVersion",
      ].sort(),
    );
    expect(JSON.stringify(tombstone)).not.toContain("private@example.test");
    expect(JSON.stringify(tombstone)).not.toContain("private answer");
    expect(
      publicRunnerFailureResponse(
        new DurableSideEffectUnknownError(),
        requestId,
      ),
    ).toEqual({
      status: 500,
      code: "side_effect_unknown",
      body: { schemaVersion: 2, outcome: "side_effect_unknown", requestId },
    });
  });

  it("retires a hard-restarted v2 intervention checkpoint whose result write was lost", async () => {
    const dataRoot = await mkdtemp(
      join(tmpdir(), "bluey-runner-safe-checkpoint-missing-result-"),
    );
    const encryptionKey = randomBytes(32);
    const requestId = "wfreq-v2-12345678-1234-5678-9234-123456789abc";
    const lookup = ambiguityLookup(requestId);
    const profileScope = profilePaths(
      dataRoot,
      lookup.accountId,
      lookup.applicationIdentityId,
    ).scope;
    const checkpoint = {
      ...ambiguityCheckpoint(profileScope, lookup),
      phase: "provider_review" as const,
      expiresAtMs: Date.now() + 60_000,
      workflow: {
        status: "provider_review" as const,
        requestId,
        providerReview: { adapter: "greenhouse" },
      },
    };
    const context = { profileScope, requestId };
    const order: string[] = [];
    await writeRunCheckpoint(dataRoot, checkpoint, encryptionKey);

    await expect(
      stabilizeSafeWorkflowCommandCheckpoint(checkpoint, {
        read: () => readResultState(dataRoot, context, encryptionKey),
        async persistFailed(result) {
          order.push("persist_failed");
          await writeResult(dataRoot, context, result, encryptionKey);
        },
        async restore() {
          order.push("restore");
        },
        async retire() {
          order.push("retire");
          await removeRunCheckpoint(
            dataRoot,
            profileScope,
            lookup.browserSessionId,
          );
        },
      }),
    ).resolves.toBe("retired");

    expect(order).toEqual(["persist_failed", "retire"]);
    const recovered = await recoverDurableRunResult(
      dataRoot,
      encryptionKey,
      lookup,
    );
    expect(recovered).toEqual(createDurableFailedRunResult(checkpoint.request));
    expect(Object.keys(recovered as object).sort()).toEqual(
      [
        "accountId",
        "applicationId",
        "applicationIdentityId",
        "browserSessionId",
        "receipt",
        "runId",
      ].sort(),
    );
    expect(JSON.stringify(recovered)).not.toContain("private@example.test");
    expect(JSON.stringify(recovered)).not.toContain("private answer");
    expect(order).not.toContain("restore");
  });

  it("restores only an exact committed needs-input result after a hard restart", async () => {
    const dataRoot = await mkdtemp(
      join(tmpdir(), "bluey-runner-safe-checkpoint-committed-result-"),
    );
    const encryptionKey = randomBytes(32);
    const requestId = "wfreq-v2-12345678-1234-5678-9234-123456789abc";
    const lookup = ambiguityLookup(requestId);
    const profileScope = profilePaths(
      dataRoot,
      lookup.accountId,
      lookup.applicationIdentityId,
    ).scope;
    const checkpoint = {
      ...ambiguityCheckpoint(profileScope, lookup),
      phase: "needs_input" as const,
      expiresAtMs: Date.now() + 60_000,
      workflow: { status: "needs_input" as const, requestId },
    };
    const context = { profileScope, requestId };
    const result = {
      accountId: lookup.accountId,
      applicationId: lookup.applicationId,
      applicationIdentityId: lookup.applicationIdentityId,
      browserSessionId: lookup.browserSessionId,
      runId: lookup.runId,
      receipt: {
        status: "needs_input" as const,
        issues: [],
        intervention: {
          kind: "browser_takeover" as const,
          title: "Review the application",
          detail: "Review the provider form before continuing.",
        },
      },
    };
    await writeRunCheckpoint(dataRoot, checkpoint, encryptionKey);
    await writeResult(dataRoot, context, result, encryptionKey);
    const restore = vi.fn(async () => {});
    const retire = vi.fn(async () => {});
    const persistFailed = vi.fn(async () => {});

    await expect(
      stabilizeSafeWorkflowCommandCheckpoint(checkpoint, {
        read: () => readResultState(dataRoot, context, encryptionKey),
        persistFailed,
        restore,
        retire,
      }),
    ).resolves.toBe("restored");
    expect(restore).toHaveBeenCalledOnce();
    expect(persistFailed).not.toHaveBeenCalled();
    expect(retire).not.toHaveBeenCalled();
  });

  it("keeps an expired committed intervention recoverable until publication", async () => {
    const requestId = "wfreq-v2-12345678-1234-5678-9234-123456789abc";
    const lookup = ambiguityLookup(requestId);
    const checkpoint = {
      ...ambiguityCheckpoint("a".repeat(40), lookup),
      phase: "needs_input" as const,
      expiresAtMs: Date.now() - 1,
      workflow: { status: "needs_input" as const, requestId },
    };
    const result = {
      accountId: lookup.accountId,
      applicationId: lookup.applicationId,
      applicationIdentityId: lookup.applicationIdentityId,
      browserSessionId: lookup.browserSessionId,
      runId: lookup.runId,
      receipt: { status: "needs_input" as const, issues: [] },
    };
    const restore = vi.fn(async () => {});
    const retire = vi.fn(async () => {});

    await expect(stabilizeSafeWorkflowCommandCheckpoint(checkpoint, {
      read: async () => ({ state: "committed", result }),
      persistFailed: vi.fn(async () => {}),
      restore,
      retire,
    })).resolves.toBe("restored");
    expect(restore).toHaveBeenCalledOnce();
    expect(retire).not.toHaveBeenCalled();
  });

  it("never restores staged, cross-bound, or terminal safe-checkpoint results", async () => {
    const requestId = "wfreq-v2-12345678-1234-5678-9234-123456789abc";
    const lookup = ambiguityLookup(requestId);
    const checkpoint = {
      ...ambiguityCheckpoint("a".repeat(40), lookup),
      phase: "needs_input" as const,
      expiresAtMs: Date.now() + 60_000,
      workflow: { status: "needs_input" as const, requestId },
    };
    const needsInput = {
      accountId: lookup.accountId,
      applicationId: lookup.applicationId,
      applicationIdentityId: lookup.applicationIdentityId,
      browserSessionId: lookup.browserSessionId,
      runId: lookup.runId,
      receipt: { status: "needs_input" as const, issues: [] },
    };
    const restore = vi.fn(async () => {});
    const retire = vi.fn(async () => {});
    const persistFailed = vi.fn(async () => {});

    await expect(
      stabilizeSafeWorkflowCommandCheckpoint(checkpoint, {
        async read() {
          return { state: "staged", result: needsInput };
        },
        persistFailed,
        restore,
        retire,
      }),
    ).rejects.toMatchObject({ code: "result_promotion_conflict" });
    await expect(
      stabilizeSafeWorkflowCommandCheckpoint(checkpoint, {
        async read() {
          return {
            state: "committed",
            result: { ...needsInput, applicationId: "application-other" },
          };
        },
        persistFailed,
        restore,
        retire,
      }),
    ).rejects.toMatchObject({ code: "result_promotion_conflict" });

    const failed = createDurableFailedRunResult(checkpoint.request);
    const removalFailure = vi.fn(async () => {
      throw new Error("checkpoint removal failed");
    });
    await expect(
      stabilizeSafeWorkflowCommandCheckpoint(checkpoint, {
        async read() {
          return { state: "committed", result: failed };
        },
        persistFailed,
        restore,
        retire: removalFailure,
      }),
    ).rejects.toThrow("checkpoint removal failed");
    expect(removalFailure).toHaveBeenCalledOnce();
    expect(restore).not.toHaveBeenCalled();

    await expect(
      stabilizeSafeWorkflowCommandCheckpoint(checkpoint, {
        async read() {
          return { state: "committed", result: failed };
        },
        persistFailed,
        restore,
        retire,
      }),
    ).resolves.toBe("retired");
    expect(retire).toHaveBeenCalledOnce();
    expect(restore).not.toHaveBeenCalled();
  });

  it("emits exact ambiguity only after durable persistence succeeds", async () => {
    const requestId = "wfreq-v2-12345678-1234-5678-9234-123456789abc";
    const finish = vi.fn(async () => {});
    const cleanup = vi.fn(async () => {});
    const lease = {
      finalSubmitAttempted: true,
      finish,
      stopHeartbeat: vi.fn(async () => {}),
    } as unknown as ActiveExecutionLease;
    let failedPersistence: unknown;
    try {
      await persistAndAbortSideEffectUnknown(
        async () => {
          throw new Error("disk unavailable");
        },
        lease,
        cleanup,
        async () => {},
      );
    } catch (error) {
      failedPersistence = error;
    }
    expect(publicRunnerFailureResponse(failedPersistence, requestId)).toEqual({
      status: 503,
      code: "ambiguity_persistence_failed",
      body: { error: "The application runner is temporarily unavailable." },
    });

    const uncompactedLease = {
      finalSubmitAttempted: true,
      finish: vi.fn(async () => {}),
    } as unknown as ActiveExecutionLease;
    let failedCompaction: unknown;
    try {
      await persistAndAbortSideEffectUnknown(
        async () => "persisted",
        uncompactedLease,
        async () => {},
        async () => {
          throw new Error("server reconciliation unavailable");
        },
      );
    } catch (error) {
      failedCompaction = error;
    }
    expect(publicRunnerFailureResponse(failedCompaction, requestId)).toEqual({
      status: 503,
      code: "ambiguity_persistence_failed",
      body: { error: "The application runner is temporarily unavailable." },
    });

    const persistedLease = {
      finalSubmitAttempted: true,
      finish: vi.fn(async () => {}),
    } as unknown as ActiveExecutionLease;
    let durableFailure: unknown;
    try {
      await persistAndAbortSideEffectUnknown(
        async () => {},
        persistedLease,
        async () => {},
        async () => {},
      );
    } catch (error) {
      durableFailure = error;
    }
    expect(publicRunnerFailureResponse(durableFailure, requestId)).toEqual({
      status: 500,
      code: "side_effect_unknown",
      body: { schemaVersion: 2, outcome: "side_effect_unknown", requestId },
    });
    expect(finish).not.toHaveBeenCalled();
    expect(persistedLease.finish).toHaveBeenCalledWith("side_effect_unknown");
  });

  it("commits an exact failed authority before terminal lease finalization", async () => {
    const dataRoot = await mkdtemp(
      join(tmpdir(), "bluey-runner-failed-authority-"),
    );
    const encryptionKey = randomBytes(32);
    const requestId = "wfreq-v2-12345678-1234-5678-9234-123456789abc";
    const lookup = ambiguityLookup(requestId);
    const profileScope = profilePaths(
      dataRoot,
      lookup.accountId,
      lookup.applicationIdentityId,
    ).scope;
    const result = createDurableFailedRunResult(lookup);
    const order: string[] = [];
    const lease = {
      finalSubmitAttempted: false,
      async finish(outcome: string) {
        order.push(`finish:${outcome}`);
      },
      async stopHeartbeat() {
        order.push("stop");
      },
    } as unknown as ActiveExecutionLease;
    let terminal: unknown;
    try {
      await persistAndFinishFailedRun(
        requestId,
        async () => {
          order.push("persist");
          await writeResult(
            dataRoot,
            { profileScope, requestId },
            result,
            encryptionKey,
          );
          return result;
        },
        lease,
        async () => {
          order.push("retire");
        },
        async () => {
          order.push("cleanup");
        },
      );
    } catch (error) {
      terminal = error;
    }

    expect(terminal).toBeInstanceOf(DurableFailedRunResultError);
    expect(order).toEqual(["persist", "retire", "cleanup", "finish:failed"]);
    await expect(
      recoverDurableRunResult(dataRoot, encryptionKey, lookup),
    ).resolves.toEqual(result);
    expect(result.receipt).toEqual({
      status: "failed",
      issues: [
        {
          field: "submission",
          message: "The automated browser run ended before submission.",
          severity: "blocking",
        },
      ],
    });
    expect(JSON.stringify(result)).not.toContain("private@example.test");
    expect(JSON.stringify(result)).not.toContain("private answer");
  });

  it("keeps a committed failed result authoritative across cleanup and finish response loss", async () => {
    const dataRoot = await mkdtemp(
      join(tmpdir(), "bluey-runner-failed-finish-loss-"),
    );
    const encryptionKey = randomBytes(32);
    const requestId = "wfreq-v2-12345678-1234-5678-9234-123456789abc";
    const lookup = ambiguityLookup(requestId);
    const profileScope = profilePaths(
      dataRoot,
      lookup.accountId,
      lookup.applicationIdentityId,
    ).scope;
    const result = createDurableFailedRunResult(lookup);
    const order: string[] = [];
    const lease = {
      finalSubmitAttempted: false,
      async finish(outcome: string) {
        order.push(`finish:${outcome}`);
        throw new Error("finish response lost");
      },
      async stopHeartbeat() {},
    } as unknown as ActiveExecutionLease;

    await expect(
      persistAndFinishFailedRun(
        requestId,
        async () => {
          order.push("persist");
          await writeResult(
            dataRoot,
            { profileScope, requestId },
            result,
            encryptionKey,
          );
          return result;
        },
        lease,
        async () => {
          order.push("retire");
        },
        async () => {
          order.push("cleanup");
          throw new Error("cleanup failed");
        },
      ),
    ).rejects.toBeInstanceOf(DurableFailedRunResultError);

    expect(order).toEqual(["persist", "retire", "cleanup", "finish:failed"]);
    const recovered = await recoverDurableRunResult(
      dataRoot,
      encryptionKey,
      lookup,
    );
    expect(authoritativeRunnerResult(recovered, requestId)).toEqual({
      ...result,
      requestId,
    });
  });

  it("never terminalizes a safe lease when terminal result persistence fails", async () => {
    const order: string[] = [];
    const lease = {
      finalSubmitAttempted: false,
      async finish(outcome: string) {
        order.push(`finish:${outcome}`);
      },
      async stopHeartbeat() {
        order.push("stop");
      },
    } as unknown as ActiveExecutionLease;

    await expect(
      persistAndFinishFailedRun(
        "wfreq-v2-12345678-1234-5678-9234-123456789abc",
        async () => {
          order.push("persist");
          throw new Error("result storage unavailable");
        },
        lease,
        async () => {
          order.push("retire");
        },
        async () => {
          order.push("cleanup");
        },
      ),
    ).rejects.toBeInstanceOf(TerminalResultPersistenceError);
    expect(order).toEqual(["persist", "cleanup", "stop"]);

    await expect(
      leaveLeasedRunRetryable(lease, async () => {
        order.push("retryable-cleanup");
      }),
    ).rejects.toBeInstanceOf(TerminalResultPersistenceError);
    expect(order).toEqual([
      "persist",
      "cleanup",
      "stop",
      "retryable-cleanup",
      "stop",
    ]);
    expect(order.some((entry) => entry.startsWith("finish:"))).toBe(false);
  });

  it.each(["final_submit_started", "final_submit_activated"] as const)(
    "recovers a prior %s checkpoint when the unknown rewrite was lost",
    async (phase) => {
      const dataRoot = await mkdtemp(
        join(tmpdir(), "bluey-runner-ambiguity-prior-marker-"),
      );
      const encryptionKey = randomBytes(32);
      const requestId = "wfreq-v2-12345678-1234-5678-9234-123456789abc";
      const lookup = ambiguityLookup(requestId);
      const profileScope = profilePaths(
        dataRoot,
        lookup.accountId,
        lookup.applicationIdentityId,
      ).scope;
      await writeRunCheckpoint(
        dataRoot,
        {
          ...ambiguityCheckpoint(profileScope, lookup),
          phase,
        },
        encryptionKey,
      );

      await expect(
        recoverDurableSideEffectUnknown(dataRoot, encryptionKey, lookup),
      ).resolves.toBe(requestId);
      const response = publicRunnerFailureResponse(
        new DurableSideEffectUnknownError(),
        requestId,
      );
      expect(response).toEqual({
        status: 500,
        code: "side_effect_unknown",
        body: { schemaVersion: 2, outcome: "side_effect_unknown", requestId },
      });
    },
  );

  it("rejects lookalike and cross-bound ambiguity recovery", async () => {
    const dataRoot = await mkdtemp(
      join(tmpdir(), "bluey-runner-ambiguity-binding-"),
    );
    const encryptionKey = randomBytes(32);
    const requestId = "wfreq-v2-12345678-1234-5678-9234-123456789abc";
    const lookup = ambiguityLookup(requestId);
    const profileScope = profilePaths(
      dataRoot,
      lookup.accountId,
      lookup.applicationIdentityId,
    ).scope;
    await writeRunCheckpoint(
      dataRoot,
      ambiguityCheckpoint(profileScope, lookup),
      encryptionKey,
    );

    await expect(
      recoverDurableSideEffectUnknown(dataRoot, encryptionKey, {
        ...lookup,
        applicationId: "application-other",
      }),
    ).rejects.toMatchObject({ code: "result_promotion_conflict" });
    await expect(
      recoverDurableSideEffectUnknown(dataRoot, encryptionKey, {
        ...lookup,
        browserSessionId: "browser-other",
      }),
    ).rejects.toMatchObject({ code: "result_promotion_conflict" });
    await expect(
      recoverDurableSideEffectUnknown(dataRoot, encryptionKey, {
        ...lookup,
        runId: "run-other",
      }),
    ).rejects.toMatchObject({ code: "result_promotion_conflict" });
    await expect(
      recoverDurableSideEffectUnknown(dataRoot, encryptionKey, {
        ...lookup,
        applicationIdentityId: "identity-other",
      }),
    ).resolves.toBeUndefined();
    await expect(
      recoverDurableSideEffectUnknown(dataRoot, encryptionKey, {
        ...lookup,
        requestId: "wfreq-v2-12345678-1234-4678-9234-123456789abc",
      }),
    ).rejects.toThrow("Invalid durable result request");
  });

  it("does not promote a nonterminal checkpoint or an unreadable scan to ambiguity", async () => {
    const dataRoot = await mkdtemp(
      join(tmpdir(), "bluey-runner-ambiguity-scan-"),
    );
    const encryptionKey = randomBytes(32);
    const requestId = "wfreq-v2-12345678-1234-5678-9234-123456789abc";
    const lookup = ambiguityLookup(requestId);
    const profileScope = profilePaths(
      dataRoot,
      lookup.accountId,
      lookup.applicationIdentityId,
    ).scope;
    const checkpoint = ambiguityCheckpoint(profileScope, lookup);
    await writeRunCheckpoint(
      dataRoot,
      {
        ...checkpoint,
        phase: "prepared",
        expiresAtMs: Date.now() + 60_000,
        workflow: { ...checkpoint.workflow, status: "prepared" },
      },
      encryptionKey,
    );
    await expect(
      recoverDurableSideEffectUnknown(dataRoot, encryptionKey, lookup),
    ).resolves.toBeUndefined();

    const checkpointScope = cloudCheckpointScope(
      profileScope,
      lookup.browserSessionId,
    );
    await writeFile(
      join(
        dataRoot,
        "run-checkpoints",
        profileScope,
        `${checkpointScope}.json.enc`,
      ),
      "unreadable-checkpoint",
    );
    await expect(
      recoverDurableSideEffectUnknown(dataRoot, encryptionKey, lookup),
    ).rejects.toThrow();
  });

  it("returns only the exact committed account, profile, and request result", async () => {
    const dataRoot = await mkdtemp(join(tmpdir(), "bluey-runner-recovery-"));
    const encryptionKey = randomBytes(32);
    const accountId = "account-123";
    const applicationIdentityId = "identity-123";
    const browserSessionId = "browser-123";
    const requestId = "run-123:initial";
    const profileScope = profilePaths(
      dataRoot,
      accountId,
      applicationIdentityId,
    ).scope;
    const receipt = {
      status: "submitted",
      confirmationText: "Thank you for applying",
      issues: [],
    };
    const result = {
      accountId,
      applicationId: "application-123",
      applicationIdentityId,
      browserSessionId,
      runId: "run-123",
      receipt,
      receiptAuthority: { leaseToken: "a".repeat(43), fence: 7 },
      receiptBundle: {
        schemaVersion: 1,
        accountId,
        applicationId: "application-123",
        applicationIdentityId,
        runId: "run-123",
        runner: "cloud",
        result: receipt,
      },
      evidenceObjects: [{ bytes_base64: "private-evidence" }],
    };
    await writeResult(
      dataRoot,
      { profileScope, requestId },
      result,
      encryptionKey,
    );

    await expect(
      recoverDurableRunResult(dataRoot, encryptionKey, {
        accountId,
        applicationId: "application-123",
        applicationIdentityId,
        browserSessionId,
        runId: "run-123",
        requestId,
      }),
    ).resolves.toEqual(result);
    await expect(
      recoverDurableRunResult(dataRoot, encryptionKey, {
        accountId,
        applicationId: "application-456",
        applicationIdentityId,
        browserSessionId,
        runId: "run-123",
        requestId,
      }),
    ).rejects.toMatchObject({ code: "result_promotion_conflict" });
    await expect(
      recoverDurableRunResult(dataRoot, encryptionKey, {
        accountId,
        applicationId: "application-123",
        applicationIdentityId,
        browserSessionId: "browser-456",
        runId: "run-123",
        requestId,
      }),
    ).rejects.toMatchObject({ code: "result_promotion_conflict" });
    await expect(
      recoverDurableRunResult(dataRoot, encryptionKey, {
        accountId,
        applicationId: "application-123",
        applicationIdentityId: "identity-456",
        browserSessionId,
        runId: "run-123",
        requestId,
      }),
    ).resolves.toBeUndefined();
    await expect(
      recoverDurableRunResult(dataRoot, encryptionKey, {
        accountId,
        applicationId: "application-123",
        applicationIdentityId,
        browserSessionId,
        runId: "run-123",
        requestId: "run-123:resume:1",
      }),
    ).resolves.toBeUndefined();
  });

  it("rejects a committed receipt whose bundle belongs to another application", async () => {
    const dataRoot = await mkdtemp(
      join(tmpdir(), "bluey-runner-cross-application-"),
    );
    const encryptionKey = randomBytes(32);
    const accountId = "account-123";
    const applicationIdentityId = "identity-123";
    const browserSessionId = "browser-123";
    const requestId = "run-123:initial";
    const profileScope = profilePaths(
      dataRoot,
      accountId,
      applicationIdentityId,
    ).scope;
    const receipt = { status: "submitted", issues: [] };
    await writeResult(
      dataRoot,
      { profileScope, requestId },
      {
        accountId,
        applicationId: "application-123",
        applicationIdentityId,
        browserSessionId,
        runId: "run-123",
        receipt,
        receiptAuthority: { leaseToken: "a".repeat(43), fence: 7 },
        receiptBundle: {
          schemaVersion: 1,
          accountId,
          applicationId: "application-other",
          applicationIdentityId,
          runId: "run-123",
          runner: "cloud",
          result: receipt,
        },
      },
      encryptionKey,
    );

    await expect(
      recoverDurableRunResult(dataRoot, encryptionKey, {
        accountId,
        applicationId: "application-123",
        applicationIdentityId,
        browserSessionId,
        runId: "run-123",
        requestId,
      }),
    ).rejects.toMatchObject({ code: "result_promotion_conflict" });
  });

  it("never exposes a staged result and rejects unbounded lookup input", async () => {
    const dataRoot = await mkdtemp(
      join(tmpdir(), "bluey-runner-recovery-stage-"),
    );
    const encryptionKey = randomBytes(32);
    const accountId = "account-123";
    const applicationIdentityId = "identity-123";
    const browserSessionId = "browser-123";
    const requestId = "run-123:initial";
    const profileScope = profilePaths(
      dataRoot,
      accountId,
      applicationIdentityId,
    ).scope;
    await stageResult(
      dataRoot,
      { profileScope, requestId },
      { receipt: { status: "submitted", issues: [] } },
      encryptionKey,
    );

    await expect(
      recoverDurableRunResult(dataRoot, encryptionKey, {
        accountId,
        applicationId: "application-123",
        applicationIdentityId,
        browserSessionId,
        runId: "run-123",
        requestId,
      }),
    ).resolves.toBeUndefined();
    await expect(
      recoverDurableRunResult(dataRoot, encryptionKey, {
        accountId,
        applicationId: "application-123",
        applicationIdentityId,
        browserSessionId,
        runId: "run-123",
        requestId: "../../another-profile",
      }),
    ).rejects.toThrow("Invalid durable result request");
    await expect(
      recoverDurableRunResult(dataRoot, encryptionKey, {
        accountId,
        applicationId: "application-123",
        applicationIdentityId,
        browserSessionId,
        runId: "run-456",
        requestId,
      }),
    ).rejects.toThrow("Invalid durable result request");
    await expect(
      recoverDurableRunResult(dataRoot, encryptionKey, {
        accountId,
        applicationId: "application-123",
        applicationIdentityId,
        browserSessionId,
        runId: "run-123",
        requestId,
        unexpected: "secret",
      }),
    ).rejects.toThrow("Invalid durable result request");
  });
});

interface AmbiguityLookup {
  accountId: string;
  applicationId: string;
  applicationIdentityId: string;
  browserSessionId: string;
  runId: string;
  requestId: string;
}

interface AmbiguityCheckpointRequest extends AmbiguityLookup {
  browserProfileId: string;
  url: string;
  packet: ApplicationPacket;
  job: NormalizedJob;
}

function ambiguityLookup(requestId: string): AmbiguityLookup {
  return {
    accountId: "account-private-123",
    applicationId: "application-private-123",
    applicationIdentityId: "identity-private-123",
    browserSessionId: "browser-private-123",
    runId: "run-private-123",
    requestId,
  };
}

function ambiguityCheckpoint(
  profileScope: string,
  lookup: AmbiguityLookup,
): CloudRunCheckpoint<AmbiguityCheckpointRequest> {
  const job: NormalizedJob = {
    externalId: "job-private-123",
    canonicalUrl: "https://jobs.example.test/apply",
    company: "Private Employer",
    title: "Private Role",
    location: "Remote",
    workplace: "remote",
    description: "Private description",
    source: "greenhouse",
  };
  const packet: ApplicationPacket = {
    applicationId: lookup.applicationId,
    jobId: "job-private-123",
    resumeVersionId: "resume-private-123",
    approvedPacketChecksum: "",
    applicationEmail: "private@example.test",
    answers: { private_question: "private answer" },
    verifiedClaimIds: [],
  };
  packet.approvedPacketChecksum = approvedExecutionChecksum(packet, job);
  return {
    version: 2,
    phase: "side_effect_unknown",
    createdAtMs: Date.parse("2026-08-13T18:00:00.000Z"),
    updatedAtMs: Date.parse("2026-08-13T18:01:00.000Z"),
    expiresAtMs: Date.parse("2026-08-14T18:00:00.000Z"),
    profileScope,
    browserSessionId: lookup.browserSessionId,
    request: {
      ...lookup,
      browserProfileId: "profile:private-123",
      url: job.canonicalUrl,
      packet,
      job,
    },
    browser: { url: job.canonicalUrl },
    workflow: { status: "side_effect_unknown", requestId: lookup.requestId },
    events: [],
    lease: {
      fence: 7,
      expiresAtMs: Date.parse("2026-08-13T18:05:00.000Z"),
      ownerId: "runner-private-1",
      leaseToken: "private-lease-token",
      purgeSubject: "A".repeat(43),
    },
  };
}

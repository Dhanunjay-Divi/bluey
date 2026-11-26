import { createHash, randomBytes } from "node:crypto";
import { mkdtemp } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it, vi } from "vitest";
import { ExecutionLeaseClient, type ActiveExecutionLease } from "../src/execution-lease.js";
import { finalizeLeasedRun } from "../src/leased-run.js";
import { recoverOrExecuteRun } from "../src/run-mutation-recovery.js";
import { publicRunnerFailureResponse } from "../src/server.js";
import {
  durableResultScope,
  readManagedResult,
  readManagedResultState,
  readResult,
  readResultState,
  stageManagedResult,
  stageResult,
  writeManagedResult,
} from "../src/result-store.js";
import {
  isRecoverableSubmittedCheckpointPhase,
  recoverManagedSubmittedResult,
  recoverSubmittedResult,
  assertDurableResultBinding,
  type SubmittedResultRecoveryAuthority,
} from "../src/submitted-result-recovery.js";
import { subjectStoragePaths } from "../src/subject-storage-layout.js";
import type { ManagedResultStorage } from "../src/subject-storage-manager.js";
import type {
  NativeRunnerInventory,
  NativeRunnerStorageDirectory,
} from "../src/native-runner-storage.js";

const LEASE_TOKEN = "a".repeat(43);
const RESULT_CONTEXT = {
  requestId: "run-123:initial",
  profileScope: "b".repeat(40),
};
const V2_RESULT_CONTEXT = {
  requestId: "wfreq-v2-12345678-1234-5678-9234-123456789abc",
  profileScope: "b".repeat(40),
};

describe("submitted result recovery", () => {
  it("replays the exact submitted finish and promotes the staged receipt after restart", async () => {
    const root = await mkdtemp(
      join(tmpdir(), "bluey-submitted-result-recovery-"),
    );
    const key = randomBytes(32);
    const result = submittedResult();
    await stageResult(root, RESULT_CONTEXT, result, key);
    const replaySubmittedFinish = vi.fn(async () => {});

    await expect(
      recoverSubmittedResult(root, key, recoveryAuthority(), {
        replaySubmittedFinish,
      }),
    ).resolves.toEqual(result);

    expect(replaySubmittedFinish).toHaveBeenCalledWith({
      accountId: "account-123",
      applicationId: "application-123",
      runId: "run-123",
      leaseToken: LEASE_TOKEN,
      fence: 7,
    });
    await expect(readResult(root, RESULT_CONTEXT, key)).resolves.toEqual(
      result,
    );
  });

  it("recovers trusted confirmation after an activation-uncertain restart", async () => {
    const root = await mkdtemp(
      join(tmpdir(), "bluey-submitted-result-uncertain-"),
    );
    const key = randomBytes(32);
    const result = submittedResult();
    await stageResult(root, RESULT_CONTEXT, result, key);
    const replaySubmittedFinish = vi.fn(async () => {});

    expect(isRecoverableSubmittedCheckpointPhase("side_effect_unknown")).toBe(
      true,
    );
    expect(isRecoverableSubmittedCheckpointPhase("final_submit_started")).toBe(
      false,
    );
    await expect(
      recoverSubmittedResult(root, key, recoveryAuthority(), {
        replaySubmittedFinish,
      }),
    ).resolves.toEqual(result);

    expect(replaySubmittedFinish).toHaveBeenCalledTimes(1);
    await expect(readResult(root, RESULT_CONTEXT, key)).resolves.toEqual(
      result,
    );
  });

  it("recovers a staged submitted result under exact v2 request authority", async () => {
    const root = await mkdtemp(join(tmpdir(), "bluey-submitted-result-v2-"));
    const key = randomBytes(32);
    const result = submittedResult();
    await stageResult(root, V2_RESULT_CONTEXT, result, key);
    const replaySubmittedFinish = vi.fn(async () => {});

    await expect(
      recoverSubmittedResult(
        root,
        key,
        {
          ...recoveryAuthority(),
          resultContext: V2_RESULT_CONTEXT,
        },
        { replaySubmittedFinish },
      ),
    ).resolves.toEqual(result);

    expect(replaySubmittedFinish).toHaveBeenCalledTimes(1);
    await expect(readResult(root, V2_RESULT_CONTEXT, key)).resolves.toEqual(
      result,
    );
  });

  it("leaves the exact receipt staged when the server cannot confirm submitted", async () => {
    const root = await mkdtemp(join(tmpdir(), "bluey-submitted-result-retry-"));
    const key = randomBytes(32);
    await stageResult(root, RESULT_CONTEXT, submittedResult(), key);
    const replaySubmittedFinish = vi.fn(async () => {
      throw new Error("server unavailable");
    });

    await expect(
      recoverSubmittedResult(root, key, recoveryAuthority(), {
        replaySubmittedFinish,
      }),
    ).rejects.toThrow("server unavailable");

    await expect(
      readResult(root, RESULT_CONTEXT, key),
    ).resolves.toBeUndefined();
    await expect(
      readResultState(root, RESULT_CONTEXT, key),
    ).resolves.toMatchObject({
      state: "staged",
      result: submittedResult(),
    });
  });

  it.each([
    ["token", { leaseToken: "z".repeat(43), fence: 7 }],
    ["fence", { leaseToken: LEASE_TOKEN, fence: 8 }],
  ])(
    "fails closed before server replay when staged receipt %s differs",
    async (_field, authority) => {
      const root = await mkdtemp(
        join(tmpdir(), "bluey-submitted-result-fence-"),
      );
      const key = randomBytes(32);
      await stageResult(root, RESULT_CONTEXT, submittedResult(authority), key);
      const replaySubmittedFinish = vi.fn(async () => {});

      await expect(
        recoverSubmittedResult(root, key, recoveryAuthority(), {
          replaySubmittedFinish,
        }),
      ).rejects.toMatchObject({ code: "result_promotion_conflict" });

      expect(replaySubmittedFinish).not.toHaveBeenCalled();
      await expect(
        readResult(root, RESULT_CONTEXT, key),
      ).resolves.toBeUndefined();
    },
  );

  it("rejects a submitted receipt bundle bound to another application", async () => {
    const root = await mkdtemp(
      join(tmpdir(), "bluey-submitted-result-application-"),
    );
    const key = randomBytes(32);
    const result = submittedResult();
    result.receiptBundle.applicationId = "application-other";
    await stageResult(root, RESULT_CONTEXT, result, key);
    const replaySubmittedFinish = vi.fn(async () => {});

    await expect(
      recoverSubmittedResult(root, key, recoveryAuthority(), {
        replaySubmittedFinish,
      }),
    ).rejects.toMatchObject({ code: "result_promotion_conflict" });

    expect(replaySubmittedFinish).not.toHaveBeenCalled();
    await expect(
      readResult(root, RESULT_CONTEXT, key),
    ).resolves.toBeUndefined();
  });

  it("never replays a non-submitted staged result as a submitted finish", async () => {
    const root = await mkdtemp(
      join(tmpdir(), "bluey-submitted-result-status-"),
    );
    const key = randomBytes(32);
    const result = submittedResult();
    result.receipt.status = "failed";
    await stageResult(root, RESULT_CONTEXT, result, key);
    const replaySubmittedFinish = vi.fn(async () => {});

    await expect(
      recoverSubmittedResult(root, key, recoveryAuthority(), {
        replaySubmittedFinish,
      }),
    ).rejects.toMatchObject({ code: "result_promotion_conflict" });

    expect(replaySubmittedFinish).not.toHaveBeenCalled();
    await expect(
      readResult(root, RESULT_CONTEXT, key),
    ).resolves.toBeUndefined();
  });

  it("returns an exact committed result without depending on a retained lease row", async () => {
    const root = await mkdtemp(
      join(tmpdir(), "bluey-submitted-result-idempotent-"),
    );
    const key = randomBytes(32);
    const result = submittedResult();
    await stageResult(root, RESULT_CONTEXT, result, key);
    const firstReplay = vi.fn(async () => {});
    await recoverSubmittedResult(root, key, recoveryAuthority(), {
      replaySubmittedFinish: firstReplay,
    });

    const restartedReplay = vi.fn(async () => {});
    await expect(
      recoverSubmittedResult(root, key, recoveryAuthority(), {
        replaySubmittedFinish: restartedReplay,
      }),
    ).resolves.toEqual(result);

    expect(restartedReplay).not.toHaveBeenCalled();
  });

  it("replays and promotes an exact managed staged receipt through a retained capability", async () => {
    const key = randomBytes(32);
    const storage = retainedManagedResultStorage(
      durableResultScope(RESULT_CONTEXT),
    );
    const result = submittedResult();
    await stageManagedResult(storage.capability, RESULT_CONTEXT, result, key);
    const replaySubmittedFinish = vi.fn(async () => {});

    await expect(
      recoverManagedSubmittedResult(
        storage.capability,
        key,
        recoveryAuthority(),
        { replaySubmittedFinish },
      ),
    ).resolves.toEqual(result);

    expect(replaySubmittedFinish).toHaveBeenCalledWith({
      accountId: "account-123",
      applicationId: "application-123",
      runId: "run-123",
      leaseToken: LEASE_TOKEN,
      fence: 7,
    });
    await expect(
      readManagedResult(storage.capability, RESULT_CONTEXT, key),
    ).resolves.toEqual(result);
    await expect(
      readManagedResultState(storage.capability, RESULT_CONTEXT, key),
    ).resolves.toMatchObject({
      state: "committed",
      result,
    });
    expect(storage.root.fileNames()).toEqual(["step-result.json.enc"]);
  });

  it("replays and promotes an exact managed v2 staged receipt after restart", async () => {
    const key = randomBytes(32);
    const storage = retainedManagedResultStorage(
      durableResultScope(V2_RESULT_CONTEXT),
    );
    const result = submittedResult();
    await stageManagedResult(
      storage.capability,
      V2_RESULT_CONTEXT,
      result,
      key,
    );
    const replaySubmittedFinish = vi.fn(async () => {});

    await expect(
      recoverManagedSubmittedResult(
        storage.capability,
        key,
        {
          ...recoveryAuthority(),
          resultContext: V2_RESULT_CONTEXT,
        },
        { replaySubmittedFinish },
      ),
    ).resolves.toEqual(result);

    expect(replaySubmittedFinish).toHaveBeenCalledTimes(1);
    await expect(
      readManagedResult(storage.capability, V2_RESULT_CONTEXT, key),
    ).resolves.toEqual(result);
  });

  it("returns an exact managed committed result idempotently without server replay", async () => {
    const key = randomBytes(32);
    const storage = retainedManagedResultStorage(
      durableResultScope(RESULT_CONTEXT),
    );
    const result = submittedResult();
    await writeManagedResult(storage.capability, RESULT_CONTEXT, result, key);
    const replaySubmittedFinish = vi.fn(async () => {});

    await expect(
      recoverManagedSubmittedResult(
        storage.capability,
        key,
        recoveryAuthority(),
        { replaySubmittedFinish },
      ),
    ).resolves.toEqual(result);

    expect(replaySubmittedFinish).not.toHaveBeenCalled();
    await expect(
      readManagedResult(storage.capability, RESULT_CONTEXT, key),
    ).resolves.toEqual(result);
  });

  it("returns a managed v2 result that became visible after an earlier missing lookup", async () => {
    const key = randomBytes(32);
    const storage = retainedManagedResultStorage(durableResultScope(V2_RESULT_CONTEXT));
    const result = submittedResult();
    const authority = recoveryAuthority();
    const readCommitted = async (binding = authority) => {
      const stored = await readManagedResult<typeof result>(storage.capability, V2_RESULT_CONTEXT, key);
      if (stored !== undefined) assertDurableResultBinding(stored, binding);
      return stored;
    };
    const requireMutation = vi.fn(async () => {});
    const assertNoDurableAmbiguity = vi.fn(async () => {});
    const recoverSubmittedCheckpoint = vi.fn(async () => undefined);
    const start = vi.fn(async () => { throw new Error("Unexpected second runner start"); });
    const operations = {
      readCommitted, requireMutation, assertNoDurableAmbiguity, recoverSubmittedCheckpoint, execute: start,
    };

    try {
      expect(await readCommitted()).toBeUndefined();
      await writeManagedResult(storage.capability, V2_RESULT_CONTEXT, result, key);
      const committed = await storage.root.inventory();
      expect(await recoverOrExecuteRun(operations)).toEqual(result);
      await expect(recoverOrExecuteRun({
        ...operations,
        readCommitted: () => readCommitted({ ...authority, applicationId: "different-application" }),
      })).rejects.toMatchObject({ code: "result_promotion_conflict" });
      expect(await storage.root.inventory()).toEqual(committed);
      expect(requireMutation).not.toHaveBeenCalled();
      expect(assertNoDurableAmbiguity).not.toHaveBeenCalled();
      expect(recoverSubmittedCheckpoint).not.toHaveBeenCalled();
      expect(start).not.toHaveBeenCalled();
    } finally {
      key.fill(0);
    }
  });

  it("rechecks after admission and starts only when no recovery path can answer", async () => {
    const events: string[] = [];
    const result = submittedResult();
    let committed: typeof result | undefined;
    let checkpoint: typeof result | undefined;
    let ambiguous = false;
    const run = (admitResult?: typeof result) => recoverOrExecuteRun({
      async readCommitted() { events.push("read"); return committed; },
      async requireMutation() { events.push("admit"); committed = admitResult; },
      async assertNoDurableAmbiguity() {
        events.push("ambiguity");
        if (ambiguous) throw new Error("Synthetic durable ambiguity");
      },
      async recoverSubmittedCheckpoint() { events.push("checkpoint"); return checkpoint; },
      async execute() { events.push("start"); return result; },
    });
    expect(await run(result)).toBe(result);
    expect(events.splice(0)).toEqual(["read", "admit", "read"]);
    committed = undefined;
    ambiguous = true;
    await expect(run()).rejects.toThrow("Synthetic durable ambiguity");
    expect(events.splice(0)).toEqual(["read", "admit", "read", "ambiguity"]);
    ambiguous = false;
    checkpoint = result;
    expect(await run()).toBe(result);
    expect(events.splice(0)).toEqual(["read", "admit", "read", "ambiguity", "checkpoint"]);
    checkpoint = undefined;
    expect(await run()).toBe(result);
    expect(events).toEqual(["read", "admit", "read", "ambiguity", "checkpoint", "start"]);
  });

  it("recovers a resumed v2 result without restoring a browser and preserves a genuine missing session", async () => {
    const key = randomBytes(32);
    const context = { ...V2_RESULT_CONTEXT, requestId: "wfreq-v2-22345678-1234-5678-9234-123456789abc" };
    const storage = retainedManagedResultStorage(durableResultScope(context));
    const result = submittedResult();
    const execute = vi.fn(async (): Promise<typeof result | undefined> => undefined);
    const resume = () => recoverOrExecuteRun({
      readCommitted: () => readManagedResult<typeof result>(storage.capability, context, key),
      requireMutation: async () => {},
      assertNoDurableAmbiguity: async () => {},
      recoverSubmittedCheckpoint: async () => undefined,
      execute,
    });

    try {
      expect(await resume()).toBeUndefined();
      expect(execute).toHaveBeenCalledTimes(1);
      await writeManagedResult(storage.capability, context, result, key);
      const committed = await storage.root.inventory();
      expect(await resume()).toEqual(result);
      expect(execute).toHaveBeenCalledTimes(1);
      expect(await storage.root.inventory()).toEqual(committed);
    } finally {
      key.fill(0);
    }
  });

  it("keeps a managed v2 result staged until the finish endpoint acknowledges it", async () => {
    const key = randomBytes(32);
    const storage = retainedManagedResultStorage(durableResultScope(V2_RESULT_CONTEXT));
    const result = submittedResult();
    const authority = { ...recoveryAuthority(), resultContext: V2_RESULT_CONTEXT };
    const finishRequests: RequestInit[] = [];
    const origin = "https://jobs-api.example";
    const fetch: typeof globalThis.fetch = async (url, init) => {
      expect(String(url)).toBe(`${origin}/api/jobs/internal/execution-leases/run-123/finish`);
      if (!init) throw new Error("Missing synthetic finish request options");
      finishRequests.push(init);
      if (finishRequests.length === 1) throw new TypeError("finish acknowledgement lost");
      if (finishRequests.length === 2) return Response.json({ phase: "submitted" });
      return new Response(null, { status: 204 });
    };
    const client = new ExecutionLeaseClient({
      origin,
      workerSigningKey: "synthetic-finish-replay-signing-key",
      ownerId: "finish-replay-test-runner",
      runnerVolume: {
        volumeId: Buffer.alloc(32, 1).toString("base64url"),
        enrollmentEpoch: 1,
        processInstanceId: Buffer.alloc(32, 2).toString("base64url"),
        keyFingerprint: "a".repeat(64),
        runtimeGrantId: "test-grant",
        runtimeSha256: "c".repeat(64),
        createExecutionLeaseClaimProof() { throw new Error("Unexpected lease claim"); },
      },
      fetch,
    });
    const recover = () => recoverManagedSubmittedResult(storage.capability, key, authority, client);

    try {
      await stageManagedResult(storage.capability, V2_RESULT_CONTEXT, result, key);
      const staged = await storage.root.inventory();
      await expect(recover()).rejects.toMatchObject({ code: "request_failed" });
      expect(await storage.root.inventory()).toEqual(staged);
      expect(await readManagedResult(storage.capability, V2_RESULT_CONTEXT, key)).toBeUndefined();
      await expect(recover()).rejects.toMatchObject({ code: "invalid_response", status: 200 });
      expect(await storage.root.inventory()).toEqual(staged);
      expect(await readManagedResult(storage.capability, V2_RESULT_CONTEXT, key)).toBeUndefined();

      await expect(recover()).resolves.toEqual(result);
      expect(finishRequests).toHaveLength(3);
      expect(new Set(finishRequests.map(({ body }) => body)).size).toBe(1);
      expect(new Set(finishRequests.map(({ headers }) => (
        new Headers(headers).get("x-bluey-jobs-worker-nonce")
      ))).size).toBe(3);
      expect(JSON.parse(String(finishRequests[0]?.body))).toEqual({
        account_id: authority.accountId,
        application_id: authority.applicationId,
        lease_token: authority.leaseToken,
        fence: authority.fence,
        outcome: "submitted",
      });
      expect(await readManagedResult(storage.capability, V2_RESULT_CONTEXT, key)).toEqual(result);

      const committed = await storage.root.inventory();
      await expect(recover()).resolves.toEqual(result);
      expect(finishRequests).toHaveLength(3);
      expect(await storage.root.inventory()).toEqual(committed);
    } finally {
      key.fill(0);
    }
  });

  it("recovers after finish succeeds but local receipt promotion fails", async () => {
    const key = randomBytes(32);
    const storage = retainedManagedResultStorage(durableResultScope(V2_RESULT_CONTEXT));
    const result = submittedResult();
    const authority = { ...recoveryAuthority(), resultContext: V2_RESULT_CONTEXT };
    const finishRequests: RequestInit[] = [];
    const origin = "https://jobs-api.example";
    const fetch: typeof globalThis.fetch = async (url, init) => {
      expect(String(url)).toBe(`${origin}/api/jobs/internal/execution-leases/run-123/finish`);
      if (!init) throw new Error("Missing synthetic finish options");
      finishRequests.push(init);
      return new Response(null, { status: 204 });
    };
    const client = () => new ExecutionLeaseClient({
      origin,
      workerSigningKey: "synthetic-promotion-recovery-test-signing-key",
      ownerId: "promotion-recovery-test-runner",
      runnerVolume: {
        volumeId: Buffer.alloc(32, 1).toString("base64url"),
        enrollmentEpoch: 1,
        processInstanceId: Buffer.alloc(32, 2).toString("base64url"),
        keyFingerprint: "a".repeat(64),
        runtimeGrantId: "test-grant",
        runtimeSha256: "c".repeat(64),
        createExecutionLeaseClaimProof() { throw new Error("Unexpected lease claim"); },
      },
      fetch,
    });
    const firstClient = client();
    const lease = {
      finalSubmitAttempted: true,
      async finish(outcome: string) {
        expect(outcome).toBe("submitted");
        await firstClient.replaySubmittedFinish(authority);
      },
    } as ActiveExecutionLease;
    const execute = vi.fn(async () => { throw new Error("Unexpected browser execution"); });
    const readCommitted = () => readManagedResult<typeof result>(storage.capability, V2_RESULT_CONTEXT, key);
    const replace = storage.root.replaceFile.bind(storage.root);
    const writes = vi.spyOn(storage.root, "replaceFile");
    writes.mockImplementationOnce(replace).mockRejectedValueOnce(new Error("Synthetic promotion failure"));
    try {
      let staged: NativeRunnerInventory | undefined;
      const first = finalizeLeasedRun({
        lease,
        intendedOutcome: "submitted",
        cleanup: async () => {},
        async stage() {
          await stageManagedResult(storage.capability, V2_RESULT_CONTEXT, result, key);
          staged = await storage.root.inventory();
        },
        commit: () => writeManagedResult(storage.capability, V2_RESULT_CONTEXT, result, key),
      }).catch(publicRunnerFailureResponse);
      expect(await first).toEqual({
        status: 503,
        code: "submitted_result_pending",
        body: { error: "The submitted result is durably pending recovery." },
      });
      expect(finishRequests).toHaveLength(1);
      expect(writes).toHaveBeenCalledTimes(2);
      expect(staged?.count).toBe(1);
      expect(await storage.root.inventory()).toEqual(staged);
      expect(await readCommitted()).toBeUndefined();

      const restartedClient = client();
      const retry = () => recoverOrExecuteRun({
        readCommitted,
        requireMutation: async () => {},
        assertNoDurableAmbiguity: async () => {},
        recoverSubmittedCheckpoint: () => recoverManagedSubmittedResult<typeof result>(
          storage.capability, key, authority, restartedClient,
        ),
        execute,
      });
      await expect(retry()).resolves.toEqual(result);
      expect(await readCommitted()).toEqual(result);
      expect(writes).toHaveBeenCalledTimes(3);
      expect(finishRequests).toHaveLength(2);
      expect(finishRequests[1]?.body).toBe(finishRequests[0]?.body);
      const nonces = finishRequests.map(({ headers }) => new Headers(headers).get("x-bluey-jobs-worker-nonce"));
      expect(nonces.every(Boolean)).toBe(true);
      expect(new Set(nonces).size).toBe(2);
      const committed = await storage.root.inventory();
      await expect(retry()).resolves.toEqual(result);
      expect(await storage.root.inventory()).toEqual(committed);
      expect(finishRequests).toHaveLength(2);
      expect(writes).toHaveBeenCalledTimes(3);
      expect(execute).not.toHaveBeenCalled();
    } finally {
      key.fill(0);
    }
  });

  it("rejects a managed staged binding conflict before server replay", async () => {
    const key = randomBytes(32);
    const storage = retainedManagedResultStorage(
      durableResultScope(RESULT_CONTEXT),
    );
    const result = submittedResult();
    result.applicationId = "application-other";
    await stageManagedResult(storage.capability, RESULT_CONTEXT, result, key);
    const replaySubmittedFinish = vi.fn(async () => {});

    await expect(
      recoverManagedSubmittedResult(
        storage.capability,
        key,
        recoveryAuthority(),
        { replaySubmittedFinish },
      ),
    ).rejects.toMatchObject({ code: "result_promotion_conflict" });

    expect(replaySubmittedFinish).not.toHaveBeenCalled();
    await expect(
      readManagedResult(storage.capability, RESULT_CONTEXT, key),
    ).resolves.toBeUndefined();
    await expect(
      readManagedResultState(storage.capability, RESULT_CONTEXT, key),
    ).resolves.toMatchObject({
      state: "staged",
      result,
    });
  });

  it("returns undefined for missing managed state without server replay", async () => {
    const key = randomBytes(32);
    const storage = retainedManagedResultStorage(
      durableResultScope(RESULT_CONTEXT),
    );
    const replaySubmittedFinish = vi.fn(async () => {});

    await expect(
      recoverManagedSubmittedResult(
        storage.capability,
        key,
        recoveryAuthority(),
        { replaySubmittedFinish },
      ),
    ).resolves.toBeUndefined();

    expect(replaySubmittedFinish).not.toHaveBeenCalled();
    expect(storage.root.fileNames()).toEqual([]);
  });
});

function recoveryAuthority(): SubmittedResultRecoveryAuthority {
  return {
    accountId: "account-123",
    applicationId: "application-123",
    applicationIdentityId: "identity-123",
    browserSessionId: "browser-123",
    runId: "run-123",
    leaseToken: LEASE_TOKEN,
    fence: 7,
    resultContext: RESULT_CONTEXT,
  };
}

function submittedResult(authority = { leaseToken: LEASE_TOKEN, fence: 7 }) {
  const receipt = {
    status: "submitted",
    confirmationText: "Thank you for applying",
    issues: [],
  };
  return {
    accountId: "account-123",
    applicationId: "application-123",
    applicationIdentityId: "identity-123",
    browserSessionId: "browser-123",
    runId: "run-123",
    receipt,
    receiptAuthority: authority,
    receiptBundle: {
      schemaVersion: 1,
      receiptId: "receipt-123",
      accountId: "account-123",
      applicationId: "application-123",
      applicationIdentityId: "identity-123",
      runId: "run-123",
      runner: "cloud",
      result: receipt,
    },
    evidenceObjects: [{ sha256: "c".repeat(64), bytes_base64: "evidence" }],
  };
}

class RetainedMemoryDirectory implements NativeRunnerStorageDirectory {
  readonly canonicalPath: string;
  readonly deviceId = "unix:602:mount:17";
  readonly linkCount = 2;
  private readonly children = new Map<string, RetainedMemoryDirectory>();
  private readonly files = new Map<string, Buffer>();

  constructor(readonly relativePath: string) {
    this.canonicalPath = `/srv/bluey-runner/${relativePath}`;
  }

  fileNames(): string[] {
    return [...this.files.keys()].sort();
  }

  async ensureChildDirectory(
    name: string,
  ): Promise<NativeRunnerStorageDirectory> {
    let child = this.children.get(name);
    if (!child) {
      child = new RetainedMemoryDirectory(`${this.relativePath}/${name}`);
      this.children.set(name, child);
    }
    return child;
  }

  async openChildDirectory(
    name: string,
  ): Promise<NativeRunnerStorageDirectory> {
    const child = this.children.get(name);
    if (!child) throw new Error("missing memory directory");
    return child;
  }

  async writeFileExclusive(name: string, contents: Buffer): Promise<boolean> {
    if (this.files.has(name)) return false;
    this.files.set(name, Buffer.from(contents));
    return true;
  }

  async replaceFile(name: string, contents: Buffer): Promise<void> {
    this.files.set(name, Buffer.from(contents));
  }

  async readFileBounded(name: string, maximumBytes: number): Promise<Buffer> {
    const contents = this.files.get(name);
    if (!contents || contents.length > maximumBytes) {
      throw new Error("missing memory file");
    }
    return Buffer.from(contents);
  }

  async inventory(): Promise<NativeRunnerInventory> {
    const entries = this.fileNames().map((name) => {
      const contents = this.files.get(name)!;
      return {
        relativePath: name,
        kind: "file" as const,
        deviceId: this.deviceId,
        linkCount: 1,
        sizeBytes: contents.length,
        sha256: createHash("sha256").update(contents).digest("hex"),
      };
    });
    return {
      entries,
      count: entries.length,
      bytes: entries.reduce((total, entry) => total + entry.sizeBytes, 0),
      sha256: createHash("sha256")
        .update(entries.map((entry) => entry.sha256).join("\n"))
        .digest("hex"),
    };
  }

  async removeEntry(name: string): Promise<void> {
    this.files.delete(name);
    this.children.delete(name);
  }
}

function retainedManagedResultStorage(scope: string): {
  capability: ManagedResultStorage;
  root: RetainedMemoryDirectory;
} {
  const subjectSha256 = "1".repeat(64);
  const paths = subjectStoragePaths(subjectSha256).result(scope);
  const root = new RetainedMemoryDirectory(paths.root.relativePath);
  const temporary = new RetainedMemoryDirectory(paths.temporary.relativePath);
  return {
    root,
    capability: {
      kind: "result",
      subjectSha256,
      scope,
      paths,
      root,
      temporary,
    },
  };
}

import { createHash, randomBytes } from "node:crypto";
import { mkdtemp } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it, vi } from "vitest";
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

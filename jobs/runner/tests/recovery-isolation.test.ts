import { randomBytes } from "node:crypto";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it, vi } from "vitest";
import {
  ProfileRecoveryBlockedError,
  ProfileRecoveryIsolation,
} from "../src/recovery-isolation.js";
import { readResult, stageResult } from "../src/result-store.js";
import { recoverSubmittedResult } from "../src/submitted-result-recovery.js";
import { publicRunnerFailure, recoverDurableRunResult } from "../src/server.js";
import { profilePaths } from "../src/profile-store.js";

const PROFILE_A = "a".repeat(40);
const PROFILE_B = "b".repeat(40);

describe("runner profile recovery isolation", () => {
  it("isolates one startup failure and continues an unrelated profile", async () => {
    const isolation = new ProfileRecoveryIsolation();
    const recovered: string[] = [];

    await expect(isolation.attemptStartup(PROFILE_A, async () => {
      throw new Error("private checkpoint failure");
    })).resolves.toBe(false);
    await expect(isolation.attemptStartup(PROFILE_B, async () => {
      recovered.push(PROFILE_B);
    })).resolves.toBe(true);

    expect(isolation.isBlocked(PROFILE_A)).toBe(true);
    expect(isolation.isBlocked(PROFILE_B)).toBe(false);
    expect(recovered).toEqual([PROFILE_B]);
    const publicFailure = publicRunnerFailure(new ProfileRecoveryBlockedError());
    expect(publicFailure).toMatchObject({ status: 503, code: "profile_recovery_blocked" });
    expect(JSON.stringify(publicFailure)).not.toContain("private checkpoint failure");
  });

  it("retries a blocked scope and clears it only after the whole retry succeeds", async () => {
    const isolation = new ProfileRecoveryIsolation();
    isolation.block(PROFILE_A);
    const retry = vi.fn()
      .mockRejectedValueOnce(new Error("checkpoint delete failed"))
      .mockResolvedValueOnce(undefined);

    await expect(isolation.requireMutation(PROFILE_A, retry))
      .rejects.toBeInstanceOf(ProfileRecoveryBlockedError);
    expect(isolation.isBlocked(PROFILE_A)).toBe(true);

    await expect(isolation.requireMutation(PROFILE_A, retry)).resolves.toBeUndefined();
    expect(isolation.isBlocked(PROFILE_A)).toBe(false);
    expect(retry).toHaveBeenCalledTimes(2);
  });

  it("keeps a committed result readable until checkpoint deletion retries successfully", async () => {
    const root = await mkdtemp(join(tmpdir(), "bluey-recovery-delete-"));
    const key = randomBytes(32);
    const profileScope = profilePaths(root, "account-123", "identity-123").scope;
    const resultContext = { requestId: "run-123:initial", profileScope };
    const receipt = { status: "submitted", issues: [] };
    const result = {
      accountId: "account-123",
      applicationId: "application-123",
      applicationIdentityId: "identity-123",
      browserSessionId: "browser-123",
      runId: "run-123",
      receipt,
      receiptAuthority: { leaseToken: "a".repeat(43), fence: 7 },
      receiptBundle: {
        schemaVersion: 1,
        accountId: "account-123",
        applicationId: "application-123",
        applicationIdentityId: "identity-123",
        runId: "run-123",
        runner: "cloud",
        result: receipt,
      },
    };
    const authority = {
      accountId: "account-123",
      applicationId: "application-123",
      applicationIdentityId: "identity-123",
      browserSessionId: "browser-123",
      runId: "run-123",
      leaseToken: "a".repeat(43),
      fence: 7,
      resultContext,
    };
    try {
      await stageResult(root, resultContext, result, key);
      await recoverSubmittedResult(root, key, authority, {
        replaySubmittedFinish: vi.fn(async () => {}),
      });
      const isolation = new ProfileRecoveryIsolation();
      isolation.block(profileScope);
      const checkpointDelete = vi.fn()
        .mockRejectedValueOnce(new Error("directory sync failed"))
        .mockResolvedValueOnce(undefined);
      const replaySubmittedFinish = vi.fn(async () => {
        throw new Error("a committed result must not need retained lease authority");
      });
      const retry = async () => {
        await recoverSubmittedResult(root, key, authority, { replaySubmittedFinish });
        await checkpointDelete();
      };

      await expect(isolation.requireMutation(profileScope, retry))
        .rejects.toBeInstanceOf(ProfileRecoveryBlockedError);
      await expect(readResult(root, resultContext, key)).resolves.toEqual(result);
      await expect(recoverDurableRunResult(root, key, {
        accountId: "account-123",
        applicationId: "application-123",
        applicationIdentityId: "identity-123",
        browserSessionId: "browser-123",
        runId: "run-123",
        requestId: "run-123:initial",
      })).resolves.toEqual(result);
      expect(isolation.isBlocked(profileScope)).toBe(true);

      await expect(isolation.requireMutation(profileScope, retry)).resolves.toBeUndefined();
      expect(isolation.isBlocked(profileScope)).toBe(false);
      expect(replaySubmittedFinish).not.toHaveBeenCalled();
      expect(checkpointDelete).toHaveBeenCalledTimes(2);
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });
});

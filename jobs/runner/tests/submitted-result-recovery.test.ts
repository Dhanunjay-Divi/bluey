import { randomBytes } from "node:crypto";
import { mkdtemp } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it, vi } from "vitest";
import { readResult, readResultState, stageResult } from "../src/result-store.js";
import {
  isRecoverableSubmittedCheckpointPhase,
  recoverSubmittedResult,
  type SubmittedResultRecoveryAuthority,
} from "../src/submitted-result-recovery.js";

const LEASE_TOKEN = "a".repeat(43);
const RESULT_CONTEXT = {
  requestId: "run-123:initial",
  profileScope: "b".repeat(40),
};

describe("submitted result recovery", () => {
  it("replays the exact submitted finish and promotes the staged receipt after restart", async () => {
    const root = await mkdtemp(join(tmpdir(), "bluey-submitted-result-recovery-"));
    const key = randomBytes(32);
    const result = submittedResult();
    await stageResult(root, RESULT_CONTEXT, result, key);
    const replaySubmittedFinish = vi.fn(async () => {});

    await expect(recoverSubmittedResult(
      root,
      key,
      recoveryAuthority(),
      { replaySubmittedFinish },
    )).resolves.toEqual(result);

    expect(replaySubmittedFinish).toHaveBeenCalledWith({
      accountId: "account-123",
      applicationId: "application-123",
      runId: "run-123",
      leaseToken: LEASE_TOKEN,
      fence: 7,
    });
    await expect(readResult(root, RESULT_CONTEXT, key)).resolves.toEqual(result);
  });

  it("recovers trusted confirmation after an activation-uncertain restart", async () => {
    const root = await mkdtemp(join(tmpdir(), "bluey-submitted-result-uncertain-"));
    const key = randomBytes(32);
    const result = submittedResult();
    await stageResult(root, RESULT_CONTEXT, result, key);
    const replaySubmittedFinish = vi.fn(async () => {});

    expect(isRecoverableSubmittedCheckpointPhase("side_effect_unknown")).toBe(true);
    expect(isRecoverableSubmittedCheckpointPhase("final_submit_started")).toBe(false);
    await expect(recoverSubmittedResult(
      root,
      key,
      recoveryAuthority(),
      { replaySubmittedFinish },
    )).resolves.toEqual(result);

    expect(replaySubmittedFinish).toHaveBeenCalledTimes(1);
    await expect(readResult(root, RESULT_CONTEXT, key)).resolves.toEqual(result);
  });

  it("leaves the exact receipt staged when the server cannot confirm submitted", async () => {
    const root = await mkdtemp(join(tmpdir(), "bluey-submitted-result-retry-"));
    const key = randomBytes(32);
    await stageResult(root, RESULT_CONTEXT, submittedResult(), key);
    const replaySubmittedFinish = vi.fn(async () => {
      throw new Error("server unavailable");
    });

    await expect(recoverSubmittedResult(
      root,
      key,
      recoveryAuthority(),
      { replaySubmittedFinish },
    )).rejects.toThrow("server unavailable");

    await expect(readResult(root, RESULT_CONTEXT, key)).resolves.toBeUndefined();
    await expect(readResultState(root, RESULT_CONTEXT, key)).resolves.toMatchObject({
      state: "staged",
      result: submittedResult(),
    });
  });

  it.each([
    ["token", { leaseToken: "z".repeat(43), fence: 7 }],
    ["fence", { leaseToken: LEASE_TOKEN, fence: 8 }],
  ])("fails closed before server replay when staged receipt %s differs", async (_field, authority) => {
    const root = await mkdtemp(join(tmpdir(), "bluey-submitted-result-fence-"));
    const key = randomBytes(32);
    await stageResult(root, RESULT_CONTEXT, submittedResult(authority), key);
    const replaySubmittedFinish = vi.fn(async () => {});

    await expect(recoverSubmittedResult(
      root,
      key,
      recoveryAuthority(),
      { replaySubmittedFinish },
    )).rejects.toMatchObject({ code: "result_promotion_conflict" });

    expect(replaySubmittedFinish).not.toHaveBeenCalled();
    await expect(readResult(root, RESULT_CONTEXT, key)).resolves.toBeUndefined();
  });

  it("rejects a submitted receipt bundle bound to another application", async () => {
    const root = await mkdtemp(join(tmpdir(), "bluey-submitted-result-application-"));
    const key = randomBytes(32);
    const result = submittedResult();
    result.receiptBundle.applicationId = "application-other";
    await stageResult(root, RESULT_CONTEXT, result, key);
    const replaySubmittedFinish = vi.fn(async () => {});

    await expect(recoverSubmittedResult(
      root,
      key,
      recoveryAuthority(),
      { replaySubmittedFinish },
    )).rejects.toMatchObject({ code: "result_promotion_conflict" });

    expect(replaySubmittedFinish).not.toHaveBeenCalled();
    await expect(readResult(root, RESULT_CONTEXT, key)).resolves.toBeUndefined();
  });

  it("never replays a non-submitted staged result as a submitted finish", async () => {
    const root = await mkdtemp(join(tmpdir(), "bluey-submitted-result-status-"));
    const key = randomBytes(32);
    const result = submittedResult();
    result.receipt.status = "failed";
    await stageResult(root, RESULT_CONTEXT, result, key);
    const replaySubmittedFinish = vi.fn(async () => {});

    await expect(recoverSubmittedResult(
      root,
      key,
      recoveryAuthority(),
      { replaySubmittedFinish },
    )).rejects.toMatchObject({ code: "result_promotion_conflict" });

    expect(replaySubmittedFinish).not.toHaveBeenCalled();
    await expect(readResult(root, RESULT_CONTEXT, key)).resolves.toBeUndefined();
  });

  it("returns an exact committed result without depending on a retained lease row", async () => {
    const root = await mkdtemp(join(tmpdir(), "bluey-submitted-result-idempotent-"));
    const key = randomBytes(32);
    const result = submittedResult();
    await stageResult(root, RESULT_CONTEXT, result, key);
    const firstReplay = vi.fn(async () => {});
    await recoverSubmittedResult(root, key, recoveryAuthority(), {
      replaySubmittedFinish: firstReplay,
    });

    const restartedReplay = vi.fn(async () => {});
    await expect(recoverSubmittedResult(root, key, recoveryAuthority(), {
      replaySubmittedFinish: restartedReplay,
    })).resolves.toEqual(result);

    expect(restartedReplay).not.toHaveBeenCalled();
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

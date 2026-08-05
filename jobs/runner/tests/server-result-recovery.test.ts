import { randomBytes } from "node:crypto";
import { mkdtemp } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { profilePaths } from "../src/profile-store.js";
import { stageResult, writeResult } from "../src/result-store.js";
import { recoverDurableRunResult } from "../src/server.js";

describe("runner durable result recovery", () => {
  it("returns only the exact committed account, profile, and request result", async () => {
    const dataRoot = await mkdtemp(join(tmpdir(), "bluey-runner-recovery-"));
    const encryptionKey = randomBytes(32);
    const accountId = "account-123";
    const applicationIdentityId = "identity-123";
    const browserSessionId = "browser-123";
    const requestId = "run-123:initial";
    const profileScope = profilePaths(dataRoot, accountId, applicationIdentityId).scope;
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
    await writeResult(dataRoot, { profileScope, requestId }, result, encryptionKey);

    await expect(recoverDurableRunResult(dataRoot, encryptionKey, {
      accountId,
      applicationId: "application-123",
      applicationIdentityId,
      browserSessionId,
      runId: "run-123",
      requestId,
    })).resolves.toEqual(result);
    await expect(recoverDurableRunResult(dataRoot, encryptionKey, {
      accountId,
      applicationId: "application-456",
      applicationIdentityId,
      browserSessionId,
      runId: "run-123",
      requestId,
    })).rejects.toMatchObject({ code: "result_promotion_conflict" });
    await expect(recoverDurableRunResult(dataRoot, encryptionKey, {
      accountId,
      applicationId: "application-123",
      applicationIdentityId,
      browserSessionId: "browser-456",
      runId: "run-123",
      requestId,
    })).rejects.toMatchObject({ code: "result_promotion_conflict" });
    await expect(recoverDurableRunResult(dataRoot, encryptionKey, {
      accountId,
      applicationId: "application-123",
      applicationIdentityId: "identity-456",
      browserSessionId,
      runId: "run-123",
      requestId,
    })).resolves.toBeUndefined();
    await expect(recoverDurableRunResult(dataRoot, encryptionKey, {
      accountId,
      applicationId: "application-123",
      applicationIdentityId,
      browserSessionId,
      runId: "run-123",
      requestId: "run-123:resume:1",
    })).resolves.toBeUndefined();
  });

  it("rejects a committed receipt whose bundle belongs to another application", async () => {
    const dataRoot = await mkdtemp(join(tmpdir(), "bluey-runner-cross-application-"));
    const encryptionKey = randomBytes(32);
    const accountId = "account-123";
    const applicationIdentityId = "identity-123";
    const browserSessionId = "browser-123";
    const requestId = "run-123:initial";
    const profileScope = profilePaths(dataRoot, accountId, applicationIdentityId).scope;
    const receipt = { status: "submitted", issues: [] };
    await writeResult(dataRoot, { profileScope, requestId }, {
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
    }, encryptionKey);

    await expect(recoverDurableRunResult(dataRoot, encryptionKey, {
      accountId,
      applicationId: "application-123",
      applicationIdentityId,
      browserSessionId,
      runId: "run-123",
      requestId,
    })).rejects.toMatchObject({ code: "result_promotion_conflict" });
  });

  it("never exposes a staged result and rejects unbounded lookup input", async () => {
    const dataRoot = await mkdtemp(join(tmpdir(), "bluey-runner-recovery-stage-"));
    const encryptionKey = randomBytes(32);
    const accountId = "account-123";
    const applicationIdentityId = "identity-123";
    const browserSessionId = "browser-123";
    const requestId = "run-123:initial";
    const profileScope = profilePaths(dataRoot, accountId, applicationIdentityId).scope;
    await stageResult(
      dataRoot,
      { profileScope, requestId },
      { receipt: { status: "submitted", issues: [] } },
      encryptionKey,
    );

    await expect(recoverDurableRunResult(dataRoot, encryptionKey, {
      accountId,
      applicationId: "application-123",
      applicationIdentityId,
      browserSessionId,
      runId: "run-123",
      requestId,
    })).resolves.toBeUndefined();
    await expect(recoverDurableRunResult(dataRoot, encryptionKey, {
      accountId,
      applicationId: "application-123",
      applicationIdentityId,
      browserSessionId,
      runId: "run-123",
      requestId: "../../another-profile",
    })).rejects.toThrow("Invalid durable result request");
    await expect(recoverDurableRunResult(dataRoot, encryptionKey, {
      accountId,
      applicationId: "application-123",
      applicationIdentityId,
      browserSessionId,
      runId: "run-456",
      requestId,
    })).rejects.toThrow("Invalid durable result request");
    await expect(recoverDurableRunResult(dataRoot, encryptionKey, {
      accountId,
      applicationId: "application-123",
      applicationIdentityId,
      browserSessionId,
      runId: "run-123",
      requestId,
      unexpected: "secret",
    })).rejects.toThrow("Invalid durable result request");
  });
});

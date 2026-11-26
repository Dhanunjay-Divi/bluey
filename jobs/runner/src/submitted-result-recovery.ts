import { timingSafeEqual } from "node:crypto";
import type { ExecutionLeaseClient } from "./execution-lease.js";
import {
  promoteManagedStagedResult,
  promoteStagedResult,
  readManagedResultState,
  readResultState,
  ResultStoreError,
  type DurableResultContext,
} from "./result-store.js";
import type { ManagedResultStorage } from "./subject-storage-manager.js";
import { requestIdMatchesRun } from "./run-checkpoint-store.js";

export interface SubmittedResultRecoveryAuthority {
  accountId: string;
  applicationId: string;
  applicationIdentityId: string;
  browserSessionId: string;
  runId: string;
  leaseToken: string;
  fence: number;
  resultContext: DurableResultContext;
}

export interface DurableResultBinding {
  accountId: string;
  applicationId: string;
  applicationIdentityId: string;
  browserSessionId: string;
  runId: string;
}

export function isRecoverableSubmittedCheckpointPhase(
  phase: string,
): phase is "final_submit_activated" | "side_effect_unknown" {
  return phase === "final_submit_activated" || phase === "side_effect_unknown";
}

/**
 * Recover a receipt that was durably staged before the server accepted the
 * terminal submitted lease transition. The staged payload remains hidden from
 * normal result reads until the exact token/fence finish replay succeeds.
 */
export async function recoverSubmittedResult<T>(
  root: string,
  key: Buffer,
  authority: SubmittedResultRecoveryAuthority,
  client: Pick<ExecutionLeaseClient, "replaySubmittedFinish">,
): Promise<T | undefined> {
  if (!validRecoveryAuthority(authority)) {
    throw new ResultStoreError("result_promotion_conflict");
  }
  const stored = await readResultState<T>(root, authority.resultContext, key);
  if (!stored) return undefined;
  assertRecoverableSubmittedResult(stored.result, authority);
  if (stored.state === "committed") return stored.result;
  await client.replaySubmittedFinish({
    accountId: authority.accountId,
    applicationId: authority.applicationId,
    runId: authority.runId,
    leaseToken: authority.leaseToken,
    fence: authority.fence,
  });
  return promoteStagedResult<T>(
    root,
    authority.resultContext,
    stored.resultSha256,
    key,
  );
}

/**
 * Retained-capability variant used by the production v2 subject-storage
 * layout. The lease replay and exact staged-result binding are identical to
 * the legacy recovery path; only storage access crosses the native boundary.
 */
export async function recoverManagedSubmittedResult<T>(
  storage: ManagedResultStorage,
  key: Buffer,
  authority: SubmittedResultRecoveryAuthority,
  client: Pick<ExecutionLeaseClient, "replaySubmittedFinish">,
): Promise<T | undefined> {
  if (!validRecoveryAuthority(authority)) {
    throw new ResultStoreError("result_promotion_conflict");
  }
  const stored = await readManagedResultState<T>(
    storage,
    authority.resultContext,
    key,
  );
  if (!stored) return undefined;
  assertRecoverableSubmittedResult(stored.result, authority);
  if (stored.state === "committed") return stored.result;
  await client.replaySubmittedFinish({
    accountId: authority.accountId,
    applicationId: authority.applicationId,
    runId: authority.runId,
    leaseToken: authority.leaseToken,
    fence: authority.fence,
  });
  return promoteManagedStagedResult<T>(
    storage,
    authority.resultContext,
    stored.resultSha256,
    key,
  );
}

function assertRecoverableSubmittedResult(
  value: unknown,
  authority: SubmittedResultRecoveryAuthority,
): void {
  assertDurableResultBinding(value, authority);
  const result = value as Record<string, unknown>;
  const receipt = result.receipt as Record<string, unknown>;
  if (
    receipt.status !== "submitted" ||
    !result.receiptAuthority ||
    typeof result.receiptAuthority !== "object" ||
    Array.isArray(result.receiptAuthority)
  ) {
    throw new ResultStoreError("result_promotion_conflict");
  }
  const receiptAuthority = result.receiptAuthority as Record<string, unknown>;
  if (
    typeof receiptAuthority.leaseToken !== "string" ||
    !Number.isSafeInteger(receiptAuthority.fence) ||
    receiptAuthority.fence !== authority.fence ||
    !secretMatches(receiptAuthority.leaseToken, authority.leaseToken)
  ) {
    throw new ResultStoreError("result_promotion_conflict");
  }
}

export function assertDurableResultBinding(
  value: unknown,
  binding: DurableResultBinding,
): void {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new ResultStoreError("result_promotion_conflict");
  }
  const result = value as Record<string, unknown>;
  const receipt = result.receipt;
  const applicationId = result.applicationId;
  const receiptStatus =
    receipt && typeof receipt === "object" && !Array.isArray(receipt)
      ? (receipt as Record<string, unknown>).status
      : undefined;
  if (
    result.accountId !== binding.accountId ||
    applicationId !== binding.applicationId ||
    result.applicationIdentityId !== binding.applicationIdentityId ||
    result.browserSessionId !== binding.browserSessionId ||
    result.runId !== binding.runId ||
    !receipt ||
    typeof receipt !== "object" ||
    Array.isArray(receipt) ||
    !["submitted", "needs_input", "failed"].includes(String(receiptStatus))
  ) {
    throw new ResultStoreError("result_promotion_conflict");
  }
  if (receiptStatus !== "submitted") return;

  const receiptBundle = result.receiptBundle;
  if (
    !receiptBundle ||
    typeof receiptBundle !== "object" ||
    Array.isArray(receiptBundle)
  ) {
    throw new ResultStoreError("result_promotion_conflict");
  }
  const bundle = receiptBundle as Record<string, unknown>;
  const bundledResult = bundle.result;
  const receiptAuthority = result.receiptAuthority;
  if (
    bundle.schemaVersion !== 1 ||
    bundle.accountId !== binding.accountId ||
    bundle.applicationId !== applicationId ||
    bundle.runId !== binding.runId ||
    bundle.runner !== "cloud" ||
    bundle.applicationIdentityId !== binding.applicationIdentityId ||
    !bundledResult ||
    typeof bundledResult !== "object" ||
    Array.isArray(bundledResult) ||
    (bundledResult as Record<string, unknown>).status !== "submitted" ||
    JSON.stringify(bundledResult) !== JSON.stringify(receipt) ||
    !validReceiptAuthority(receiptAuthority)
  ) {
    throw new ResultStoreError("result_promotion_conflict");
  }
}

function validReceiptAuthority(value: unknown): boolean {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false;
  const authority = value as Record<string, unknown>;
  return (
    Object.keys(authority).length === 2 &&
    typeof authority.leaseToken === "string" &&
    /^[A-Za-z0-9_-]{43}$/.test(authority.leaseToken) &&
    Number.isSafeInteger(authority.fence) &&
    Number(authority.fence) > 0
  );
}

function secretMatches(left: string, right: string): boolean {
  const leftBytes = Buffer.from(left, "utf8");
  const rightBytes = Buffer.from(right, "utf8");
  return (
    leftBytes.length === rightBytes.length &&
    leftBytes.length > 0 &&
    timingSafeEqual(leftBytes, rightBytes)
  );
}

function validRecoveryAuthority(
  authority: SubmittedResultRecoveryAuthority,
): boolean {
  const requestId = authority.resultContext.requestId;
  const validRequestId = requestIdMatchesRun(authority.runId, requestId);
  return (
    /^[A-Za-z0-9_-]{3,160}$/.test(authority.accountId) &&
    /^[A-Za-z0-9_-]{3,160}$/.test(authority.applicationId) &&
    /^[A-Za-z0-9_-]{3,160}$/.test(authority.applicationIdentityId) &&
    /^[A-Za-z0-9_-]{3,160}$/.test(authority.browserSessionId) &&
    /^[A-Za-z0-9_-]{3,160}$/.test(authority.runId) &&
    /^[A-Za-z0-9_-]{43}$/.test(authority.leaseToken) &&
    Number.isSafeInteger(authority.fence) &&
    authority.fence > 0 &&
    validRequestId
  );
}

import type {
  ActiveExecutionLease,
  ExecutionLeaseClaim,
  ExecutionLeaseClient,
  ExecutionLeaseFinishOutcome,
} from "./execution-lease.js";

export interface ReceiptLike {
  receipt: { status: "submitted" | "needs_input" | "failed" };
}

export class LeasedRunError extends Error {
  constructor(readonly outcome: "failed" | "side_effect_unknown" | "submitted_result_pending") {
    super(`Leased runner execution ended as ${outcome}.`);
    this.name = "LeasedRunError";
  }
}

export async function beginLeasedRun<T>(
  client: Pick<ExecutionLeaseClient, "claim">,
  claim: ExecutionLeaseClaim,
  execute: (lease: ActiveExecutionLease) => Promise<T>,
  onExecutionFailure?: (lease: ActiveExecutionLease) => Promise<never>,
): Promise<{ lease: ActiveExecutionLease; execution: T }> {
  const lease = await client.claim(claim);
  try {
    const execution = await execute(lease);
    return { lease, execution };
  } catch (error) {
    if (onExecutionFailure) return onExecutionFailure(lease);
    throw error;
  }
}

export function terminalLeaseOutcome(
  status: ReceiptLike["receipt"]["status"],
  lease: Pick<ActiveExecutionLease, "finalSubmitAttempted">,
): ExecutionLeaseFinishOutcome {
  if (status === "submitted") return lease.finalSubmitAttempted ? "submitted" : "side_effect_unknown";
  if (lease.finalSubmitAttempted) return "side_effect_unknown";
  if (status === "failed") return "failed";
  return "released";
}

export async function finalizeLeasedRun<T>(options: {
  lease: ActiveExecutionLease;
  intendedOutcome: Exclude<ExecutionLeaseFinishOutcome, "side_effect_unknown">;
  cleanup(): Promise<void>;
  stage(): Promise<void>;
  commit(): Promise<T>;
}): Promise<T> {
  if (options.lease.finalSubmitAttempted && options.intendedOutcome !== "submitted") {
    return abortLeasedRun(options.lease, options.cleanup);
  }

  try {
    await options.cleanup();
    await options.stage();
  } catch {
    return abortLeasedRun(options.lease, async () => {});
  }

  try {
    await options.lease.finish(options.intendedOutcome);
  } catch {
    throw new LeasedRunError(options.lease.finalSubmitAttempted ? "side_effect_unknown" : "failed");
  }
  try {
    return await options.commit();
  } catch {
    throw new LeasedRunError(
      options.intendedOutcome === "submitted" ? "submitted_result_pending" : "failed",
    );
  }
}

export async function abortLeasedRun(
  lease: ActiveExecutionLease,
  cleanup: () => Promise<void>,
): Promise<never> {
  const outcome = lease.finalSubmitAttempted ? "side_effect_unknown" : "failed";
  try {
    await cleanup();
  } catch {
    // Lease finalization is still required even when browser/profile cleanup fails.
  }
  try {
    await lease.finish(outcome);
  } catch {
    // The caller receives a redacted terminal classification either way.
  }
  throw new LeasedRunError(outcome);
}

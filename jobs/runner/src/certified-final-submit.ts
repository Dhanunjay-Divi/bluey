import type { FinalSubmitProof } from "@bluey/jobs-automation";
import type { ActiveExecutionLease } from "./execution-lease.js";

type FinalSubmitAuthorizer = Pick<ActiveExecutionLease, "beforeFinalSubmit">;
type FinalSubmitCheckpointAuthority = Pick<
  ActiveExecutionLease,
  "finalSubmitAuthorized"
>;

/**
 * Phase B must authorize the one-use submit before a durable irreversible
 * checkpoint can be written. The caller may activate the provider control
 * only after this boundary resolves.
 */
export async function authorizeCloudFinalSubmitBeforeCheckpoint(
  lease: FinalSubmitAuthorizer,
  proof: FinalSubmitProof,
  writeCheckpoint: () => Promise<void>,
): Promise<void> {
  await lease.beforeFinalSubmit(proof);
  await writeCheckpoint();
}

/**
 * A failed Phase B attempt is single-shot, but it is not durable local proof
 * that submit was authorized. Only a successful response may retain or write
 * an irreversible cloud checkpoint.
 */
export function hasCloudIrreversibleCheckpointAuthority(
  lease: FinalSubmitCheckpointAuthority,
): boolean {
  return lease.finalSubmitAuthorized;
}

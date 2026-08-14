import type { FinalSubmitProof } from "@bluey/jobs-automation";
import type { ActiveExecutionLease } from "./execution-lease.js";

type FinalSubmitAuthorizer = Pick<ActiveExecutionLease, "beforeFinalSubmit">;
type FinalSubmitCheckpointAuthority = Pick<
  ActiveExecutionLease,
  "finalSubmitAuthorized"
>;

/**
 * Record the irreversible-attempt boundary before Phase B I/O. A lost Phase B
 * success response must remain recoverable after a hard restart even when the
 * caller cannot write a second checkpoint. The caller may activate the
 * provider control only after Phase B resolves successfully.
 */
export async function authorizeCloudFinalSubmitBeforeCheckpoint(
  lease: FinalSubmitAuthorizer,
  proof: FinalSubmitProof,
  writeCheckpoint: () => Promise<void>,
): Promise<void> {
  await writeCheckpoint();
  await lease.beforeFinalSubmit(proof);
}

/**
 * A failed Phase B attempt does not expose certified click authority. The
 * conservative pre-I/O checkpoint is still retained because a transport
 * failure cannot distinguish a server rejection from a lost success response.
 */
export function hasCloudIrreversibleCheckpointAuthority(
  lease: FinalSubmitCheckpointAuthority,
): boolean {
  return lease.finalSubmitAuthorized;
}

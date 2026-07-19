import type { FinalSubmitActivationOutcome } from "@bluey/jobs-automation";
import {
  durableFinalSubmitHooks,
  type FinalSubmitHooks,
} from "./irreversible-submit.js";
import {
  localRunAuthorization,
  type LocalRunDelivery,
} from "./local-run-contracts.js";

/**
 * Re-checks the claimed run authority immediately before the durable marker
 * and irreversible employer-facing click. Result delivery is too late to be
 * the first expiry check because the application may already be submitted.
 */
export function authorizedFinalSubmitHooks(
  runDirectory: string,
  delivery: LocalRunDelivery,
): FinalSubmitHooks {
  const durable = durableFinalSubmitHooks(runDirectory);
  return {
    async beforeFinalSubmit(): Promise<void> {
      localRunAuthorization(delivery, "result");
      await durable.beforeFinalSubmit();
    },
    async afterFinalSubmit(outcome: FinalSubmitActivationOutcome): Promise<void> {
      await durable.afterFinalSubmit(outcome);
    },
  };
}

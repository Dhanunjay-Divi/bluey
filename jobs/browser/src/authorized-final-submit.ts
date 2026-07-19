import type { FinalSubmitActivationOutcome } from "@bluey/jobs-automation";
import {
  durableFinalSubmitHooks,
  type FinalSubmitHooks,
} from "./irreversible-submit.js";
import {
  localRunAuthorization,
  type LocalRunDelivery,
  type StartRunRequest,
} from "./local-run-contracts.js";

export interface FinalSubmitAuthorizationRequest {
  readonly accountId: string;
  readonly applicationId: string;
  readonly applicationIdentityId: string;
  readonly runId: string;
  readonly apiOrigin: string;
  readonly capability: string;
  readonly expiresAtMs: number;
}

export type FinalSubmitAuthorizer = (
  request: Readonly<FinalSubmitAuthorizationRequest>,
) => Promise<void>;

/**
 * Re-checks the claimed run authority immediately before the durable marker
 * and irreversible employer-facing click. Result delivery is too late to be
 * the first expiry check because the application may already be submitted.
 * A live server authorizer can be injected without coupling this fence to a
 * route; any denial or transport error rejects before the marker and click.
 */
export function authorizedFinalSubmitHooks(
  runDirectory: string,
  request: Pick<StartRunRequest,
    "accountId" | "applicationId" | "applicationIdentityId" | "runId">,
  delivery: LocalRunDelivery,
  authorize?: FinalSubmitAuthorizer,
): FinalSubmitHooks {
  const durable = durableFinalSubmitHooks(runDirectory);
  return {
    async beforeFinalSubmit(): Promise<void> {
      const { capability } = localRunAuthorization(delivery, "result");
      await authorize?.(Object.freeze({
        accountId: request.accountId,
        applicationId: request.applicationId,
        applicationIdentityId: request.applicationIdentityId,
        runId: request.runId,
        apiOrigin: delivery.apiOrigin,
        capability,
        expiresAtMs: delivery.capabilities.expiresAtMs,
      }));
      // A network authorizer can outlive a near-expiry token. Re-check locally
      // before acquiring the irreversible marker.
      localRunAuthorization(delivery, "result");
      await durable.beforeFinalSubmit();
    },
    async afterFinalSubmit(outcome: FinalSubmitActivationOutcome): Promise<void> {
      await durable.afterFinalSubmit(outcome);
    },
  };
}

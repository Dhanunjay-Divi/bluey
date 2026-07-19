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
import { LocalBrowserError } from "./local-failure.js";

const AUTHORIZE_SUBMIT_TIMEOUT_MS = 10_000;

export type FinalSubmitFetch = (
  input: string | URL | Request,
  init?: RequestInit,
) => Promise<Response>;

/**
 * Re-checks the claimed run authority immediately before the durable marker
 * and irreversible employer-facing click. Result delivery is too late to be
 * the first expiry check because the application may already be submitted.
 * The submit capability is checked against live server authority on every
 * employer-facing attempt. Any denial, malformed response, timeout, or
 * transport error rejects before the marker and click.
 */
export function authorizedFinalSubmitHooks(
  runDirectory: string,
  request: Pick<StartRunRequest, "runId">,
  delivery: LocalRunDelivery,
  fetchImpl: FinalSubmitFetch = fetch,
): FinalSubmitHooks {
  const durable = durableFinalSubmitHooks(runDirectory);
  return {
    async beforeFinalSubmit(): Promise<void> {
      const { capability } = localRunAuthorization(delivery, "submit");
      await authorizeFinalSubmit(
        delivery.apiOrigin,
        request.runId,
        capability,
        fetchImpl,
      );
      // The request can outlive a near-expiry capability. Re-check locally
      // immediately before acquiring the irreversible marker.
      localRunAuthorization(delivery, "submit");
      await durable.beforeFinalSubmit();
    },
    async afterFinalSubmit(outcome: FinalSubmitActivationOutcome): Promise<void> {
      await durable.afterFinalSubmit(outcome);
    },
  };
}

async function authorizeFinalSubmit(
  apiOrigin: string,
  runId: string,
  capability: string,
  fetchImpl: FinalSubmitFetch,
): Promise<void> {
  let response: Response;
  try {
    response = await fetchImpl(
      `${apiOrigin}/api/jobs/local-runs/${encodeURIComponent(runId)}/authorize-submit`,
      {
        method: "POST",
        headers: {
          Accept: "application/json",
          "Content-Type": "application/json",
        },
        body: JSON.stringify({ capability }),
        cache: "no-store",
        credentials: "omit",
        redirect: "error",
        signal: AbortSignal.timeout(AUTHORIZE_SUBMIT_TIMEOUT_MS),
      },
    );
  } catch {
    throw new LocalBrowserError("launch_expired");
  }
  if (!response.ok) throw new LocalBrowserError("launch_expired");

  let body: unknown;
  try {
    body = await response.json();
  } catch {
    throw new LocalBrowserError("launch_expired");
  }
  if (!body || typeof body !== "object" || Array.isArray(body)) {
    throw new LocalBrowserError("launch_expired");
  }
  const authorization = body as Record<string, unknown>;
  if (authorization.authorized !== true
    || !Number.isSafeInteger(authorization.authorizedAtMs)
    || (authorization.authorizedAtMs as number) <= 0) {
    throw new LocalBrowserError("launch_expired");
  }
}

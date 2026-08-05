import {
  ApprovedExecutionIntegrityError,
  assertMaterializedDocumentSnapshot,
  assertApprovedExecutionChecksum,
  createFinalSubmitProof,
  FinalSubmitProofError,
  type FinalSubmitActivationOutcome,
  type MaterializedDocuments,
  type ProviderFinalSubmitProof,
} from "@bluey/jobs-automation";
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
  request: Pick<StartRunRequest, "runId" | "job" | "packet">,
  delivery: LocalRunDelivery,
  documents: MaterializedDocuments,
  currentPageUrl: () => string,
  fetchImpl: FinalSubmitFetch = fetch,
): FinalSubmitHooks {
  const durable = durableFinalSubmitHooks(runDirectory);
  return {
    async beforeFinalSubmit(providerProof: ProviderFinalSubmitProof): Promise<void> {
      let finalSubmitProof;
      try {
        if (!request.job) throw new FinalSubmitProofError();
        assertApprovedExecutionChecksum(request.packet, request.job);
        localRunAuthorization(delivery, "submit");
        await assertMaterializedDocumentSnapshot(documents.resume);
        if (documents.coverLetter) {
          await assertMaterializedDocumentSnapshot(documents.coverLetter);
        }
        finalSubmitProof = createFinalSubmitProof(providerProof, {
          resume: {
            versionId: request.packet.resumeVersionId,
            sha256: documents.resume.sha256,
          },
          ...(documents.coverLetter ? {
            coverLetter: { sha256: documents.coverLetter.sha256 },
          } : {}),
        }, {
          approvedCanonicalUrl: request.job.canonicalUrl,
          pageUrl: currentPageUrl(),
        });
        assertApprovedExecutionChecksum(request.packet, request.job);
      } catch (error) {
        if (error instanceof FinalSubmitProofError
          || error instanceof ApprovedExecutionIntegrityError) {
          throw new LocalBrowserError("launch_mismatch");
        }
        throw error;
      }
      const { capability } = localRunAuthorization(delivery, "submit");
      // The marker is the local crash boundary. It must reach durable storage
      // before the server can advance this run to click_started.
      await durable.beforeFinalSubmit(providerProof);
      await authorizeFinalSubmit(
        delivery.apiOrigin,
        request.runId,
        capability,
        finalSubmitProof,
        fetchImpl,
      );
      // The request can outlive a near-expiry capability. Re-check locally
      // immediately before allowing the employer-facing click.
      localRunAuthorization(delivery, "submit");
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
  finalSubmitProof: ReturnType<typeof createFinalSubmitProof>,
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
        body: JSON.stringify({
          capability,
          final_submit_proof: finalSubmitProof,
        }),
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

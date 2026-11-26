import {
  ApprovedExecutionIntegrityError,
  assertMaterializedDocumentSnapshot,
  assertApprovedExecutionChecksum,
  createFinalSubmitProof,
  FinalSubmitProofError,
  type ApplicationPacket,
  type AtsCertifiedReceiptAuthority,
  type FinalSubmitActivationOutcome,
  type FinalSubmitProof,
  type MaterializedDocuments,
  type NormalizedJob,
  type ProviderFinalSubmitProof,
} from "@bluey/jobs-automation";
import {
  durableFinalSubmitHooks,
  FinalSubmitMarkerError,
  finalSubmitMarkerExists,
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

export type FinalSubmitTimeoutSignal = () => AbortSignal;

export interface AuthorizedFinalSubmitHooks extends FinalSubmitHooks {
  atsCertifiedReceiptAuthority(): AtsCertifiedReceiptAuthority | undefined;
}

export interface CertifiedAutoProviderApproval {
  adapter: "greenhouse" | "lever";
  adapterVersion: string;
}

/**
 * Certified Auto may clear the provider's beta review gate, but only from the
 * checksum-bound schema-v3 admission. Phase B remains the separate pre-click
 * authority decision performed by `authorizedFinalSubmitHooks`.
 */
export function certifiedAutoProviderApproval(
  packet: ApplicationPacket,
  job: NormalizedJob,
): CertifiedAutoProviderApproval | undefined {
  assertApprovedExecutionChecksum(packet, job);
  const admission = packet.approvedExecutionAdmission;
  const certification = admission?.kind === "track_auto_submit"
    ? admission.ats_certification
    : undefined;
  if (packet.approvedExecutionSchemaVersion !== 3
    || !certification
    || certification.provider !== job.source) {
    return undefined;
  }
  return {
    adapter: certification.provider,
    adapterVersion: certification.adapter_version,
  };
}

/**
 * Re-checks the claimed run authority immediately before the durable marker
 * and irreversible employer-facing click. Result delivery is too late to be
 * the first expiry check because the application may already be submitted.
 * The submit capability is checked against live server authority on every
 * employer-facing attempt. Any denial, malformed response, timeout, or
 * transport error rejects before the local marker and click. Because the
 * server may already have committed Phase B, an ambiguous response is terminal
 * side-effect uncertainty and must preserve recovery state.
 */
export function authorizedFinalSubmitHooks(
  runDirectory: string,
  request: Pick<
    StartRunRequest,
    "accountId" | "applicationId" | "runId" | "job" | "packet"
  >,
  delivery: LocalRunDelivery,
  documents: MaterializedDocuments,
  currentPageUrl: () => string,
  fetchImpl: FinalSubmitFetch = fetch,
  timeoutSignal: FinalSubmitTimeoutSignal = () =>
    AbortSignal.timeout(AUTHORIZE_SUBMIT_TIMEOUT_MS),
): AuthorizedFinalSubmitHooks {
  const durable = durableFinalSubmitHooks(runDirectory);
  let certifiedReceiptAuthority: AtsCertifiedReceiptAuthority | undefined;
  return {
    async beforeFinalSubmit(providerProof: ProviderFinalSubmitProof): Promise<void> {
      if (await finalSubmitMarkerExists(runDirectory)) {
        throw new FinalSubmitMarkerError("submit_authority_exists");
      }
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
        }, request.packet.approvedExecutionAdmission);
        assertApprovedExecutionChecksum(request.packet, request.job);
      } catch (error) {
        if (error instanceof FinalSubmitProofError
          || error instanceof ApprovedExecutionIntegrityError) {
          throw new LocalBrowserError("launch_mismatch");
        }
        throw error;
      }
      const { capability } = localRunAuthorization(delivery, "submit");
      // Server Phase B consumes the one-use authority before the local crash
      // marker can exist. An explicit denial is safely pre-marker. An
      // unavailable response also cannot reach the employer submit control,
      // but may hide a committed Phase B and is therefore non-retryable.
      const receiptAuthority = await authorizeFinalSubmit(
        delivery.apiOrigin,
        request,
        capability,
        finalSubmitProof,
        fetchImpl,
        timeoutSignal,
      );
      certifiedReceiptAuthority = receiptAuthority;
      // The request can outlive a near-expiry capability. Re-check locally
      // immediately before allowing the employer-facing click. Phase B has
      // already committed at this point, so any local failure is terminal and
      // cannot be reported as a retryable launch failure.
      try {
        localRunAuthorization(delivery, "submit");
        await durable.beforeFinalSubmit(providerProof);
      } catch {
        throw new LocalBrowserError("submit_outcome_unknown");
      }
    },
    async afterFinalSubmit(outcome: FinalSubmitActivationOutcome): Promise<void> {
      await durable.afterFinalSubmit(outcome);
    },
    atsCertifiedReceiptAuthority(): AtsCertifiedReceiptAuthority | undefined {
      return certifiedReceiptAuthority
        ? structuredClone(certifiedReceiptAuthority)
        : undefined;
    },
  };
}

async function authorizeFinalSubmit(
  apiOrigin: string,
  request: Pick<
    StartRunRequest,
    "accountId" | "applicationId" | "runId" | "job" | "packet"
  >,
  capability: string,
  finalSubmitProof: ReturnType<typeof createFinalSubmitProof>,
  fetchImpl: FinalSubmitFetch,
  timeoutSignal: FinalSubmitTimeoutSignal,
): Promise<AtsCertifiedReceiptAuthority | undefined> {
  let response: Response;
  try {
    response = await fetchImpl(
      `${apiOrigin}/api/jobs/local-runs/${encodeURIComponent(request.runId)}/authorize-submit`,
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
        signal: timeoutSignal(),
      },
    );
  } catch {
    throw new LocalBrowserError("submit_outcome_unknown");
  }
  if (!response.ok) {
    if (response.status >= 400 && response.status < 500) {
      throw new LocalBrowserError("launch_expired");
    }
    throw new LocalBrowserError("submit_outcome_unknown");
  }

  let body: unknown;
  try {
    body = await response.json();
  } catch {
    throw new LocalBrowserError("submit_outcome_unknown");
  }
  if (!body || typeof body !== "object" || Array.isArray(body)) {
    throw new LocalBrowserError("submit_outcome_unknown");
  }
  const authorization = body as Record<string, unknown>;
  const certified = finalSubmitProof.schemaVersion === 4;
  const expectedKeys = certified
    ? ["atsCertifiedReceiptAuthority", "authorized", "authorizedAtMs"]
    : ["authorized", "authorizedAtMs"];
  if (!hasExactKeys(authorization, expectedKeys)
    || authorization.authorized !== true
    || !Number.isSafeInteger(authorization.authorizedAtMs)
    || (authorization.authorizedAtMs as number) <= 0) {
    throw new LocalBrowserError("submit_outcome_unknown");
  }
  if (!certified) return undefined;
  try {
    return certifiedAuthority(
      authorization.atsCertifiedReceiptAuthority,
      authorization.authorizedAtMs as number,
      request,
      finalSubmitProof,
    );
  } catch {
    throw new LocalBrowserError("submit_outcome_unknown");
  }
}

const SHA256_PATTERN = /^[a-f0-9]{64}$/;
const CERTIFIED_AUTHORITY_KEYS = [
  "accountId",
  "activationGeneration",
  "activationSha256",
  "adapter",
  "adapterBundleSha256",
  "adapterVersion",
  "applicationAttemptId",
  "applicationId",
  "bindingConsumedAtMs",
  "bindingFence",
  "bindingSha256",
  "canaryReservationSha256",
  "layoutObservationSha256",
  "layoutSetSha256",
  "manifestSha256",
  "meteringReservationSha256",
  "phaseBRequestId",
  "provider",
  "rolloutChannel",
  "runId",
  "runnerKind",
  "runnerTargetSha256",
  "schemaVersion",
  "observedSurfaceSha256",
  "targetKeySha256",
] as const;

function certifiedAuthority(
  value: unknown,
  authorizedAtMs: number,
  request: Pick<
    StartRunRequest,
    "accountId" | "applicationId" | "runId" | "job" | "packet"
  >,
  proof: Extract<FinalSubmitProof, { schemaVersion: 4 }>,
): AtsCertifiedReceiptAuthority {
  if (!isRecord(value)
    || !hasExactKeys(value, CERTIFIED_AUTHORITY_KEYS)
    || value.schemaVersion !== 1
    || (value.provider !== "greenhouse" && value.provider !== "lever")
    || (value.adapter !== "greenhouse" && value.adapter !== "lever")
    || value.runnerKind !== "local"
    || (value.rolloutChannel !== "canary" && value.rolloutChannel !== "general")
    || !positiveSafeInteger(value.activationGeneration)
    || !positiveSafeInteger(value.bindingFence)
    || !positiveSafeInteger(value.bindingConsumedAtMs)
    || ![
      value.accountId,
      value.applicationId,
      value.runId,
      value.adapterVersion,
      value.applicationAttemptId,
      value.phaseBRequestId,
    ].every(validId)
    || ![
      value.manifestSha256,
      value.activationSha256,
      value.targetKeySha256,
      value.layoutSetSha256,
      value.layoutObservationSha256,
      value.observedSurfaceSha256,
      value.adapterBundleSha256,
      value.runnerTargetSha256,
      value.bindingSha256,
      value.canaryReservationSha256,
      value.meteringReservationSha256,
    ].every(validSha256)) {
    throw new LocalBrowserError("launch_expired");
  }

  const certification = proof.certification;
  if (value.accountId !== request.accountId
    || value.applicationId !== request.applicationId
    || value.applicationId !== request.packet.applicationId
    || value.runId !== request.runId
    || value.provider !== request.job?.source
    || value.provider !== proof.adapter
    || value.provider !== certification.provider
    || value.adapter !== proof.adapter
    || value.adapterVersion !== proof.adapterVersion
    || value.adapterVersion !== certification.adapterVersion
    || value.manifestSha256 !== certification.manifestSha256
    || value.activationSha256 !== certification.activationSha256
    || value.activationGeneration !== certification.activationGeneration
    || value.targetKeySha256 !== certification.targetKeySha256
    || value.layoutSetSha256 !== certification.layoutSetSha256
    || value.observedSurfaceSha256 !== proof.observedSurface.surfaceSha256
    || value.adapterBundleSha256 !== certification.adapterBundleSha256
    || !certification.runnerTargetSha256s.includes(value.runnerTargetSha256 as string)
    || value.bindingConsumedAtMs !== authorizedAtMs
    || value.bindingConsumedAtMs > certification.expiresAtMs) {
    throw new LocalBrowserError("launch_expired");
  }
  return structuredClone(value) as unknown as AtsCertifiedReceiptAuthority;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

function hasExactKeys(
  value: Record<string, unknown>,
  expected: readonly string[],
): boolean {
  const actual = Object.keys(value).sort();
  const sortedExpected = [...expected].sort();
  return actual.length === sortedExpected.length
    && actual.every((key, index) => key === sortedExpected[index]);
}

function validId(value: unknown): value is string {
  return typeof value === "string"
    && value.length >= 1
    && value.length <= 240
    && value.trim() === value
    && !/[\u0000-\u001f\u007f]/u.test(value);
}

function validSha256(value: unknown): value is string {
  return typeof value === "string" && SHA256_PATTERN.test(value);
}

function positiveSafeInteger(value: unknown): value is number {
  return Number.isSafeInteger(value) && (value as number) > 0;
}

import type {
  ExecutionResult,
  ProviderAdapterRegistryOptions,
  SubmissionReceipt,
} from "@bluey/jobs-automation";
import { certifiedProviderJobKey } from "@bluey/jobs-automation";

export const LOCAL_PROVIDER_APPROVAL_PENDING =
  "Approve this submission in Bluey Jobs, then continue here. This resume link cannot authorize the final click by itself.";

export interface LocalProviderFinalReview {
  adapter: "greenhouse" | "lever";
  adapterVersion: string;
}

const CONFIRMATION_PATTERNS: Record<LocalProviderFinalReview["adapter"], readonly RegExp[]> = {
  greenhouse: [
    /^(?:your )?application (?:has been|was) (?:successfully )?(?:submitted|received)[.!]?$/i,
    /^we(?: have|'ve) received your application[.!]?$/i,
  ],
  lever: [
    /^thank you for submitting your application[.!]?$/i,
    /^(?:your )?application (?:has been|was) (?:successfully )?(?:submitted|received)[.!]?$/i,
    /^we(?: have|'ve) received your application[.!]?$/i,
  ],
};

const NON_CONFIRMATION_PATTERNS = [
  /\balready (?:applied|submitted (?:an|your) application)\b/i,
  /\bapplication (?:was|has been) already submitted\b/i,
  /\b(?:unable|failed) to submit (?:the |your )?application\b/i,
  /\bcould not submit (?:the |your )?application\b/i,
] as const;

export interface LocalProviderConfirmationObservation {
  readonly kind: "manual_submission_observed";
  readonly binding: "exact_job" | "unbound";
}

export type LocalProviderConfirmationDisposition =
  | "continue"
  | "manual_submission_observed"
  | "submit_outcome_unknown";

export function localProviderFinalReview(
  execution: ExecutionResult,
): LocalProviderFinalReview | undefined {
  if (execution.adapter !== "greenhouse" && execution.adapter !== "lever") return undefined;
  const { receipt } = execution;
  const intervention = receipt.intervention;
  if (receipt.status !== "needs_input"
    || receipt.issues.length !== 0
    || intervention?.kind !== "browser_takeover"
    || intervention.resolution?.kind !== "browser_takeover"
    || intervention.resolution.resumeAfter !== true) {
    return undefined;
  }
  return { adapter: execution.adapter, adapterVersion: execution.adapterVersion };
}

export function reconcileLocalProviderConfirmation(
  review: LocalProviderFinalReview,
  expectedCanonicalJobUrl: string,
  bodyText: string,
  confirmationUrl: string,
): LocalProviderConfirmationObservation | undefined {
  const expectedJobKey = providerJobKey(review.adapter, expectedCanonicalJobUrl, "submit");
  const observedJobKey = providerJobKey(review.adapter, confirmationUrl, "confirmation");
  const confirmationText = confirmationEvidence(review.adapter, bodyText);
  if (!confirmationText) return undefined;
  return Object.freeze({
    kind: "manual_submission_observed",
    binding: expectedJobKey && observedJobKey && expectedJobKey === observedJobKey
      ? "exact_job"
      : "unbound",
  });
}

export function localProviderConfirmationDisposition(
  observation: LocalProviderConfirmationObservation | undefined,
  markerExists: boolean,
): LocalProviderConfirmationDisposition {
  if (observation) return "manual_submission_observed";
  return markerExists ? "submit_outcome_unknown" : "continue";
}

export function pendingProviderReviewReceipt(): SubmissionReceipt {
  return {
    status: "needs_input",
    issues: [],
    intervention: {
      kind: "browser_takeover",
      title: "Submission approval required",
      detail: LOCAL_PROVIDER_APPROVAL_PENDING,
      resolution: { kind: "browser_takeover", resumeAfter: true },
    },
  };
}

export function isApprovedLocalResumeAction(
  value: unknown,
  expectedRunId: string,
  nowMs: number = Date.now(),
): boolean {
  if (!value || typeof value !== "object") return false;
  const action = value as Record<string, unknown>;
  return action.action === "approve_submission"
    && action.run_id === expectedRunId
    && typeof action.intervention_id === "string"
    && /^[A-Za-z0-9_-]{3,160}$/.test(action.intervention_id)
    && typeof action.expires_at_ms === "number"
    && Number.isSafeInteger(action.expires_at_ms)
    && action.expires_at_ms > nowMs;
}

export function providerOptionsForApprovedReview(
  review: LocalProviderFinalReview,
): ProviderAdapterRegistryOptions {
  return review.adapter === "greenhouse"
    ? { greenhouse: { finalReviewApproval: async () => true } }
    : { lever: { finalReviewApproval: async () => true } };
}

function confirmationEvidence(
  adapter: LocalProviderFinalReview["adapter"],
  bodyText: string,
): string | undefined {
  const lines = bodyText.split(/\n+/u).map((line) => line.replace(/\s+/g, " ").trim()).filter(Boolean);
  for (const line of lines) {
    const sentences = line.match(/[^.!?]+[.!?]?/gu) ?? [];
    const candidates = [line, ...sentences.map((sentence) => sentence.trim())];
    const evidence = candidates.find((candidate) => (
      CONFIRMATION_PATTERNS[adapter].some((pattern) => pattern.test(candidate))
      || NON_CONFIRMATION_PATTERNS.some((pattern) => pattern.test(candidate))
    ));
    if (evidence) return evidence.slice(0, 500);
  }
  return undefined;
}

function providerJobKey(
  adapter: LocalProviderFinalReview["adapter"],
  rawUrl: string,
  purpose: "submit" | "confirmation",
): string | undefined {
  try {
    return certifiedProviderJobKey(adapter, rawUrl, purpose);
  } catch {
    return undefined;
  }
}

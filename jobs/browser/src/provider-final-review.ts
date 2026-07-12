import type {
  ExecutionResult,
  ProviderAdapterRegistryOptions,
  SubmissionReceipt,
} from "@bluey/jobs-automation";

export const LOCAL_PROVIDER_APPROVAL_PENDING =
  "Approve this submission in Bluey Jobs, then continue here. This resume link cannot authorize the final click by itself.";

export interface LocalProviderFinalReview {
  adapter: "greenhouse" | "lever";
  adapterVersion: string;
}

const CONFIRMATION_PATTERNS: Record<LocalProviderFinalReview["adapter"], readonly RegExp[]> = {
  greenhouse: [
    /\b(?:thank you|thanks) for applying\b/i,
    /\b(?:your )?application (?:has been|was) (?:successfully )?(?:submitted|received)\b/i,
    /\bwe(?: have|'ve) received your application\b/i,
  ],
  lever: [
    /\bthank you for (?:submitting )?your application\b/i,
    /\b(?:thank you|thanks) for applying\b/i,
    /\b(?:your )?application (?:has been|was) (?:successfully )?(?:submitted|received)\b/i,
    /\bwe(?: have|'ve) received your application\b/i,
  ],
};

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
  bodyText: string,
  confirmationUrl: string,
  now: () => Date = () => new Date(),
): ExecutionResult | undefined {
  const normalized = bodyText.replace(/\s+/g, " ").trim();
  if (!CONFIRMATION_PATTERNS[review.adapter].some((pattern) => pattern.test(normalized))) {
    return undefined;
  }
  return {
    adapter: review.adapter,
    adapterVersion: review.adapterVersion,
    receipt: {
      status: "submitted",
      confirmationText: normalized.slice(0, 500),
      confirmationUrl,
      submittedAt: now().toISOString(),
      issues: [],
    },
  };
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

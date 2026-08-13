import type {
  AtsCertificationStatus,
  AtsCertificationSummary,
  AtsCertifiedRunnerKind,
  EligibilityReason,
  JobEligibilityDecision,
  SubmissionCapability,
} from "../types";

const SUMMARY_KEYS = [
  "adapter_version",
  "canary_available",
  "certified_runner_kinds",
  "expires_at_ms",
  "last_verified_at_ms",
  "next_action",
  "provider_label",
  "reason",
  "status",
] as const;

const CAPABILITIES = new Set<SubmissionCapability>([
  "certified",
  "beta_review",
  "handoff",
  "unknown_review",
  "blocked",
]);
const STATUSES = new Set<AtsCertificationStatus>([
  "active",
  "review_only",
  "expired",
  "suspended",
  "revoked",
  "drifted",
]);
const PROVIDER_LABELS = new Set([
  "Greenhouse",
  "Lever",
  "Workday",
  "Ashby",
  "SmartRecruiters",
  "Employer site",
  "Application site",
  "Application system",
]);
const RUNNER_KINDS = new Set<AtsCertifiedRunnerKind>(["local", "cloud"]);
const ADAPTER_VERSION = /^[A-Za-z0-9][A-Za-z0-9._-]{0,79}$/;
const REASON_CODE = /^[a-z][a-z0-9_]{0,79}$/;
const PASSED_CHECK = /^[a-z][a-z0-9_]{0,119}$/;
const OPAQUE_HEX = /(?:^|[^a-f\d])[a-f\d]{32,}(?:$|[^a-f\d])/i;
const OPAQUE_TOKEN = /\b[A-Za-z\d_-]{28,}\b/;
const UUID = /\b[a-f\d]{8}-[a-f\d]{4}-[1-5][a-f\d]{3}-[89ab][a-f\d]{3}-[a-f\d]{12}\b/i;
const INTERNAL_LABEL = new RegExp(
  "\\b(?:account|activation|canary|check|circuit|evidence|manifest|rollout|"
    + "signature|target|tenant)[ _-]?(?:count|digest|id|identity|key|path|"
    + "sha(?:256)?|threshold|total)\\b",
  "i",
);
const INTERNAL_COUNT = new RegExp(
  "\\b\\d+(?:\\s*(?:/|of)\\s*\\d+)?\\s+(?:canary\\s+accounts?|accounts?|"
    + "activations?|canaries|checks?|circuits?|evidence objects?|manifests?|"
    + "targets?|tenants?)\\b",
  "i",
);
const INTERNAL_QUOTA = new RegExp(
  "\\b(?:account|activation|canary|check|circuit|evidence|manifest|rollout|"
    + "target|tenant)s?\\b[^\\n.!?]{0,48}\\b\\d+(?:\\s*(?:/|of)\\s*\\d+)?\\b",
  "i",
);
const INTERNAL_CHECK = /\bATS-[A-Z]+-\d{3}\b/;
const SIGNATURE_DETAIL = /\b(?:private key|public key|selectors?|signature(?: material)?)\b/i;
const EMAIL_ADDRESS = /\b[A-Z\d._%+-]+@[A-Z\d.-]+\.[A-Z]{2,}\b/i;
const PROVIDER_TARGET = /\b(?:[a-z][a-z\d_-]{1,31}:){2,}[a-z\d_-]+\b/i;
const FUTURE_CLOCK_SKEW_MS = 5 * 60 * 1_000;

export interface AtsCertificationPresentation {
  server_authored: boolean;
  capability: SubmissionCapability;
  can_queue_cloud: boolean;
  provider_label: string;
  adapter_version: string;
  certified_runner_kinds: AtsCertifiedRunnerKind[];
  status: AtsCertificationStatus;
  last_verified_at_ms?: number;
  expires_at_ms?: number;
  reason: string;
  next_action: string;
  canary_available: boolean;
}

const FALLBACK_REASON =
  "Current runner certification details are unavailable. Bluey will keep this application in Review.";
const FALLBACK_ACTION = "Use Review first while Bluey verifies this application system.";

export function decodeAtsCertificationSummary(value: unknown): AtsCertificationSummary | undefined {
  const summary = objectValue(value);
  if (!summary || !hasExactKeys(summary, SUMMARY_KEYS)) return undefined;

  const status = summary.status;
  const runners = summary.certified_runner_kinds;
  const adapterVersion = summary.adapter_version;
  const lastVerifiedAtMs = summary.last_verified_at_ms;
  const expiresAtMs = summary.expires_at_ms;
  if (typeof status !== "string"
    || !STATUSES.has(status as AtsCertificationStatus)
    || typeof summary.provider_label !== "string"
    || !PROVIDER_LABELS.has(summary.provider_label)
    || (adapterVersion !== null
      && !safeAdapterVersion(adapterVersion))
    || !Array.isArray(runners)
    || runners.length > 2
    || new Set(runners).size !== runners.length
    || runners.some((runner) => typeof runner !== "string"
      || !RUNNER_KINDS.has(runner as AtsCertifiedRunnerKind))
    || (lastVerifiedAtMs !== null && !validTimestamp(lastVerifiedAtMs))
    || (expiresAtMs !== null && !validTimestamp(expiresAtMs))
    || (typeof lastVerifiedAtMs === "number"
      && typeof expiresAtMs === "number"
      && lastVerifiedAtMs > expiresAtMs)
    || !safeSummaryText(summary.reason)
    || !safeSummaryText(summary.next_action)
    || typeof summary.canary_available !== "boolean") {
    return undefined;
  }

  const typedStatus = status as AtsCertificationStatus;
  const typedRunners = runners as AtsCertifiedRunnerKind[];
  if ((typedStatus === "active" && (
    typedRunners.length === 0
    || adapterVersion === null
    || lastVerifiedAtMs === null
    || expiresAtMs === null
  ))
    || (summary.canary_available && typedStatus !== "active")) {
    return undefined;
  }

  return {
    provider_label: summary.provider_label,
    adapter_version: adapterVersion as string | null,
    certified_runner_kinds: [...typedRunners],
    status: typedStatus,
    last_verified_at_ms: lastVerifiedAtMs as number | null,
    expires_at_ms: expiresAtMs as number | null,
    reason: summary.reason,
    next_action: summary.next_action,
    canary_available: summary.canary_available,
  };
}

export function portalEligibilityDecision(
  value: unknown,
  fallbackCanPrepare = false,
  nowMs = Date.now(),
): JobEligibilityDecision {
  const decision = decodeEligibilityDecision(value) ?? fallbackEligibility(fallbackCanPrepare);
  const summary = decodeAtsCertificationSummary(decision.ats_certification);
  const currentActive = Boolean(decision.capability === "certified"
    && summary
    && summary.status === "active"
    && summary.expires_at_ms !== null
    && summary.expires_at_ms > nowMs
    && summary.last_verified_at_ms !== null
    && summary.last_verified_at_ms <= nowMs + FUTURE_CLOCK_SKEW_MS);

  if (decision.capability !== "certified") {
    return {
      ...decision,
      can_auto_submit: false,
      // The server still carries a local-runner field for parked native clients.
      // Never surface that authority through the web-only launch experience.
      can_queue_local: false,
      ats_certification: summary,
    };
  }

  if (!currentActive || !summary) return reviewOnlyDecision(decision, summary);

  const canQueueCloud = decision.can_queue_cloud
    && summary.certified_runner_kinds.includes("cloud");
  return {
    ...decision,
    can_auto_submit: decision.can_auto_submit && canQueueCloud,
    // Local Browser distribution is parked for the web launch. Preserve the
    // server response type, but never project local queue authority into the
    // customer portal.
    can_queue_local: false,
    can_queue_cloud: canQueueCloud,
    ats_certification: summary,
  };
}

export function atsCertificationPresentation(
  value: unknown,
  nowMs = Date.now(),
): AtsCertificationPresentation {
  const decision = decodeEligibilityDecision(value);
  const summary = decodeAtsCertificationSummary(decision?.ats_certification);
  if (!decision || !summary
    || (summary.status === "active" && decision.capability !== "certified")
    || (summary.last_verified_at_ms !== null
      && summary.last_verified_at_ms > nowMs + FUTURE_CLOCK_SKEW_MS)) {
    return fallbackPresentation();
  }
  const safeDecision = portalEligibilityDecision(decision, false, nowMs);

  const expiredLocally = summary.status === "active"
    && summary.expires_at_ms !== null
    && summary.expires_at_ms <= nowMs;
  return {
    server_authored: true,
    capability: safeDecision.capability,
    can_queue_cloud: safeDecision.can_queue_cloud,
    provider_label: summary.provider_label,
    adapter_version: summary.adapter_version || "Not verified",
    certified_runner_kinds: [...summary.certified_runner_kinds],
    status: expiredLocally ? "expired" : summary.status,
    ...(summary.last_verified_at_ms === null
      ? {}
      : { last_verified_at_ms: summary.last_verified_at_ms }),
    ...(summary.expires_at_ms === null ? {} : { expires_at_ms: summary.expires_at_ms }),
    reason: expiredLocally
      ? "The server-provided certification window has expired."
      : summary.reason,
    next_action: expiredLocally ? FALLBACK_ACTION : summary.next_action,
    canary_available: expiredLocally ? false : summary.canary_available,
  };
}

function decodeEligibilityDecision(value: unknown): JobEligibilityDecision | undefined {
  const decision = objectValue(value);
  if (!decision
    || typeof decision.capability !== "string"
    || !CAPABILITIES.has(decision.capability as SubmissionCapability)
    || typeof decision.can_prepare !== "boolean"
    || typeof decision.can_auto_submit !== "boolean"
    || typeof decision.can_queue_local !== "boolean"
    || typeof decision.can_queue_cloud !== "boolean"
    || !nonNegativeSafeInteger(decision.evaluated_at_ms)) {
    return undefined;
  }
  const hardFailures = decodeReasons(decision.hard_failures);
  const reviewReasons = decodeReasons(decision.review_reasons);
  const passedChecks = decodePassedChecks(decision.passed_checks);
  if (!hardFailures || !reviewReasons || !passedChecks) return undefined;
  return {
    capability: decision.capability as SubmissionCapability,
    can_prepare: decision.can_prepare,
    can_auto_submit: decision.can_auto_submit,
    can_queue_local: decision.can_queue_local,
    can_queue_cloud: decision.can_queue_cloud,
    hard_failures: hardFailures,
    review_reasons: reviewReasons,
    passed_checks: passedChecks,
    evaluated_at_ms: decision.evaluated_at_ms,
    ats_certification: decision.ats_certification,
  };
}

function decodeReasons(value: unknown): EligibilityReason[] | undefined {
  if (!Array.isArray(value) || value.length > 64) return undefined;
  const reasons: EligibilityReason[] = [];
  for (const item of value) {
    const reason = objectValue(item);
    if (!reason
      || typeof reason.code !== "string"
      || !REASON_CODE.test(reason.code)
      || !boundedText(reason.message, 1, 500)) {
      return undefined;
    }
    reasons.push({ code: reason.code, message: reason.message });
  }
  return reasons;
}

function decodePassedChecks(value: unknown): string[] | undefined {
  if (!Array.isArray(value)
    || value.length > 128
    || value.some((check) => typeof check !== "string" || !PASSED_CHECK.test(check))) {
    return undefined;
  }
  return [...value] as string[];
}

function reviewOnlyDecision(
  decision: JobEligibilityDecision,
  summary?: AtsCertificationSummary,
): JobEligibilityDecision {
  const reason = summary?.reason || FALLBACK_REASON;
  const reviewReasons = decision.review_reasons.some(
    (item) => item.code === "ats_certification_unavailable",
  )
    ? decision.review_reasons
    : [
        ...decision.review_reasons,
        { code: "ats_certification_unavailable", message: reason },
      ];
  return {
    ...decision,
    capability: "unknown_review",
    can_auto_submit: false,
    can_queue_local: false,
    can_queue_cloud: false,
    review_reasons: reviewReasons,
    ats_certification: summary,
  };
}

function fallbackEligibility(canPrepare: boolean): JobEligibilityDecision {
  return {
    capability: "unknown_review",
    can_prepare: canPrepare,
    can_auto_submit: false,
    can_queue_local: false,
    can_queue_cloud: false,
    hard_failures: [],
    review_reasons: [{ code: "eligibility_pending", message: FALLBACK_REASON }],
    passed_checks: [],
    evaluated_at_ms: 0,
  };
}

function fallbackPresentation(): AtsCertificationPresentation {
  return {
    server_authored: false,
    capability: "unknown_review",
    can_queue_cloud: false,
    provider_label: "Application system",
    adapter_version: "Not verified",
    certified_runner_kinds: [],
    status: "review_only",
    reason: FALLBACK_REASON,
    next_action: FALLBACK_ACTION,
    canary_available: false,
  };
}

function safeSummaryText(value: unknown): value is string {
  return boundedText(value, 1, 240)
    && !value.includes("://")
    && !value.includes("/api/")
    && !OPAQUE_HEX.test(` ${value} `)
    && !OPAQUE_TOKEN.test(value)
    && !UUID.test(value)
    && !INTERNAL_LABEL.test(value)
    && !INTERNAL_COUNT.test(value)
    && !INTERNAL_QUOTA.test(value)
    && !INTERNAL_CHECK.test(value)
    && !SIGNATURE_DETAIL.test(value)
    && !EMAIL_ADDRESS.test(value)
    && !PROVIDER_TARGET.test(value);
}

function safeAdapterVersion(value: unknown): value is string {
  return typeof value === "string"
    && ADAPTER_VERSION.test(value)
    && !OPAQUE_HEX.test(` ${value} `)
    && !OPAQUE_TOKEN.test(value)
    && !UUID.test(value)
    && !INTERNAL_LABEL.test(value)
    && !INTERNAL_CHECK.test(value)
    && !SIGNATURE_DETAIL.test(value)
    && !PROVIDER_TARGET.test(value);
}

function boundedText(
  value: unknown,
  minimum: number,
  maximum: number,
): value is string {
  return typeof value === "string"
    && value.length >= minimum
    && value.length <= maximum
    && value.trim() === value
    && !/[\u0000-\u001f\u007f]/u.test(value);
}

function hasExactKeys(
  value: Record<string, unknown>,
  keys: readonly string[],
): boolean {
  const actual = Object.keys(value).sort();
  return actual.length === keys.length && actual.every((key, index) => key === keys[index]);
}

function objectValue(value: unknown): Record<string, unknown> | undefined {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? value as Record<string, unknown>
    : undefined;
}

function positiveSafeInteger(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value > 0;
}

function validTimestamp(value: unknown): value is number {
  return positiveSafeInteger(value) && !Number.isNaN(new Date(value).getTime());
}

function nonNegativeSafeInteger(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
}

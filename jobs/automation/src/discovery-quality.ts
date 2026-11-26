const HOUR_MS = 60 * 60 * 1_000;
const DAY_MS = 24 * HOUR_MS;

export const DISCOVERY_QUALITY_STAGES = ["rank", "prepare", "queue"] as const;

export type DiscoveryQualityStage = (typeof DISCOVERY_QUALITY_STAGES)[number];
export type DiscoveryLeadProvenance = "original_source" | "external_feed";
export type DiscoveryAvailability = "open" | "closed" | "unknown";

export type DiscoveryCanonicalIdentity =
  | { status: "canonical"; canonicalJobId: string }
  | { status: "duplicate"; canonicalJobId: string }
  | { status: "repost"; canonicalJobId: string }
  | { status: "unknown" };

export type EmployerDomainVerification =
  | {
      status: "verified";
      employerId: string;
      canonicalDomain: string;
      applicationDomain: string;
    }
  | { status: "unknown"; applicationDomain: string | null }
  | { status: "mismatch"; canonicalDomain: string; applicationDomain: string }
  | { status: "impersonated"; canonicalDomain: string; applicationDomain: string };

export type ScamRiskSignalCode =
  | "application_fee"
  | "advance_payment"
  | "crypto_or_gift_card"
  | "equipment_purchase"
  | "financial_credentials_request"
  | "identity_document_request"
  | "malware_or_download"
  | "off_platform_contact"
  | "personal_email_contact"
  | "suspicious_redirect"
  | "unrealistic_compensation"
  | "urgent_pressure"
  | "unverified_recruiter";

export type ScamRiskSignalSource = "application" | "contact" | "domain" | "posting";

export interface ScamRiskSignal {
  code: ScamRiskSignalCode;
  source: ScamRiskSignalSource;
}

const SCAM_RISK_SIGNAL_CODES: ReadonlySet<string> = new Set<ScamRiskSignalCode>([
  "application_fee",
  "advance_payment",
  "crypto_or_gift_card",
  "equipment_purchase",
  "financial_credentials_request",
  "identity_document_request",
  "malware_or_download",
  "off_platform_contact",
  "personal_email_contact",
  "suspicious_redirect",
  "unrealistic_compensation",
  "urgent_pressure",
  "unverified_recruiter",
]);

const SCAM_RISK_SIGNAL_SOURCES: ReadonlySet<string> = new Set<ScamRiskSignalSource>([
  "application",
  "contact",
  "domain",
  "posting",
]);

export type ScamRiskAssessment =
  | { status: "clear"; signals: readonly [] }
  | { status: "unknown"; signals: readonly [] }
  | { status: "review"; signals: readonly [ScamRiskSignal, ...ScamRiskSignal[]] }
  | { status: "blocked"; signals: readonly [ScamRiskSignal, ...ScamRiskSignal[]] };

export type OriginalSourceField =
  | "canonical_url"
  | "company"
  | "employment_type"
  | "job_id"
  | "location"
  | "posted_at"
  | "title";

const ORIGINAL_SOURCE_FIELDS: ReadonlySet<string> = new Set<OriginalSourceField>([
  "canonical_url",
  "company",
  "employment_type",
  "job_id",
  "location",
  "posted_at",
  "title",
]);

/** `verified_open` means every required original-source field matched the candidate. */
export type OriginalSourceTruth =
  | { status: "unknown" }
  | { status: "unreachable"; checkedAt: string }
  | { status: "verified_closed"; checkedAt: string }
  | {
      status: "mismatch";
      checkedAt: string;
      mismatchedFields: readonly [OriginalSourceField, ...OriginalSourceField[]];
    }
  | {
      status: "verified_open";
      checkedAt: string;
      snapshotExpiresAt: string;
    };

export interface DiscoveryQualityInput {
  /** Explicit clock input keeps the decision deterministic. */
  evaluatedAt: string;
  provenance: DiscoveryLeadProvenance;
  availability: DiscoveryAvailability;
  postedAt: string | null;
  canonical: DiscoveryCanonicalIdentity;
  employer: EmployerDomainVerification;
  scamRisk: ScamRiskAssessment;
  originalSource: OriginalSourceTruth;
}

export interface DiscoveryQualityPolicy {
  maximumPostingAgeMs: number;
  maximumQueueVerificationAgeMs: number;
  maximumFutureClockSkewMs: number;
}

export const DEFAULT_DISCOVERY_QUALITY_POLICY: Readonly<DiscoveryQualityPolicy> = Object.freeze({
  maximumPostingAgeMs: 14 * DAY_MS,
  maximumQueueVerificationAgeMs: 24 * HOUR_MS,
  maximumFutureClockSkewMs: DAY_MS,
});

export type DiscoveryQualityCheck =
  | "evaluation"
  | "availability"
  | "freshness"
  | "canonicalization"
  | "employer_domain"
  | "scam_risk"
  | "original_source"
  | "queue_revalidation";

export type DiscoveryQualityCheckOutcome = "pass" | "review" | "fail";

export type DiscoveryQualityReasonCode =
  | "evaluation_time_invalid"
  | "job_closed"
  | "job_availability_unknown"
  | "posting_date_unknown"
  | "posting_date_invalid"
  | "posting_date_in_future"
  | "job_stale"
  | "canonical_identity_invalid"
  | "canonical_identity_unknown"
  | "canonical_duplicate"
  | "repost_duplicate"
  | "employer_verification_invalid"
  | "employer_verification_unknown"
  | "employer_domain_mismatch"
  | "employer_impersonation_detected"
  | "scam_risk_invalid"
  | "scam_risk_unknown"
  | "scam_risk_requires_review"
  | "scam_risk_blocked"
  | "external_feed_lead_only"
  | "original_source_unknown"
  | "original_source_unreachable"
  | "original_source_closed"
  | "original_source_mismatch"
  | "original_source_proof_invalid"
  | "original_source_snapshot_expired"
  | "original_source_revalidation_stale";

export interface DiscoveryQualityCheckDecision {
  check: DiscoveryQualityCheck;
  outcome: DiscoveryQualityCheckOutcome;
  reasonCodes: readonly DiscoveryQualityReasonCode[];
}

export interface DiscoveryQualityStageDecision {
  stage: DiscoveryQualityStage;
  allowed: boolean;
  reasonCodes: readonly DiscoveryQualityReasonCode[];
}

export interface DiscoveryQualityDecision {
  allowedStages: readonly DiscoveryQualityStage[];
  stages: Readonly<Record<DiscoveryQualityStage, DiscoveryQualityStageDecision>>;
  checks: readonly DiscoveryQualityCheckDecision[];
  canonicalJobId: string | null;
  scamSignals: readonly ScamRiskSignal[];
  requiresOriginalSourceRevalidation: boolean;
}

/**
 * Converts discovery evidence into stage authority. It performs no I/O, reads no
 * ambient clock, and never treats an external feed as original-source proof.
 */
export function decideDiscoveryQuality(
  input: Readonly<DiscoveryQualityInput>,
  policy: Readonly<DiscoveryQualityPolicy> = DEFAULT_DISCOVERY_QUALITY_POLICY,
): DiscoveryQualityDecision {
  validatePolicy(policy);
  const evaluatedAtMs = parseTimestamp(input.evaluatedAt);
  const checks: DiscoveryQualityCheckDecision[] = [];

  checks.push(evaluatedAtMs === null
    ? check("evaluation", "fail", ["evaluation_time_invalid"])
    : check("evaluation", "pass"));
  checks.push(evaluateAvailability(input.availability));
  checks.push(evaluateFreshness(input.postedAt, evaluatedAtMs, policy));
  checks.push(evaluateCanonicalization(input.canonical));
  checks.push(evaluateEmployer(input.employer));
  checks.push(evaluateScamRisk(input.scamRisk));

  const originalSourceChecks = evaluateOriginalSource(
    input.originalSource,
    input.provenance,
    evaluatedAtMs,
    policy,
  );
  checks.push(originalSourceChecks.source, originalSourceChecks.queue);

  const rankReasons = reasonsFor(checks, ["fail"], "foundation");
  const prepareReasons = distinctReasons([
    ...rankReasons,
    ...reasonsFor(checks, ["review"], "foundation"),
  ]);
  const queueReasons = distinctReasons([
    ...prepareReasons,
    ...reasonsFor(checks, ["fail", "review"], "queue"),
  ]);
  const stages = Object.freeze({
    rank: stageDecision("rank", rankReasons),
    prepare: stageDecision("prepare", prepareReasons),
    queue: stageDecision("queue", queueReasons),
  });
  const allowedStages = Object.freeze(
    DISCOVERY_QUALITY_STAGES.filter((stage) => stages[stage].allowed),
  );
  const allReasons = checks.flatMap((item) => item.reasonCodes);

  return Object.freeze({
    allowedStages,
    stages,
    checks: Object.freeze(checks),
    canonicalJobId: canonicalJobId(input.canonical),
    scamSignals: Object.freeze(input.scamRisk.signals.map((signal) => Object.freeze({ ...signal }))),
    requiresOriginalSourceRevalidation: allReasons.some(isRevalidationReason),
  });
}

export function isDiscoveryQualityStageAllowed(
  decision: Readonly<DiscoveryQualityDecision>,
  stage: DiscoveryQualityStage,
): boolean {
  return decision.stages[stage].allowed;
}

function evaluateAvailability(availability: DiscoveryAvailability): DiscoveryQualityCheckDecision {
  if (availability === "open") return check("availability", "pass");
  if (availability === "closed") return check("availability", "fail", ["job_closed"]);
  return check("availability", "review", ["job_availability_unknown"]);
}

function evaluateFreshness(
  postedAt: string | null,
  evaluatedAtMs: number | null,
  policy: Readonly<DiscoveryQualityPolicy>,
): DiscoveryQualityCheckDecision {
  if (postedAt === null || postedAt.trim() === "") {
    return check("freshness", "review", ["posting_date_unknown"]);
  }
  const postedAtMs = parseTimestamp(postedAt);
  if (postedAtMs === null) return check("freshness", "fail", ["posting_date_invalid"]);
  if (evaluatedAtMs === null) return check("freshness", "fail", ["evaluation_time_invalid"]);
  if (postedAtMs > evaluatedAtMs + policy.maximumFutureClockSkewMs) {
    return check("freshness", "fail", ["posting_date_in_future"]);
  }
  if (evaluatedAtMs - postedAtMs > policy.maximumPostingAgeMs) {
    return check("freshness", "fail", ["job_stale"]);
  }
  return check("freshness", "pass");
}

function evaluateCanonicalization(identity: DiscoveryCanonicalIdentity): DiscoveryQualityCheckDecision {
  if (identity.status === "unknown") {
    return check("canonicalization", "review", ["canonical_identity_unknown"]);
  }
  if (!identity.canonicalJobId.trim()) {
    return check("canonicalization", "fail", ["canonical_identity_invalid"]);
  }
  if (identity.status === "duplicate") {
    return check("canonicalization", "fail", ["canonical_duplicate"]);
  }
  if (identity.status === "repost") {
    return check("canonicalization", "fail", ["repost_duplicate"]);
  }
  return check("canonicalization", "pass");
}

function evaluateEmployer(verification: EmployerDomainVerification): DiscoveryQualityCheckDecision {
  if (verification.status === "unknown") {
    return check("employer_domain", "review", ["employer_verification_unknown"]);
  }
  if (verification.status === "mismatch") {
    return check("employer_domain", "fail", ["employer_domain_mismatch"]);
  }
  if (verification.status === "impersonated") {
    return check("employer_domain", "fail", ["employer_impersonation_detected"]);
  }
  if (
    !verification.employerId.trim()
    || !isDomain(verification.canonicalDomain)
    || !isDomain(verification.applicationDomain)
  ) {
    return check("employer_domain", "fail", ["employer_verification_invalid"]);
  }
  return check("employer_domain", "pass");
}

function evaluateScamRisk(assessment: ScamRiskAssessment): DiscoveryQualityCheckDecision {
  if (!validScamSignals(assessment.signals)) {
    return check("scam_risk", "fail", ["scam_risk_invalid"]);
  }
  if (assessment.status === "clear") {
    return assessment.signals.length === 0
      ? check("scam_risk", "pass")
      : check("scam_risk", "fail", ["scam_risk_invalid"]);
  }
  if (assessment.status === "unknown") {
    return assessment.signals.length === 0
      ? check("scam_risk", "review", ["scam_risk_unknown"])
      : check("scam_risk", "fail", ["scam_risk_invalid"]);
  }
  if (assessment.signals.length === 0) {
    return check("scam_risk", "fail", ["scam_risk_invalid"]);
  }
  return assessment.status === "review"
    ? check("scam_risk", "review", ["scam_risk_requires_review"])
    : check("scam_risk", "fail", ["scam_risk_blocked"]);
}

function evaluateOriginalSource(
  truth: OriginalSourceTruth,
  provenance: DiscoveryLeadProvenance,
  evaluatedAtMs: number | null,
  policy: Readonly<DiscoveryQualityPolicy>,
): { source: DiscoveryQualityCheckDecision; queue: DiscoveryQualityCheckDecision } {
  const leadReason: DiscoveryQualityReasonCode[] = provenance === "external_feed"
    ? ["external_feed_lead_only"]
    : [];
  if (truth.status === "unknown") {
    return originalSourceReview([...leadReason, "original_source_unknown"]);
  }

  const checkedAtMs = parseTimestamp(truth.checkedAt);
  if (checkedAtMs === null || evaluatedAtMs === null) {
    return originalSourceFailure([...leadReason, "original_source_proof_invalid"]);
  }
  if (checkedAtMs > evaluatedAtMs + policy.maximumFutureClockSkewMs) {
    return originalSourceFailure([...leadReason, "original_source_proof_invalid"]);
  }
  if (truth.status === "unreachable") {
    return originalSourceReview([...leadReason, "original_source_unreachable"]);
  }
  if (truth.status === "verified_closed") {
    return originalSourceFailure(["original_source_closed"]);
  }
  if (truth.status === "mismatch") {
    const fields = new Set(truth.mismatchedFields);
    if (
      fields.size !== truth.mismatchedFields.length
      || fields.size === 0
      || [...fields].some((field) => !ORIGINAL_SOURCE_FIELDS.has(field))
    ) {
      return originalSourceFailure([...leadReason, "original_source_proof_invalid"]);
    }
    return originalSourceFailure([...leadReason, "original_source_mismatch"]);
  }

  const expiresAtMs = parseTimestamp(truth.snapshotExpiresAt);
  if (expiresAtMs === null || expiresAtMs <= checkedAtMs) {
    return originalSourceFailure([...leadReason, "original_source_proof_invalid"]);
  }
  if (expiresAtMs <= evaluatedAtMs) {
    return originalSourceReview([...leadReason, "original_source_snapshot_expired"]);
  }

  const queue = evaluatedAtMs - checkedAtMs > policy.maximumQueueVerificationAgeMs
    ? check("queue_revalidation", "fail", ["original_source_revalidation_stale"])
    : check("queue_revalidation", "pass");
  return { source: check("original_source", "pass"), queue };
}

function originalSourceFailure(
  reasonCodes: readonly DiscoveryQualityReasonCode[],
): { source: DiscoveryQualityCheckDecision; queue: DiscoveryQualityCheckDecision } {
  return {
    source: check("original_source", "fail", reasonCodes),
    queue: check("queue_revalidation", "fail", reasonCodes),
  };
}

function originalSourceReview(
  reasonCodes: readonly DiscoveryQualityReasonCode[],
): { source: DiscoveryQualityCheckDecision; queue: DiscoveryQualityCheckDecision } {
  return {
    source: check("original_source", "review", reasonCodes),
    queue: check("queue_revalidation", "review", reasonCodes),
  };
}

function check(
  checkName: DiscoveryQualityCheck,
  outcome: DiscoveryQualityCheckOutcome,
  reasonCodes: readonly DiscoveryQualityReasonCode[] = [],
): DiscoveryQualityCheckDecision {
  return Object.freeze({
    check: checkName,
    outcome,
    reasonCodes: Object.freeze(distinctReasons(reasonCodes)),
  });
}

function stageDecision(
  stage: DiscoveryQualityStage,
  reasonCodes: readonly DiscoveryQualityReasonCode[],
): DiscoveryQualityStageDecision {
  const reasons = Object.freeze(distinctReasons(reasonCodes));
  return Object.freeze({ stage, allowed: reasons.length === 0, reasonCodes: reasons });
}

function reasonsFor(
  checks: readonly DiscoveryQualityCheckDecision[],
  outcomes: readonly DiscoveryQualityCheckOutcome[],
  scope: "foundation" | "queue",
): DiscoveryQualityReasonCode[] {
  return checks
    .filter((item) => outcomes.includes(item.outcome))
    .filter((item) => scope === "queue"
      ? item.check === "queue_revalidation"
      : item.check !== "queue_revalidation")
    .flatMap((item) => item.reasonCodes);
}

function distinctReasons(
  reasonCodes: readonly DiscoveryQualityReasonCode[],
): DiscoveryQualityReasonCode[] {
  return [...new Set(reasonCodes)];
}

function canonicalJobId(identity: DiscoveryCanonicalIdentity): string | null {
  return identity.status === "unknown" || !identity.canonicalJobId.trim()
    ? null
    : identity.canonicalJobId.trim();
}

function isRevalidationReason(reason: DiscoveryQualityReasonCode): boolean {
  return reason === "external_feed_lead_only"
    || reason === "original_source_unknown"
    || reason === "original_source_unreachable"
    || reason === "original_source_mismatch"
    || reason === "original_source_proof_invalid"
    || reason === "original_source_snapshot_expired"
    || reason === "original_source_revalidation_stale";
}

function parseTimestamp(value: string): number | null {
  if (typeof value !== "string" || value.trim() === "") return null;
  const timestamp = Date.parse(value);
  return Number.isFinite(timestamp) ? timestamp : null;
}

function isDomain(value: string): boolean {
  const domain = value.trim().toLowerCase().replace(/\.$/, "");
  if (!domain || domain.length > 253 || domain.includes(":") || domain.includes("/")) return false;
  const labels = domain.split(".");
  return labels.length >= 2 && labels.every((label) => (
    /^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/.test(label)
  ));
}

function validScamSignals(signals: readonly ScamRiskSignal[]): boolean {
  return signals.every((signal) => (
    SCAM_RISK_SIGNAL_CODES.has(signal.code) && SCAM_RISK_SIGNAL_SOURCES.has(signal.source)
  ));
}

function validatePolicy(policy: Readonly<DiscoveryQualityPolicy>): void {
  const entries = [
    ["maximumPostingAgeMs", policy.maximumPostingAgeMs],
    ["maximumQueueVerificationAgeMs", policy.maximumQueueVerificationAgeMs],
    ["maximumFutureClockSkewMs", policy.maximumFutureClockSkewMs],
  ] as const;
  for (const [name, value] of entries) {
    if (!Number.isFinite(value) || value < 0) {
      throw new RangeError(`Discovery quality policy ${name} must be a finite non-negative number`);
    }
  }
  if (policy.maximumPostingAgeMs === 0 || policy.maximumQueueVerificationAgeMs === 0) {
    throw new RangeError("Discovery quality age limits must be greater than zero");
  }
}

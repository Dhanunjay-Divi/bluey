import { describe, expect, it } from "vitest";

import {
  DEFAULT_DISCOVERY_QUALITY_POLICY,
  decideDiscoveryQuality,
  isDiscoveryQualityStageAllowed,
  type DiscoveryQualityInput,
  type DiscoveryQualityReasonCode,
  type EmployerDomainVerification,
  type ScamRiskAssessment,
} from "../src/discovery-quality.js";

const EVALUATED_AT = "2026-08-02T12:00:00.000Z";

describe("discovery quality decisions", () => {
  it("allows every stage after an external lead is verified against a current original source", () => {
    const decision = decideDiscoveryQuality(input());

    expect(decision.allowedStages).toEqual(["rank", "prepare", "queue"]);
    expect(decision.stages).toEqual({
      rank: { stage: "rank", allowed: true, reasonCodes: [] },
      prepare: { stage: "prepare", allowed: true, reasonCodes: [] },
      queue: { stage: "queue", allowed: true, reasonCodes: [] },
    });
    expect(decision.checks.every((check) => check.outcome === "pass")).toBe(true);
    expect(decision.canonicalJobId).toBe("job-canonical-1");
    expect(decision.requiresOriginalSourceRevalidation).toBe(false);
    expect(isDiscoveryQualityStageAllowed(decision, "queue")).toBe(true);
  });

  it("keeps an external-feed row as a lead until original-source truth is known", () => {
    const decision = decideDiscoveryQuality(input({ originalSource: { status: "unknown" } }));

    expect(decision.allowedStages).toEqual(["rank"]);
    expect(reasons(decision, "rank")).toEqual([]);
    expect(reasons(decision, "prepare")).toEqual([
      "external_feed_lead_only",
      "original_source_unknown",
    ]);
    expect(reasons(decision, "queue")).toEqual(reasons(decision, "prepare"));
    expect(decision.requiresOriginalSourceRevalidation).toBe(true);
  });

  it("keeps unknown direct-source truth review-only", () => {
    const decision = decideDiscoveryQuality(input({
      provenance: "original_source",
      originalSource: { status: "unknown" },
    }));

    expect(decision.allowedStages).toEqual(["rank"]);
    expect(reasons(decision, "rank")).toEqual([]);
    expect(reasons(decision, "prepare")).toEqual(["original_source_unknown"]);
    expect(reasons(decision, "queue")).not.toContain("external_feed_lead_only");
    expect(decision.requiresOriginalSourceRevalidation).toBe(true);
  });

  it("rejects a job when either discovery or the original source says it is closed", () => {
    const discoveryClosed = decideDiscoveryQuality(input({ availability: "closed" }));
    const originalClosed = decideDiscoveryQuality(input({
      originalSource: { status: "verified_closed", checkedAt: "2026-08-02T11:30:00.000Z" },
    }));

    expect(discoveryClosed.allowedStages).toEqual([]);
    expect(reasons(discoveryClosed, "rank")).toContain("job_closed");
    expect(originalClosed.allowedStages).toEqual([]);
    expect(reasons(originalClosed, "rank")).toContain("original_source_closed");
    expect(originalClosed.requiresOriginalSourceRevalidation).toBe(false);
  });

  it("keeps unknown availability and unreachable original sources review-only", () => {
    const unknownAvailability = decideDiscoveryQuality(input({ availability: "unknown" }));
    const unreachableOriginal = decideDiscoveryQuality(input({
      originalSource: { status: "unreachable", checkedAt: "2026-08-02T11:30:00.000Z" },
    }));

    expect(unknownAvailability.allowedStages).toEqual(["rank"]);
    expect(reasons(unknownAvailability, "prepare")).toContain("job_availability_unknown");
    expect(unreachableOriginal.allowedStages).toEqual(["rank"]);
    expect(reasons(unreachableOriginal, "prepare")).toEqual([
      "external_feed_lead_only",
      "original_source_unreachable",
    ]);
  });

  it.each([
    [null, "posting_date_unknown"],
    ["not-a-time", "posting_date_invalid"],
    ["2026-07-18T11:59:59.999Z", "job_stale"],
    ["2026-08-03T12:00:00.001Z", "posting_date_in_future"],
  ] as const)("rejects invalid freshness evidence %s", (postedAt, expectedReason) => {
    const decision = decideDiscoveryQuality(input({ postedAt }));

    if (postedAt === null) {
      expect(decision.allowedStages).toEqual(["rank"]);
      expect(reasons(decision, "prepare")).toContain(expectedReason);
    } else {
      expect(decision.allowedStages).toEqual([]);
      expect(reasons(decision, "rank")).toContain(expectedReason);
    }
  });

  it("accepts posting and queue-verification ages exactly on their limits", () => {
    const decision = decideDiscoveryQuality(input({
      postedAt: "2026-07-19T12:00:00.000Z",
      originalSource: {
        status: "verified_open",
        checkedAt: "2026-08-01T12:00:00.000Z",
        snapshotExpiresAt: "2026-08-03T12:00:00.000Z",
      },
    }));

    expect(decision.allowedStages).toEqual(["rank", "prepare", "queue"]);
  });

  it.each([
    ["duplicate", "canonical_duplicate"],
    ["repost", "repost_duplicate"],
  ] as const)("routes a %s to its canonical job instead of processing it twice", (status, reason) => {
    const decision = decideDiscoveryQuality(input({
      canonical: { status, canonicalJobId: "job-canonical-existing" },
    }));

    expect(decision.allowedStages).toEqual([]);
    expect(decision.canonicalJobId).toBe("job-canonical-existing");
    expect(reasons(decision, "rank")).toContain(reason);
  });

  it("keeps unknown canonical identity review-only and rejects malformed identity", () => {
    const unknown = decideDiscoveryQuality(input({ canonical: { status: "unknown" } }));
    const malformed = decideDiscoveryQuality(input({
      canonical: { status: "canonical", canonicalJobId: "  " },
    }));

    expect(unknown.allowedStages).toEqual(["rank"]);
    expect(reasons(unknown, "prepare")).toContain("canonical_identity_unknown");
    expect(unknown.canonicalJobId).toBeNull();
    expect(reasons(malformed, "rank")).toContain("canonical_identity_invalid");
  });

  it.each([
    [
      { status: "unknown", applicationDomain: null },
      "employer_verification_unknown",
    ],
    [
      { status: "mismatch", canonicalDomain: "acme.test", applicationDomain: "acme-careers.test" },
      "employer_domain_mismatch",
    ],
    [
      { status: "impersonated", canonicalDomain: "acme.test", applicationDomain: "acme-hiring.test" },
      "employer_impersonation_detected",
    ],
  ] as const)("routes untrusted employer/domain evidence safely", (employer, reason) => {
    const decision = decideDiscoveryQuality(input({ employer }));

    if (employer.status === "unknown") {
      expect(decision.allowedStages).toEqual(["rank"]);
      expect(reasons(decision, "prepare")).toContain(reason);
    } else {
      expect(decision.allowedStages).toEqual([]);
      expect(reasons(decision, "rank")).toContain(reason);
    }
  });

  it("fails closed when a claimed verified employer has malformed authority fields", () => {
    const employer: EmployerDomainVerification = {
      status: "verified",
      employerId: "employer-1",
      canonicalDomain: "https://acme.test",
      applicationDomain: "jobs.lever.co",
    };
    const decision = decideDiscoveryQuality(input({ employer }));

    expect(decision.allowedStages).toEqual([]);
    expect(reasons(decision, "rank")).toContain("employer_verification_invalid");
  });

  it("allows ranking only when scam signals require human review", () => {
    const scamRisk: ScamRiskAssessment = {
      status: "review",
      signals: [{ code: "personal_email_contact", source: "contact" }],
    };
    const decision = decideDiscoveryQuality(input({ scamRisk }));

    expect(decision.allowedStages).toEqual(["rank"]);
    expect(reasons(decision, "rank")).toEqual([]);
    expect(reasons(decision, "prepare")).toEqual(["scam_risk_requires_review"]);
    expect(reasons(decision, "queue")).toEqual(["scam_risk_requires_review"]);
    expect(decision.scamSignals).toEqual(scamRisk.signals);
  });

  it.each([
    [
      { status: "unknown", signals: [] },
      "scam_risk_unknown",
    ],
    [
      {
        status: "blocked",
        signals: [{ code: "application_fee", source: "application" }],
      },
      "scam_risk_blocked",
    ],
  ] as const)("routes unresolved or blocking scam risk safely", (scamRisk, reason) => {
    const decision = decideDiscoveryQuality(input({ scamRisk }));

    if (scamRisk.status === "unknown") {
      expect(decision.allowedStages).toEqual(["rank"]);
      expect(reasons(decision, "prepare")).toContain(reason);
    } else {
      expect(decision.allowedStages).toEqual([]);
      expect(reasons(decision, "rank")).toContain(reason);
    }
  });

  it("rejects original-source field mismatches", () => {
    const decision = decideDiscoveryQuality(input({
      originalSource: {
        status: "mismatch",
        checkedAt: "2026-08-02T11:30:00.000Z",
        mismatchedFields: ["company", "canonical_url"],
      },
    }));

    expect(decision.allowedStages).toEqual([]);
    expect(reasons(decision, "rank")).toContain("original_source_mismatch");
    expect(decision.requiresOriginalSourceRevalidation).toBe(true);
  });

  it("rejects an expired or internally invalid original-source snapshot", () => {
    const expired = decideDiscoveryQuality(input({
      originalSource: {
        status: "verified_open",
        checkedAt: "2026-08-02T10:00:00.000Z",
        snapshotExpiresAt: EVALUATED_AT,
      },
    }));
    const invalid = decideDiscoveryQuality(input({
      originalSource: {
        status: "verified_open",
        checkedAt: "2026-08-02T11:00:00.000Z",
        snapshotExpiresAt: "2026-08-02T10:59:59.999Z",
      },
    }));

    expect(expired.allowedStages).toEqual(["rank"]);
    expect(reasons(expired, "prepare")).toContain("original_source_snapshot_expired");
    expect(invalid.allowedStages).toEqual([]);
    expect(reasons(invalid, "rank")).toContain("original_source_proof_invalid");
  });

  it("allows rank and prepare but fails closed before queue when live revalidation is stale", () => {
    const decision = decideDiscoveryQuality(input({
      originalSource: {
        status: "verified_open",
        checkedAt: "2026-08-01T11:59:59.999Z",
        snapshotExpiresAt: "2026-08-04T12:00:00.000Z",
      },
    }));

    expect(decision.allowedStages).toEqual(["rank", "prepare"]);
    expect(reasons(decision, "queue")).toEqual(["original_source_revalidation_stale"]);
    expect(decision.requiresOriginalSourceRevalidation).toBe(true);
  });

  it("fails closed for an invalid evaluation time and rejects invalid policy", () => {
    const decision = decideDiscoveryQuality(input({ evaluatedAt: "later" }));

    expect(decision.allowedStages).toEqual([]);
    expect(reasons(decision, "rank")).toContain("evaluation_time_invalid");
    expect(() => decideDiscoveryQuality(input(), {
      ...DEFAULT_DISCOVERY_QUALITY_POLICY,
      maximumPostingAgeMs: 0,
    })).toThrow(RangeError);
  });

  it("does not mutate evidence and returns deterministic frozen decisions", () => {
    const evidence = deepFreeze(input());
    const first = decideDiscoveryQuality(evidence);
    const second = decideDiscoveryQuality(evidence);

    expect(second).toEqual(first);
    expect(Object.isFrozen(first)).toBe(true);
    expect(Object.isFrozen(first.allowedStages)).toBe(true);
    expect(Object.isFrozen(first.scamSignals)).toBe(true);
    expect(evidence.originalSource).toEqual({
      status: "verified_open",
      checkedAt: "2026-08-02T11:00:00.000Z",
      snapshotExpiresAt: "2026-08-03T12:00:00.000Z",
    });
  });
});

function input(overrides: Partial<DiscoveryQualityInput> = {}): DiscoveryQualityInput {
  return {
    evaluatedAt: EVALUATED_AT,
    provenance: "external_feed",
    availability: "open",
    postedAt: "2026-08-01T12:00:00.000Z",
    canonical: { status: "canonical", canonicalJobId: "job-canonical-1" },
    employer: {
      status: "verified",
      employerId: "employer-acme",
      canonicalDomain: "acme.test",
      applicationDomain: "jobs.lever.co",
    },
    scamRisk: { status: "clear", signals: [] },
    originalSource: {
      status: "verified_open",
      checkedAt: "2026-08-02T11:00:00.000Z",
      snapshotExpiresAt: "2026-08-03T12:00:00.000Z",
    },
    ...overrides,
  };
}

function reasons(
  decision: ReturnType<typeof decideDiscoveryQuality>,
  stage: "rank" | "prepare" | "queue",
): readonly DiscoveryQualityReasonCode[] {
  return decision.stages[stage].reasonCodes;
}

function deepFreeze<T>(value: T): T {
  if (!value || typeof value !== "object" || Object.isFrozen(value)) return value;
  for (const nested of Object.values(value as Record<string, unknown>)) deepFreeze(nested);
  return Object.freeze(value);
}

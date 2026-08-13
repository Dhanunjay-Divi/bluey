import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { AtsCertificationSummaryCard } from "../components/AtsCertificationSummary";
import type { AtsCertificationSummary, JobEligibilityDecision } from "../types";
import {
  atsCertificationPresentation,
  decodeAtsCertificationSummary,
  portalEligibilityDecision,
} from "./ats-certification";

const NOW_MS = Date.now();

function activeSummary(): AtsCertificationSummary {
  return {
    provider_label: "Greenhouse",
    adapter_version: "2026.07.1-beta.1",
    certified_runner_kinds: ["local"],
    status: "active",
    last_verified_at_ms: NOW_MS - 60_000,
    expires_at_ms: NOW_MS + 60 * 60_000,
    reason: "Server verification is current for this job and runner.",
    next_action: "Review the application kit and choose Local Browser.",
    canary_available: true,
  };
}

function reviewOnlySummary(): AtsCertificationSummary {
  return {
    provider_label: "Greenhouse",
    adapter_version: null,
    certified_runner_kinds: [],
    status: "review_only",
    last_verified_at_ms: null,
    expires_at_ms: null,
    reason: "This application requires reviewed automation.",
    next_action: "Review the exact application kit before continuing.",
    canary_available: false,
  };
}

function eligibility(
  summary: unknown = activeSummary(),
): JobEligibilityDecision {
  return {
    capability: "certified",
    can_prepare: true,
    can_auto_submit: true,
    can_queue_local: true,
    can_queue_cloud: true,
    hard_failures: [],
    review_reasons: [],
    passed_checks: ["ats_certified"],
    evaluated_at_ms: NOW_MS,
    ats_certification: summary,
  };
}

describe("ATS certification portal boundary", () => {
  it("accepts only the exact bounded server summary", () => {
    expect(decodeAtsCertificationSummary(activeSummary())).toEqual(activeSummary());
    expect(decodeAtsCertificationSummary({
      ...activeSummary(),
      target_key: "greenhouse:private-tenant:123",
    })).toBeUndefined();
    expect(decodeAtsCertificationSummary({
      ...activeSummary(),
      reason: "Use target_key greenhouse:private-tenant:123.",
    })).toBeUndefined();
    expect(decodeAtsCertificationSummary({
      ...activeSummary(),
      reason: `Authority ${"a".repeat(64)} is active.`,
    })).toBeUndefined();
    expect(decodeAtsCertificationSummary({
      ...activeSummary(),
      reason: "The internal target count is 4.",
    })).toBeUndefined();
    expect(decodeAtsCertificationSummary({
      ...activeSummary(),
      next_action: "Inspect ATS-SUBMIT-001 and its signature material.",
    })).toBeUndefined();
    expect(decodeAtsCertificationSummary({
      ...activeSummary(),
      adapter_version: "a".repeat(64),
    })).toBeUndefined();
    expect(decodeAtsCertificationSummary({
      ...activeSummary(),
      adapter_version: "tenant_id",
    })).toBeUndefined();
    expect(decodeAtsCertificationSummary({
      ...activeSummary(),
      reason: "Canary capacity is 2/10.",
    })).toBeUndefined();
    expect(decodeAtsCertificationSummary({
      ...activeSummary(),
      next_action: "Choose from 2/10 canary accounts.",
    })).toBeUndefined();
    expect(decodeAtsCertificationSummary({
      ...activeSummary(),
      expires_at_ms: Number.MAX_SAFE_INTEGER,
    })).toBeUndefined();
  });

  it.each([
    "adapter_version",
    "canary_available",
    "certified_runner_kinds",
    "expires_at_ms",
    "last_verified_at_ms",
    "next_action",
    "provider_label",
    "reason",
    "status",
  ] as const)("fails closed when mixed-version data omits %s", (field) => {
    const summary: Record<string, unknown> = { ...activeSummary() };
    delete summary[field];

    expect(decodeAtsCertificationSummary(summary)).toBeUndefined();
    expect(portalEligibilityDecision(eligibility(summary), false, NOW_MS)).toMatchObject({
      capability: "unknown_review",
      can_auto_submit: false,
      can_queue_local: false,
      can_queue_cloud: false,
    });
  });

  it("allows truthful review-only summaries with unavailable adapter and times", () => {
    const summary: AtsCertificationSummary = {
      provider_label: "Application system",
      adapter_version: null,
      certified_runner_kinds: [],
      status: "review_only",
      last_verified_at_ms: null,
      expires_at_ms: null,
      reason: "This application system requires review.",
      next_action: "Review the application kit before continuing.",
      canary_available: false,
    };

    expect(decodeAtsCertificationSummary(summary)).toEqual(summary);
    expect(decodeAtsCertificationSummary({
      ...summary,
      provider_label: "Application site",
    })).toMatchObject({ provider_label: "Application site" });
  });

  it("projects only cloud-certified authority into the web portal", () => {
    const decoded = portalEligibilityDecision(eligibility(), false, NOW_MS);
    const cloudOnly = portalEligibilityDecision(eligibility({
      ...activeSummary(),
      certified_runner_kinds: ["cloud"],
    }), false, NOW_MS);

    expect(decoded.capability).toBe("certified");
    expect(decoded.can_auto_submit).toBe(false);
    expect(decoded.can_queue_local).toBe(false);
    expect(decoded.can_queue_cloud).toBe(false);
    expect(cloudOnly.capability).toBe("certified");
    expect(cloudOnly.can_auto_submit).toBe(true);
    expect(cloudOnly.can_queue_local).toBe(false);
    expect(cloudOnly.can_queue_cloud).toBe(true);
  });

  it("fails missing, malformed, stale, and non-active certification closed", () => {
    const missing = portalEligibilityDecision(eligibility(null), false, NOW_MS);
    const malformed = portalEligibilityDecision(
      eligibility({ ...activeSummary(), certified_runner_kinds: ["desktop"] }),
      false,
      NOW_MS,
    );
    const expired = portalEligibilityDecision(
      eligibility({
        ...activeSummary(),
        last_verified_at_ms: NOW_MS - 120_000,
        expires_at_ms: NOW_MS - 60_000,
      }),
      false,
      NOW_MS,
    );
    const revoked = portalEligibilityDecision(
      eligibility({
        ...activeSummary(),
        status: "revoked",
        canary_available: false,
      }),
      false,
      NOW_MS,
    );

    for (const decision of [missing, malformed, expired, revoked]) {
      expect(decision.capability).toBe("unknown_review");
      expect(decision.can_auto_submit).toBe(false);
      expect(decision.can_queue_local).toBe(false);
      expect(decision.can_queue_cloud).toBe(false);
    }
    expect(atsCertificationPresentation(eligibility({
      ...activeSummary(),
      status: "revoked",
      canary_available: false,
    }), NOW_MS).capability).toBe("unknown_review");
  });

  it("does not present an active certification when capability is not certified", () => {
    const inconsistent = {
      ...eligibility(),
      capability: "beta_review" as const,
      can_auto_submit: false,
      can_queue_local: true,
    };

    expect(portalEligibilityDecision(inconsistent, false, NOW_MS)).toMatchObject({
      can_auto_submit: false,
      can_queue_local: false,
      can_queue_cloud: true,
    });
    expect(atsCertificationPresentation(inconsistent, NOW_MS)).toMatchObject({
      server_authored: false,
      capability: "unknown_review",
      status: "review_only",
      certified_runner_kinds: [],
      canary_available: false,
    });
  });

  it("labels queueable beta authority without claiming certification", () => {
    const beta: JobEligibilityDecision = {
      ...eligibility(reviewOnlySummary()),
      capability: "beta_review",
      can_auto_submit: false,
      can_queue_local: true,
      can_queue_cloud: true,
    };
    const presentation = atsCertificationPresentation(beta, NOW_MS);
    const markup = renderToStaticMarkup(<AtsCertificationSummaryCard eligibility={beta} />);

    expect(presentation).toMatchObject({
      server_authored: true,
      capability: "beta_review",
      can_queue_cloud: true,
      status: "review_only",
      certified_runner_kinds: [],
    });
    expect(markup).toContain("Reviewed beta");
    expect(markup).toContain("Beta · Final review");
    expect(markup).toContain("Cloud · final review");
    expect(markup).toContain("Bluey pauses again for final form approval.");
    expect(markup).not.toContain("continue on the original job site");
    expect(markup).not.toContain(">Certified<");
  });

  it("keeps nonqueueable beta and retained local authority in Review", () => {
    const localOnly: JobEligibilityDecision = {
      ...eligibility(reviewOnlySummary()),
      capability: "beta_review",
      can_auto_submit: false,
      can_queue_local: true,
      can_queue_cloud: false,
    };
    const presentation = atsCertificationPresentation(localOnly, NOW_MS);
    const markup = renderToStaticMarkup(<AtsCertificationSummaryCard eligibility={localOnly} />);

    expect(presentation).toMatchObject({
      can_queue_cloud: false,
      certified_runner_kinds: [],
    });
    expect(portalEligibilityDecision(localOnly, false, NOW_MS).can_queue_local).toBe(false);
    expect(markup).toContain("Review only");
    expect(markup).toContain("Not certified");
    expect(markup).toContain("continue on the original job site");
    expect(markup).not.toContain("Reviewed beta");
  });

  it("renders only display-safe summary fields and never opaque authority material", () => {
    const cloudSummary = {
      ...activeSummary(),
      certified_runner_kinds: ["cloud"] as const,
      reason: "The current job and cloud runner passed server verification.",
      next_action: "Review the application kit and choose cloud automation.",
    };
    const validMarkup = renderToStaticMarkup(
      <AtsCertificationSummaryCard eligibility={eligibility(cloudSummary)} />,
    );
    const malformedMarkup = renderToStaticMarkup(
      <AtsCertificationSummaryCard eligibility={eligibility({
        ...cloudSummary,
        target_key: "greenhouse:private-tenant:123",
      })} />,
    );

    expect(validMarkup).toContain("CLOUD AUTOMATION");
    expect(validMarkup).toContain("Greenhouse");
    expect(validMarkup).toContain("2026.07.1-beta.1");
    expect(validMarkup).toContain("Cloud runner");
    expect(validMarkup).toContain("Last verified");
    expect(validMarkup).toContain("Expires");
    expect(validMarkup).toContain("The current job and cloud runner passed server verification.");
    expect(validMarkup).toContain("Review the application kit and choose cloud automation.");
    expect(validMarkup).toContain("Available");
    expect(validMarkup).not.toContain("Local Browser");
    expect(validMarkup).not.toContain("private-tenant");
    expect(malformedMarkup).toContain("Server certification summary unavailable");
    expect(malformedMarkup).toContain("Review only");
    expect(malformedMarkup).not.toContain("private-tenant");
  });

  it("keeps a local-only certification out of the web launch surface", () => {
    const markup = renderToStaticMarkup(
      <AtsCertificationSummaryCard eligibility={eligibility()} />,
    );

    expect(markup).toContain("CLOUD AUTOMATION");
    expect(markup).toContain("Not certified");
    expect(markup).toContain("Review only");
    expect(markup).toContain("continue on the original job site");
    expect(markup).not.toContain("Local Browser");
  });

  it.each([
    ["active", "Active"],
    ["review_only", "Review only"],
    ["expired", "Expired"],
    ["suspended", "Suspended"],
    ["revoked", "Revoked"],
    ["drifted", "Drifted"],
  ] as const)("renders the bounded server-authored %s state", (status, label) => {
    const summary: AtsCertificationSummary = {
      ...activeSummary(),
      certified_runner_kinds: ["cloud"],
      reason: "The current job and cloud runner passed server verification.",
      next_action: "Review the application kit and choose cloud automation.",
      status,
      canary_available: status === "active",
    };
    const decision = status === "active"
      ? eligibility(summary)
      : { ...eligibility(summary), can_auto_submit: false };
    const markup = renderToStaticMarkup(
      <AtsCertificationSummaryCard eligibility={decision} />,
    );

    expect(markup).toContain(`>${label}</span>`);
    expect(markup).toContain("Greenhouse");
    expect(markup).toContain("2026.07.1-beta.1");
    expect(markup).toContain("Cloud runner");
    expect(markup).toContain("The current job and cloud runner passed server verification.");
    expect(markup).not.toContain("target_key");
    expect(markup).not.toContain("sha256");
  });

  it("derives local expiry even if an old server still says active", () => {
    const summary = {
      ...activeSummary(),
      last_verified_at_ms: NOW_MS - 120_000,
      expires_at_ms: NOW_MS - 60_000,
    };
    const presentation = atsCertificationPresentation(eligibility(summary), NOW_MS);

    expect(presentation.status).toBe("expired");
    expect(presentation.canary_available).toBe(false);
    expect(presentation.next_action).toContain("Review first");
  });
});

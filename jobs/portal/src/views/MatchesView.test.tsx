import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { DiscoverySource, DiscoverySourceHealth, JobPosting, RunnerAvailability } from "../types";
import {
  canAutoSubmit,
  DiscoverySourceHealthList,
  JOB_IMPORT_ACTION_LABEL,
  JOB_IMPORT_DESCRIPTION,
  JOB_IMPORT_FALLBACK_LABEL,
  autoSubmitUnavailableReason,
  DISCOVERY_SOURCE_STALE_AFTER_MS,
  discoverySourceAction,
  discoverySourceState,
  isCandidateLead,
  isMatchEligibleForDefaultView,
  isMatchVisibleByState,
  isRecentPosting,
  postingAgeLabel,
  visibleMatches,
} from "./MatchesView";

function source(
  status: DiscoverySource["status"],
  health: DiscoverySourceHealth,
  lastSuccessAtMs: number | null = null,
): DiscoverySource {
  return {
    id: `${status}-${health}`,
    provider: "greenhouse",
    config: { company: "Northwind" },
    status,
    health,
    last_success_at_ms: lastSuccessAtMs,
  };
}

function match(canPrepare: boolean, capability: NonNullable<JobPosting["eligibility"]>["capability"] = "beta_review"): JobPosting {
  const canAutoSubmit = canPrepare && capability === "certified";
  return {
    id: "job-1",
    canonical_key: "job-1",
    source: "greenhouse",
    external_id: "1",
    company: "Acme",
    title: "Software Engineer",
    location: "Austin, TX",
    workplace: "On-site",
    canonical_url: "https://boards.greenhouse.io/acme/jobs/1",
    description: "",
    compensation: "",
    track_id: "track-1",
    match_score: 88,
    matched_reasons: [],
    missing_requirements: [],
    last_verified_at_ms: Date.now(),
    availability_status: "active",
    status: "matched",
    created_at_ms: 1,
    updated_at_ms: 1,
    eligibility: {
      capability,
      can_prepare: canPrepare,
      can_auto_submit: canAutoSubmit,
      can_queue_local: canAutoSubmit,
      can_queue_cloud: canAutoSubmit,
      hard_failures: canPrepare ? [] : [{ code: "location", message: "Austin is outside your selected locations." }],
      review_reasons: canPrepare ? [{ code: "beta", message: "Review first is required." }] : [],
      passed_checks: [],
      evaluated_at_ms: 1,
    },
  };
}

const runners = (available: boolean): RunnerAvailability => ({
  local: {
    status: available ? "available" : "invited_beta",
    available,
    plan_included: true,
    distribution_enabled: available,
    reason: available ? "Available." : "Bluey Browser is still in invited beta.",
    next_action: available ? "Run locally." : "Use Review first.",
  },
  cloud: {
    status: "upgrade_required",
    available: false,
    plan_included: false,
    distribution_enabled: false,
    reason: "Cloud plan required.",
    next_action: "View plans.",
  },
  auto_submit_available: available,
  auto_submit_reason: available
    ? "Auto-submit is available."
    : "Auto-submit is not available in this release because your included runner is still in invited beta.",
});

describe("discovery source health", () => {
  const now = Date.UTC(2026, 6, 29, 12);

  it.each(["degraded", "paused", "waiting"] as const)("preserves the %s server health state", (health) => {
    expect(discoverySourceState(source("active", health), now)).toBe(health);
  });

  it("keeps a recently successful healthy source healthy", () => {
    expect(discoverySourceState(source("active", "healthy", now - 60_000), now)).toBe("healthy");
  });

  it("treats a healthy source without a successful sync as waiting", () => {
    expect(discoverySourceState(source("active", "healthy"), now)).toBe("waiting");
  });

  it("treats an overdue healthy source as degraded", () => {
    const overdue = now - DISCOVERY_SOURCE_STALE_AFTER_MS - 1;
    expect(discoverySourceState(source("active", "healthy", overdue), now)).toBe("degraded");
  });

  it("treats a server-paused source as paused regardless of its prior health", () => {
    expect(discoverySourceState(source("paused", "healthy", now), now)).toBe("paused");
  });

  it("gives degraded and paused sources concise next steps", () => {
    expect(discoverySourceAction("degraded")).toBe(
      "Updates are delayed. Bluey is retrying; add an urgent job link meanwhile.",
    );
    expect(discoverySourceAction("paused")).toBe("Contact support to resume it. Paste urgent roles meanwhile.");
  });

  it("keeps paste-link discovery available when sources are not configured", () => {
    const html = renderToStaticMarkup(
      <DiscoverySourceHealthList
        sources={[]}
        onAddJob={() => undefined}
        onWatchCompanies={() => undefined}
      />,
    );

    expect(html).toContain("Automatic discovery is not connected");
    expect(html).toContain("Connect employer career pages for automatic checks");
    expect(html).toContain("Watch companies");
    expect(html).toContain("Add job link");
  });

  it("names the managed aggregate without exposing implementation-oriented source IDs", () => {
    const managed = {
      ...source("active", "healthy"),
      provider: "curated_feed",
      config: { company: "Curated career feeds" },
    } as DiscoverySource;
    const html = renderToStaticMarkup(
      <DiscoverySourceHealthList
        sources={[managed]}
        onAddJob={() => undefined}
        onWatchCompanies={() => undefined}
      />,
    );

    expect(html).toContain("Career feeds");
    expect(html).toContain("Public, allowlisted feeds");
    expect(html).not.toContain("Curated_feed");
  });
});

describe("job-link import", () => {
  it("keeps the URL-first contract distinct from its manual fallback", () => {
    expect(JOB_IMPORT_DESCRIPTION).toContain("direct employer link");
    expect(JOB_IMPORT_DESCRIPTION).toContain("checks freshness");
    expect(JOB_IMPORT_FALLBACK_LABEL).toBe("Can't import this link? Enter details manually");
    expect(JOB_IMPORT_ACTION_LABEL).toBe("Import & score");
  });

  it("does not present Bluey's import time as the employer posting date", () => {
    const job = {
      availability_status: "unknown",
      created_at_ms: Date.now(),
      posted_at_ms: null,
    } as unknown as JobPosting;

    expect(postingAgeLabel(job)).toBe("Posting date not listed");
    expect(isRecentPosting(job, 14)).toBe(true);
  });

  it("requires original-employer verification for managed-feed leads", () => {
    expect(isCandidateLead({
      source: "curated_feed:feed-simplify-new-grad",
      availability_status: "unknown",
      last_verified_at_ms: undefined,
    })).toBe(true);
    expect(isCandidateLead({
      source: "lever",
      availability_status: "active",
      last_verified_at_ms: Date.now(),
    })).toBe(false);
  });
});

describe("large match sets", () => {
  it("shows the first 50 results without discarding the remaining matches", () => {
    const matches = Array.from({ length: 125 }, (_, index) => ({ id: `job-${index + 1}` }));

    expect(visibleMatches(matches, 50)).toHaveLength(50);
    expect(visibleMatches(matches, 100)).toHaveLength(100);
    expect(visibleMatches(matches, 150)).toHaveLength(125);
  });

  it("does not let historical or passed rows inflate active tab counts", () => {
    const now = Date.now();
    const active = {
      status: "matched",
      availability_status: "active",
      posted_at_ms: now - 2 * 86_400_000,
    } as JobPosting;
    const stale = { ...active, posted_at_ms: now - 30 * 86_400_000 } as JobPosting;
    const expired = { ...active, availability_status: "expired" } as JobPosting;
    const skipped = { ...active, status: "skipped" } as JobPosting;

    expect(isMatchVisibleByState(active, 14, false, false)).toBe(true);
    expect(isMatchVisibleByState(stale, 14, false, false)).toBe(false);
    expect(isMatchVisibleByState(expired, 14, false, false)).toBe(false);
    expect(isMatchVisibleByState(skipped, 14, false, false)).toBe(false);
    expect(isMatchVisibleByState(active, 14, true, false)).toBe(false);
    expect(isMatchVisibleByState(active, 14, true, true)).toBe(true);
  });
});

describe("Career Track filtering and submission truth", () => {
  it("shows eligible jobs and hides hard-filter failures from the default match view", () => {
    expect(isMatchEligibleForDefaultView(match(true))).toBe(true);
    expect(isMatchEligibleForDefaultView(match(false))).toBe(false);
  });

  it("keeps managed-feed leads visible until original-source verification runs", () => {
    const lead = {
      ...match(false),
      source: "curated_feed:feed-simplify-new-grad",
      availability_status: "unknown",
      last_verified_at_ms: undefined,
    };

    expect(isMatchEligibleForDefaultView(lead)).toBe(true);
  });

  it("explains why beta, handoff, and hard-filtered jobs cannot Auto-submit", () => {
    expect(autoSubmitUnavailableReason(match(true, "beta_review"))).toContain("beta");
    expect(autoSubmitUnavailableReason(match(true, "handoff"))).toContain("user-controlled handoff");
    expect(autoSubmitUnavailableReason(match(false))).toContain("Career Track rules");
  });

  it("explains runner rollout separately from ATS eligibility", () => {
    const certified = match(true, "certified");

    expect(canAutoSubmit(certified, runners(false))).toBe(false);
    expect(autoSubmitUnavailableReason(certified, runners(false))).toContain("invited beta");
    expect(canAutoSubmit(certified, runners(true))).toBe(true);
    expect(autoSubmitUnavailableReason(certified, runners(true))).toBeUndefined();
  });
});

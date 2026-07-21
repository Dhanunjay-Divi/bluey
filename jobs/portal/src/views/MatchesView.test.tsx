import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { DiscoverySource, DiscoverySourceHealth, JobPosting } from "../types";
import {
  DiscoverySourceHealthList,
  JOB_IMPORT_ACTION_LABEL,
  JOB_IMPORT_DESCRIPTION,
  JOB_IMPORT_FALLBACK_LABEL,
  discoverySourceAction,
  discoverySourceState,
  isCandidateLead,
  isRecentPosting,
  postingAgeLabel,
  visibleMatches,
} from "./MatchesView";

function source(status: DiscoverySource["status"], health: DiscoverySourceHealth): DiscoverySource {
  return {
    id: `${status}-${health}`,
    provider: "greenhouse",
    config: { company: "Northwind" },
    status,
    health,
    last_success_at_ms: null,
  };
}

describe("discovery source health", () => {
  it.each(["healthy", "degraded", "paused", "waiting"] as const)("preserves the %s server health state", (health) => {
    expect(discoverySourceState(source("active", health))).toBe(health);
  });

  it("treats a server-paused source as paused regardless of its prior health", () => {
    expect(discoverySourceState(source("paused", "healthy"))).toBe("paused");
  });

  it("gives degraded and paused sources concise next steps", () => {
    expect(discoverySourceAction("degraded")).toBe("Bluey will retry. Paste urgent roles meanwhile.");
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
});

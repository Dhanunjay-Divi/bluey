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
  isRecentPosting,
  postingAgeLabel,
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
    const html = renderToStaticMarkup(<DiscoverySourceHealthList sources={[]} onAddJob={() => undefined} />);

    expect(html).toContain("Automatic discovery is not connected");
    expect(html).toContain("Add a job link now. Bluey will verify and rank it against the selected Career Track.");
    expect(html).toContain("Add job link");
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
});

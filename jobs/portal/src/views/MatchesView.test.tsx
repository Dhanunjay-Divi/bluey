import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { DiscoverySource, DiscoverySourceHealth } from "../types";
import { DiscoverySourceHealthList, discoverySourceAction, discoverySourceState } from "./MatchesView";

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

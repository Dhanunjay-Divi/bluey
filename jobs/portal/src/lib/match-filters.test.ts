import { describe, expect, it } from "vitest";
import type { JobPosting } from "../types";
import {
  DEFAULT_MATCH_FILTERS,
  clearMatchViewFilters,
  filterMatches,
  hasMatchViewFilters,
  normalizeMatchSearch,
  normalizeWorkplace,
  readMatchFilters,
  writeMatchFilters,
} from "./match-filters";

function job(
  id: string,
  overrides: Partial<JobPosting> = {},
): JobPosting {
  return {
    id,
    canonical_key: id,
    source: "greenhouse",
    external_id: id,
    company: "Acme",
    title: "Software Engineer",
    location: "New York, NY",
    workplace: "On-site",
    canonical_url: `https://boards.greenhouse.io/acme/jobs/${id}`,
    description: "",
    compensation: "",
    track_id: "track-active",
    match_score: 85,
    matched_reasons: [],
    missing_requirements: [],
    availability_status: "active",
    status: "matched",
    created_at_ms: 1,
    updated_at_ms: 1,
    ...overrides,
  };
}

describe("match filter URL state", () => {
  it("restores valid filters after refresh", () => {
    const filters = readMatchFilters(
      new URLSearchParams(
        "q=platform&track=track-active&score=83&workplace=remote"
          + "&unprepared=1&outside=1&passed=1&density=compact",
      ),
      new Set(["track-active"]),
    );

    expect(filters).toEqual({
      query: "platform",
      activeTrack: "track-active",
      minimumScore: 85,
      workplace: "remote",
      onlyUnprepared: true,
      showOutsideTrack: true,
      showPassed: true,
      density: "compact",
    });
  });

  it("falls back safely when a saved Career Track is inactive", () => {
    const filters = readMatchFilters(
      new URLSearchParams("track=track-inactive"),
      new Set(["track-active"]),
    );

    expect(filters.activeTrack).toBe("all");
  });

  it("clamps score and rejects unknown workplace values", () => {
    const filters = readMatchFilters(
      new URLSearchParams("score=999&workplace=somewhere"),
      new Set(),
    );

    expect(filters.minimumScore).toBe(100);
    expect(filters.workplace).toBe("all");
  });

  it("writes only active view filters while preserving preview state", () => {
    const params = writeMatchFilters(
      new URLSearchParams("preview=1&scenario=large&stale=old"),
      {
        ...DEFAULT_MATCH_FILTERS,
        query: "engineer",
        activeTrack: "track-active",
        workplace: "hybrid",
      },
    );

    expect(params.toString()).toBe(
      "preview=1&scenario=large&q=engineer&track=track-active&workplace=hybrid",
    );
  });

  it("preserves meaningful search text and omits whitespace-only queries", () => {
    const meaningful = writeMatchFilters(
      new URLSearchParams("preview=1"),
      { ...DEFAULT_MATCH_FILTERS, query: "  Platform    Engineer  " },
    );
    const empty = writeMatchFilters(
      new URLSearchParams("preview=1"),
      { ...DEFAULT_MATCH_FILTERS, query: "   " },
    );

    expect(meaningful.get("q")).toBe("  Platform    Engineer  ");
    expect(empty.toString()).toBe("preview=1");
  });
});

describe("match filtering", () => {
  const matches = [
    job("remote", {
      company: "Northwind",
      title: "Platform Engineer",
      location: "United States",
      workplace: "Remote",
      match_score: 95,
    }),
    job("hybrid", {
      company: "Contoso",
      title: "Product Engineer",
      location: "Austin, TX",
      workplace: "Hybrid",
      match_score: 80,
    }),
    job("onsite", {
      company: "Fabrikam",
      title: "Data Engineer",
      location: "Chicago, IL",
      workplace: "In person",
      match_score: 75,
    }),
  ];

  it("treats whitespace-only search as no search", () => {
    const filters = { ...DEFAULT_MATCH_FILTERS, query: "   " };

    expect(filterMatches(matches, filters, new Set())).toHaveLength(3);
    expect(hasMatchViewFilters(filters)).toBe(false);
  });

  it("normalizes case and repeated whitespace in search", () => {
    const filters = {
      ...DEFAULT_MATCH_FILTERS,
      query: "  platform    engineer ",
    };

    expect(normalizeMatchSearch(filters.query)).toBe("platform engineer");
    expect(filterMatches(matches, filters, new Set()).map((item) => item.id))
      .toEqual(["remote"]);
  });

  it("matches normalized workplace categories exactly", () => {
    expect(normalizeWorkplace("Remote - US")).toBe("remote");
    expect(normalizeWorkplace("Hybrid / 3 days")).toBe("hybrid");
    expect(normalizeWorkplace("In person")).toBe("on-site");

    const filters = { ...DEFAULT_MATCH_FILTERS, workplace: "on-site" as const };
    expect(filterMatches(matches, filters, new Set()).map((item) => item.id))
      .toEqual(["onsite"]);
  });

  it("combines score, Career Track, and preparation filters", () => {
    const filters = {
      ...DEFAULT_MATCH_FILTERS,
      activeTrack: "track-active",
      minimumScore: 80,
      onlyUnprepared: true,
    };

    expect(filterMatches(matches, filters, new Set(["remote"])).map((item) => item.id))
      .toEqual(["hybrid"]);
  });

  it("clears narrowing filters without losing the chosen context", () => {
    const cleared = clearMatchViewFilters({
      ...DEFAULT_MATCH_FILTERS,
      query: "engineer",
      activeTrack: "track-active",
      minimumScore: 90,
      workplace: "remote",
      onlyUnprepared: true,
      showOutsideTrack: true,
      density: "compact",
    });

    expect(cleared).toEqual({
      ...DEFAULT_MATCH_FILTERS,
      activeTrack: "track-active",
      showOutsideTrack: true,
      density: "compact",
    });
  });
});

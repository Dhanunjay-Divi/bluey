import { renderToStaticMarkup } from "react-dom/server";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it } from "vitest";
import { previewWorkspace } from "../data/preview";
import type {
  AtsCertificationSummary,
  AutoSubmitAuthorization,
  CareerTrack,
  DiscoverySource,
  DiscoverySourceHealth,
  JobPosting,
  RunnerAvailability,
} from "../types";
import {
  DISCOVERY_SOURCE_STALE_AFTER_MS,
  discoverySourceAction,
  discoverySourceState,
} from "../lib/discovery-source";
import {
  canAutoSubmit,
  DiscoverySourceHealthList,
  JOB_IMPORT_ACTION_LABEL,
  JOB_IMPORT_DESCRIPTION,
  JOB_IMPORT_FALLBACK_LABEL,
  MatchDetailReadOnlyNotice,
  MatchesView,
  PassedMatchActions,
  OriginalPostingVerificationAction,
  RequirementsLoadAction,
  autoSubmitUnavailableReason,
  canLoadPostingRequirements,
  canVerifySavedCuratedLead,
  checkMatchOriginal,
  isCandidateLead,
  isMatchVisibleInDefaultView,
  isMatchVisibleByState,
  isRecentPosting,
  jobEligibility,
  loadedMatchScopeNotice,
  matchDetailAfterRequirementsLoad,
  matchDetailAfterVerification,
  matchDetailActionBlockReason,
  matchDetailPosting,
  matchDetailTrackBlockReason,
  matchOriginalCheckMode,
  postingAgeLabel,
  visibleMatches,
} from "./MatchesView";

function certificationSummary(
  capability: NonNullable<JobPosting["eligibility"]>["capability"],
): AtsCertificationSummary {
  const certified = capability === "certified";
  return {
    provider_label: certified ? "Greenhouse" : "Application system",
    adapter_version: certified ? "2026.07.1-beta.1" : null,
    certified_runner_kinds: certified ? ["cloud"] : [],
    status: certified ? "active" : "review_only",
    last_verified_at_ms: certified ? Date.now() - 60_000 : null,
    expires_at_ms: certified ? Date.now() + 60 * 60_000 : null,
    reason: certified
      ? "The current job and cloud runner passed server verification."
      : "Review first is required for this application system.",
    next_action: certified
      ? "Review the application kit and choose an available runner."
      : "Review the application kit before continuing.",
    canary_available: false,
  };
}

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
      ats_certification: certificationSummary(capability),
    },
  };
}

const runners = (available: boolean): RunnerAvailability => ({
  local: {
    status: "limited_beta",
    available: false,
    plan_included: false,
    distribution_enabled: false,
    reason: "Local execution is parked.",
    next_action: "Use cloud automation or Review.",
  },
  cloud: {
    status: available ? "available" : "limited_beta",
    available,
    plan_included: true,
    distribution_enabled: available,
    reason: available ? "Available." : "Cloud automation is temporarily unavailable in this limited public beta.",
    next_action: available ? "Queue in the cloud." : "Use Review first.",
  },
  auto_submit_available: available,
  auto_submit_reason: available
    ? "Auto-submit is available."
    : "Cloud automation is temporarily unavailable in this limited public beta.",
});

const authorization = (status: AutoSubmitAuthorization["status"] = "active"): AutoSubmitAuthorization => ({
  id: "authorization-1",
  career_track_id: "track-1",
  application_identity_id: "identity-1",
  source_resume_asset_id: "resume-1",
  revision_no: 1,
  authorized_at_ms: 1,
  status,
});

describe("passed match actions", () => {
  it("offers Restore as the only action and preserves the Career Track rules", () => {
    const html = renderToStaticMarkup(<PassedMatchActions busy={false} onRestore={() => undefined} />);
    expect(html.match(/<button\b/g)).toHaveLength(1);
    expect(html).toContain("Restore match");
    expect(html).toContain("Career Track rules still apply");
    expect(html).not.toContain("Prepare application");
    expect(html).not.toContain("disabled");
  });

  it("shows progress and disables Restore while feedback is saving", () => {
    const html = renderToStaticMarkup(<PassedMatchActions busy onRestore={() => undefined} />);
    expect(html).toContain("Restoring…");
    expect(html).toContain("disabled");
  });
});

describe("selected match Career Track authority", () => {
  const selected = match(true, "certified");
  const matchingTrack = previewWorkspace.tracks.find((track) => track.id === selected.track_id)!;

  function withRefreshMarker(track: CareerTrack, codes: string[]): CareerTrack {
    return {
      ...track,
      policy: {
        ...track.policy,
        authority: {
          review_reason_codes: codes,
        } as NonNullable<CareerTrack["policy"]["authority"]>,
      },
    };
  }

  it("blocks a missing or paused selected Track as inactive", () => {
    expect(matchDetailTrackBlockReason(selected, [])).toBe("inactive");
    expect(matchDetailTrackBlockReason(selected, [{ ...matchingTrack, active: false }]))
      .toBe("inactive");
  });

  it("blocks only the matching Track's explicit portal refresh marker", () => {
    const otherTrack = { ...matchingTrack, id: "track-other" };
    expect(matchDetailTrackBlockReason(
      selected,
      [withRefreshMarker(matchingTrack, ["portal_authority_refresh_required"])],
    )).toBe("refresh_required");
    expect(matchDetailTrackBlockReason(
      selected,
      [matchingTrack, withRefreshMarker(otherTrack, ["portal_authority_refresh_required"])],
    )).toBeNull();
    expect(matchDetailTrackBlockReason(
      selected,
      [withRefreshMarker(matchingTrack, [""])],
    )).toBeNull();
    expect(matchDetailTrackBlockReason(null, [matchingTrack])).toBeNull();
  });

  it("renders a nonblank status for each read-only Track reason", () => {
    const inactive = renderToStaticMarkup(<MatchDetailReadOnlyNotice reason="inactive" />);
    expect(inactive).toContain('role="status"');
    expect(inactive).toContain("This Career Track is paused or no longer available.");
    expect(inactive.trim()).not.toBe("");

    const refresh = renderToStaticMarkup(<MatchDetailReadOnlyNotice reason="refresh_required" />);
    expect(refresh).toContain('role="status"');
    expect(refresh).toContain("Refresh Matches to use the current Career Track.");
    expect(refresh.trim()).not.toBe("");
  });

  it("keeps a retained detail read-only if refreshed Matches omit or retarget it", () => {
    const selectedMatch = match(true, "certified");
    expect(matchDetailPosting(selectedMatch, [])).toBe(selectedMatch);
    expect(matchDetailActionBlockReason(selectedMatch, [matchingTrack], [])).toBe("match_unconfirmed");
    expect(matchDetailActionBlockReason(selectedMatch, [matchingTrack], [
      { ...selectedMatch, canonical_url: "https://boards.greenhouse.io/acme/jobs/2" },
    ])).toBe("match_unconfirmed");
    expect(matchDetailActionBlockReason(selectedMatch, [matchingTrack], [
      { ...selectedMatch, track_id: "track-other" },
    ])).toBe("match_unconfirmed");
    expect(matchDetailActionBlockReason(selectedMatch, [matchingTrack], [selectedMatch])).toBeNull();
    expect(renderToStaticMarkup(<MatchDetailReadOnlyNotice reason="match_unconfirmed" />))
      .toContain("This posting isn’t in your current Matches.");
  });
});

describe("selected curated lead verification", () => {
  const curatedUnknownLead = {
    source: "curated_feed:feed-one",
    availability_status: "unknown",
  } as Pick<JobPosting, "source" | "availability_status">;

  it("only enables verification for an available unknown curated lead", () => {
    expect(canVerifySavedCuratedLead(curatedUnknownLead, true)).toBe(true);
    expect(canVerifySavedCuratedLead(curatedUnknownLead, false)).toBe(false);
    expect(canVerifySavedCuratedLead(curatedUnknownLead, "true")).toBe(false);
    expect(canVerifySavedCuratedLead({ ...curatedUnknownLead, availability_status: "active" }, true))
      .toBe(false);
    expect(canVerifySavedCuratedLead({ ...curatedUnknownLead, source: "greenhouse_import" }, true))
      .toBe(false);
  });

  it("keeps ordinary candidate leads outside the curated verification path", () => {
    expect(isCandidateLead({
      source: "pasted_link",
      availability_status: "unknown",
      last_verified_at_ms: null,
    })).toBe(true);
    expect(canVerifySavedCuratedLead({
      source: "pasted_link",
      availability_status: "unknown",
    }, true)).toBe(false);
  });

  it("hides the action when the server gate is unavailable", () => {
    const html = renderToStaticMarkup(
      <OriginalPostingVerificationAction
        available={false}
        pending={false}
        blocked={false}
        onVerify={() => undefined}
      />,
    );
    expect(html).toBe("");
  });

  it("renders the verification action and makes a pending request busy and unavailable", () => {
    const ready = renderToStaticMarkup(
      <OriginalPostingVerificationAction
        available
        pending={false}
        blocked={false}
        onVerify={() => undefined}
      />,
    );
    expect(ready).toContain("Verify original posting");
    expect(ready).not.toContain("disabled");

    const pending = renderToStaticMarkup(
      <OriginalPostingVerificationAction
        available
        pending
        blocked={false}
        onVerify={() => undefined}
      />,
    );
    expect(pending).toContain("Checking posting…");
    expect(pending).toContain('aria-busy="true"');
    expect(pending).toContain("disabled");
  });

  it("renders a distinct bodyless requirements action only when its exact gate is available", () => {
    const ready = renderToStaticMarkup(
      <RequirementsLoadAction
        available
        pending={false}
        blocked={false}
        onLoad={() => undefined}
      />,
    );
    expect(ready).toContain("Load requirements");
    expect(ready).not.toContain("Verify original posting");

    const pending = renderToStaticMarkup(
      <RequirementsLoadAction
        available
        pending
        blocked={false}
        onLoad={() => undefined}
      />,
    );
    expect(pending).toContain("Loading requirements…");
    expect(pending).toContain('aria-busy="true"');
    expect(pending).toContain("disabled");

    expect(renderToStaticMarkup(
      <RequirementsLoadAction
        available={false}
        pending={false}
        blocked={false}
        onLoad={() => undefined}
      />,
    )).toBe("");
  });
});

describe("verify an original posting separately from Prepare", () => {
  const ordinary = {
    id: "saved-lead",
    source: "pasted_link",
    availability_status: "unknown",
    last_verified_at_ms: null,
    canonical_url: "https://example.test/jobs/one",
    track_id: "track-one",
  } as JobPosting;
  const curated = { ...ordinary, source: "curated_feed:feed-one" };
  const verified = { ...ordinary, id: "verified-job", source: "lever", availability_status: "active",
    last_verified_at_ms: 1_800_000_000_000 };
  const unexpected = async (): Promise<never> => { throw new Error("unexpected verification route"); };

  it("imports an ordinary lead with no manual details and returns the verified match", async () => {
    expect(matchOriginalCheckMode(ordinary, false)).toBe("import");
    const result = await checkMatchOriginal(ordinary, false, unexpected, async (input) => {
      expect(input).toEqual({
        canonical_url: ordinary.canonical_url,
        track_id: ordinary.track_id,
        pasted_description: "",
        company: "",
        title: "",
        location: "",
        workplace: "Unknown",
        compensation: "",
      });
      return verified;
    });
    expect(result).toEqual(verified);
    expect(matchOriginalCheckMode(result, true)).toBeNull();
  });

  it("refuses a different canonical job even when the Track and verdict match", async () => {
    const changed = { ...verified, canonical_url: "https://example.test/jobs/two" };
    await expect(checkMatchOriginal(ordinary, false, unexpected, async () => changed))
      .rejects.toThrow("different posting link");
    expect(matchDetailAfterVerification(ordinary, ordinary, changed)).toBeNull();
  });

  it("verifies curated rows by stored ID only and never falls back around the server flag", async () => {
    expect(matchOriginalCheckMode(curated, true)).toBe("curated");
    expect(await checkMatchOriginal(curated, true, async (id) => {
      expect(id).toBe(curated.id);
      return { ...verified, id: curated.id };
    }, unexpected)).toMatchObject({ id: curated.id, source: "lever" });
    await expect(checkMatchOriginal(curated, false, unexpected, unexpected))
      .rejects.toThrow("verification is not available");
  });

  it.each([
    ordinary,
    { ...verified, last_verified_at_ms: null },
    { ...verified, availability_status: "unknown" },
    { ...verified, last_verified_at_ms: Number.NaN },
    { ...verified, track_id: "foreign-track" },
  ])("does not treat a missing or invalid verification result as success", async (unverified) => {
    await expect(checkMatchOriginal(ordinary, false, unexpected, async () => unverified))
      .rejects.toThrow("couldn’t verify the original posting");
  });

  it("does not substitute a different match ID for the selected curated row", async () => {
    await expect(checkMatchOriginal(curated, true, async () => verified, unexpected))
      .rejects.toThrow("couldn’t verify the original posting");
  });

  it("keeps a summary lead visible without allowing Prepare or generic verification", async () => {
    const prior = match(true, "certified");
    const summary: JobPosting = {
      ...prior,
      source: "smartrecruiters",
      description: "",
      eligibility: {
        ...prior.eligibility!,
        review_reasons: [{ code: "posting_requirements_not_loaded", message: "Full requirements are not loaded." }],
      },
    };
    expect(isCandidateLead(summary)).toBe(true);
    expect(isMatchVisibleInDefaultView(summary)).toBe(true);
    expect(jobEligibility(summary)).toMatchObject({ can_prepare: false, can_queue_cloud: false, can_auto_submit: false });
    expect(matchOriginalCheckMode(summary, true)).toBeNull();
    expect(matchOriginalCheckMode(summary, false)).toBeNull();
    expect(canLoadPostingRequirements(summary, true, "")).toBe(true);
    expect(canLoadPostingRequirements(summary, undefined, "")).toBe(false);
    expect(canLoadPostingRequirements(summary, false, "")).toBe(false);
    expect(canLoadPostingRequirements(summary, 1, "")).toBe(false);
    expect(canLoadPostingRequirements(summary, true, "?preview=1")).toBe(false);
    await expect(checkMatchOriginal(summary, true, unexpected, unexpected)).rejects.toThrow("verification is not available");
    expect(isCandidateLead({ ...prior, source: "smartrecruiters" })).toBe(false);
  });

  it("accepts non-actionable requirements only for the unchanged selected ID and Track", () => {
    const started = match(true, "certified");
    started.description = "";
    started.eligibility = {
      ...started.eligibility!,
      capability: "blocked",
      can_prepare: false,
      can_queue_local: false,
      can_queue_cloud: false,
      can_auto_submit: false,
      review_reasons: [{ code: "posting_requirements_not_loaded", message: "Load requirements." }],
    };
    const loaded = {
      ...started,
      description: "Build reliable systems.",
      eligibility: {
        ...started.eligibility!,
        evaluated_at_ms: started.eligibility!.evaluated_at_ms + 1,
        can_prepare: false,
        can_queue_local: false,
        can_queue_cloud: false,
        can_auto_submit: false,
        review_reasons: [{ code: "original_source_unverified", message: "Verify this posting." }],
      },
    };

    expect(matchDetailAfterRequirementsLoad(started, started, loaded)).toBe(loaded);
    const fresh = match(true, "certified");
    const authoritativeReadback = {
      ...loaded,
      eligibility: {
        ...fresh.eligibility!,
        evaluated_at_ms: loaded.eligibility!.evaluated_at_ms + 1,
      },
    };
    expect(matchDetailAfterRequirementsLoad(started, authoritativeReadback, loaded))
      .toBe(authoritativeReadback);

    for (const field of [
      "can_prepare",
      "can_queue_local",
      "can_queue_cloud",
      "can_auto_submit",
    ] as const) {
      expect(matchDetailAfterRequirementsLoad(started, started, {
        ...loaded,
        eligibility: { ...loaded.eligibility, [field]: true },
      })).toBeNull();
      const missing = { ...loaded.eligibility } as Partial<NonNullable<JobPosting["eligibility"]>>;
      delete missing[field];
      expect(matchDetailAfterRequirementsLoad(started, started, {
        ...loaded,
        eligibility: missing as JobPosting["eligibility"],
      })).toBeNull();
    }

    expect(matchDetailAfterRequirementsLoad(started, started, { ...loaded, description: "" }))
      .toBeNull();
    expect(matchDetailAfterRequirementsLoad(
      started,
      started,
      { ...loaded, source: "smartrecruiters_import" },
    )).toBeNull();
    expect(matchDetailAfterRequirementsLoad(
      started,
      started,
      { ...loaded, canonical_url: `${loaded.canonical_url}/other` },
    )).toBeNull();
    expect(matchDetailAfterRequirementsLoad(started, null, loaded)).toBeNull();
    expect(matchDetailAfterRequirementsLoad(
      started,
      { ...started, track_id: "another-track" },
      loaded,
    )).toBeNull();
    expect(matchDetailAfterRequirementsLoad(
      started,
      started,
      { ...loaded, id: "another-match" },
    )).toBeNull();
    expect(matchDetailAfterRequirementsLoad(
      started,
      started,
      { ...loaded, track_id: "another-track" },
    )).toBeNull();
    expect(matchDetailAfterRequirementsLoad(started, postingOnIdentityHold(), loaded)).toBeNull();
    expect(matchDetailAfterRequirementsLoad(started, started, started)).toBeNull();
  });
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

  it("does not label future dates as posted today", () => {
    const now = 1_800_000_000_000;
    const day = 86_400_000;
    const job = { availability_status: "active", posted_at_ms: now + day };
    expect(isRecentPosting(job, 14, now)).toBe(true);
    expect(postingAgeLabel(job, now)).toBe("Posting date is in the future");

    const invalid = { ...job, posted_at_ms: now + day + 1 };
    expect(isRecentPosting(invalid, 14, now)).toBe(false);
    expect(postingAgeLabel(invalid, now)).toBe("Posting date needs verification");
    expect(postingAgeLabel({ posted_at_ms: now }, now)).toBe("Posted today");
    expect(isRecentPosting({ ...job, posted_at_ms: NaN }, 14, now)).toBe(false);
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

  it("states that search, sorting, filters, and counts cover loaded rows only", () => {
    expect(loadedMatchScopeNotice(50, true)).toBe(
      "Search, sorting, filters, and Career Track counts currently cover "
        + "the 50 loaded jobs. Load more to expand this view.",
    );
    expect(loadedMatchScopeNotice(50, false)).toBeNull();
  });

  it("does not let historical or passed rows inflate active tab counts", () => {
    const now = Date.now();
    const active = {
      status: "matched",
      availability_status: "active",
      posted_at_ms: now - 2 * 86_400_000,
    } as JobPosting;
    const stale = { ...active, posted_at_ms: now - 30 * 86_400_000 } as JobPosting;
    const future = { ...active, posted_at_ms: now + 7 * 86_400_000 };
    const expired = { ...active, availability_status: "expired" } as JobPosting;
    const skipped = { ...active, status: "skipped" } as JobPosting;

    expect(isMatchVisibleByState(active, 14, false, false)).toBe(true);
    expect(isMatchVisibleByState(stale, 14, false, false)).toBe(false);
    expect(isMatchVisibleByState(future, 14, false, false)).toBe(false);
    expect(isMatchVisibleByState(future, 14, true, true)).toBe(true);
    expect(isMatchVisibleByState(stale, 14, true, true)).toBe(true);
    expect(isMatchVisibleByState({ ...active, posted_at_ms: undefined }, 14, true, true)).toBe(true);
    expect(isMatchVisibleByState(future, 14, false, true)).toBe(false);
    expect(isMatchVisibleByState(expired, 14, false, false)).toBe(false);
    expect(isMatchVisibleByState(skipped, 14, false, false)).toBe(false);
    expect(isMatchVisibleByState(expired, 14, true, true)).toBe(false);
    expect(isMatchVisibleByState(skipped, 14, true, true)).toBe(false);
    expect(isMatchVisibleByState(active, 14, true, false)).toBe(false);
    expect(isMatchVisibleByState(active, 14, true, true)).toBe(true);
  });
});

describe("Career Track filtering and submission truth", () => {
  it("shows eligible jobs and hides hard-filter failures from the default match view", () => {
    expect(isMatchVisibleInDefaultView(match(true))).toBe(true);
    expect(isMatchVisibleInDefaultView(match(false))).toBe(false);
  });

  it("keeps managed-feed leads visible until original-source verification runs", () => {
    const lead = {
      ...match(false),
      source: "curated_feed:feed-simplify-new-grad",
      availability_status: "unknown",
      last_verified_at_ms: undefined,
    };

    expect(isMatchVisibleInDefaultView(lead)).toBe(true);
  });

  it("shows a posting identity hold without exposing ordinary out-of-track jobs", () => {
    const track = previewWorkspace.tracks.find((item) => item.active);
    if (!track) throw new Error("Expected an active preview Career Track");
    const held = {
      ...postingOnIdentityHold(),
      track_id: track.id,
      title: "Held engineer posting",
    };
    const outside = {
      ...match(false),
      id: "outside-match",
      track_id: track.id,
      title: "Ordinary out-of-track posting",
    };
    const workspace = {
      ...previewWorkspace,
      matches: [held, outside],
      candidate_events: [],
      matches_page: { ...previewWorkspace.matches_page, has_more: false },
    };
    const unexpectedAction = async (): Promise<never> => {
      throw new Error("Rendering Matches must not perform any job action");
    };
    const html = renderToStaticMarkup(
      <MemoryRouter>
        <MatchesView
          workspace={workspace}
          previewSearch=""
          onAddJob={unexpectedAction}
          onVerifyOriginal={unexpectedAction}
          onLoadRequirements={unexpectedAction}
          onPrepare={unexpectedAction}
          onSaveCandidateEvent={unexpectedAction}
          onSearchDiscoverySources={unexpectedAction}
          onConnectDiscoverySource={unexpectedAction}
          onLoadMore={unexpectedAction}
        />
      </MemoryRouter>,
    );
    expect(html).toContain("Held engineer posting");
    expect(html).toContain('status-pill capability-blocked">Posting needs review');
    expect(html).not.toContain("Ordinary out-of-track posting");
    expect(html).not.toContain("Verify original posting");
    expect(html).not.toContain("Prepare application");
  });

  it("does not let otherwise-certified flags or candidate lead verification bypass a posting hold", async () => {
    const held = postingOnIdentityHold();
    expect(jobEligibility(held)).toMatchObject({
      capability: "blocked",
      can_prepare: false,
      can_queue_local: false,
      can_queue_cloud: false,
      can_auto_submit: false,
    });
    expect(canAutoSubmit(held, runners(true), [authorization()])).toBe(false);
    expect(autoSubmitUnavailableReason(held)).toContain("posting’s identity needs review");

    const heldLead = { ...held, availability_status: "unknown", last_verified_at_ms: undefined };
    expect(isCandidateLead(heldLead)).toBe(true);
    expect(matchOriginalCheckMode(heldLead, true)).toBeNull();
    const unexpectedVerify = async (): Promise<never> => { throw new Error("Unexpected verification"); };
    await expect(checkMatchOriginal(heldLead, true, unexpectedVerify, unexpectedVerify))
      .rejects.toThrow("Original posting verification is not available");
  });

  it("uses a refreshed hold over an earlier actionable details selection", () => {
    const snapshot = match(true, "certified");
    const held = postingOnIdentityHold();
    expect(matchDetailPosting(snapshot, [{ ...held, id: "unrelated" }])).toBe(snapshot);
    expect(matchDetailPosting(snapshot, [held])).toBe(held);
    expect(matchDetailPosting(held, [snapshot])).toBe(held);

    const malformed = {
      ...held,
      eligibility: { ...held.eligibility!, evaluated_at_ms: -1 },
    };
    expect(jobEligibility(malformed).can_prepare).toBe(false);
    expect(isMatchVisibleInDefaultView(malformed)).toBe(false);
  });

  it("uses the refreshed source denial in an open details selection", () => {
    const snapshot = match(true, "certified");
    const closed: JobPosting = {
      ...snapshot,
      availability_status: "expired",
      eligibility: {
        ...match(false, "blocked").eligibility!,
        hard_failures: [{ code: "original_source_rejected", message: "The original posting is no longer available." }],
        evaluated_at_ms: snapshot.eligibility!.evaluated_at_ms + 1,
      },
    };
    // Availability and eligibility may change without changing the stored posting timestamp.
    expect(closed.updated_at_ms).toBe(snapshot.updated_at_ms);
    const current = matchDetailPosting(snapshot, [closed]);
    expect(current).toBe(closed);
    expect(jobEligibility(current!).can_prepare).toBe(false);
    expect(canAutoSubmit(current!, runners(true), [authorization()])).toBe(false);
    expect(matchDetailPosting(current, [])).toBe(closed);
    expect(matchDetailPosting(current, [snapshot])).toBe(closed);
  });

  it("keeps a newer checked detail when a ranked row is older or belongs to another track", () => {
    const old = { ...match(false), availability_status: "unknown", last_verified_at_ms: null };
    const snapshot = match(true, "certified");
    snapshot.eligibility!.evaluated_at_ms += 1;
    expect(matchDetailPosting(snapshot, [])).toBe(snapshot);
    expect(matchDetailPosting(snapshot, [old])).toBe(snapshot);
    expect(matchDetailPosting(snapshot, [{ ...old, eligibility: undefined }])).toBe(snapshot);
    expect(matchDetailPosting(snapshot, [{ ...old, track_id: "other-track", eligibility: {
      ...old.eligibility!, evaluated_at_ms: snapshot.eligibility!.evaluated_at_ms + 1,
    } }])).toBe(snapshot);
  });

  it("lets a tied refresh remove actions but never grant new ones", () => {
    const actionable = match(true, "certified");
    const noCloud = match(true, "beta_review");
    const blocked = match(false, "blocked");
    expect(matchDetailPosting(actionable, [noCloud])).toBe(noCloud);
    expect(matchDetailPosting(noCloud, [actionable])).toBe(noCloud);
    expect(matchDetailPosting(actionable, [blocked])).toBe(blocked);
    expect(matchDetailPosting(blocked, [actionable])).toBe(blocked);
  });

  it("does not apply a pending verification after a committed hold, denial or track change", async () => {
    const started = { ...match(false), availability_status: "unknown", last_verified_at_ms: null };
    const verified = match(true, "certified");
    const hold = postingOnIdentityHold();
    const denied = match(false, "blocked");
    denied.eligibility!.evaluated_at_ms = verified.eligibility!.evaluated_at_ms + 1;
    for (const latest of [hold, denied, { ...started, track_id: "other-track" }, null]) {
      let displayed: JobPosting | null = started;
      let finish!: (job: JobPosting) => void;
      const response = new Promise<JobPosting>((resolve) => { finish = resolve; });
      const completion = response.then((job) => matchDetailAfterVerification(started, displayed, job));
      displayed = latest;
      finish(verified);
      expect(await completion).toBeNull();
    }
    expect(matchDetailAfterVerification(started, started, verified)).toBe(verified);
    expect(matchDetailAfterVerification(started, started, { ...verified, track_id: "other-track" })).toBeNull();
    expect(matchDetailAfterVerification(started, started, { ...verified, id: "normalized-import" }))
      .toMatchObject({ id: "normalized-import" });
  });

  it("rejects a late same-ID verification after the committed source target changes", () => {
    const started = {
      ...match(false),
      source: "curated_feed:acme",
      availability_status: "unknown",
      last_verified_at_ms: null,
    };
    const verified = {
      ...match(true, "certified"),
      id: started.id,
      source: "greenhouse",
      canonical_url: started.canonical_url,
      eligibility: { ...match(true, "certified").eligibility!, evaluated_at_ms: 4 },
    };
    const changedUrl = {
      ...verified,
      canonical_url: `${verified.canonical_url}/replacement`,
      eligibility: { ...verified.eligibility!, evaluated_at_ms: 3 },
    };
    const changedSource = {
      ...verified,
      source: "lever",
      eligibility: { ...verified.eligibility!, evaluated_at_ms: 3 },
    };

    expect(matchDetailAfterVerification(started, changedUrl, verified)).toBeNull();
    expect(matchDetailAfterVerification(started, changedSource, verified)).toBeNull();
  });

  it("accepts an exact committed verification target but not a stale decision", () => {
    const started = {
      ...match(false),
      source: "curated_feed:acme",
      availability_status: "unknown",
      last_verified_at_ms: null,
    };
    const verified = {
      ...match(true, "certified"),
      id: started.id,
      source: "greenhouse",
      canonical_url: started.canonical_url,
      eligibility: { ...match(true, "certified").eligibility!, evaluated_at_ms: 4 },
    };
    const committed = {
      ...verified,
      eligibility: { ...verified.eligibility!, evaluated_at_ms: 3 },
    };
    const newer = {
      ...committed,
      eligibility: { ...committed.eligibility!, evaluated_at_ms: 5 },
    };

    expect(matchDetailAfterVerification(started, committed, verified)).toBe(verified);
    expect(matchDetailAfterVerification(started, newer, verified)).toBeNull();
  });

  it("explains why beta, handoff, and hard-filtered jobs cannot Auto-submit", () => {
    expect(autoSubmitUnavailableReason(match(true, "beta_review"))).toContain("beta");
    expect(autoSubmitUnavailableReason(match(true, "handoff"))).toContain("user-controlled handoff");
    expect(autoSubmitUnavailableReason(match(false))).toContain("Career Track rules");
  });

  it("explains runner rollout separately from ATS eligibility", () => {
    const certified = match(true, "certified");

    expect(canAutoSubmit(certified, runners(false), [authorization()])).toBe(false);
    expect(autoSubmitUnavailableReason(certified, runners(false), [authorization()])).toContain("limited public beta");
    expect(canAutoSubmit(certified, runners(true), [])).toBe(false);
    expect(autoSubmitUnavailableReason(certified, runners(true), [])).toContain("Career Track");
    expect(canAutoSubmit(certified, runners(true), [authorization("needs_review")])).toBe(false);
    expect(autoSubmitUnavailableReason(certified, runners(true), [authorization("needs_review")])).toContain(
      "changed",
    );
    expect(canAutoSubmit(certified, runners(true), [authorization()])).toBe(true);
    expect(autoSubmitUnavailableReason(certified, runners(true), [authorization()])).toBeUndefined();
  });

  it("fails a mixed-version certified response closed when its summary is missing", () => {
    const certified = match(true, "certified");
    delete certified.eligibility?.ats_certification;

    expect(jobEligibility(certified).capability).toBe("unknown_review");
    expect(canAutoSubmit(certified, runners(true), [authorization()])).toBe(false);
    expect(autoSubmitUnavailableReason(certified, runners(true), [authorization()])).toContain(
      "certification details are unavailable",
    );
  });

  it("does not expose a local-only certification through cloud automation", () => {
    const localCertified = match(true, "certified");
    if (localCertified.eligibility?.ats_certification) {
      localCertified.eligibility.ats_certification = {
        ...localCertified.eligibility.ats_certification,
        certified_runner_kinds: ["local"],
        reason: "The current job and local runner passed server verification.",
        next_action: "Review the application kit before continuing.",
      };
    }

    expect(canAutoSubmit(localCertified, runners(true), [authorization()])).toBe(false);
    expect(autoSubmitUnavailableReason(
      localCertified,
      runners(true),
      [authorization()],
    )).toContain("cloud automation");
  });

  it("does not treat a parked local runner as web automation availability", () => {
    const localOnly = {
      ...runners(false),
      local: {
        ...runners(false).local,
        status: "available" as const,
        available: true,
        distribution_enabled: true,
      },
      auto_submit_available: true,
    };

    expect(canAutoSubmit(match(true, "certified"), localOnly, [authorization()])).toBe(false);
    expect(autoSubmitUnavailableReason(
      match(true, "certified"),
      localOnly,
      [authorization()],
    )).toContain("limited public beta");
  });
});

function postingOnIdentityHold(): JobPosting {
  const job = match(true, "certified");
  if (!job.eligibility) throw new Error("Expected a certified fixture");
  return {
    ...job,
    source: "workday",
    canonical_url: "https://example.invalid/workday/1",
    eligibility: {
      ...job.eligibility,
      hard_failures: [{
        code: "workday_reconciliation_required",
        message: "This Workday posting has an identity change that needs review before preparing or applying.",
      }],
    },
  };
}

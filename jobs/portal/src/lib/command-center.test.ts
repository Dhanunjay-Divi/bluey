import { describe, expect, it } from "vitest";
import { previewWorkspace } from "../data/preview";
import type { JobsWorkspace } from "../types";
import {
  commandCenterHref,
  commandCenterRecency,
  commandCenterSummary,
} from "./command-center";

const now = 2_000_000_000_000;

function workspace(overrides: Partial<JobsWorkspace> = {}): JobsWorkspace {
  return { ...previewWorkspace, ...overrides };
}

describe("career command center summary", () => {
  it("shows only active matches from the rolling 24-hour window and sorts by score", () => {
    const matches = previewWorkspace.matches.map((job, index) => ({
      ...job,
      id: `job-${index}`,
      match_score: [80, 96, 91, 99][index],
      posted_at_ms: [now - 60_000, now - 20 * 60 * 60_000, now - 25 * 60 * 60_000, now + 1][index],
      availability_status: index === 0 ? "expired" : "active",
    }));

    const summary = commandCenterSummary(workspace({ matches }), now);

    expect(summary.today_matches.map((job) => job.id)).toEqual(["job-1"]);
  });

  it("keeps unprojected resume and canonical Track authority fail-closed", () => {
    const summary = commandCenterSummary(workspace({
      discovery_sources: previewWorkspace.discovery_sources.map((source, index) => ({
        ...source,
        last_success_at_ms: index === 0 ? now - 60_000 : source.last_success_at_ms,
      })),
    }), now, "?preview=1&scenario=command");

    expect(summary.readiness.map((item) => item.id)).toEqual([
      "resume",
      "identity",
      "tracks",
      "facts",
      "sources",
      "inbox",
    ]);
    expect(summary.readiness_ready).toBe(3);
    expect(summary.readiness_required).toBe(5);
    expect(summary.readiness.find((item) => item.id === "resume")).toMatchObject({
      state: "reported",
      detail: "The workspace reports a resume reference, but this view has no authoritative asset read-back.",
    });
    expect(summary.readiness.find((item) => item.id === "tracks")).toMatchObject({
      state: "reported",
      detail: (
        "2 active Tracks have a verified identity binding. Canonical role, location, policy, "
        + "and resume binding are not proven here."
      ),
    });
    expect(summary.readiness.find((item) => item.id === "inbox")?.state).toBe("optional");
    expect(summary.inbox.connected.map((connection) => connection.provider)).toEqual(["gmail"]);
    expect(summary.inbox.calendars.map((calendar) => calendar.provider)).toEqual([
      "google_calendar",
    ]);
    expect(summary.inbox.linkedin_supported).toBe(false);
    expect(summary.readiness.find((item) => item.id === "resume")?.href).toBe(
      "/resume?preview=1&scenario=command",
    );
  });

  it("fails required setup checks closed when resume, identity, facts, and sources are absent", () => {
    const summary = commandCenterSummary(workspace({
      profile: {
        ...previewWorkspace.profile,
        source_resume_name: "",
        source_resume_asset_id: "",
        source_resume_sha256: "",
      },
      facts: [],
      application_identities: [],
      mailbox_connections: [],
      discovery_sources: [],
    }), now);

    expect(summary.readiness_ready).toBe(0);
    expect(summary.readiness_required).toBe(5);
    expect(summary.readiness.filter((item) => item.state === "needs_action")).toHaveLength(5);
    expect(summary.readiness.find((item) => item.id === "inbox")?.state).toBe("optional");
    expect(summary.next_actions[0]?.id).toBe("interventions");
    expect(summary.next_actions.some((action) => action.id === "readiness-resume")).toBe(true);
  });

  it("fails resume and Track readiness closed for a partial source-resume binding", () => {
    const summary = commandCenterSummary(workspace({
      profile: {
        ...previewWorkspace.profile,
        source_resume_asset_id: "resume-asset-without-hash",
        source_resume_sha256: "",
      },
    }), now);

    expect(summary.readiness.find((item) => item.id === "resume")).toMatchObject({
      state: "needs_action",
      detail: "The source resume binding is incomplete; re-import it before preparation.",
    });
    expect(summary.readiness.find((item) => item.id === "tracks")?.state).toBe("reported");
  });

  it("keeps a plausible but unverified resume reference reported rather than ready", () => {
    const summary = commandCenterSummary(workspace({
      profile: {
        ...previewWorkspace.profile,
        source_resume_asset_id: "nonexistent-asset",
        source_resume_sha256: "a".repeat(64),
      },
    }), now);

    expect(summary.readiness.find((item) => item.id === "resume")?.state).toBe("reported");
    expect(summary.readiness_ready).toBeLessThan(summary.readiness_required);
  });

  it("reports identity binding without treating legacy Track fields as canonical", () => {
    const legacyTrack = {
      ...previewWorkspace.tracks[0],
      role: "",
      locations: [],
      policy: {
        ...previewWorkspace.tracks[0].policy,
        role_family: "",
      },
    };
    const summary = commandCenterSummary(workspace({ tracks: [legacyTrack] }), now);

    expect(summary.readiness.find((item) => item.id === "tracks")).toMatchObject({
      state: "reported",
    });
    expect(summary.readiness.find((item) => item.id === "tracks")?.detail).toContain(
      "not proven here",
    );
  });

  it("orders blocking application decisions ahead of reviews and fresh matches", () => {
    const uncertain = {
      ...previewWorkspace.applications[0],
      id: "uncertain-application",
      state: "side_effect_unknown",
    } as const;
    const summary = commandCenterSummary(workspace({
      applications: [uncertain, ...previewWorkspace.applications],
      matches: previewWorkspace.matches.map((job) => ({
        ...job,
        posted_at_ms: now - 60_000,
      })),
    }), now);

    expect(summary.next_actions.slice(0, 3).map((action) => action.id)).toEqual([
      "interventions",
      "reconcile",
      "reviews",
    ]);
    expect(summary.outcomes.prepared).toBe(1);
    expect(summary.outcomes.in_flight).toBe(2);
    expect(summary.outcomes.needs_reconciliation).toBe(1);
  });

  it("excludes paused-Track and passed jobs while preserving latest restore semantics", () => {
    const baseJob = previewWorkspace.matches[0];
    const activeTrack = previewWorkspace.tracks[0];
    const pausedTrack = {
      ...previewWorkspace.tracks[1],
      id: "track-paused",
      active: false,
    };
    const freshJob = (id: string, trackId: string, matchScore: number) => ({
      ...baseJob,
      id,
      track_id: trackId,
      match_score: matchScore,
      availability_status: "active" as const,
      posted_at_ms: now - 60_000,
    });
    const baseEvent = previewWorkspace.candidate_events[0];
    const feedback = (id: string, jobId: string, action: string, createdAtMs: number) => ({
      ...baseEvent,
      id,
      event_type: "match_feedback" as const,
      job_id: jobId,
      application_id: undefined,
      action,
      created_at_ms: createdAtMs,
      updated_at_ms: createdAtMs,
    });
    const hardIneligible = freshJob("hard-ineligible", activeTrack.id, 99);
    const summary = commandCenterSummary(workspace({
      tracks: [activeTrack, pausedTrack],
      matches: [
        hardIneligible,
        freshJob("paused-track", pausedTrack.id, 98),
        freshJob("passed", activeTrack.id, 97),
        freshJob("restored", activeTrack.id, 96),
        freshJob("eligible", activeTrack.id, 95),
      ].map((job) => job.id === hardIneligible.id
        ? { ...job, eligibility: { ...job.eligibility!, can_prepare: false } }
        : job),
      candidate_events: [
        feedback("pass-current", "passed", "pass", now - 1_000),
        feedback("pass-old", "restored", "pass", now - 2_000),
        feedback("restore-current", "restored", "restore", now - 1_000),
      ],
    }), now);

    expect(summary.today_matches.map((job) => job.id)).toEqual([
      "hard-ineligible",
      "restored",
      "eligible",
    ]);
  });

  it("separates healthy, waiting, and attention source states", () => {
    const healthy = {
      ...previewWorkspace.discovery_sources[0],
      id: "source-healthy",
      status: "active" as const,
      health: "healthy" as const,
      last_success_at_ms: now - 60_000,
    };
    const degraded = {
      ...previewWorkspace.discovery_sources[1],
      id: "source-degraded",
      status: "active" as const,
      health: "degraded" as const,
    };
    const waiting = {
      ...previewWorkspace.discovery_sources[3],
      id: "source-waiting",
      status: "active" as const,
      health: "waiting" as const,
      last_success_at_ms: null,
    };
    const pausedWaiting = {
      ...previewWorkspace.discovery_sources[0],
      id: "source-paused-waiting",
      status: "paused" as const,
      health: "waiting" as const,
    };
    const summary = commandCenterSummary(workspace({
      discovery_sources: [healthy, degraded, waiting, pausedWaiting],
    }), now);

    expect(summary.source_health.healthy).toBe(1);
    expect(summary.source_health.attention).toBe(2);
    expect(summary.source_health.waiting).toBe(1);
    expect(
      summary.source_health.healthy
      + summary.source_health.attention
      + summary.source_health.waiting,
    ).toBe(summary.source_health.sources.length);
  });

  it("does not count never-synced or stale stored-healthy sources as healthy", () => {
    const sources = [
      {
        ...previewWorkspace.discovery_sources[0],
        id: "never-synced",
        health: "healthy" as const,
        last_success_at_ms: null,
      },
      {
        ...previewWorkspace.discovery_sources[0],
        id: "stale",
        health: "healthy" as const,
        last_success_at_ms: now - 13 * 60 * 60_000,
      },
    ];
    const summary = commandCenterSummary(workspace({ discovery_sources: sources }), now);

    expect(summary.source_health).toMatchObject({ healthy: 0, attention: 1, waiting: 1 });
    expect(summary.readiness.find((item) => item.id === "sources")?.state).toBe("needs_action");
  });

  it("counts one offer per application instead of duplicate outcome events", () => {
    const baseOutcome = previewWorkspace.candidate_events[0];
    const summary = commandCenterSummary(workspace({
      candidate_events: [
        { ...baseOutcome, id: "offer-1", action: "offer" },
        { ...baseOutcome, id: "offer-2", action: "offer" },
      ],
    }), now);

    expect(summary.outcomes.offers).toBe(1);
  });

  it("preserves both calendar providers and orders a connected one first", () => {
    const summary = commandCenterSummary(workspace({
      integrations: [
        {
          id: "google-calendar",
          provider: "google_calendar",
          status: "disconnected",
          account_label: "",
          capabilities: ["interview_calendar"],
          updated_at_ms: now,
        },
        {
          id: "outlook-calendar",
          provider: "outlook_calendar",
          status: "connected",
          account_label: "candidate@example.test",
          capabilities: ["interview_calendar"],
          updated_at_ms: now,
        },
      ],
    }), now);

    expect(summary.inbox.calendars.map((calendar) => [calendar.provider, calendar.status]))
      .toEqual([
        ["outlook_calendar", "connected"],
        ["google_calendar", "disconnected"],
      ]);
  });
});

describe("career command center formatting", () => {
  it("preserves router query state without adding the public basename", () => {
    expect(commandCenterHref("/applications", "?preview=1")).toBe(
      "/applications?preview=1",
    );
  });

  it("formats bounded sync recency without exposing raw timestamps", () => {
    expect(commandCenterRecency(null, now)).toBe("No successful sync yet");
    expect(commandCenterRecency(undefined, now)).toBe("No successful sync yet");
    expect(commandCenterRecency(Number.NaN, now)).toBe("No successful sync yet");
    expect(commandCenterRecency(now - 30_000, now)).toBe("just now");
    expect(commandCenterRecency(now - 12 * 60_000, now)).toBe("12m ago");
    expect(commandCenterRecency(now - 3 * 60 * 60_000, now)).toBe("3h ago");
    expect(commandCenterRecency(now - 2 * 24 * 60 * 60_000, now)).toBe("2d ago");
  });
});

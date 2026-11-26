import { describe, expect, it } from "vitest";
import type { CandidateEvent } from "../types";
import {
  applicationIssues,
  latestApplicationOutcome,
  passedMatchJobIds,
} from "./candidate-events";

function event(overrides: Partial<CandidateEvent>): CandidateEvent {
  return {
    id: "event-1",
    event_type: "match_feedback",
    job_id: "job-1",
    action: "pass",
    reasons: [],
    note: "",
    status: "recorded",
    created_at_ms: 1,
    updated_at_ms: 1,
    ...overrides,
  };
}

describe("candidate event projections", () => {
  it("uses the latest append-only match feedback as the current preference", () => {
    const events = [
      event({ id: "pass", action: "pass", created_at_ms: 10 }),
      event({ id: "restore", action: "restore", created_at_ms: 20 }),
    ];

    expect(passedMatchJobIds(events).has("job-1")).toBe(false);
  });

  it("projects the latest user-confirmed outcome without changing application state", () => {
    const events = [
      event({ id: "interview", event_type: "application_outcome", application_id: "app-1", action: "interview", created_at_ms: 20 }),
      event({ id: "offer", event_type: "application_outcome", application_id: "app-1", action: "offer", created_at_ms: 30 }),
    ];

    expect(latestApplicationOutcome(events, "app-1")?.action).toBe("offer");
  });

  it("indexes feedback with the same latest-event and timestamp tie rules", () => {
    const events = [
      event({ id: "a", job_id: "tie", action: "pass", created_at_ms: 20 }),
      event({ id: "z", job_id: "tie", action: "restore", created_at_ms: 20 }),
      event({ id: "older", job_id: "other", action: "restore", created_at_ms: 10 }),
      event({ id: "newer", job_id: "other", action: "pass", created_at_ms: 20 }),
      event({ job_id: "other", event_type: "application_issue", action: "restore", created_at_ms: 30 }),
      event({ job_id: undefined }),
    ];
    for (const rows of [events, [...events].reverse()]) {
      expect([...passedMatchJobIds(rows)]).toEqual(["other"]);
    }
  });

  it("retains feedback for a job outside the currently loaded posting page", () => {
    const rows = [
      event({ job_id: "loaded", action: "restore" }),
      event({ job_id: "not-on-this-page", action: "pass" }),
    ];
    expect([...passedMatchJobIds(rows)]).toEqual(["not-on-this-page"]);
  });

  it("returns only issues for the selected application in newest-first order", () => {
    const events = [
      event({ id: "old", event_type: "application_issue", application_id: "app-1", action: "site_problem", created_at_ms: 10 }),
      event({ id: "other", event_type: "application_issue", application_id: "app-2", action: "billing", created_at_ms: 30 }),
      event({ id: "new", event_type: "application_issue", application_id: "app-1", action: "wrong_information", created_at_ms: 20 }),
    ];

    expect(applicationIssues(events, "app-1").map((item) => item.id)).toEqual(["new", "old"]);
  });
});

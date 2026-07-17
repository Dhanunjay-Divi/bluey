import { describe, expect, it } from "vitest";
import type { CandidateEvent } from "../types";
import {
  applicationIssues,
  isJobPassed,
  latestApplicationOutcome,
  latestMatchFeedback,
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

    expect(latestMatchFeedback(events, "job-1")?.id).toBe("restore");
    expect(isJobPassed(events, "job-1")).toBe(false);
  });

  it("projects the latest user-confirmed outcome without changing application state", () => {
    const events = [
      event({ id: "interview", event_type: "application_outcome", application_id: "app-1", action: "interview", created_at_ms: 20 }),
      event({ id: "offer", event_type: "application_outcome", application_id: "app-1", action: "offer", created_at_ms: 30 }),
    ];

    expect(latestApplicationOutcome(events, "app-1")?.action).toBe("offer");
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

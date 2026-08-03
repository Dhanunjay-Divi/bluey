import { describe, expect, it } from "vitest";
import type { JobApplication, JobEligibilityDecision, RunnerAvailability } from "../types";
import {
  answerInterventionActionLabel,
  applicationCountFor,
  applicationNeedsReview,
  hasAvailableRunner,
  runnerUnavailableReason,
} from "./ApplicationsView";

const eligibility = (
  capability: JobEligibilityDecision["capability"],
  canQueue = false,
): JobEligibilityDecision => ({
  capability,
  can_prepare: true,
  can_auto_submit: canQueue,
  can_queue_local: canQueue,
  can_queue_cloud: canQueue,
  hard_failures: [],
  review_reasons: [],
  passed_checks: [],
  evaluated_at_ms: 1,
});

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

describe("reviewed application runner availability", () => {
  it("keeps beta, handoff, and unknown application systems in review or handoff", () => {
    expect(runnerUnavailableReason(eligibility("beta_review"), runners(true))).toContain("still in beta");
    expect(runnerUnavailableReason(eligibility("handoff"), runners(true))).toContain("user-controlled handoff");
    expect(runnerUnavailableReason(eligibility("unknown_review"), runners(true))).toContain("not certified");
  });

  it("requires an actually distributed runner even for a certified application", () => {
    const certified = eligibility("certified", true);

    expect(hasAvailableRunner(certified, runners(false))).toBe(false);
    expect(runnerUnavailableReason(certified, runners(false))).toContain("invited beta");
    expect(hasAvailableRunner(certified, runners(true))).toBe(true);
  });

  it("surfaces a hard Career Track failure before runner messaging", () => {
    const blocked = eligibility("certified", true);
    blocked.hard_failures = [{ code: "location", message: "This job is outside your selected locations." }];

    expect(runnerUnavailableReason(blocked, runners(false))).toBe(
      "This job is outside your selected locations.",
    );
  });
});

describe("uncertain submission review", () => {
  const application = (state: JobApplication["state"]): JobApplication => ({
    id: `application-${state}`,
    job_id: "job-one",
    state,
    submission_mode: "review_first",
    match_score: 91,
    resume_version_id: "resume-one",
    cover_letter: "",
    answers: [],
    receipt: {},
    run_id: "run-one",
    created_at_ms: 1,
    updated_at_ms: 1,
  });

  it("keeps a side-effect-unknown application in Needs review", () => {
    const uncertain = application("side_effect_unknown");

    expect(applicationNeedsReview(uncertain)).toBe(true);
    expect(applicationCountFor("review", [uncertain, application("submitted")])).toBe(1);
  });
});

describe("answer intervention review", () => {
  it("labels the answer action as a save-for-review step", () => {
    expect(answerInterventionActionLabel(false)).toBe("Save answer for review");
    expect(answerInterventionActionLabel(true)).toBe("Saving...");
  });
});

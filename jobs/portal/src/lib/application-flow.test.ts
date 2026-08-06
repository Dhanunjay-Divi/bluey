import { describe, expect, it } from "vitest";
import type {
  AtsCertificationSummary,
  BrowserSession,
  Intervention,
  JobApplication,
  JobPosting,
  RunnerAvailability,
} from "../types";
import {
  applicationAfterInterventionResolution,
  browserSessionAfterInterventionResolution,
  effectiveSubmissionMode,
  interventionActionResumesApplication,
  interventionResolutionToast,
  isFinalSubmissionReview,
  runnerEligibleApplications,
} from "./application-flow";

const runners = (available: boolean): RunnerAvailability => ({
  local: {
    status: available ? "available" : "invited_beta",
    available,
    plan_included: true,
    distribution_enabled: available,
    reason: available ? "Available." : "Invited beta.",
    next_action: available ? "Run locally." : "Review first.",
  },
  cloud: {
    status: "upgrade_required",
    available: false,
    plan_included: false,
    distribution_enabled: false,
    reason: "Upgrade required.",
    next_action: "View plans.",
  },
  auto_submit_available: available,
  auto_submit_reason: available ? "Auto-submit is available." : "Auto-submit is still in invited beta.",
});

const application = (state: JobApplication["state"]): JobApplication => ({
  id: state,
  job_id: "job-1",
  state,
  submission_mode: "review_first",
  match_score: 90,
  answers: [],
  cover_letter: "",
  receipt: {},
  created_at_ms: 1,
  updated_at_ms: 1,
});

const browserSession: BrowserSession = {
  id: "browser-1",
  runner: "cloud",
  status: "paused",
  current_company: "Acme",
  current_step: "Waiting for an answer",
  application_id: "queued",
  created_at_ms: 1,
  updated_at_ms: 1,
};

const NOW_MS = Date.now();

const activeCertification = (): AtsCertificationSummary => ({
  provider_label: "Greenhouse",
  adapter_version: "2026.07.1-beta.1",
  certified_runner_kinds: ["local"],
  status: "active",
  last_verified_at_ms: NOW_MS - 60_000,
  expires_at_ms: NOW_MS + 60 * 60_000,
  reason: "Server verification is current for this job and runner.",
  next_action: "Review the application kit and choose Local Browser.",
  canary_available: true,
});

const job = (canAutoSubmit: boolean, summary: unknown = activeCertification()): JobPosting => ({
  id: "job-1",
  canonical_key: "job-1",
  source: "greenhouse",
  external_id: "1",
  company: "Acme",
  title: "Engineer",
  location: "Remote",
  workplace: "Remote",
  canonical_url: "https://boards.greenhouse.io/acme/jobs/1",
  description: "",
  compensation: "",
  track_id: "track-1",
  match_score: 90,
  matched_reasons: [],
  missing_requirements: [],
  availability_status: "active",
  status: "matched",
  created_at_ms: 1,
  updated_at_ms: 1,
  eligibility: {
    capability: canAutoSubmit ? "certified" : "beta_review",
    can_prepare: true,
    can_auto_submit: canAutoSubmit,
    can_queue_local: true,
    can_queue_cloud: true,
    hard_failures: [],
    review_reasons: [],
    passed_checks: [],
    evaluated_at_ms: NOW_MS,
    ats_certification: canAutoSubmit ? summary : undefined,
  },
});

describe("application workflow boundaries", () => {
  it("never exposes awaiting-review packets to a browser runner", () => {
    expect(runnerEligibleApplications([
      application("awaiting_review"),
      application("queued"),
      application("needs_input"),
    ]).map((item) => item.state)).toEqual(["queued"]);
  });

  it("downgrades Auto-submit unless both the job and a runner are authorized", () => {
    const missingSummary = job(true);
    delete missingSummary.eligibility?.ats_certification;
    const cloudOnly = {
      ...runners(true),
      local: { ...runners(true).local, available: false },
      cloud: { ...runners(true).cloud, available: true },
    };

    expect(effectiveSubmissionMode(job(false), "auto_submit", runners(true), true)).toBe("review_first");
    expect(effectiveSubmissionMode(job(true), "auto_submit", runners(false), true)).toBe("review_first");
    expect(effectiveSubmissionMode(job(true), "auto_submit", runners(true), false)).toBe("review_first");
    expect(effectiveSubmissionMode(job(true), "auto_submit", runners(true), true)).toBe("auto_submit");
    expect(effectiveSubmissionMode(missingSummary, "auto_submit", runners(true), true))
      .toBe("review_first");
    expect(effectiveSubmissionMode(job(true), "auto_submit", cloudOnly, true))
      .toBe("review_first");
  });

  it("returns answer-bearing applications to review without resuming the browser", () => {
    const returned = { ...application("queued"), answers: [{ question: "Why?", value: "Because." }] };

    expect(applicationAfterInterventionResolution(application("needs_input"), returned, "answer", 2))
      .toMatchObject({ state: "awaiting_review", answers: returned.answers });
    expect(applicationAfterInterventionResolution(application("needs_input"), undefined, "answer", 2))
      .toMatchObject({ state: "awaiting_review", updated_at_ms: 2 });
    expect(browserSessionAfterInterventionResolution(browserSession, "answer", 2)).toMatchObject({
      status: "paused",
      current_step: "Application kit changed; review required",
      updated_at_ms: 2,
    });
    expect(interventionActionResumesApplication("answer")).toBe(false);
    expect(interventionResolutionToast("answer")).toBe(
      "Answer saved. The updated application kit requires review.",
    );
  });

  it.each(["approve_email_otp", "approve_submission"])(
    "retains resume behavior for %s",
    (action) => {
      expect(interventionActionResumesApplication(action)).toBe(true);
      expect(applicationAfterInterventionResolution(application("needs_input"), undefined, action, 2))
        .toMatchObject({ state: "queued", updated_at_ms: 2 });
      expect(browserSessionAfterInterventionResolution(browserSession, action, 2)).toMatchObject({
        status: "queued",
        current_step: "Resuming application",
        updated_at_ms: 2,
      });
    },
  );

  it("recognizes only the structured, open final-review intervention", () => {
    const intervention = finalReviewIntervention();
    expect(isFinalSubmissionReview(intervention)).toBe(true);
    expect(isFinalSubmissionReview({ ...intervention, status: "resolved" })).toBe(false);
    expect(isFinalSubmissionReview({ ...intervention, kind: "captcha" })).toBe(false);
    expect(isFinalSubmissionReview({ ...intervention, resolution_kind: "email_otp_approval" })).toBe(false);
    expect(isFinalSubmissionReview({ ...intervention, choices: ["Continue"] })).toBe(false);
    expect(isFinalSubmissionReview({
      ...intervention,
      title: "Review this application",
      metadata: {
        receipt: {
          ...(intervention.metadata.receipt as object),
          intervention: {
            ...((intervention.metadata.receipt as { intervention: object }).intervention),
            title: "Review this application",
          },
        },
      },
    })).toBe(false);
    expect(isFinalSubmissionReview({
      ...intervention,
      metadata: { receipt: { ...(intervention.metadata.receipt as object), issues: [{ severity: "blocking" }] } },
    })).toBe(false);
  });
});

function finalReviewIntervention(): Intervention {
  const title = "Review the Greenhouse application";
  const detail = "Review every employer-facing field and document in the preserved form, then approve submission.";
  return {
    id: "intervention-final-review",
    application_id: "application-1",
    kind: "browser_takeover",
    status: "open",
    title,
    detail,
    choices: [],
    resolution_kind: "browser_takeover",
    resume_after_resolution: true,
    provider: "",
    provider_message_id: "",
    metadata: {
      receipt: {
        status: "needs_input",
        issues: [],
        intervention: {
          kind: "browser_takeover",
          title,
          detail,
          takeoverUrl: "https://jobs-browser.bluey.sh/sessions/browser-1",
          resolution: { kind: "browser_takeover", resumeAfter: true },
        },
      },
    },
    created_at_ms: 1,
  };
}

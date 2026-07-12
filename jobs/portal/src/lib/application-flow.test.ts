import { describe, expect, it } from "vitest";
import type { Intervention, JobApplication, JobPosting } from "../types";
import { effectiveSubmissionMode, isFinalSubmissionReview, runnerEligibleApplications } from "./application-flow";

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

const job = (canAutoSubmit: boolean): JobPosting => ({
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
    evaluated_at_ms: 1,
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

  it("downgrades Auto-submit to Review first unless the server authorizes it", () => {
    expect(effectiveSubmissionMode(job(false), "auto_submit")).toBe("review_first");
    expect(effectiveSubmissionMode(job(true), "auto_submit")).toBe("auto_submit");
  });

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

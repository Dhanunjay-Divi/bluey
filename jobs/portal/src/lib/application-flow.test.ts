import { describe, expect, it } from "vitest";
import type {
  AtsCertificationSummary,
  BrowserSession,
  Intervention,
  InterventionResolutionResult,
  JobApplication,
  JobPosting,
  RunnerAvailability,
  WorkflowCommandAdmission,
} from "../types";
import {
  applicationAfterInterventionResolution,
  browserSessionAfterInterventionResolution,
  effectiveSubmissionMode,
  interventionActionResumesApplication,
  interventionResolutionToast,
  cloudAutomationEligibleApplications,
  isCloudAutomationEligibleApplication,
  isFinalSubmissionReview,
  workspaceNeedsCloudAutomationRefresh,
} from "./application-flow";

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
    reason: available ? "Available." : "Limited public beta.",
    next_action: available ? "Queue in the cloud." : "Review first.",
  },
  auto_submit_available: available,
  auto_submit_reason: available ? "Auto-submit is available." : "Auto-submit is temporarily unavailable in this limited public beta.",
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

function resolution(status: string, command?: WorkflowCommandAdmission): InterventionResolutionResult {
  return { intervention: { ...finalReviewIntervention(), status }, application: null, workflow_command: command };
}

const resumeCommand: WorkflowCommandAdmission = {
  schema_version: 2,
  command_id: "command-1",
  request_id: "request-1",
  workflow_id: "workflow-1",
  operation: "resume",
  state: "pending",
  replayed: false,
};

const activeCertification = (): AtsCertificationSummary => ({
  provider_label: "Greenhouse",
  adapter_version: "2026.07.1-beta.1",
  certified_runner_kinds: ["cloud"],
  status: "active",
  last_verified_at_ms: NOW_MS - 60_000,
  expires_at_ms: NOW_MS + 60 * 60_000,
  reason: "Server verification is current for this job and runner.",
  next_action: "Review the application kit and choose cloud automation.",
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

function evaluatedAt(posting: JobPosting, evaluated_at_ms: number): JobPosting {
  if (!posting.eligibility) throw new Error("Expected the fixture to have eligibility.");
  return { ...posting, eligibility: { ...posting.eligibility, evaluated_at_ms } };
}

describe("application workflow boundaries", () => {
  it("queues only cloud-authorized applications with no active run", () => {
    const applications = [
      application("awaiting_review"),
      application("queued"),
      application("needs_input"),
    ];
    const workspace = automationWorkspace(applications);

    expect(cloudAutomationEligibleApplications(workspace).map((item) => item.state))
      .toEqual(["queued"]);
    expect(isCloudAutomationEligibleApplication(workspace, applications[1])).toBe(true);

    expect(cloudAutomationEligibleApplications({
      ...workspace,
      runner_availability: runners(false),
    })).toEqual([]);
    expect(cloudAutomationEligibleApplications({
      ...workspace,
      browser_sessions: [browserSession],
    })).toEqual([]);
    expect(cloudAutomationEligibleApplications({
      ...workspace,
      matches: [job(true, { ...activeCertification(), certified_runner_kinds: ["local"] })],
    })).toEqual([]);
  });

  it("uses the frozen receipt authority when the application job is beyond the ranked page", () => {
    const queued = {
      ...application("queued"),
      job_id: "application-job-051",
      receipt: { eligibility: job(true).eligibility },
    };
    const workspace = {
      ...automationWorkspace([queued]),
      matches: [],
      application_jobs: [{
        id: queued.job_id,
        company: "Historical company",
        title: "Historical role",
        location: "Remote",
        track_id: "track-1",
      }],
    };

    expect(cloudAutomationEligibleApplications(workspace)).toEqual([queued]);
  });

  it.each(["revoked", "suspended", "drifted", "expired", "review_only"] as const)(
    "vetoes a frozen queued certification when the equal-or-newer ranked job is %s",
    (status) => {
      const frozen = {
        ...application("queued"),
        receipt: { eligibility: job(true).eligibility },
      };
      const denied = job(true, {
        ...activeCertification(),
        status,
        certified_runner_kinds: [],
        canary_available: false,
        ...(status === "expired" ? { expires_at_ms: NOW_MS - 1 } : {}),
      });
      for (const evaluatedAtMs of [NOW_MS, NOW_MS + 1]) {
        expect(isCloudAutomationEligibleApplication({
          ...automationWorkspace([frozen]),
          matches: [evaluatedAt(denied, evaluatedAtMs)],
        }, frozen)).toBe(false);
      }
    },
  );

  it("vetoes a frozen queued certification when the current ranked job loses cloud certification", () => {
    const frozen = {
      ...application("queued"),
      receipt: { eligibility: job(true).eligibility },
    };
    expect(isCloudAutomationEligibleApplication({
      ...automationWorkspace([frozen]),
      matches: [job(true, { ...activeCertification(), certified_runner_kinds: ["local"] })],
    }, frozen)).toBe(false);
  });

  it("does not let a newer positive ranked certification elevate a frozen denial", () => {
    const frozen = {
      ...application("queued"),
      receipt: { eligibility: job(true, { ...activeCertification(), status: "revoked", canary_available: false }).eligibility },
    };
    expect(isCloudAutomationEligibleApplication({
      ...automationWorkspace([frozen]),
      matches: [evaluatedAt(job(true), NOW_MS + 1)],
    }, frozen)).toBe(false);
  });

  it("keeps frozen eligibility for older or out-of-page ranked projections", () => {
    const frozen = {
      ...application("queued"),
      receipt: { eligibility: job(true).eligibility },
    };
    const staleDenied = job(true, { ...activeCertification(), status: "revoked", canary_available: false });
    expect(isCloudAutomationEligibleApplication({
      ...automationWorkspace([frozen]),
      matches: [evaluatedAt(staleDenied, NOW_MS - 1)],
    }, frozen)).toBe(true);
    expect(isCloudAutomationEligibleApplication({
      ...automationWorkspace([frozen]),
      matches: [],
    }, frozen)).toBe(true);
  });

  it("does not veto a frozen active cloud certification for only a current aggregate queue denial", () => {
    const frozen = {
      ...application("queued"),
      receipt: { eligibility: job(true).eligibility },
    };
    const aggregateDenied = job(true);
    aggregateDenied.eligibility = { ...aggregateDenied.eligibility!, can_queue_cloud: false };
    expect(isCloudAutomationEligibleApplication({
      ...automationWorkspace([frozen]),
      matches: [aggregateDenied],
    }, frozen)).toBe(true);
  });

  it("downgrades Auto-submit unless both the job and a runner are authorized", () => {
    const missingSummary = job(true);
    delete missingSummary.eligibility?.ats_certification;
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

    expect(effectiveSubmissionMode(job(false), "auto_submit", runners(true), true)).toBe("review_first");
    expect(effectiveSubmissionMode(job(true), "auto_submit", runners(false), true)).toBe("review_first");
    expect(effectiveSubmissionMode(job(true), "auto_submit", runners(true), false)).toBe("review_first");
    expect(effectiveSubmissionMode(job(true), "auto_submit", runners(true), true)).toBe("auto_submit");
    expect(effectiveSubmissionMode(missingSummary, "auto_submit", runners(true), true))
      .toBe("review_first");
    expect(effectiveSubmissionMode(job(true), "auto_submit", localOnly, true))
      .toBe("review_first");
  });

  it("returns answer-bearing applications to review without resuming the browser", () => {
    const prior = application("needs_input");
    const returned: JobApplication = { ...prior, state: "awaiting_review",
      updated_at_ms: 2, answers: [{ question: "Why?", value: "Because." }] };

    expect(applicationAfterInterventionResolution(prior, returned, "answer", 2)).toBe(returned);
    expect(applicationAfterInterventionResolution(prior, undefined, "answer", 2))
      .toMatchObject({ state: "awaiting_review", updated_at_ms: 2 });
    expect(browserSessionAfterInterventionResolution(browserSession, "answer", 2)).toMatchObject({
      status: "paused",
      current_step: "Application kit changed; review required",
      updated_at_ms: 2,
    });
    expect(interventionActionResumesApplication("answer")).toBe(false);
    expect(interventionResolutionToast("answer", resolution("resolved"))).toBe(
      "Answer saved. The updated application kit requires review.",
    );
  });

  it.each(["approve_email_otp", "approve_submission"])(
    "does not invent application or browser state without authoritative rows for %s",
    (action) => {
      expect(interventionActionResumesApplication(action)).toBe(true);
      const states = ["awaiting_review", "queued", "running", "needs_input", "side_effect_unknown"] as const;
      for (const state of states) {
        const prior = application(state);
        expect(applicationAfterInterventionResolution(prior, null, action, 2)).toBe(prior);
        expect(applicationAfterInterventionResolution(prior, undefined, action, 2)).toBe(prior);
      }
      expect(browserSessionAfterInterventionResolution(browserSession, action, 2)).toBe(browserSession);
    },
  );

  it("uses only a matching application response no older than the visible state", () => {
    const prior = { ...application("running"), updated_at_ms: 8 };
    const confirmed = { ...prior, state: "submitted" as const, updated_at_ms: 9 };
    const action = "approve_submission";
    expect(applicationAfterInterventionResolution(prior, confirmed, action, 10)).toBe(confirmed);
    expect(applicationAfterInterventionResolution(prior, { ...confirmed, id: "another" }, action, 10)).toBe(prior);
    expect(applicationAfterInterventionResolution(prior, { ...confirmed, updated_at_ms: 7 }, action, 10)).toBe(prior);
  });

  it("does not show an approval or resume claim when the email code expires at the server", () => {
    expect(interventionResolutionToast("approve_email_otp", resolution("expired")))
      .toBe("The email code expired. This approval did not resume the run.");
    expect(interventionResolutionToast("approve_email_otp", resolution("open", resumeCommand)))
      .toBe("Approval was not confirmed. Check the run before trying again.");
  });

  it.each([
    ["pending", "Waiting for cloud automation to resume."],
    ["claimed", "Waiting for cloud automation to resume."],
    ["delivering", "Waiting for cloud automation to resume."],
    ["accepted", "Cloud automation accepted the resume request."],
    ["delivery_unknown", "Bluey is checking whether cloud automation resumed."],
    ["identity_conflict", "The resume request did not go through. Check the run."],
    ["rejected", "The resume request did not go through. Check the run."],
    ["cancelled", "The resume request did not go through. Check the run."],
  ] as const)("represents %s without claiming the application was submitted", (state, message) => {
    expect(interventionResolutionToast("approve_submission", resolution("approved", { ...resumeCommand, state })))
      .toBe(`Submission approval saved. ${message}`);
  });

  it("acknowledges approval without inventing a resume for missing or non-resume receipts", () => {
    for (const command of [undefined, { ...resumeCommand, operation: "start" as const }]) {
      expect(interventionResolutionToast("approve_submission", resolution("approved", command)))
        .toBe("Submission approval saved. Check the run for its latest status.");
    }
  });

  it("polls only while managed cloud work can still change without user input", () => {
    const session = { ...browserSession, runner: "cloud" as const };
    const approvedIntervention = {
      ...finalReviewIntervention(),
      application_id: session.application_id,
      status: "approved",
    };

    expect(workspaceNeedsCloudAutomationRefresh({
      browser_sessions: [{ ...session, status: "queued" }],
      interventions: [],
    })).toBe(true);
    expect(workspaceNeedsCloudAutomationRefresh({
      browser_sessions: [{ ...session, status: "running" }],
      interventions: [],
    })).toBe(true);
    expect(workspaceNeedsCloudAutomationRefresh({
      browser_sessions: [{ ...session, status: "needs_input" }],
      interventions: [approvedIntervention],
    })).toBe(true);

    expect(workspaceNeedsCloudAutomationRefresh({
      browser_sessions: [{ ...session, status: "needs_input" }],
      interventions: [{ ...approvedIntervention, status: "open" }],
    })).toBe(false);
    expect(workspaceNeedsCloudAutomationRefresh({
      browser_sessions: [{ ...session, runner: "local", status: "running" }],
      interventions: [],
    })).toBe(false);
    expect(workspaceNeedsCloudAutomationRefresh({
      browser_sessions: [{ ...session, status: "complete" }],
      interventions: [],
    })).toBe(false);
  });

  it("recognizes only the structured, open final-review intervention", () => {
    const intervention = finalReviewIntervention();
    const takeoverUrl = "https://jobs-browser.bluey.sh/sessions/browser-1";
    expect(isFinalSubmissionReview(intervention, takeoverUrl)).toBe(true);
    expect(isFinalSubmissionReview(intervention)).toBe(false);
    expect(isFinalSubmissionReview(intervention, `${takeoverUrl}-other`)).toBe(false);
    expect(isFinalSubmissionReview({ ...intervention, status: "resolved" }, takeoverUrl)).toBe(false);
    expect(isFinalSubmissionReview({ ...intervention, kind: "captcha" }, takeoverUrl)).toBe(false);
    expect(isFinalSubmissionReview(
      { ...intervention, resolution_kind: "email_otp_approval" },
      takeoverUrl,
    )).toBe(false);
    expect(isFinalSubmissionReview({ ...intervention, choices: ["Continue"] }, takeoverUrl))
      .toBe(false);
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
    }, takeoverUrl)).toBe(false);
    expect(isFinalSubmissionReview({
      ...intervention,
      metadata: { receipt: { ...(intervention.metadata.receipt as object), issues: [{ severity: "blocking" }] } },
    }, takeoverUrl)).toBe(false);
  });
});

function automationWorkspace(applications: JobApplication[]): Parameters<
  typeof cloudAutomationEligibleApplications
>[0] {
  return {
    applications,
    application_jobs: [],
    browser_sessions: [],
    matches: [job(true)],
    runner_availability: runners(true),
  };
}

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

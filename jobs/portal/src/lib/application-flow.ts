import type {
  BrowserSession,
  Intervention,
  JobApplication,
  JobPosting,
  RunnerAvailability,
  SubmissionMode,
} from "../types";
import { portalEligibilityDecision } from "./ats-certification";

export function runnerEligibleApplications(applications: JobApplication[]): JobApplication[] {
  return applications.filter((application) => application.state === "queued");
}

export function interventionActionResumesApplication(action: string): boolean {
  return action === "approve_email_otp" || action === "approve_submission";
}

export function applicationAfterInterventionResolution(
  application: JobApplication,
  returnedApplication: JobApplication | undefined,
  action: string,
  updatedAtMs: number,
): JobApplication {
  if (action === "answer") {
    return {
      ...(returnedApplication || application),
      state: "awaiting_review",
      updated_at_ms: returnedApplication?.updated_at_ms || updatedAtMs,
    };
  }
  if (returnedApplication) return returnedApplication;
  if (!interventionActionResumesApplication(action)) return application;
  return { ...application, state: "queued", updated_at_ms: updatedAtMs };
}

export function browserSessionAfterInterventionResolution(
  session: BrowserSession,
  action: string,
  updatedAtMs: number,
): BrowserSession {
  if (action === "answer") {
    return {
      ...session,
      status: "paused",
      current_step: "Application kit changed; review required",
      updated_at_ms: updatedAtMs,
    };
  }
  if (!interventionActionResumesApplication(action)) return session;
  return {
    ...session,
    status: "queued",
    current_step: "Resuming application",
    updated_at_ms: updatedAtMs,
  };
}

export function interventionResolutionToast(action: string): string {
  if (action === "approve_submission") {
    return "Submission approved. Bluey is completing the application.";
  }
  if (action === "approve_email_otp") return "Email code approved. Bluey is resuming.";
  if (action === "answer") return "Answer saved. The updated application kit requires review.";
  return "Intervention resolved.";
}

export function effectiveSubmissionMode(
  job: JobPosting,
  requested: SubmissionMode,
  runners: RunnerAvailability,
  trackAuthorized: boolean,
): SubmissionMode {
  const eligibility = portalEligibilityDecision(job.eligibility);
  const certifiedRunnerAvailable = (eligibility.can_queue_local && runners.local.available)
    || (eligibility.can_queue_cloud && runners.cloud.available);
  return requested === "auto_submit"
    && eligibility.can_auto_submit
    && certifiedRunnerAvailable
    && runners.auto_submit_available
    && trackAuthorized
    ? "auto_submit"
    : "review_first";
}

export function isFinalSubmissionReview(intervention: Intervention | undefined): boolean {
  if (!intervention
    || intervention.status !== "open"
    || intervention.kind !== "browser_takeover"
    || intervention.resolution_kind !== "browser_takeover"
    || intervention.resume_after_resolution !== true
    || intervention.choices.length !== 0) return false;

  const receipt = record(intervention.metadata.receipt);
  const source = record(receipt.intervention);
  const resolution = record(source.resolution);
  const knownProviderCheckpoint = (
    intervention.title === "Review the Greenhouse application"
      && intervention.detail === "Review every employer-facing field and document in the preserved form, then approve submission."
  ) || (
    intervention.title === "Review this Lever application"
      && intervention.detail === "Review every answer and attachment in the preserved browser. Bluey will not submit until you explicitly approve final review."
  );
  return receipt.status === "needs_input"
    && Array.isArray(receipt.issues)
    && receipt.issues.length === 0
    && source.kind === "browser_takeover"
    && typeof source.takeoverUrl === "string"
    && source.takeoverUrl.length > 0
    && source.title === intervention.title
    && source.detail === intervention.detail
    && resolution.kind === "browser_takeover"
    && resolution.resumeAfter === true
    && knownProviderCheckpoint;
}

function record(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : {};
}

import type {
  BrowserSession,
  Intervention,
  InterventionResolutionResult,
  JobApplication,
  JobPosting,
  JobsWorkspace,
  RunnerAvailability,
  SubmissionMode,
} from "../types";
import { portalEligibilityDecision, requiresCandidateSubmission } from "./ats-certification";
import { workspaceJobById } from "./workspace-jobs";

export function cloudAutomationEligibleApplications(workspace: Pick<
  JobsWorkspace,
  "application_jobs" | "applications" | "browser_sessions" | "matches" | "runner_availability"
>): JobApplication[] {
  return workspace.applications.filter(
    (application) => isCloudAutomationEligibleApplication(workspace, application),
  );
}

export function workspaceNeedsCloudAutomationRefresh(workspace: Pick<
  JobsWorkspace,
  "browser_sessions" | "interventions"
>): boolean {
  return workspace.browser_sessions.some((session) => {
    if (session.runner !== "cloud") return false;
    if (session.status === "queued" || session.status === "running") return true;
    if (session.status !== "needs_input" || !session.application_id) return false;
    return workspace.interventions.some(
      (intervention) => intervention.application_id === session.application_id
        && intervention.status === "approved",
    );
  });
}

export function isCloudAutomationEligibleApplication(
  workspace: Pick<
    JobsWorkspace,
    "application_jobs" | "browser_sessions" | "matches" | "runner_availability"
  >,
  application: JobApplication,
): boolean {
  if (application.state !== "queued" || !workspace.runner_availability.cloud.available) {
    return false;
  }
  if (workspace.browser_sessions.some(
    (session) => session.application_id === application.id
      && !["complete", "failed"].includes(session.status),
  )) {
    return false;
  }
  const job = workspaceJobById(workspace, application.job_id);
  const currentEligibility = job && "eligibility" in job ? job.eligibility : undefined;
  const storedEligibility = application.receipt.eligibility;
  const nowMs = Date.now();
  const eligibility = portalEligibilityDecision(
    storedEligibility && typeof storedEligibility === "object"
      ? storedEligibility
      : currentEligibility,
    false,
    nowMs,
  );
  if (!eligibility.can_queue_cloud) return false;
  const current = currentEligibility
    ? portalEligibilityDecision(currentEligibility, false, nowMs)
    : undefined;
  // A candidate-only restriction removes runner authority, even when the
  // current posting lacks a runner certificate or the frozen grant is legacy.
  if (requiresCandidateSubmission(current)) return false;
  if (eligibility.capability !== "certified" || !current) return true;

  // The receipt remains the frozen decision. A current full match can veto a
  // withdrawn ATS certificate, but cannot grant authority or replace pacing
  // and other application-specific checks with its aggregate queue decision.
  const certification = current.ats_certification;
  return !certification || current.evaluated_at_ms < eligibility.evaluated_at_ms
    || (current.capability === "certified" && certification.status === "active"
      && certification.certified_runner_kinds.includes("cloud"));
}

export function interventionActionResumesApplication(action: string): boolean {
  return action === "approve_email_otp" || action === "approve_submission";
}

export function applicationAfterInterventionResolution(
  application: JobApplication,
  returnedApplication: JobApplication | null | undefined,
  action: string,
  updatedAtMs: number,
): JobApplication {
  if (returnedApplication) {
    return returnedApplication.id === application.id
      && returnedApplication.updated_at_ms >= application.updated_at_ms
      ? returnedApplication : application;
  }
  if (action === "answer") {
    return { ...application, state: "awaiting_review", updated_at_ms: updatedAtMs };
  }
  // Approval can expire or be recorded without admitting a runner resume.
  // No returned application means there is no new authoritative state.
  return application;
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
  // A resume admission is not a browser-session update. The workspace
  // refresh will read the actual browser status and takeover link.
  return session;
}

export function interventionResolutionToast(
  action: string,
  result: Pick<InterventionResolutionResult, "intervention" | "workflow_command">,
): string {
  if (action === "answer") return "Answer saved. The updated application kit requires review.";
  if (!interventionActionResumesApplication(action)) return "Intervention resolved.";

  const status = result.intervention.status;
  if (status === "expired" && action === "approve_email_otp") {
    return "The email code expired. This approval did not resume the run.";
  }
  if (status !== "approved" && status !== "resolved") {
    return "Approval was not confirmed. Check the run before trying again.";
  }

  const approval = action === "approve_email_otp" ? "Email code approved." : "Submission approval saved.";
  const command = result.workflow_command;
  if (command?.schema_version !== 2 || command.operation !== "resume") {
    return `${approval} Check the run for its latest status.`;
  }
  switch (command.state) {
    case "pending":
    case "claimed":
    case "delivering":
      return `${approval} Waiting for cloud automation to resume.`;
    case "accepted":
      return `${approval} Cloud automation accepted the resume request.`;
    case "delivery_unknown":
      return `${approval} Bluey is checking whether cloud automation resumed.`;
    case "identity_conflict":
    case "rejected":
    case "cancelled":
      return `${approval} The resume request did not go through. Check the run.`;
    default:
      return `${approval} Check the run for its latest status.`;
  }
}

export function effectiveSubmissionMode(
  job: JobPosting,
  requested: SubmissionMode,
  runners: RunnerAvailability,
  trackAuthorized: boolean,
): SubmissionMode {
  const eligibility = portalEligibilityDecision(job.eligibility);
  const certifiedRunnerAvailable = eligibility.can_queue_cloud && runners.cloud.available;
  return requested === "auto_submit"
    && eligibility.can_auto_submit
    && certifiedRunnerAvailable
    && runners.auto_submit_available
    && trackAuthorized
    ? "auto_submit"
    : "review_first";
}

export function isFinalSubmissionReview(
  intervention: Intervention | undefined,
  activeTakeoverUrl?: string,
): boolean {
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
    && source.takeoverUrl === activeTakeoverUrl
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

import type { Intervention, JobApplication, JobPosting, SubmissionMode } from "../types";

export function runnerEligibleApplications(applications: JobApplication[]): JobApplication[] {
  return applications.filter((application) => application.state === "queued");
}

export function effectiveSubmissionMode(job: JobPosting, requested: SubmissionMode): SubmissionMode {
  return requested === "auto_submit" && job.eligibility?.can_auto_submit === true
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

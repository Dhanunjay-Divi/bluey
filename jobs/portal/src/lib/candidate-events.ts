import type { CandidateEvent } from "../types";

export const matchPassReasons = [
  ["role_mismatch", "Wrong role"],
  ["location", "Location"],
  ["compensation", "Compensation"],
  ["seniority", "Seniority"],
  ["company", "Company"],
  ["sponsorship", "Sponsorship"],
  ["already_applied", "Already applied"],
  ["not_interested", "Not interested"],
  ["other", "Something else"],
] as const;

export const applicationIssueReasons = [
  ["site_problem", "Job site problem"],
  ["wrong_information", "Wrong information"],
  ["duplicate_application", "Possible duplicate"],
  ["submission_status", "Submission status"],
  ["billing", "Billing or allowance"],
  ["other", "Something else"],
] as const;

export const applicationOutcomes = [
  ["interview", "Interview"],
  ["offer", "Offer"],
  ["rejected", "Not selected"],
  ["withdrawn", "Withdrawn"],
] as const;

export function latestMatchFeedback(events: CandidateEvent[], jobId: string): CandidateEvent | undefined {
  return latestEvent(events, (event) => event.event_type === "match_feedback" && event.job_id === jobId);
}

export function isJobPassed(events: CandidateEvent[], jobId: string): boolean {
  return latestMatchFeedback(events, jobId)?.action === "pass";
}

export function latestApplicationOutcome(events: CandidateEvent[], applicationId: string): CandidateEvent | undefined {
  return latestEvent(
    events,
    (event) => event.event_type === "application_outcome" && event.application_id === applicationId,
  );
}

export function applicationIssues(events: CandidateEvent[], applicationId: string): CandidateEvent[] {
  return events
    .filter((event) => event.event_type === "application_issue" && event.application_id === applicationId)
    .sort(compareNewestFirst);
}

export function eventActionLabel(action: string): string {
  const all = [...matchPassReasons, ...applicationIssueReasons, ...applicationOutcomes] as ReadonlyArray<readonly [string, string]>;
  return all.find(([value]) => value === action)?.[1] || action.replaceAll("_", " ");
}

function latestEvent(events: CandidateEvent[], predicate: (event: CandidateEvent) => boolean): CandidateEvent | undefined {
  return events.filter(predicate).sort(compareNewestFirst)[0];
}

function compareNewestFirst(left: CandidateEvent, right: CandidateEvent): number {
  if (right.created_at_ms !== left.created_at_ms) return right.created_at_ms - left.created_at_ms;
  return right.id.localeCompare(left.id);
}

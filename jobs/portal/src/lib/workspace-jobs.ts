import type { ApplicationJobContext, JobPosting, JobsWorkspace } from "../types";

type WorkspaceJobContext = Pick<JobsWorkspace, "application_jobs" | "matches">;
export type WorkspaceJob = ApplicationJobContext | JobPosting;

export function workspaceJobLookup(
  workspace: WorkspaceJobContext,
): ReadonlyMap<string, WorkspaceJob> {
  const jobs = new Map<string, WorkspaceJob>();
  for (const job of workspace.application_jobs) jobs.set(job.id, job);
  for (const job of workspace.matches) jobs.set(job.id, job);
  return jobs;
}

export function workspaceJobById(
  workspace: WorkspaceJobContext,
  jobId: string,
): WorkspaceJob | undefined {
  return workspace.matches.find((job) => job.id === jobId)
    ?? workspace.application_jobs.find((job) => job.id === jobId);
}

import { describe, expect, it } from "vitest";
import { previewWorkspace } from "../data/preview";
import type { ApplicationJobContext, JobPosting } from "../types";
import { workspaceJobById, workspaceJobLookup } from "./workspace-jobs";

function applicationContext(index: number): ApplicationJobContext {
  return {
    id: `application-job-${index.toString().padStart(3, "0")}`,
    company: `Company ${index}`,
    title: `Historical role ${index}`,
    location: "Remote",
    track_id: "track-engineering",
  };
}

describe("workspace application job context", () => {
  it("keeps all 501 historical application labels reachable outside the ranked page", () => {
    const applicationJobs = Array.from({ length: 501 }, (_, index) => applicationContext(index));
    const workspace = {
      matches: previewWorkspace.matches.slice(0, 1),
      application_jobs: applicationJobs,
    };

    expect(workspaceJobLookup(workspace).size).toBe(502);
    expect(workspaceJobById(workspace, "application-job-500")).toEqual(applicationJobs[500]);
  });

  it("prefers the current ranked representation after an application job enters a later page", () => {
    const frozen = applicationContext(51);
    const current = {
      ...previewWorkspace.matches[0],
      id: frozen.id,
      company: "Current company",
      title: "Current role",
    } satisfies JobPosting;

    expect(workspaceJobById({ matches: [current], application_jobs: [frozen] }, frozen.id))
      .toBe(current);
  });
});

import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { previewWorkspace } from "../data/preview";
import { ResumeView } from "./ResumeView";

describe("resume application history", () => {
  it("keeps exact labels for an application job beyond the ranked page", () => {
    const application = {
      ...previewWorkspace.applications[0],
      id: "application-history-51",
      job_id: "application-job-051",
      resume_version_id: "resume-history-51",
    };
    const workspace = {
      ...previewWorkspace,
      matches: [],
      applications: [application],
      application_jobs: [{
        id: application.job_id,
        company: "Historical company",
        title: "Historical role",
        location: "Remote",
        track_id: "track-engineering",
      }],
    };
    const html = renderToStaticMarkup(
      <ResumeView
        workspace={workspace}
        resumeVersions={{}}
        onSave={vi.fn(async () => undefined)}
        onImportResume={vi.fn(async (_file, profile) => profile)}
        onCommit={vi.fn(async () => undefined)}
        onLoadResume={vi.fn(async () => undefined)}
      />,
    );

    expect(html).toContain("Historical role");
    expect(html).toContain("Historical company");
  });
});

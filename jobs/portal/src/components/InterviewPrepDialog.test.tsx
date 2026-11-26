import type { ReactNode } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { previewWorkspace } from "../data/preview";
import type { JobApplication, ResumeVersion } from "../types";
import { InterviewPrepDialog } from "./InterviewPrepDialog";

vi.mock("./Dialog", () => ({
  Dialog: ({ title, description, children }: { title: string; description?: string; children: ReactNode }) => (
    <div><h2>{title}</h2><p>{description}</p>{children}</div>
  ),
}));

const application = previewWorkspace.applications.find((item) => item.state === "submitted")!;
const job = previewWorkspace.matches.find((item) => item.id === application.job_id)!;
const resume: ResumeVersion = {
  id: "resume-3",
  job_id: job.id,
  version_no: 3,
  mode: "factual",
  content: {},
  diff: {},
  claim_ids: ["fact-1", "fact-2"],
  checksum: "preview-checksum",
  created_at_ms: 0,
};

function renderReceipt(
  schemaVersion: number, objectSchemaVersion: number, overrides: Record<string, unknown> = {},
  workspace = previewWorkspace,
  selectedResume = resume,
): string {
  const receipt = {
    ...application.receipt,
    schemaVersion,
    receiptObject: { schemaVersion: objectSchemaVersion },
    ...overrides,
  };
  const submitted = { ...application, receipt };
  return renderToStaticMarkup(
    <InterviewPrepDialog
      target={{ application: submitted, resume: selectedResume }}
      workspace={{ ...workspace, applications: [submitted] }}
      currentResume={selectedResume}
      onClose={() => {}}
    />,
  );
}

function renderRefreshedWorkspace(applications: JobApplication[], currentResume: ResumeVersion | undefined) {
  return renderToStaticMarkup(
    <InterviewPrepDialog
      target={{ application, resume }}
      workspace={{ ...previewWorkspace, applications }}
      currentResume={currentResume}
      onClose={() => {}}
    />,
  );
}

describe("interview prep receipt snapshots", () => {
  it.each([1, 2])("shows frozen answers and requires opt-in for schema %s", (schema) => {
    const markup = renderReceipt(schema, schema);
    expect(markup).toContain("1 final answer");
    expect(markup).toContain("Prepare for this interview");
    expect(markup).toMatch(/<button[^>]*disabled=""[^>]*>.*Generate with Bluey/);
    expect(markup).not.toContain("Prep is not ready");
  });

  it.each([[2, 1], [3, 3]])("cannot generate with receipt schema %s and object schema %s", (schema, objectSchema) => {
    const markup = renderReceipt(schema, objectSchema);
    expect(markup).toContain("The final submission receipt is incomplete.");
    expect(markup).not.toContain("Generate with Bluey");
  });

  it("does not substitute a resume whose document cannot be bound to the receipt", () => {
    const markup = renderReceipt(1, 1, { documents: [] });
    expect(markup).toContain("does not contain a matching submitted resume document");
    expect(markup).not.toContain("Generate with Bluey");
  });

  it("reports no frozen answers for a legacy packet without answers", () => {
    const markup = renderReceipt(1, 1, { packet: { jobId: job.id, resumeVersionId: resume.id } });
    expect(markup).toContain("0 final answers");
  });

  it("opens with the submitted company and role even when the posting is no longer in the workspace", () => {
    const markup = renderReceipt(2, 2, { job: { company: "Frozen employer", title: "Frozen role" } },
      { ...previewWorkspace, matches: [] });
    expect(markup).toContain("Interview prep: Frozen employer");
    expect(markup).toContain("Frozen role · Submitted resume v3");
    expect(markup).toContain("Generate with Bluey");
  });

  it("does not show a different live title or company after the posting changes", () => {
    const markup = renderReceipt(1, 1, { job: { company: "Frozen employer", title: "Frozen role" } },
      { ...previewWorkspace, matches: [{ ...job, company: "Changed employer", title: "Changed role" }] });
    expect(markup).toContain("Frozen employer");
    expect(markup).not.toContain("Changed employer");
    expect(markup).not.toContain("Changed role");
  });

  it.each([undefined, { company: "  ", title: "Role" }, { company: "Employer", title: 123 }])(
    "does not offer generation for invalid job evidence %j", (submittedJob) => {
      const markup = renderReceipt(1, 1, { job: submittedJob });
      expect(markup).toContain("The final submission receipt is missing the submitted job snapshot.");
      expect(markup).not.toContain("Generate with Bluey");
    },
  );

  it("continues to refuse a resume from another job", () => {
    const markup = renderReceipt(1, 1, {}, previewWorkspace, { ...resume, job_id: "unrelated-job" });
    expect(markup).toContain("The submitted job and resume do not match this application.");
    expect(markup).not.toContain("Generate with Bluey");
  });

  it("does not generate without the server-issued receipt fingerprint", () => {
    const markup = renderReceipt(1, 1, {}, { ...previewWorkspace, interview_prep_receipts: {} });
    expect(markup).toContain("Refresh Bluey Jobs to confirm the submitted receipt");
    expect(markup).not.toContain("Generate with Bluey");
  });

  it.each([
    { change: "removed application", applications: [] },
    { change: "changed status", applications: [{ ...application, state: "cancelled" }] },
    { change: "changed job", applications: [{ ...application, job_id: "another-job" }] },
    { change: "changed résumé ID", applications: [{ ...application, resume_version_id: "resume-new" }] },
    { change: "changed receipt with same IDs", applications: [{ ...application, receipt: {
      ...application.receipt, job: { company: "New company", title: "Another role" },
    } }] },
  ] as { change: string; applications: JobApplication[] }[])("hides the old scope for $change", ({ applications }) => {
    const markup = renderRefreshedWorkspace(applications, resume);
    expect(markup).toContain("Interview prep changed");
    expect(markup).not.toContain("Generate with Bluey");
    expect(markup).not.toContain("receipt-app-3");
  });

  it("hides the old scope for a changed loaded checksum with the same resume ID", () => {
    const markup = renderRefreshedWorkspace([application], { ...resume, checksum: "new-checksum" });
    expect(markup).toContain("Interview prep changed");
    expect(markup).not.toContain("Generate with Bluey");
  });

  it("hides the old scope when the loaded résumé cache entry disappears", () => {
    const markup = renderRefreshedWorkspace([application], undefined);
    expect(markup).toContain("Interview prep changed");
    expect(markup).not.toContain("Generate with Bluey");
    expect(markup).not.toContain("receipt-app-3");
  });

  it("keeps the same scope when an otherwise unchanged receipt is loaded again", () => {
    const markup = renderRefreshedWorkspace([{ ...application, updated_at_ms: application.updated_at_ms + 1,
      receipt: structuredClone(application.receipt) }], { ...resume });
    expect(markup).toContain("Generate with Bluey");
    expect(markup).not.toContain("Interview prep changed");
  });
});

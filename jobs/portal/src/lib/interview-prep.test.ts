import { describe, expect, it } from "vitest";
import { createApplicationReceipt } from "@bluey/jobs-automation";
import type { ApplicationEvidence, CareerFact, JobApplication, JobPosting, ResumeVersion } from "../types";
import { buildPortalInterviewPrep } from "./interview-prep";

const job: JobPosting = {
  id: "job-1",
  canonical_key: "acme-engineer",
  source: "greenhouse",
  external_id: "external-1",
  company: "Acme",
  title: "Product Engineer",
  location: "New York, NY",
  workplace: "Hybrid",
  canonical_url: "https://boards.greenhouse.io/acme/jobs/1",
  description: "You will build reliable TypeScript product workflows with design partners.",
  compensation: "$150k-$180k",
  track_id: "track-1",
  match_score: 92,
  matched_reasons: ["TypeScript"],
  missing_requirements: [],
  availability_status: "active",
  status: "matched",
  created_at_ms: 1,
  updated_at_ms: 1,
};

const resume: ResumeVersion = {
  id: "resume-1",
  job_id: "job-1",
  version_no: 1,
  mode: "factual",
  content: {
    contact: { email: "candidate@example.com" },
    summary: "Product engineer",
    skills: ["TypeScript"],
  },
  diff: {},
  claim_ids: ["fact-1", "fact-contact"],
  checksum: "a".repeat(64),
  created_at_ms: 1,
};

const facts: CareerFact[] = [
  {
    id: "fact-1",
    category: "experience",
    label: "TypeScript delivery",
    value: "Built reliable TypeScript workflows with product and design partners.",
    source: "resume_import",
    verification_status: "confirmed",
    schema_version: 1,
    created_at_ms: 1,
    updated_at_ms: 1,
  },
  {
    id: "fact-contact",
    category: "contact",
    label: "Email address",
    value: "candidate@example.com",
    source: "user_entry",
    verification_status: "confirmed",
    schema_version: 1,
    created_at_ms: 1,
    updated_at_ms: 1,
  },
];

function application(): JobApplication {
  const receipt = createApplicationReceipt({
    receiptId: "receipt-1",
    accountId: "account-1",
    runId: "run-1",
    runner: "cloud",
    applicationIdentityId: "identity-1",
    browserProfileId: "profile-1",
    adapter: "greenhouse",
    adapterVersion: "1.0.0",
    generatedAt: "2026-07-10T12:00:00.000Z",
    job: {
      externalId: job.external_id,
      canonicalUrl: job.canonical_url,
      company: job.company,
      title: job.title,
      location: job.location,
      workplace: "hybrid",
      description: job.description,
      compensation: job.compensation,
      source: "greenhouse",
    },
    packet: {
      applicationId: "application-1",
      jobId: job.id,
      resumeVersionId: resume.id,
      answers: { motivation: "I like dependable product systems.", email: "candidate@example.com" },
      verifiedClaimIds: resume.claim_ids,
      applicationIdentityId: "identity-1",
      applicationEmail: "candidate@example.com",
      browserProfileId: "profile-1",
    },
    documents: [{ kind: "resume", versionId: resume.id, storageKey: "jobs/resume-1.pdf", sha256: resume.checksum }],
    events: [{ id: "event-1", occurredAt: "2026-07-10T11:59:00.000Z", type: "submitted" }],
    result: { status: "submitted", confirmationText: "Application received", submittedAt: "2026-07-10T11:59:00.000Z", issues: [] },
    screenshotKeys: ["jobs/receipt-1/confirmation.png"],
  });
  return {
    id: "application-1",
    job_id: job.id,
    resume_version_id: resume.id,
    state: "submitted",
    submission_mode: "review_first",
    match_score: 92,
    answers: [],
    cover_letter: "",
    receipt: receipt as unknown as Record<string, unknown>,
    created_at_ms: 1,
    updated_at_ms: 2,
    submitted_at_ms: Date.parse("2026-07-10T11:59:00.000Z"),
  };
}

const evidence: ApplicationEvidence[] = [{
  id: "interview-1",
  application_id: "application-1",
  kind: "interview_event",
  label: "Hiring manager interview",
  provider: "google_calendar",
  file_name: "",
  media_type: "",
  storage_key: "",
  sha256: "",
  occurred_at_ms: Date.parse("2026-07-15T15:00:00.000Z"),
  metadata: { attendee_email: "manager@acme.example" },
  created_at_ms: 3,
}];

describe("portal interview preparation", () => {
  it("creates a Bluey router request from the exact submitted application", () => {
    const launch = buildPortalInterviewPrep({
      application: application(),
      job,
      resume,
      facts,
      evidence,
      generatedAt: "2026-07-11T16:00:00.000Z",
    });

    expect(launch.packet).toMatchObject({
      applicationId: "application-1",
      receiptId: "receipt-1",
      resumeVersionId: "resume-1",
      interviewLabel: "Hiring manager interview",
    });
    expect(JSON.stringify(launch.packet)).toContain("fact-1");
    expect(JSON.stringify(launch.packet)).not.toContain("candidate@example.com");
    expect(JSON.stringify(launch.packet)).not.toContain("manager@acme.example");
  });

  it("does not manufacture a prep packet from a legacy partial receipt", () => {
    const value = application();
    value.receipt = { confirmation: "Application received" };
    expect(() => buildPortalInterviewPrep({ application: value, job, resume, facts, evidence }))
      .toThrow("not evidence-complete enough");
  });

  it("rejects cross-job and cross-resume launches", () => {
    expect(() => buildPortalInterviewPrep({
      application: { ...application(), job_id: "job-2" },
      job,
      resume,
      facts,
      evidence,
    })).toThrow("job does not match");
    expect(() => buildPortalInterviewPrep({
      application: application(),
      job,
      resume: { ...resume, id: "resume-2" },
      facts,
      evidence,
    })).toThrow("resume does not match");
  });
});

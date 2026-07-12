import {
  buildInterviewPrepPacket,
  type InterviewPrepPacket,
  type VerifiedCareerClaim,
} from "@bluey/jobs-automation/interview-prep";
import type { ApplicationEvidenceRecord, ApplicationReceiptBundle } from "@bluey/jobs-automation";
import type {
  ApplicationEvidence,
  CareerFact,
  JobApplication,
  JobPosting,
  ResumeVersion,
} from "../types";

export interface PortalInterviewPrepInput {
  application: JobApplication;
  job: JobPosting;
  resume: ResumeVersion;
  facts: CareerFact[];
  evidence: ApplicationEvidence[];
  generatedAt?: string;
}

export interface PortalInterviewPrepLaunch {
  packet: InterviewPrepPacket;
}

export function buildPortalInterviewPrep(input: PortalInterviewPrepInput): PortalInterviewPrepLaunch {
  if (input.application.state !== "submitted") {
    throw new Error("Interview preparation starts after a confirmed submission");
  }
  if (input.application.job_id !== input.job.id) {
    throw new Error("Interview preparation job does not match this application");
  }
  if (input.application.resume_version_id !== input.resume.id) {
    throw new Error("Interview preparation resume does not match this application");
  }

  const receipt = receiptBundle(input.application.receipt);
  if (receipt.applicationId !== input.application.id || receipt.packet.jobId !== input.job.id) {
    throw new Error("Submission receipt does not match this application and job");
  }
  const facts = new Map(input.facts.map((fact) => [fact.id, fact]));
  const claims = input.resume.claim_ids.map((id): VerifiedCareerClaim => {
    const fact = facts.get(id);
    if (!fact) throw new Error(`Submitted resume fact ${id} is unavailable`);
    return {
      id,
      category: fact.category,
      label: fact.label,
      statement: factStatement(fact),
      status: fact.verification_status === "confirmed"
        ? "confirmed"
        : fact.verification_status === "rejected"
          ? "rejected"
          : "unverified",
      source: fact.source,
    };
  });
  const packet = buildInterviewPrepPacket({
    receipt,
    resume: {
      versionId: input.resume.id,
      content: input.resume.content as unknown as Record<string, unknown>,
      claims,
    },
    evidence: prepEvidence(input.evidence),
    generatedAt: input.generatedAt,
  });
  return { packet };
}

function receiptBundle(value: Record<string, unknown>): ApplicationReceiptBundle {
  const packet = objectValue(value.packet);
  const result = objectValue(value.result);
  const job = objectValue(value.job);
  if (value.schemaVersion !== 1
    || typeof value.receiptId !== "string"
    || typeof value.applicationId !== "string"
    || typeof value.generatedAt !== "string"
    || typeof packet?.jobId !== "string"
    || typeof packet?.resumeVersionId !== "string"
    || typeof result?.status !== "string"
    || typeof job?.company !== "string"
    || !Array.isArray(value.documents)
    || !Array.isArray(value.events)
    || !Array.isArray(value.screenshotKeys)) {
    throw new Error("Submission receipt is not evidence-complete enough for grounded interview preparation");
  }
  return structuredClone(value) as unknown as ApplicationReceiptBundle;
}

function prepEvidence(values: ApplicationEvidence[]): ApplicationEvidenceRecord[] {
  return values
    .filter((value) => ["resume", "cover_letter", "attachment", "submission_confirmation", "status_email", "interview_event"].includes(value.kind))
    .map((value) => ({
      ...value,
      kind: value.kind as ApplicationEvidenceRecord["kind"],
    }));
}

function factStatement(fact: CareerFact): string {
  if (typeof fact.value === "string") return fact.value.trim();
  if (typeof fact.value === "number" || typeof fact.value === "boolean") return String(fact.value);
  if (fact.value && typeof fact.value === "object") {
    const record = fact.value as Record<string, unknown>;
    for (const key of ["statement", "value", "summary", "description", "text"]) {
      if (typeof record[key] === "string" && record[key].trim()) return record[key].trim();
    }
    return JSON.stringify(record);
  }
  return "";
}

function objectValue(value: unknown): Record<string, unknown> | undefined {
  return value && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : undefined;
}

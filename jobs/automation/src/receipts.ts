import type { ApplicationPacket, NormalizedJob, SubmissionReceipt } from "./contracts.js";

export interface ReceiptDocument {
  kind: "resume" | "cover_letter" | "attachment";
  versionId?: string;
  storageKey: string;
  sha256: string;
  fileName?: string;
  mediaType?: string;
}

export interface ReceiptEvent {
  id: string;
  occurredAt: string;
  type: string;
  detail?: Record<string, unknown>;
}

export interface ApplicationReceiptBundle {
  schemaVersion: 1;
  receiptId: string;
  accountId: string;
  applicationId: string;
  runId: string;
  generatedAt: string;
  runner: "local" | "cloud";
  job: NormalizedJob;
  packet: {
    jobId: string;
    resumeVersionId: string;
    answers: Record<string, string>;
    verifiedClaimIds: string[];
  };
  documents: ReceiptDocument[];
  events: ReceiptEvent[];
  result: SubmissionReceipt;
  finalUrl?: string;
  screenshotKeys: string[];
}

export interface CreateReceiptInput {
  receiptId: string;
  accountId: string;
  runId: string;
  runner: "local" | "cloud";
  job: NormalizedJob;
  packet: ApplicationPacket;
  documents: ReceiptDocument[];
  events: ReceiptEvent[];
  result: SubmissionReceipt;
  generatedAt?: string;
  finalUrl?: string;
  screenshotKeys?: string[];
}

export interface ApplicationEvidenceRecord {
  id: string;
  application_id: string;
  kind: "resume" | "cover_letter" | "attachment" | "submission_confirmation" | "status_email" | "interview_event";
  label: string;
  provider: string;
  file_name: string;
  media_type: string;
  storage_key: string;
  sha256: string;
  resume_version_id?: string;
  occurred_at_ms: number;
  metadata: Record<string, unknown>;
  created_at_ms: number;
}

export interface LinkedProviderEvidenceInput {
  applicationId: string;
  kind: "status_email" | "interview_event";
  provider: "gmail" | "outlook_email" | "google_calendar" | "outlook_calendar";
  externalId: string;
  label: string;
  occurredAt: string;
  metadata?: Record<string, unknown>;
}

export function createApplicationReceipt(input: CreateReceiptInput): ApplicationReceiptBundle {
  return {
    schemaVersion: 1,
    receiptId: input.receiptId,
    accountId: input.accountId,
    applicationId: input.packet.applicationId,
    runId: input.runId,
    generatedAt: input.generatedAt ?? new Date().toISOString(),
    runner: input.runner,
    job: structuredClone(input.job),
    packet: {
      jobId: input.packet.jobId,
      resumeVersionId: input.packet.resumeVersionId,
      answers: sortRecord(input.packet.answers),
      verifiedClaimIds: [...input.packet.verifiedClaimIds].sort(),
    },
    documents: [...input.documents].sort((left, right) => left.storageKey.localeCompare(right.storageKey)),
    events: [...input.events].sort((left, right) => left.occurredAt.localeCompare(right.occurredAt) || left.id.localeCompare(right.id)),
    result: structuredClone(input.result),
    finalUrl: input.finalUrl,
    screenshotKeys: [...(input.screenshotKeys ?? [])].sort(),
  };
}

export function applicationEvidenceFromReceipt(receipt: ApplicationReceiptBundle): ApplicationEvidenceRecord[] {
  const createdAt = Date.parse(receipt.generatedAt);
  const occurredAt = Date.parse(receipt.result.submittedAt ?? receipt.generatedAt);
  const documents = receipt.documents.map((document, index): ApplicationEvidenceRecord => ({
    id: `${receipt.receiptId}:document:${index}`,
    application_id: receipt.applicationId,
    kind: document.kind,
    label: document.kind === "resume" ? "Resume submitted" : document.kind === "cover_letter" ? "Cover letter submitted" : "Attachment submitted",
    provider: receipt.job.source,
    file_name: document.fileName ?? fileNameFromKey(document.storageKey),
    media_type: document.mediaType ?? mediaTypeFor(document.storageKey),
    storage_key: document.storageKey,
    sha256: document.sha256,
    resume_version_id: document.kind === "resume" ? document.versionId ?? receipt.packet.resumeVersionId : document.versionId,
    occurred_at_ms: occurredAt,
    metadata: { attached_to_submission: true, runner: receipt.runner, run_id: receipt.runId },
    created_at_ms: createdAt,
  }));
  if (receipt.result.status === "submitted") {
    documents.push({
      id: `${receipt.receiptId}:confirmation`,
      application_id: receipt.applicationId,
      kind: "submission_confirmation",
      label: receipt.result.confirmationText || "Application submitted",
      provider: receipt.job.source,
      file_name: "",
      media_type: "",
      storage_key: "",
      sha256: "",
      occurred_at_ms: occurredAt,
      metadata: {
        external_id: receipt.result.confirmationUrl || receipt.receiptId,
        confirmation: receipt.result.confirmationText || "Application submitted",
        confirmation_url: receipt.result.confirmationUrl,
        final_url: receipt.finalUrl,
        screenshot_keys: receipt.screenshotKeys,
      },
      created_at_ms: createdAt,
    });
  }
  return documents;
}

export function linkedProviderEvidence(input: LinkedProviderEvidenceInput): ApplicationEvidenceRecord {
  if (!input.applicationId.trim() || !input.externalId.trim()) {
    throw new Error("Linked inbox and calendar evidence needs an application and provider event ID");
  }
  const occurredAt = Date.parse(input.occurredAt);
  if (!Number.isFinite(occurredAt)) throw new Error("Linked evidence needs a valid occurrence time");
  return {
    id: `${input.provider}:${input.externalId}`,
    application_id: input.applicationId,
    kind: input.kind,
    label: input.label,
    provider: input.provider,
    file_name: "",
    media_type: "",
    storage_key: "",
    sha256: "",
    occurred_at_ms: occurredAt,
    metadata: { ...input.metadata, external_id: input.externalId },
    created_at_ms: Date.now(),
  };
}

export async function fingerprintReceipt(receipt: ApplicationReceiptBundle): Promise<string> {
  const bytes = new TextEncoder().encode(stableStringify(receipt));
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  return [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, "0")).join("");
}

export function stableStringify(value: unknown): string {
  return JSON.stringify(sortValue(value));
}

function sortValue(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(sortValue);
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value as Record<string, unknown>)
        .sort(([left], [right]) => left.localeCompare(right))
        .map(([key, nested]) => [key, sortValue(nested)]),
    );
  }
  return value;
}

function sortRecord(value: Record<string, string>): Record<string, string> {
  return Object.fromEntries(Object.entries(value).sort(([left], [right]) => left.localeCompare(right)));
}

function fileNameFromKey(storageKey: string): string {
  const name = storageKey.split("/").filter(Boolean).at(-1);
  return name || "application-document";
}

function mediaTypeFor(storageKey: string): string {
  const lower = storageKey.toLowerCase();
  if (lower.endsWith(".pdf")) return "application/pdf";
  if (lower.endsWith(".docx")) return "application/vnd.openxmlformats-officedocument.wordprocessingml.document";
  return "application/octet-stream";
}

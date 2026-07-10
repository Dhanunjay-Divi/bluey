import type { ApplicationPacket, NormalizedJob, SubmissionReceipt } from "./contracts.js";

export interface ReceiptDocument {
  kind: "resume" | "cover_letter" | "attachment";
  versionId?: string;
  storageKey: string;
  sha256: string;
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

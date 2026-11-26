import type { InterviewPrepCompletionResponse } from "../api";
import { safeDownloadFileName, saveDownloadedBlob } from "./download";

export interface InterviewPrepDownloadScope {
  applicationId: string;
  receiptId: string;
  resumeVersionId: string;
  resumeChecksum: string;
  resumeDocumentSha256: string;
}

export interface InterviewPrepCandidateReviewInput {
  factsChecked: boolean;
  reasoningChecked: boolean;
}

export interface InterviewPrepDraftExport {
  schema_version: 1;
  status: "draft";
  application_id: string;
  prep_id: string;
  generated_at_ms: number;
  content: string;
  grounding: {
    receipt_id: string;
    receipt_fingerprint: string;
    resume_version_id: string;
    resume_checksum: string;
    resume_document_sha256: string;
    answer_keys_used: string[];
    answer_keys_omitted: string[];
  };
  generation: {
    provider: string;
    model: string;
    cost_cents: number;
    cost_label: string | null;
    confidence: number | null;
  };
  candidate_review?: {
    kind: "candidate_self_review";
    facts_checked: true;
    reasoning_checked: true;
  };
}

export interface InterviewPrepDraftDownload {
  fileName: string;
  payload: InterviewPrepDraftExport;
  json: string;
}

export type InterviewPrepDownloadFormat = "text" | "json";

export function buildInterviewPrepDraftDownload(
  response: InterviewPrepCompletionResponse,
  scope: InterviewPrepDownloadScope,
  review?: InterviewPrepCandidateReviewInput,
): InterviewPrepDraftDownload {
  assertInterviewPrepDownloadScope(response, scope);
  assertCompleteCandidateReview(review);
  const payload: InterviewPrepDraftExport = {
    schema_version: 1,
    status: "draft",
    application_id: response.application_id,
    prep_id: response.id,
    generated_at_ms: response.generated_at_ms,
    content: response.content,
    grounding: {
      receipt_id: response.grounding.receipt_id,
      receipt_fingerprint: response.grounding.receipt_fingerprint,
      resume_version_id: response.grounding.resume_version_id,
      resume_checksum: response.grounding.resume_checksum,
      resume_document_sha256: response.grounding.resume_document_sha256,
      answer_keys_used: [...response.grounding.answer_keys_used],
      answer_keys_omitted: [...response.grounding.answer_keys_omitted],
    },
    generation: {
      provider: response.provider,
      model: response.model,
      cost_cents: response.cost_cents,
      cost_label: response.cost_label,
      confidence: response.confidence,
    },
    ...(review ? {
      candidate_review: {
        kind: "candidate_self_review" as const,
        facts_checked: true as const,
        reasoning_checked: true as const,
      },
    } : {}),
  };
  return {
    fileName: interviewPrepDraftFileName(response.id),
    payload,
    json: `${JSON.stringify(payload, null, 2)}\n`,
  };
}

export function downloadInterviewPrepDraft(
  response: InterviewPrepCompletionResponse,
  scope: InterviewPrepDownloadScope,
  review: InterviewPrepCandidateReviewInput | undefined,
  format: InterviewPrepDownloadFormat,
): void {
  if (format === "text") {
    const draft = buildInterviewPrepRehearsalDownload(response, scope, review);
    saveDownloadedBlob(new Blob([draft.text], { type: "text/plain;charset=utf-8" }), draft.fileName);
    return;
  }
  const draft = buildInterviewPrepDraftDownload(response, scope, review);
  saveDownloadedBlob(
    new Blob([draft.json], { type: "application/json;charset=utf-8" }),
    draft.fileName,
  );
}

export function buildInterviewPrepRehearsalDownload(
  response: InterviewPrepCompletionResponse,
  scope: InterviewPrepDownloadScope,
  review?: InterviewPrepCandidateReviewInput,
): { fileName: string; text: string } {
  assertInterviewPrepDownloadScope(response, scope);
  assertCompleteCandidateReview(review);
  const reviewNote = review
    ? "You marked the facts and reasoning checked. Bluey has not verified this draft."
    : "Check the facts and reasoning before rehearsing. Bluey has not verified this draft.";
  return {
    fileName: interviewPrepDraftFileName(response.id, "txt"),
    text: ["Bluey Jobs | Interview practice draft", reviewNote, "",
      "Rehearse in your own words. Use only examples you can explain and substantiate.", "",
      response.content, ""].join("\n"),
  };
}

function assertCompleteCandidateReview(
  review: InterviewPrepCandidateReviewInput | undefined,
): void {
  if (
    review !== undefined &&
    (typeof review !== "object" ||
      review === null ||
      review.factsChecked !== true ||
      review.reasoningChecked !== true)
  ) {
    throw new Error("Candidate review requires both facts and reasoning checks.");
  }
}

export function interviewPrepDraftFileName(prepId: string, extension: "json" | "txt" = "json"): string {
  const source = prepId.startsWith("prep-") ? prepId.slice("prep-".length) : prepId;
  const token = source
    .slice(0, 256)
    .normalize("NFKD")
    .replace(/[^A-Za-z0-9._-]+/g, "-")
    .replace(/^\.+/, "")
    .replace(/^-+|-+$/g, "")
    .slice(0, 96) || "draft";
  return safeDownloadFileName(
    `bluey-interview-prep-${token}.${extension}`,
    `bluey-interview-prep-draft.${extension}`,
  );
}

export function assertInterviewPrepDownloadScope(
  response: InterviewPrepCompletionResponse,
  scope: InterviewPrepDownloadScope,
): void {
  if (
    response.application_id !== scope.applicationId ||
    response.grounding.receipt_id !== scope.receiptId ||
    response.grounding.resume_version_id !== scope.resumeVersionId ||
    response.grounding.resume_checksum !== scope.resumeChecksum ||
    response.grounding.resume_document_sha256 !== scope.resumeDocumentSha256
  ) {
    throw new Error("The interview preparation draft no longer matches the selected application.");
  }
}

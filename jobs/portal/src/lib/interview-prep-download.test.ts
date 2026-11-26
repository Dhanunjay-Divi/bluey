import { describe, expect, it, vi } from "vitest";
import type { InterviewPrepCompletionResponse } from "../api";
import { saveDownloadedBlob } from "./download";
import {
  buildInterviewPrepDraftDownload,
  buildInterviewPrepRehearsalDownload,
  downloadInterviewPrepDraft,
  interviewPrepDraftFileName,
  type InterviewPrepDownloadScope,
} from "./interview-prep-download";

vi.mock("./download", async (importOriginal) => ({
  ...await importOriginal<typeof import("./download")>(),
  saveDownloadedBlob: vi.fn(),
}));

const SCOPE: InterviewPrepDownloadScope = {
  applicationId: "application-1",
  receiptId: "receipt-1",
  resumeVersionId: "resume-1",
  resumeChecksum: "resume-checksum",
  resumeDocumentSha256: "b".repeat(64),
};

function completion(): InterviewPrepCompletionResponse {
  return {
    schema_version: 1,
    id: "prep-1234567890abcdef12345678",
    application_id: SCOPE.applicationId,
    content: "Practice this draft, then check every personal example.",
    generated_at_ms: 1_797_000_000_000,
    provider: "managed-provider",
    model: "balanced-model",
    cost_cents: 3,
    balance_cents_after: 997,
    trial_seconds_remaining: 0,
    cost_label: "3 cents",
    confidence: 0.8,
    grounding: {
      receipt_id: SCOPE.receiptId,
      receipt_fingerprint: "a".repeat(64),
      resume_version_id: SCOPE.resumeVersionId,
      resume_checksum: "resume-checksum",
      resume_document_sha256: "b".repeat(64),
      answer_keys_used: ["availability"],
      answer_keys_omitted: ["authorization"],
    },
  };
}

describe("interview preparation draft download", () => {
  it("builds a local draft envelope with selected-scope provenance", () => {
    const draft = buildInterviewPrepDraftDownload(completion(), SCOPE);

    expect(draft.fileName).toBe(
      "bluey-interview-prep-1234567890abcdef12345678.json",
    );
    expect(draft.payload).toMatchObject({
      schema_version: 1,
      status: "draft",
      application_id: SCOPE.applicationId,
      prep_id: "prep-1234567890abcdef12345678",
      grounding: {
        receipt_id: SCOPE.receiptId,
        resume_version_id: SCOPE.resumeVersionId,
      },
    });
    expect(draft.payload).not.toHaveProperty("balance_cents_after");
    expect(draft.payload).not.toHaveProperty("trial_seconds_remaining");
    expect(draft.payload).not.toHaveProperty("candidate_review");
    expect(draft.json.endsWith("\n")).toBe(true);
    expect(JSON.parse(draft.json)).toEqual(draft.payload);
  });

  it("adds only an explicit complete candidate self-review annotation", () => {
    const draft = buildInterviewPrepDraftDownload(completion(), SCOPE, {
      factsChecked: true,
      reasoningChecked: true,
    });

    expect(draft.payload.status).toBe("draft");
    expect(draft.payload.candidate_review).toEqual({
      kind: "candidate_self_review",
      facts_checked: true,
      reasoning_checked: true,
    });
    expect(draft.payload.grounding).toEqual(completion().grounding);
    expect(draft.payload.candidate_review).not.toHaveProperty("reviewed_at_ms");
  });

  it.each([
    [{ factsChecked: false, reasoningChecked: true }],
    [{ factsChecked: true, reasoningChecked: false }],
    [{ factsChecked: false, reasoningChecked: false }],
  ] as const)("refuses an incomplete candidate review annotation", (review) => {
    expect(() => buildInterviewPrepDraftDownload(completion(), SCOPE, review)).toThrow(
      "Candidate review requires both facts and reasoning checks.",
    );
  });

  it.each([
    ["application", { applicationId: "application-2" }],
    ["receipt", { receiptId: "receipt-2" }],
    ["resume", { resumeVersionId: "resume-2" }],
    ["resume checksum", { resumeChecksum: "different-revision" }],
    ["resume document hash", { resumeDocumentSha256: "c".repeat(64) }],
  ] as const)("refuses a %s scope mismatch", (_label, replacement) => {
    expect(() => buildInterviewPrepDraftDownload(completion(), {
      ...SCOPE,
      ...replacement,
    })).toThrow("draft no longer matches the selected application");
  });

  it("labels the export as a draft without mutating the validated response", () => {
    const response = completion();
    const before = structuredClone(response);
    const draft = buildInterviewPrepDraftDownload(response, SCOPE);

    draft.payload.grounding.answer_keys_used.push("new-local-label");
    expect(response).toEqual(before);
    expect(draft.payload.status).toBe("draft");
    expect(draft.json).not.toContain('"status": "reviewed"');
  });

  it("creates a portable leaf filename from an untrusted prep label", () => {
    expect(interviewPrepDraftFileName("../Prep: Candidate? * one")).toBe(
      "bluey-interview-prep-Prep-Candidate-one.json",
    );
  });
});

describe("readable rehearsal download", () => {
  it("saves multiline practice content as inert text and omits structured metadata", async () => {
    const response = completion();
    response.content = "Question: What changed?\n\nFollow-up: Why?\nLiteral text: <h1>Example</h1>";
    downloadInterviewPrepDraft(response, SCOPE, undefined, "text");

    const [blob, fileName] = vi.mocked(saveDownloadedBlob).mock.lastCall!;
    expect(blob.type).toBe("text/plain;charset=utf-8");
    expect(fileName).toBe("bluey-interview-prep-1234567890abcdef12345678.txt");
    const text = await blob.text();
    expect(text).toContain("Interview practice draft");
    expect(text).toContain("Check the facts and reasoning before rehearsing.");
    expect(text).toContain(`${response.content}\n`);
    for (const privateValue of [SCOPE.applicationId, SCOPE.receiptId, SCOPE.resumeVersionId,
      SCOPE.resumeChecksum, SCOPE.resumeDocumentSha256, response.provider, response.model,
      "997", "availability", "authorization"]) {
      expect(text).not.toContain(privateValue);
    }
  });

  it("distinguishes both candidate checks from Bluey verification", () => {
    const draft = buildInterviewPrepRehearsalDownload(completion(), SCOPE, {
      factsChecked: true,
      reasoningChecked: true,
    });
    expect(draft.text).toContain("You marked the facts and reasoning checked.");
    expect(draft.text).toContain("Bluey has not verified this draft.");
  });

  it("does not save after a scope mismatch or an incomplete explicit check", () => {
    vi.mocked(saveDownloadedBlob).mockClear();
    expect(() => downloadInterviewPrepDraft(completion(), {
      ...SCOPE, resumeVersionId: "different-resume",
    }, undefined, "text")).toThrow("draft no longer matches the selected application");
    expect(() => downloadInterviewPrepDraft(completion(), SCOPE, {
      factsChecked: true, reasoningChecked: false,
    }, "text")).toThrow("Candidate review requires both facts and reasoning checks.");
    expect(saveDownloadedBlob).not.toHaveBeenCalled();
  });

  it("preserves the structured JSON download", async () => {
    const response = completion();
    downloadInterviewPrepDraft(response, SCOPE, undefined, "json");
    const [blob, fileName] = vi.mocked(saveDownloadedBlob).mock.lastCall!;
    expect(blob.type).toBe("application/json;charset=utf-8");
    expect(fileName).toMatch(/\.json$/);
    expect(JSON.parse(await blob.text())).toEqual(buildInterviewPrepDraftDownload(response, SCOPE).payload);
  });
});

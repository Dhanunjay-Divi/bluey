import { describe, expect, it } from "vitest";
import {
  evaluateC2CRehearsal,
  prepareC2CRehearsalReview,
  simulateC2CRehearsal,
} from "../src/c2c-rehearsal.js";
import type { C2CRehearsalDraft, C2CRehearsalInput } from "../src/c2c-rehearsal.js";

const NOW = 1_800_000_000_000;
const HASHES = {
  terms: "a".repeat(64),
  rightsEvidence: "b".repeat(64),
  representationEvidence: "c".repeat(64),
};

function validInput(): C2CRehearsalInput {
  const requirement = {
    id: "requirement-1",
    revision: 3,
    sourceId: "source-1",
    sourceRevision: 7,
    termsSha256: HASHES.terms,
    vendorId: "vendor-1",
    engagement: "c2c" as const,
    observedAtMs: NOW - 80,
    verifiedAtMs: NOW - 70,
    expiresAtMs: NOW + 90,
  };
  const sourceRights = {
    sourceId: requirement.sourceId,
    sourceRevision: requirement.sourceRevision,
    termsSha256: requirement.termsSha256,
    vendorId: requirement.vendorId,
    state: "approved" as const,
    permitsMatching: true,
    evidenceSha256: HASHES.rightsEvidence,
    effectiveAtMs: NOW - 100,
    expiresAtMs: NOW + 100,
  };
  const representation = {
    accountId: "account-1",
    trackId: "track-1",
    requirementId: requirement.id,
    requirementRevision: requirement.revision,
    vendorId: requirement.vendorId,
    revision: 2,
    state: "active" as const,
    evidenceSha256: HASHES.representationEvidence,
    effectiveAtMs: NOW - 100,
    expiresAtMs: NOW + 100,
  };

  return {
    accountId: representation.accountId,
    trackId: representation.trackId,
    nowMs: NOW,
    requirement,
    sourceRights,
    representation,
  };
}

function validDraft(): C2CRehearsalDraft {
  return {
    recipientId: "recipient-1",
    recipientEvidenceSha256: "d".repeat(64),
    subject: "Data engineering C2C introduction",
    body: "Please review this local rehearsal draft before any external action.",
    proposedDocumentSha256: "e".repeat(64),
    attachment_refs: [],
  };
}

function expectNoEffects(value: ReturnType<typeof evaluateC2CRehearsal>): void {
  expect(value.canContact).toBe(false);
  expect(value.canAttach).toBe(false);
  expect(value.canSubmit).toBe(false);
}

function preparedReviewSha256(
  input: C2CRehearsalInput,
  draft: C2CRehearsalDraft,
): string {
  const prepared = prepareC2CRehearsalReview(input, draft);

  expect(prepared.state).toBe("review_only");
  if (prepared.state !== "review_only") {
    throw new Error("expected valid rehearsal review");
  }
  return prepared.reviewSha256;
}

describe("C2C rehearsal policy", () => {
  it("permits only review when exact rights and representation are current", () => {
    const decision = evaluateC2CRehearsal(validInput());

    expect(decision.state).toBe("review_only");
    expectNoEffects(decision);
  });

  it.each([
    "denied",
    "terms_changed",
    "expired",
    "review_required",
  ] as const)("blocks a %s source-rights state", (state) => {
    const input = validInput();
    const decision = evaluateC2CRehearsal({
      ...input,
      sourceRights: { ...input.sourceRights, state },
    });

    expect(decision.state).toBe("blocked");
    expectNoEffects(decision);
  });

  it("blocks missing or non-matching source rights", () => {
    const input = validInput();
    const missing = evaluateC2CRehearsal({ ...input, sourceRights: null });
    const notPermitted = evaluateC2CRehearsal({
      ...input,
      sourceRights: { ...input.sourceRights, permitsMatching: false },
    });
    const changedTerms = evaluateC2CRehearsal({
      ...input,
      sourceRights: { ...input.sourceRights, termsSha256: "d".repeat(64) },
    });
    const changedSourceRevision = evaluateC2CRehearsal({
      ...input,
      sourceRights: { ...input.sourceRights, sourceRevision: 8 },
    });
    const changedVendor = evaluateC2CRehearsal({
      ...input,
      sourceRights: { ...input.sourceRights, vendorId: "vendor-2" },
    });

    expect(missing.state).toBe("blocked");
    expect(notPermitted.state).toBe("blocked");
    expect(changedTerms.state).toBe("blocked");
    expect(changedSourceRevision.state).toBe("blocked");
    expect(changedVendor.state).toBe("blocked");
    expectNoEffects(missing);
    expectNoEffects(notPermitted);
    expectNoEffects(changedTerms);
    expectNoEffects(changedSourceRevision);
    expectNoEffects(changedVendor);
  });

  it.each([
    ["future verification", (input: ReturnType<typeof validInput>) => ({
      ...input,
      requirement: { ...input.requirement, verifiedAtMs: NOW + 1 },
    })],
    ["requirement expiry at the exclusive boundary", (input: ReturnType<typeof validInput>) => ({
      ...input,
      requirement: { ...input.requirement, expiresAtMs: NOW },
    })],
    ["rights not yet effective", (input: ReturnType<typeof validInput>) => ({
      ...input,
      sourceRights: { ...input.sourceRights, effectiveAtMs: NOW + 1 },
    })],
    ["observation before source rights became effective", (input: ReturnType<typeof validInput>) => ({
      ...input,
      requirement: { ...input.requirement, observedAtMs: NOW - 101 },
    })],
    ["rights expiry at the exclusive boundary", (input: ReturnType<typeof validInput>) => ({
      ...input,
      sourceRights: { ...input.sourceRights, expiresAtMs: NOW },
    })],
    ["representation not yet effective", (input: ReturnType<typeof validInput>) => ({
      ...input,
      representation: { ...input.representation, effectiveAtMs: NOW + 1 },
    })],
    ["representation expiry at the exclusive boundary", (input: ReturnType<typeof validInput>) => ({
      ...input,
      representation: { ...input.representation, expiresAtMs: NOW },
    })],
  ] as const)("blocks %s", (_caseName, makeInput) => {
    const decision = evaluateC2CRehearsal(makeInput(validInput()));

    expect(decision.state).toBe("blocked");
    expectNoEffects(decision);
  });

  it.each([
    ["account", (input: ReturnType<typeof validInput>) => ({ ...input, accountId: "account-2" })],
    ["track", (input: ReturnType<typeof validInput>) => ({ ...input, trackId: "track-2" })],
    ["vendor", (input: ReturnType<typeof validInput>) => ({
      ...input,
      representation: { ...input.representation, vendorId: "vendor-2" },
    })],
    ["requirement ID", (input: ReturnType<typeof validInput>) => ({
      ...input,
      representation: { ...input.representation, requirementId: "requirement-2" },
    })],
    ["requirement revision", (input: ReturnType<typeof validInput>) => ({
      ...input,
      representation: { ...input.representation, requirementRevision: 4 },
    })],
  ] as const)("blocks a mismatched representation %s", (_field, makeInput) => {
    const decision = evaluateC2CRehearsal(makeInput(validInput()));

    expect(decision.state).toBe("blocked");
    expectNoEffects(decision);
  });

  it("blocks a missing or revoked representation", () => {
    const input = validInput();
    const missing = evaluateC2CRehearsal({ ...input, representation: null });
    const revoked = evaluateC2CRehearsal({
      ...input,
      representation: { ...input.representation, state: "revoked" },
    });

    expect(missing.state).toBe("blocked");
    expect(revoked.state).toBe("blocked");
    expectNoEffects(missing);
    expectNoEffects(revoked);
  });

  it("blocks unknown engagement and never grants an external effect", () => {
    const input = validInput();
    const decision = evaluateC2CRehearsal({
      ...input,
      requirement: { ...input.requirement, engagement: "unknown" },
    });

    expect(decision.state).toBe("blocked");
    expectNoEffects(decision);
  });

  it.each(["w2", "1099"] as const)("blocks non-C2C %s engagement", (engagement) => {
    const input = validInput();
    const decision = evaluateC2CRehearsal({
      ...input,
      requirement: { ...input.requirement, engagement },
    });

    expect(decision.state).toBe("blocked");
    expectNoEffects(decision);
  });

  it("blocks malformed opaque identifiers and evidence hashes", () => {
    const input = validInput();
    const malformedAccount = evaluateC2CRehearsal({ ...input, accountId: "account id" });
    const malformedRequirementHash = evaluateC2CRehearsal({
      ...input,
      requirement: { ...input.requirement, termsSha256: "a".repeat(63) },
    });
    const malformedRightsEvidence = evaluateC2CRehearsal({
      ...input,
      sourceRights: { ...input.sourceRights, evidenceSha256: "B".repeat(64) },
    });
    const malformedRepresentationEvidence = evaluateC2CRehearsal({
      ...input,
      representation: { ...input.representation, evidenceSha256: "c".repeat(63) },
    });

    expect(malformedAccount.state).toBe("blocked");
    expect(malformedRequirementHash.state).toBe("blocked");
    expect(malformedRightsEvidence.state).toBe("blocked");
    expect(malformedRepresentationEvidence.state).toBe("blocked");
    expectNoEffects(malformedAccount);
    expectNoEffects(malformedRequirementHash);
    expectNoEffects(malformedRightsEvidence);
    expectNoEffects(malformedRepresentationEvidence);
  });

  it("binds simulation to the exact approved review and keeps every effect disabled", () => {
    const input = validInput();
    const draft = validDraft();
    const reviewSha256 = preparedReviewSha256(input, draft);
    const result = simulateC2CRehearsal(input, draft, {
      reviewSha256,
      approvedAtMs: NOW - 1,
      expiresAtMs: NOW + 1,
    });

    expect(result.state).toBe("simulated_only");
    if (result.state !== "simulated_only") {
      throw new Error("expected an exact approved rehearsal simulation");
    }
    expect(result.reviewSha256).toBe(reviewSha256);
    expect(result.attachment_refs).toEqual([]);
    expect(result.canContact).toBe(false);
    expect(result.canAttach).toBe(false);
    expect(result.canSubmit).toBe(false);
  });

  it.each([
    ["recipient", (draft: C2CRehearsalDraft) => ({ ...draft, recipientId: "recipient-2" })],
    ["recipient evidence", (draft: C2CRehearsalDraft) => ({
      ...draft,
      recipientEvidenceSha256: "f".repeat(64),
    })],
    ["subject", (draft: C2CRehearsalDraft) => ({ ...draft, subject: "Changed subject" })],
    ["body", (draft: C2CRehearsalDraft) => ({ ...draft, body: "Changed body" })],
    ["proposed document", (draft: C2CRehearsalDraft) => ({
      ...draft,
      proposedDocumentSha256: null,
    })],
  ] as const)("blocks a %s change after review", (_field, changeDraft) => {
    const input = validInput();
    const draft = validDraft();
    const result = simulateC2CRehearsal(input, changeDraft(draft), {
      reviewSha256: preparedReviewSha256(input, draft),
      approvedAtMs: NOW - 1,
      expiresAtMs: NOW + 1,
    });

    expect(result.state).toBe("blocked");
  });

  it.each([
    ["wrong", (reviewSha256: string) => ({
      reviewSha256: reviewSha256 === "f".repeat(64) ? "e".repeat(64) : "f".repeat(64),
      approvedAtMs: NOW - 1,
      expiresAtMs: NOW + 1,
    })],
    ["expired at the exclusive boundary", (reviewSha256: string) => ({
      reviewSha256,
      approvedAtMs: NOW - 1,
      expiresAtMs: NOW,
    })],
    ["future", (reviewSha256: string) => ({
      reviewSha256,
      approvedAtMs: NOW + 1,
      expiresAtMs: NOW + 2,
    })],
    ["pre-verification", (reviewSha256: string) => ({
      reviewSha256,
      approvedAtMs: NOW - 71,
      expiresAtMs: NOW + 1,
    })],
  ] as const)("blocks a %s approval", (_kind, makeApproval) => {
    const input = validInput();
    const draft = validDraft();
    const result = simulateC2CRehearsal(input, draft, makeApproval(preparedReviewSha256(input, draft)));

    expect(result.state).toBe("blocked");
  });

  it("blocks a grant made for an earlier review after requirement revision changes", () => {
    const input = validInput();
    const draft = validDraft();
    const priorReviewSha256 = preparedReviewSha256(input, draft);
    const changedInput = {
      ...input,
      requirement: { ...input.requirement, revision: input.requirement.revision + 1 },
      representation: {
        ...input.representation!,
        requirementRevision: input.requirement.revision + 1,
      },
    };
    const result = simulateC2CRehearsal(changedInput, draft, {
      reviewSha256: priorReviewSha256,
      approvedAtMs: NOW - 1,
      expiresAtMs: NOW + 1,
    });

    expect(result.state).toBe("blocked");
  });

  it("requires a fresh review for a representation revision with otherwise identical evidence", () => {
    const input = validInput();
    const draft = validDraft();
    const review = preparedReviewSha256(input, draft);
    const later = { ...input, representation: { ...input.representation, revision: 3 } };
    expect(preparedReviewSha256(later, draft)).not.toBe(review);
    expect(simulateC2CRehearsal(later, draft, {
      reviewSha256: review, approvedAtMs: NOW - 1, expiresAtMs: NOW + 1,
    })).toMatchObject({ state: "blocked", reason: "review_not_current" });
    expect(evaluateC2CRehearsal({ ...input, representation: { ...input.representation, revision: 0 } }))
      .toMatchObject({ state: "blocked", reason: "representation_unverified" });
  });

  it("rejects a nonempty attachment runtime value", () => {
    const input = validInput();
    const draft = validDraft();
    const unsafeDraft = {
      ...draft,
      attachment_refs: ["jobs/resumes/resume-1.pdf"],
    } as unknown as C2CRehearsalDraft;
    const result = simulateC2CRehearsal(input, unsafeDraft, {
      reviewSha256: preparedReviewSha256(input, draft),
      approvedAtMs: NOW - 1,
      expiresAtMs: NOW + 1,
    });

    expect(result.state).toBe("blocked");
  });

  it.each([
    ["newline", "Data engineering C2C\nBcc: other@example.com"],
    ["lone UTF-16 surrogate", "Data engineering \ud800"],
  ] as const)("blocks a subject containing a %s", (_kind, subject) => {
    const result = prepareC2CRehearsalReview(validInput(), {
      ...validDraft(),
      subject,
    });

    expect(result.state).toBe("blocked");
  });
});

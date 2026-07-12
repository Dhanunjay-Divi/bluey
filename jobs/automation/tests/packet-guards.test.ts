import { describe, expect, it } from "vitest";
import { assertRunnablePacket, assertSubmissionReceiptComplete, type ApplicationReceiptBundle, type ApplicationPacket } from "../src/index.js";

describe("application runner guards", () => {
  it("requires the frozen application identity and browser profile before a browser run", () => {
    expect(() => assertRunnablePacket(packet({ applicationIdentityId: undefined })))
      .toThrow("applicationIdentityId");
    expect(() => assertRunnablePacket(packet({ browserProfileId: "" })))
      .toThrow("browserProfileId");
    expect(() => assertRunnablePacket(packet())).not.toThrow();
  });

  it("requires submitted receipts to prove the exact resume, identity, and confirmation", () => {
    expect(() => assertSubmissionReceiptComplete(receipt())).not.toThrow();
    expect(() => assertSubmissionReceiptComplete(receipt({ documents: [] }))).toThrow("resume document");
    expect(() => assertSubmissionReceiptComplete(receipt({
      documents: [{ kind: "resume", versionId: "other-resume", storageKey: "resume.pdf", sha256: "a".repeat(64) }],
    }))).toThrow("resume document does not match");
    expect(() => assertSubmissionReceiptComplete(receipt({
      result: { status: "submitted", issues: [] },
      screenshotKeys: [],
    }))).toThrow("submission confirmation");
  });
});

function packet(overrides: Partial<ApplicationPacket> = {}): ApplicationPacket {
  return {
    applicationId: "application-1",
    jobId: "job-1",
    resumeVersionId: "resume-1",
    answers: { email: "ada@example.com" },
    verifiedClaimIds: ["claim-1"],
    applicationIdentityId: "identity-1",
    applicationEmail: "ada@example.com",
    browserProfileId: "profile-1",
    ...overrides,
  };
}

function receipt(overrides: Partial<ApplicationReceiptBundle> = {}): ApplicationReceiptBundle {
  return {
    schemaVersion: 1,
    receiptId: "receipt-1",
    accountId: "account-1",
    applicationId: "application-1",
    runId: "run-1",
    generatedAt: "2026-07-11T12:00:00.000Z",
    runner: "local",
    applicationIdentityId: "identity-1",
    browserProfileId: "profile-1",
    adapter: "greenhouse",
    adapterVersion: "1.0.0",
    job: {
      externalId: "job-1",
      canonicalUrl: "https://boards.greenhouse.io/acme/jobs/1",
      company: "Acme",
      title: "Software Engineer",
      location: "New York, NY",
      workplace: "hybrid",
      description: "Build.",
      source: "greenhouse",
    },
    packet: {
      jobId: "job-1",
      resumeVersionId: "resume-1",
      answers: { email: "ada@example.com" },
      verifiedClaimIds: ["claim-1"],
      applicationEmail: "ada@example.com",
    },
    documents: [{ kind: "resume", versionId: "resume-1", storageKey: "resume.pdf", sha256: "a".repeat(64) }],
    events: [],
    result: {
      status: "submitted",
      confirmationText: "Application received",
      confirmationUrl: "https://boards.greenhouse.io/acme/jobs/1/confirmation",
      submittedAt: "2026-07-11T12:00:00.000Z",
      issues: [],
    },
    finalUrl: "https://boards.greenhouse.io/acme/jobs/1/confirmation",
    screenshotKeys: ["receipt.png"],
    ...overrides,
  };
}

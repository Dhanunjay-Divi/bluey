import { describe, expect, it } from "vitest";
import { createApplicationReceipt, fingerprintReceipt } from "../src/index.js";

describe("application receipt bundles", () => {
  it("keeps exact packet answers, documents, events, and submission evidence", async () => {
    const receipt = createApplicationReceipt({
      receiptId: "receipt-1",
      accountId: "account-1",
      runId: "run-1",
      runner: "cloud",
      generatedAt: "2026-07-10T12:00:00.000Z",
      job: {
        externalId: "job-1",
        canonicalUrl: "https://jobs.acme.com/job-1",
        company: "Acme",
        title: "Engineer",
        location: "New York, NY",
        workplace: "hybrid",
        description: "Build useful things.",
        source: "greenhouse",
      },
      packet: {
        applicationId: "application-1",
        jobId: "job-1",
        resumeVersionId: "resume-job-1",
        resumePath: "/packets/job-1/resume.pdf",
        answers: { sponsorship: "No", location: "New York, NY" },
        verifiedClaimIds: ["claim-2", "claim-1"],
      },
      documents: [{ kind: "resume", versionId: "resume-job-1", storageKey: "receipts/resume.pdf", sha256: "abc" }],
      events: [
        { id: "event-2", occurredAt: "2026-07-10T11:59:02.000Z", type: "submitted" },
        { id: "event-1", occurredAt: "2026-07-10T11:58:00.000Z", type: "started" },
      ],
      result: {
        status: "submitted",
        confirmationText: "Application received",
        confirmationUrl: "https://jobs.acme.com/job-1/confirmation",
        submittedAt: "2026-07-10T11:59:02.000Z",
        screenshotPath: "receipts/confirmation.png",
        issues: [],
      },
      finalUrl: "https://jobs.acme.com/job-1/confirmation",
      screenshotKeys: ["receipts/confirmation.png"],
    });

    expect(receipt.packet.verifiedClaimIds).toEqual(["claim-1", "claim-2"]);
    expect(receipt.events.map((event) => event.type)).toEqual(["started", "submitted"]);
    expect(receipt.result.confirmationText).toBe("Application received");
    expect(await fingerprintReceipt(receipt)).toMatch(/^[a-f0-9]{64}$/);
    expect(await fingerprintReceipt(receipt)).toBe(await fingerprintReceipt(structuredClone(receipt)));
  });
});

import { describe, expect, it } from "vitest";
import { applicationEvidenceFromReceipt, createApplicationReceipt, fingerprintReceipt, linkedProviderEvidence } from "../src/index.js";

describe("application receipt bundles", () => {
  it("keeps exact packet answers, documents, events, and submission evidence", async () => {
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
        applicationIdentityId: "identity-1",
        applicationEmail: "ada@example.com",
        browserProfileId: "profile-1",
      },
      documents: [{ kind: "resume", versionId: "resume-job-1", storageKey: "receipts/resume.pdf", sha256: "a".repeat(64) }],
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
    expect(receipt.applicationIdentityId).toBe("identity-1");
    expect(receipt.browserProfileId).toBe("profile-1");
    expect(receipt.packet.applicationEmail).toBe("ada@example.com");
    const evidence = applicationEvidenceFromReceipt(receipt);
    expect(evidence).toHaveLength(2);
    expect(evidence[0]).toMatchObject({
      application_id: "application-1",
      kind: "resume",
      file_name: "resume.pdf",
      resume_version_id: "resume-job-1",
    });
    expect(evidence[1]).toMatchObject({ kind: "submission_confirmation", label: "Application received" });
    expect(await fingerprintReceipt(receipt)).toMatch(/^[a-f0-9]{64}$/);
    expect(await fingerprintReceipt(receipt)).toBe(await fingerprintReceipt(structuredClone(receipt)));
  });

  it("links mailbox and calendar events to one application with provider IDs", () => {
    const email = linkedProviderEvidence({
      applicationId: "application-1",
      kind: "status_email",
      provider: "gmail",
      externalId: "message-123",
      label: "Interview invitation received",
      occurredAt: "2026-07-11T14:00:00.000Z",
      metadata: { subject: "Your Acme interview" },
    });
    const interview = linkedProviderEvidence({
      applicationId: "application-1",
      kind: "interview_event",
      provider: "google_calendar",
      externalId: "event-456",
      label: "Technical interview",
      occurredAt: "2026-07-14T16:00:00.000Z",
    });
    expect(email.metadata.external_id).toBe("message-123");
    expect(interview.application_id).toBe("application-1");
    expect(() => linkedProviderEvidence({ ...emailToInput(email), externalId: "" })).toThrow("provider event ID");
  });
});

function emailToInput(evidence: ReturnType<typeof linkedProviderEvidence>) {
  return {
    applicationId: evidence.application_id,
    kind: "status_email" as const,
    provider: "gmail" as const,
    externalId: String(evidence.metadata.external_id),
    label: evidence.label,
    occurredAt: new Date(evidence.occurred_at_ms).toISOString(),
  };
}

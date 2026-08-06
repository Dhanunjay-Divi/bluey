import { describe, expect, it } from "vitest";
import {
  applicationEvidenceFromReceipt,
  approvedExecutionChecksum,
  createApplicationReceipt,
  fingerprintReceipt,
  linkedProviderEvidence,
  type ApplicationPacket,
  type AtsCertifiedReceiptAuthority,
  type NormalizedJob,
} from "../src/index.js";

describe("application receipt bundles", () => {
  it("keeps exact packet answers, documents, events, and submission evidence", async () => {
    const job: NormalizedJob = {
      externalId: "job-1",
      canonicalUrl: "https://jobs.acme.com/job-1",
      company: "Acme",
      title: "Engineer",
      location: "New York, NY",
      workplace: "hybrid",
      description: "Build useful things.",
      source: "greenhouse",
    };
    const packet: ApplicationPacket = {
      applicationId: "application-1",
      jobId: "job-1",
      resumeVersionId: "resume-job-1",
      approvedPacketChecksum: "",
      resumePath: "/packets/job-1/resume.pdf",
      answers: { sponsorship: "No", location: "New York, NY" },
      verifiedClaimIds: ["claim-2", "claim-1"],
      applicationIdentityId: "identity-1",
      applicationEmail: "ada@example.com",
      browserProfileId: "profile-1",
    };
    packet.approvedPacketChecksum = approvedExecutionChecksum(packet, job);
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
      job,
      packet,
      documents: [{ kind: "resume", versionId: "resume-job-1", storageKey: "receipts/resume.pdf", sha256: "a".repeat(64) }],
      events: [
        { id: "event-2", occurredAt: "2026-07-10T11:59:02.000Z", type: "submitted" },
        { id: "event-1", occurredAt: "2026-07-10T11:58:00.000Z", type: "started" },
      ],
      result: {
        status: "submitted",
        submitHttpStatus: 200,
        confirmationText: "Application received",
        confirmationUrl: "https://jobs.acme.com/job-1/confirmation",
        submittedAt: "2026-07-10T11:59:02.000Z",
        screenshotPath: "receipts/confirmation.png",
        issues: [],
      },
      finalUrl: "https://jobs.acme.com/job-1/confirmation",
      screenshotKeys: ["receipts/confirmation.png"],
    });
    receipt.receiptObject = {
      storageKey: "receipts/receipt-1.json",
      sha256: "c".repeat(64),
      mediaType: "application/json",
      sizeBytes: 123,
      schemaVersion: 1,
    };

    expect(receipt.packet.verifiedClaimIds).toEqual(["claim-1", "claim-2"]);
    expect(receipt.events.map((event) => event.type)).toEqual(["started", "submitted"]);
    expect(receipt.result.confirmationText).toBe("Application received");
    expect(receipt.applicationIdentityId).toBe("identity-1");
    expect(receipt.browserProfileId).toBe("profile-1");
    expect(receipt.packet.applicationEmail).toBe("ada@example.com");
    expect(receipt.packet.answers).toEqual(packet.answers);
    expect(receipt.packet.approvedPacketChecksum).toBe(packet.approvedPacketChecksum);
    const evidence = applicationEvidenceFromReceipt(receipt);
    expect(evidence).toHaveLength(3);
    expect(evidence[0]).toMatchObject({
      application_id: "application-1",
      kind: "resume",
      file_name: "resume.pdf",
      resume_version_id: "resume-job-1",
    });
    expect(evidence[1]).toMatchObject({ kind: "submission_confirmation", label: "Application received" });
    expect(evidence[2]).toMatchObject({
      kind: "application_receipt",
      file_name: "receipt-1.json",
      media_type: "application/json",
      storage_key: "receipts/receipt-1.json",
      sha256: "c".repeat(64),
      resume_version_id: "resume-job-1",
      metadata: {
        immutable: true,
        receipt_id: "receipt-1",
        schema_version: 1,
        size_bytes: 123,
      },
    });
    expect(await fingerprintReceipt(receipt)).toMatch(/^[a-f0-9]{64}$/);
    expect(await fingerprintReceipt(receipt)).toBe(await fingerprintReceipt(structuredClone(receipt)));

    const changedPacket = structuredClone(packet);
    changedPacket.answers.sponsorship = "Yes";
    expect(() => createApplicationReceipt({
      ...receiptInputForIntegrity(job, changedPacket),
    })).toThrow("changed after review");
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

  it("creates schema v2 only with exact Phase-B certified receipt authority", () => {
    const { input, authority } = certifiedReceiptInput();
    expect(() => createApplicationReceipt(input)).toThrow(
      "Certified submission receipt authority is missing",
    );

    const receipt = createApplicationReceipt({
      ...input,
      atsCertifiedReceiptAuthority: authority,
    });

    expect(receipt.schemaVersion).toBe(2);
    expect(receipt.packet.approvedExecutionSchemaVersion).toBe(3);
    expect(receipt.packet.approvedExecutionAdmission).toEqual(
      input.packet.approvedExecutionAdmission,
    );
    expect(receipt.atsCertifiedReceiptAuthority).toEqual(authority);
    authority.phaseBRequestId = "mutated-after-create";
    expect(receipt.atsCertifiedReceiptAuthority?.phaseBRequestId).toBe(
      "phase-b-request-1",
    );
  });

  it.each(["needs_input", "failed"] as const)(
    "keeps certified pre-Phase-B %s receipts on schema v1",
    (status) => {
      const { input } = certifiedReceiptInput();
      const receipt = createApplicationReceipt({
        ...input,
        result: { status, issues: [] },
        screenshotKeys: [],
      });

      expect(receipt.schemaVersion).toBe(1);
      expect(receipt.packet.approvedExecutionSchemaVersion).toBeUndefined();
      expect(receipt.packet.approvedExecutionAdmission).toBeUndefined();
      expect(receipt.atsCertifiedReceiptAuthority).toBeUndefined();
    },
  );

  it("uses schema v2 for a non-submitted result after Phase B exists", () => {
    const { input, authority } = certifiedReceiptInput();
    const receipt = createApplicationReceipt({
      ...input,
      result: { status: "needs_input", issues: [] },
      screenshotKeys: [],
      atsCertifiedReceiptAuthority: authority,
    });

    expect(receipt.schemaVersion).toBe(2);
    expect(receipt.packet.approvedExecutionSchemaVersion).toBe(3);
    expect(receipt.packet.approvedExecutionAdmission).toEqual(
      input.packet.approvedExecutionAdmission,
    );
    expect(receipt.atsCertifiedReceiptAuthority).toEqual(authority);
  });
});

function receiptInputForIntegrity(job: NormalizedJob, packet: ApplicationPacket) {
  return {
    receiptId: "receipt-integrity",
    accountId: "account-1",
    runId: "run-integrity",
    runner: "cloud" as const,
    applicationIdentityId: "identity-1",
    browserProfileId: "profile-1",
    adapter: "greenhouse",
    adapterVersion: "1.0.0",
    job,
    packet,
    documents: [{
      kind: "resume" as const,
      versionId: "resume-job-1",
      storageKey: "receipts/resume.pdf",
      sha256: "a".repeat(64),
    }],
    events: [],
    result: { status: "failed" as const, issues: [] },
    screenshotKeys: [],
  };
}

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

function certifiedReceiptInput() {
  const job: NormalizedJob = {
    externalId: "job-certified",
    canonicalUrl: "https://boards.greenhouse.io/acme/jobs/certified",
    company: "Acme",
    title: "Engineer",
    location: "Remote",
    workplace: "remote",
    description: "Build safely.",
    source: "greenhouse",
  };
  const packet: ApplicationPacket = {
    applicationId: "application-certified",
    jobId: "job-certified",
    resumeVersionId: "resume-certified",
    approvedPacketChecksum: "",
    answers: { email: "ada@example.com" },
    verifiedClaimIds: ["claim-certified"],
    applicationIdentityId: "identity-certified",
    applicationEmail: "ada@example.com",
    browserProfileId: "profile-certified",
    approvedExecutionSchemaVersion: 3,
    approvedExecutionAdmission: {
      kind: "track_auto_submit",
      authorization_id: "authorization-certified",
      career_track_id: "track-certified",
      revision_no: 2,
      authority_fingerprint: "b".repeat(64),
      ats_certification: {
        schema_version: 1,
        provider: "greenhouse",
        adapter_version: "2026.07.1-beta.1",
        variant_key: "public",
        layout_contract_version: 1,
        surface_sha256: "d".repeat(64),
        manifest_sha256: "1".repeat(64),
        activation_sha256: "2".repeat(64),
        activation_generation: 3,
        target_key_sha256: "3".repeat(64),
        layout_set_sha256: "4".repeat(64),
        adapter_bundle_sha256: "6".repeat(64),
        runner_target_sha256s: ["7".repeat(64)],
        expires_at_ms: 9_007_199_254_740_000,
      },
    },
  };
  packet.approvedPacketChecksum = approvedExecutionChecksum(packet, job);
  const authority: AtsCertifiedReceiptAuthority = {
    schemaVersion: 1,
    accountId: "account-certified",
    applicationId: "application-certified",
    runId: "run-certified",
    provider: "greenhouse",
    adapter: "greenhouse",
    adapterVersion: "2026.07.1-beta.1",
    manifestSha256: "1".repeat(64),
    activationSha256: "2".repeat(64),
    activationGeneration: 3,
    targetKeySha256: "3".repeat(64),
    layoutSetSha256: "4".repeat(64),
    layoutObservationSha256: "5".repeat(64),
    observedSurfaceSha256: "d".repeat(64),
    adapterBundleSha256: "6".repeat(64),
    runnerKind: "cloud",
    runnerTargetSha256: "7".repeat(64),
    bindingSha256: "8".repeat(64),
    bindingFence: 5,
    bindingConsumedAtMs: Date.parse("2026-07-10T11:59:59.000Z"),
    applicationAttemptId: "attempt-certified",
    phaseBRequestId: "phase-b-request-1",
    rolloutChannel: "canary",
    canaryReservationSha256: "9".repeat(64),
    meteringReservationSha256: "a".repeat(64),
  };
  return {
    input: {
      receiptId: "receipt-certified",
      accountId: "account-certified",
      runId: "run-certified",
      runner: "cloud" as const,
      adapter: "greenhouse",
      adapterVersion: "2026.07.1-beta.1",
      generatedAt: "2026-07-10T12:00:00.000Z",
      job,
      packet,
      documents: [{
        kind: "resume" as const,
        versionId: "resume-certified",
        storageKey: "receipts/resume-certified.pdf",
        sha256: "c".repeat(64),
      }],
      events: [],
      result: {
        status: "submitted" as const,
        submitHttpStatus: 200,
        confirmationText: "Application received",
        submittedAt: "2026-07-10T12:00:00.000Z",
        issues: [],
      },
      screenshotKeys: ["receipts/confirmation-certified.png"],
    },
    authority,
  };
}

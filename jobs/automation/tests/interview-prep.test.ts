import { describe, expect, it } from "vitest";
import {
  buildInterviewPrepPacket,
  approvedExecutionChecksum,
  createApplicationReceipt,
  extractRoleSignals,
  interviewPrepUserMessage,
  type ApplicationReceiptBundle,
  type SubmittedResumeSnapshot,
} from "../src/index.js";

function receipt(): ApplicationReceiptBundle {
  const job = {
    externalId: "job-1",
    canonicalUrl: "https://boards.greenhouse.io/acme/jobs/1",
    company: "Acme",
    title: "Senior Product Engineer",
    location: "New York, NY",
    workplace: "hybrid" as const,
    source: "greenhouse" as const,
    description: [
      "You will design reliable TypeScript services for customer-facing workflows.",
      "Collaborate with product and design partners to deliver accessible experiences.",
      "Experience operating distributed systems in production is preferred.",
    ].join("\n"),
  };
  const packet = {
    applicationId: "application-1",
    jobId: "job-1",
    resumeVersionId: "resume-1",
    approvedPacketChecksum: "",
    answers: {
      motivation: "I enjoy turning complex workflows into dependable products.",
      candidate_email: "ada@example.com",
      gender: "Prefer not to say",
      phone_number: "+1 212 555 0199",
    },
    verifiedClaimIds: ["claim-typescript", "claim-collaboration", "claim-contact"],
    applicationIdentityId: "identity-1",
    applicationEmail: "ada@example.com",
    browserProfileId: "profile-1",
  };
  packet.approvedPacketChecksum = approvedExecutionChecksum(packet, job);
  return createApplicationReceipt({
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
    documents: [{
      kind: "resume",
      versionId: "resume-1",
      storageKey: "jobs/resumes/resume-1.pdf",
      sha256: "a".repeat(64),
    }],
    events: [{ id: "event-1", occurredAt: "2026-07-10T11:59:00.000Z", type: "submitted" }],
    result: {
      status: "submitted",
      submitHttpStatus: 200,
      confirmationText: "Application received",
      confirmationUrl: "https://boards.greenhouse.io/acme/jobs/1/confirmation",
      submittedAt: "2026-07-10T11:59:00.000Z",
      issues: [],
    },
    screenshotKeys: ["jobs/receipts/receipt-1/confirmation.png"],
  });
}

function resume(): SubmittedResumeSnapshot {
  return {
    versionId: "resume-1",
    content: {
      contact: { email: "ada@example.com", phone: "+1 212 555 0199" },
      summary: "Product engineer focused on reliable customer workflows.",
      skills: ["TypeScript", "Distributed systems", "Accessibility"],
      employment: [{ company: "Northwind", highlights: ["Led a TypeScript platform migration."] }],
    },
    claims: [
      {
        id: "claim-typescript",
        label: "TypeScript platform",
        statement: "Led a TypeScript platform migration used by customer-facing workflows.",
        status: "confirmed",
        tags: ["TypeScript", "reliability"],
      },
      {
        id: "claim-collaboration",
        label: "Cross-functional delivery",
        statement: "Delivered accessible product flows with product and design partners.",
        status: "confirmed",
        tags: ["accessibility", "collaboration"],
      },
      {
        id: "claim-contact",
        category: "contact",
        label: "Email address",
        statement: "ada@example.com",
        status: "confirmed",
      },
    ],
  };
}

describe("interview preparation packets", () => {
  it("grounds preparation in the exact submission and filters private application data", () => {
    const packet = buildInterviewPrepPacket({
      receipt: receipt(),
      resume: resume(),
      generatedAt: "2026-07-11T16:00:00.000Z",
      evidence: [
        {
          id: "calendar-1",
          application_id: "application-1",
          kind: "interview_event",
          label: "Technical interview",
          provider: "google_calendar",
          file_name: "",
          media_type: "",
          storage_key: "",
          sha256: "",
          occurred_at_ms: Date.parse("2026-07-14T16:00:00.000Z"),
          metadata: { attendees: ["private@example.com"] },
          created_at_ms: Date.parse("2026-07-11T14:00:00.000Z"),
        },
        {
          id: "other-application",
          application_id: "application-2",
          kind: "interview_event",
          label: "Wrong interview",
          provider: "google_calendar",
          file_name: "",
          media_type: "",
          storage_key: "",
          sha256: "",
          occurred_at_ms: Date.parse("2026-07-12T16:00:00.000Z"),
          metadata: {},
          created_at_ms: Date.parse("2026-07-11T14:00:00.000Z"),
        },
      ],
    });

    expect(packet).toMatchObject({
      prepId: "prep-receipt-1",
      applicationId: "application-1",
      resumeVersionId: "resume-1",
      interviewLabel: "Technical interview",
      interviewAt: "2026-07-14T16:00:00.000Z",
    });
    expect(packet.resumeContent).not.toHaveProperty("contact");
    expect(packet.commitments).toEqual([{
      key: "motivation",
      answer: "I enjoy turning complex workflows into dependable products.",
      sourceId: "application-answer-1",
    }]);
    expect(JSON.stringify(packet)).not.toContain("ada@example.com");
    expect(JSON.stringify(packet)).not.toContain("Prefer not to say");
    expect(JSON.stringify(packet)).not.toContain("private@example.com");
    expect(packet.questions.some((question) => question.claimIds.includes("claim-typescript"))).toBe(true);
    expect(packet.sources.some((source) => source.id === "resume-claim-claim-collaboration")).toBe(true);
    expect(packet.launchPrompt).toContain("never invent experience");
    const message = interviewPrepUserMessage(packet);
    expect(message).toContain("Document context:");
    expect(message).toContain("Submitted resume version: resume-1");
    expect(message).toContain("Evidence-linked practice plan:");
    expect(message).not.toContain("ada@example.com");
    expect(message).not.toContain("private@example.com");
  });

  it("creates an explicit truth gap when the submitted resume does not support a requirement", () => {
    const value = receipt();
    value.job.description = "You must have extensive Kubernetes cluster administration experience.";
    const packet = buildInterviewPrepPacket({ receipt: value, resume: resume() });
    const gap = packet.questions.find((question) => question.category === "truth_gap");

    expect(gap).toMatchObject({ claimIds: [], needsCandidateInput: true });
    expect(gap?.question).toContain("closest truthful experience");
    expect(packet.warnings).toHaveLength(1);
  });

  it("rejects a resume version other than the one sent to the employer", () => {
    const value = resume();
    value.versionId = "resume-newer-but-not-submitted";
    expect(() => buildInterviewPrepPacket({ receipt: receipt(), resume: value }))
      .toThrow("does not match the submitted resume version");
  });

  it("rejects missing or unconfirmed submitted claims", () => {
    const missing = resume();
    missing.claims = [missing.claims[0], missing.claims[2]];
    expect(() => buildInterviewPrepPacket({ receipt: receipt(), resume: missing }))
      .toThrow("claim-collaboration is missing");

    const unconfirmed = resume();
    unconfirmed.claims[1] = { ...unconfirmed.claims[1], status: "unverified" };
    expect(() => buildInterviewPrepPacket({ receipt: receipt(), resume: unconfirmed }))
      .toThrow("claim-collaboration is not confirmed");
  });

  it("keeps an imported claim that was present in the exact submitted resume", () => {
    const imported = resume();
    imported.claims[1] = {
      ...imported.claims[1],
      status: "unverified",
      source: "resume_import",
    };
    const packet = buildInterviewPrepPacket({ receipt: receipt(), resume: imported });
    expect(packet.sources.some((source) => source.id === "resume-claim-claim-collaboration")).toBe(true);
  });

  it("requires the evidence-complete receipt boundary", () => {
    const value = receipt();
    value.screenshotKeys = [];
    expect(() => buildInterviewPrepPacket({ receipt: value, resume: resume() }))
      .toThrow("confirmation screenshot");
  });
});

describe("role signal extraction", () => {
  it("prioritizes requirements and removes duplicates", () => {
    const signals = extractRoleSignals([
      "About us: we make useful things for teams around the world.",
      "You will design reliable TypeScript services for customer workflows.",
      "You will design reliable TypeScript services for customer workflows.",
      "Experience with accessible product development is preferred.",
    ].join("\n"));

    expect(signals).toEqual([
      "You will design reliable TypeScript services for customer workflows.",
      "Experience with accessible product development is preferred.",
    ]);
  });
});

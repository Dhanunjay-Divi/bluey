import { describe, expect, it } from "vitest";
import {
  classifyApplicationMessage,
  correlateApplicationMessage,
  planApplicationCommunication,
  type AnswerMemoryRecord,
  type CommunicationApplication,
  type NormalizedApplicationMessage,
} from "../src/index.js";

const application: CommunicationApplication = {
  id: "app-acme-swe",
  jobId: "job-acme-swe",
  company: "Acme",
  title: "Software Engineer",
  applicationEmail: "jobs@example.com",
  trackId: "track-swe",
  employerDomains: ["acme.com"],
};

function message(overrides: Partial<NormalizedApplicationMessage> = {}): NormalizedApplicationMessage {
  return {
    provider: "gmail",
    connectionId: "mailbox-1",
    externalId: "message-1",
    fromAddress: "recruiter@acme.com",
    fromName: "Riley Recruiter",
    toAddresses: ["jobs@example.com"],
    subject: "Acme Software Engineer application",
    text: "Thank you for applying.",
    receivedAt: "2026-07-23T15:00:00.000Z",
    ...overrides,
  };
}

function memory(id: string, key: string, value: string): AnswerMemoryRecord {
  return {
    id,
    key,
    question: key,
    value,
    scope: "track",
    scope_id: "track-swe",
    confirmed: true,
    source: "user",
  };
}

describe("application communications", () => {
  it("classifies common recruiter outcomes in deterministic priority order", () => {
    expect(classifyApplicationMessage(message({ text: "We would like to schedule an interview." }))).toBe("interview");
    expect(classifyApplicationMessage(message({ text: "We regret to inform you that we are not moving forward." }))).toBe("rejection");
    expect(classifyApplicationMessage(message({ text: "We are pleased to offer you the role." }))).toBe("offer");
    expect(classifyApplicationMessage(message({ text: "Please complete this coding assessment." }))).toBe("assessment");
  });

  it("requires one strong application match before planning a reply", () => {
    const result = correlateApplicationMessage(message(), [
      application,
      { ...application, id: "app-other", company: "Other", applicationEmail: "other@example.com" },
    ]);
    expect(result).toMatchObject({
      applicationId: "app-acme-swe",
      confidence: "strong",
    });
    expect(result.reasons).toContain("application email matches");
    expect(result.reasons).toContain("employer domain matches");
  });

  it("creates a reviewable reply using only confirmed scoped answer memory", () => {
    const result = planApplicationCommunication(
      message({
        text: "Could you share your portfolio?\nWhat is your phone number?",
      }),
      [application],
      {
        answerMemory: [
          memory("portfolio", "portfolio", "https://example.com/portfolio"),
          memory("phone", "phone", "+1 555 010 2020"),
        ],
      },
    );
    expect(result.kind).toBe("information_request");
    expect(result.reply.action).toBe("draft");
    expect(result.reply.answerMemoryIds).toEqual(["portfolio", "phone"]);
    expect(result.reply.body).toContain("https://example.com/portfolio");
    expect(result.reply.body).toContain("+1 555 010 2020");
  });

  it("allows auto-send only when explicitly enabled and every answer is verified", () => {
    const result = planApplicationCommunication(
      message({ text: "Please provide your portfolio" }),
      [application],
      {
        autoReplyEnabled: true,
        answerMemory: [memory("portfolio", "portfolio", "https://example.com/portfolio")],
      },
    );
    expect(result.reply.action).toBe("auto_send");
  });

  it("pauses for legal, compensation, assessment, interview, and missing facts", () => {
    const legal = planApplicationCommunication(
      message({ text: "Will you require visa sponsorship?" }),
      [application],
      { autoReplyEnabled: true, answerMemory: [memory("sponsorship", "sponsorship", "No")] },
    );
    expect(legal.reply.action).toBe("needs_input");
    expect(legal.reply.blockedQuestions).toEqual(["Will you require visa sponsorship?"]);

    const missing = planApplicationCommunication(
      message({ text: "What is your portfolio URL?" }),
      [application],
      { autoReplyEnabled: true },
    );
    expect(missing.reply.action).toBe("needs_input");
    expect(missing.reply.missingQuestions).toEqual(["What is your portfolio URL?"]);

    expect(planApplicationCommunication(
      message({ text: "Please complete this assessment." }),
      [application],
    ).reply.action).toBe("needs_input");
    expect(planApplicationCommunication(
      message({ text: "Please send your availability for an interview." }),
      [application],
    ).reply.action).toBe("needs_input");
  });

  it("maps evidence-backed outcomes and calendar candidates without auto-replying", () => {
    const interview = planApplicationCommunication(
      message({ text: "We would like to schedule an interview next week." }),
      [application],
    );
    expect(interview.outcome).toBe("interview");
    expect(interview.calendarCandidate).toBe(true);
    expect(interview.reply.action).toBe("needs_input");

    const rejected = planApplicationCommunication(
      message({ text: "We regret to inform you that we are not moving forward." }),
      [application],
    );
    expect(rejected.outcome).toBe("rejected");
    expect(rejected.reply.action).toBe("no_reply");
  });

  it("refuses ambiguous messages shared by multiple applications", () => {
    const shared = { ...application, employerDomains: [] };
    const result = planApplicationCommunication(
      message({
        fromAddress: "notifications@ats.example",
        subject: "Application update",
        text: "Could you provide your portfolio?",
      }),
      [
        shared,
        { ...shared, id: "app-acme-two", jobId: "job-acme-two", title: "Platform Engineer" },
      ],
      { autoReplyEnabled: true, answerMemory: [memory("portfolio", "portfolio", "https://example.com")] },
    );
    expect(result.correlation.confidence).toBe("none");
    expect(result.correlation.ambiguousApplicationIds).toEqual(["app-acme-swe", "app-acme-two"]);
    expect(result.reply.action).toBe("needs_input");
  });
});

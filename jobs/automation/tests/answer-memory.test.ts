import { describe, expect, it } from "vitest";
import {
  answerMemoryKeyForQuestion,
  buildRememberedAnswers,
  normalizeAnswerMemoryKey,
  selectAnswerMemory,
  type AnswerMemoryRecord,
} from "../src/index.js";

describe("answer memory selection", () => {
  it("normalizes repeated job application questions into stable keys", () => {
    expect(normalizeAnswerMemoryKey("Are you authorized to work in the U.S.?")).toBe("are_you_authorized_to_work_in_the_u_s");
    expect(answerMemoryKeyForQuestion("Will you now or in the future require visa sponsorship?")).toBe("sponsorship");
    expect(answerMemoryKeyForQuestion("What are your salary expectations?")).toBe("compensation");
  });

  it("prefers company answers, then track answers, then account defaults", () => {
    const answers = [
      memory("account-auth", "work_authorization", "Yes", "account"),
      memory("track-auth", "work_authorization", "Yes, for US roles", "track", "track-sde"),
      memory("company-auth", "work_authorization", "Yes, no restrictions for Acme", "company", "Acme"),
    ];

    expect(selectAnswerMemory("Are you authorized to work?", answers, { trackId: "track-sde", company: "Acme" })?.answer.id)
      .toBe("company-auth");
    expect(selectAnswerMemory("Are you authorized to work?", answers, { trackId: "track-sde", company: "OtherCo" })?.answer.id)
      .toBe("track-auth");
    expect(selectAnswerMemory("Are you authorized to work?", answers, { trackId: "track-de", company: "OtherCo" })?.answer.id)
      .toBe("account-auth");
  });

  it("does not reuse unconfirmed answers", () => {
    const answers = [
      { ...memory("draft", "sponsorship", "No", "account"), confirmed: false },
    ];
    expect(selectAnswerMemory("Do you require sponsorship?", answers)).toBeUndefined();
  });

  it("builds a runner answer map for unanswered required fields", () => {
    const answers = [
      memory("salary", "compensation", "$170,000", "account"),
      memory("portfolio", "portfolio", "https://example.com", "track", "track-sde"),
    ];
    expect(buildRememberedAnswers(
      ["Salary expectation", "Portfolio or website"],
      answers,
      { trackId: "track-sde" },
    )).toEqual({
      compensation: "$170,000",
      portfolio: "https://example.com",
    });
  });
});

function memory(
  id: string,
  key: string,
  value: string,
  scope: AnswerMemoryRecord["scope"],
  scopeId?: string,
): AnswerMemoryRecord {
  return {
    id,
    key,
    question: key,
    value,
    scope,
    scope_id: scopeId,
    confirmed: true,
    source: "user",
    updated_at_ms: Date.now(),
  } as AnswerMemoryRecord;
}

import { describe, expect, it } from "vitest";
import {
  ASSISTANT_PROFILE_SCHEMA_VERSION,
  EMPTY_ASSISTANT_PROFILE,
  MAX_ASSISTANT_COMPANY_CHARS,
  MAX_ASSISTANT_INSTRUCTIONS_CHARS,
  MAX_ASSISTANT_ROLE_CHARS,
  MAX_PRIORITY_QUESTION_CHARS,
  MAX_PRIORITY_QUESTIONS,
  assistantProfilesEqual,
  charCount,
  normalizeAssistantProfile,
  priorityQuestionErrorKey,
  validateAssistantProfile,
  type AssistantProfile,
} from "./assistantProfile";

function profile(overrides: Partial<AssistantProfile> = {}): AssistantProfile {
  return { ...EMPTY_ASSISTANT_PROFILE, ...overrides };
}

describe("assistant profile normalization", () => {
  it("normalizes single-line fields, preserves instruction lines, and discards empty questions", () => {
    const source = { application_id: "app-1", receipt_id: "receipt-1" };
    const normalized = normalizeAssistantProfile(profile({
      target_role: "  Staff\u0000\n  Engineer  ",
      company: "  Acme\t  Research  ",
      custom_instructions: "  Keep\nuseful\tspacing\u0007  ",
      priority_questions: ["  Tell me\n about   scale  ", " \u0000 ", "Trade-offs?"],
      source,
    }));

    expect(normalized).toEqual({
      schema_version: ASSISTANT_PROFILE_SCHEMA_VERSION,
      mode: "general",
      target_role: "Staff Engineer",
      company: "Acme Research",
      custom_instructions: "Keep\nuseful\tspacing",
      priority_questions: ["Tell me about scale", "Trade-offs?"],
      source,
    });
  });

  it("treats normalized drafts as equal while preserving meaningful order", () => {
    expect(assistantProfilesEqual(
      profile({ target_role: " Engineer ", priority_questions: [" First? "] }),
      profile({ target_role: "Engineer", priority_questions: ["First?"] }),
    )).toBe(true);
    expect(assistantProfilesEqual(
      profile({ priority_questions: ["First?", "Second?"] }),
      profile({ priority_questions: ["Second?", "First?"] }),
    )).toBe(false);
  });
});

describe("assistant profile validation", () => {
  it("accepts every field at its Rust boundary limit", () => {
    const errors = validateAssistantProfile(profile({
      target_role: "r".repeat(MAX_ASSISTANT_ROLE_CHARS),
      company: "c".repeat(MAX_ASSISTANT_COMPANY_CHARS),
      custom_instructions: "i".repeat(MAX_ASSISTANT_INSTRUCTIONS_CHARS),
      priority_questions: Array.from(
        { length: MAX_PRIORITY_QUESTIONS },
        (_, index) => `${index}`.padEnd(MAX_PRIORITY_QUESTION_CHARS, "q"),
      ),
    }));

    expect(errors).toEqual({});
  });

  it("reports over-limit fields, question count, and individual question rows", () => {
    const questions = Array.from({ length: MAX_PRIORITY_QUESTIONS + 1 }, (_, index) => `Question ${index}`);
    questions[0] = "   ";
    questions[1] = "q".repeat(MAX_PRIORITY_QUESTION_CHARS + 1);

    const errors = validateAssistantProfile(profile({
      target_role: "r".repeat(MAX_ASSISTANT_ROLE_CHARS + 1),
      company: "c".repeat(MAX_ASSISTANT_COMPANY_CHARS + 1),
      custom_instructions: "i".repeat(MAX_ASSISTANT_INSTRUCTIONS_CHARS + 1),
      priority_questions: questions,
    }));

    expect(errors.target_role).toBeTruthy();
    expect(errors.company).toBeTruthy();
    expect(errors.custom_instructions).toBeTruthy();
    expect(errors.priority_questions).toBeTruthy();
    expect(errors[priorityQuestionErrorKey(0)]).toMatch(/Enter a question/);
    expect(errors[priorityQuestionErrorKey(1)]).toMatch(/500 characters/);
  });

  it("counts Unicode code points like the Rust character limits", () => {
    expect(charCount("A😀B")).toBe(3);
  });

  it("rejects unsupported schemas and modes", () => {
    const errors = validateAssistantProfile(profile({
      schema_version: 99,
      mode: "unsupported" as AssistantProfile["mode"],
    }));
    expect(errors.schema_version).toBeTruthy();
    expect(errors.mode).toBeTruthy();
  });
});

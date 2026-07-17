import { describe, expect, it } from "vitest";
import {
  decideRunnerInterventionResolution,
  parseRunnerInterventionResolution,
} from "../src/intervention-policy.js";

const base = { requestId: "run-1:resume:1", profileScope: "a".repeat(40) };

describe("runner intervention reapproval policy", () => {
  it.each([
    { ...base, action: "answer", field: "salary", answer: "$150,000" },
    { ...base, action: "approve_submission", field: "legal_name" },
    { ...base, action: "approve_email_otp", answer: "123456" },
    { ...base, action: "answer" },
  ])("refuses browser resume for content-changing or unknown input %#", (resolution) => {
    expect(decideRunnerInterventionResolution(resolution)).toMatchObject({
      kind: "requires_reapproval",
    });
  });

  it.each([
    "approve_submission",
    "approve_email_otp",
    "resume_browser_takeover",
    "browser_takeover_complete",
  ])("allows the explicit content-neutral action %s", (action) => {
    expect(decideRunnerInterventionResolution({ ...base, action })).toEqual({ kind: "resume" });
  });

  it("rejects malformed or unexpected direct-runner payloads", () => {
    expect(() => parseRunnerInterventionResolution({ ...base, action: 42 })).toThrow("action is invalid");
    expect(() => parseRunnerInterventionResolution({
      ...base,
      action: "approve_submission",
      answers: { salary: "$150,000" },
    })).toThrow("unsupported data");
  });
});

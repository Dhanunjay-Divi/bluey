import { describe, expect, it } from "vitest";
import {
  decideInterventionResolution,
  parseInterventionResolution,
} from "../src/intervention-policy.js";

describe("workflow intervention reapproval policy", () => {
  it.each([
    { action: "answer", field: "salary", answer: "$150,000" },
    { action: "approve_submission", field: "legal_name" },
    { action: "approve_email_otp", answer: "123456" },
    { action: "answer" },
  ])("blocks content-changing or unknown resolution %#", (resolution) => {
    expect(decideInterventionResolution(resolution)).toMatchObject({
      kind: "requires_reapproval",
    });
  });

  it.each([
    "approve_submission",
    "approve_email_otp",
    "resume_browser_takeover",
    "browser_takeover_complete",
  ])("allows the explicit content-neutral action %s", (action) => {
    expect(decideInterventionResolution({ action })).toEqual({
      kind: "resume",
      resolution: { action },
    });
  });

  it("rejects malformed gateway input", () => {
    expect(() => parseInterventionResolution({ action: "" })).toThrow("action is invalid");
    expect(() => parseInterventionResolution({ action: "answer", answer: 42 })).toThrow("answer is invalid");
    expect(() => parseInterventionResolution({ action: "approve_submission", answers: {} }))
      .toThrow("unsupported data");
  });
});

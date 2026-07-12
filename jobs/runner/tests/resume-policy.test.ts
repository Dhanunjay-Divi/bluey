import { describe, expect, it } from "vitest";
import { providerOptionsForResumeAction } from "../src/resume-policy.js";

describe("runner final-review resolution", () => {
  it("approves Greenhouse and Lever only for the exact resumed action", async () => {
    for (const action of [undefined, "approve", "submit", "approve_submission ", "APPROVE_SUBMISSION"]) {
      expect(providerOptionsForResumeAction(action)).toBeUndefined();
    }

    const options = providerOptionsForResumeAction("approve_submission");
    expect(options).toBeDefined();
    expect(await options?.greenhouse?.finalReviewApproval?.({} as never)).toBe(true);
    expect(await options?.lever?.finalReviewApproval?.({} as never)).toBe(true);
  });
});

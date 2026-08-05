import { describe, expect, it } from "vitest";
import {
  classifyLocalFailure,
  isLocalSideEffectReason,
  LocalBrowserError,
} from "../src/local-failure.js";

describe("local browser failure classification", () => {
  it("persists manual submission observation as non-retryable side-effect uncertainty", async () => {
    const failure = await classifyLocalFailure(
      undefined,
      new LocalBrowserError("manual_submission_observed"),
    );

    expect(failure).toMatchObject({
      status: "side_effect_unknown",
      code: "manual_submission_observed",
      preservePage: true,
    });
    expect(failure.message).toMatch(/outside its authorized submit path/i);
    expect(failure.message).toMatch(/will not retry automatically/i);
    expect(isLocalSideEffectReason(failure.code)).toBe(true);
    expect(JSON.stringify(failure)).not.toMatch(/person@example\.test|secret-token|private answer/i);
  });
});

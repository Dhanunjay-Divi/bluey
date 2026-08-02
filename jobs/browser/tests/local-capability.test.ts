import { describe, expect, it } from "vitest";
import { localRunCapabilities, localRunRequestPayload } from "../src/local-capability.js";

const capability = (marker: string) => `${marker.repeat(48)}.${"a".repeat(64)}`;

describe("local browser run capabilities", () => {
  it("accepts operation-specific capabilities and strips them from the run payload", () => {
    const claim = {
      runId: "run-123",
      applicationId: "app-123",
      _blueyCapabilities: {
        result: capability("r"),
        resume: capability("s"),
        expiresAtMs: 2_000,
      },
    };

    expect(localRunCapabilities(claim, "run-123", 1_000)).toEqual(claim._blueyCapabilities);
    expect(localRunRequestPayload(claim)).toEqual({
      runId: "run-123",
      applicationId: "app-123",
    });
  });

  it("rejects expired, swapped, malformed, and mismatched capabilities", () => {
    const valid = {
      runId: "run-123",
      _blueyCapabilities: {
        result: capability("r"),
        resume: capability("s"),
        expiresAtMs: 2_000,
      },
    };
    expect(() => localRunCapabilities(valid, "other-run", 1_000)).toThrow();
    expect(() => localRunCapabilities(valid, "run-123", 2_000)).toThrow();
    expect(() => localRunCapabilities({
      ...valid,
      _blueyCapabilities: { ...valid._blueyCapabilities, result: "not-a-capability" },
    }, "run-123", 1_000)).toThrow();
    expect(() => localRunCapabilities({
      ...valid,
      _blueyCapabilities: {
        ...valid._blueyCapabilities,
        resume: valid._blueyCapabilities.result,
      },
    }, "run-123", 1_000)).toThrow();
  });
});

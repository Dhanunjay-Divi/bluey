import { describe, expect, it } from "vitest";
import { restartDisposition } from "../src/recovery.js";

describe("durable browser restart policy", () => {
  const now = Date.parse("2026-07-16T12:00:00.000Z");

  it.each(["prepared", "needs_input", "provider_review"] as const)(
    "restores unexpired safe phase %s",
    (phase) => expect(restartDisposition(phase, now + 60_000, now)).toBe("restore"),
  );

  it("does not restore expired safe work", () => {
    expect(restartDisposition("needs_input", now, now)).toBe("expired");
  });

  it.each(["final_submit_started", "final_submit_activated", "side_effect_unknown"] as const)(
    "never automatically restores irreversible phase %s",
    (phase) => expect(restartDisposition(phase, now + 60_000, now)).toBe("side_effect_unknown"),
  );

  it("lets a durable submit marker override a stale safe checkpoint", () => {
    expect(restartDisposition("prepared", now + 60_000, now, true)).toBe("side_effect_unknown");
  });
});

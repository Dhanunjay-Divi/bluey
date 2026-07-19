import { describe, expect, it } from "vitest";
import { PowerAdmissionGuard } from "../src/power-admission.js";

describe("verifiable power admission", () => {
  it("blocks new work only for explicit suspend or lock signals", () => {
    const guard = new PowerAdmissionGuard();
    expect(guard.decision()).toEqual({ allowed: true });
    guard.setLocked(true);
    expect(guard.decision()).toEqual({ allowed: false, reason: "device_unavailable" });
    guard.setLocked(false);
    guard.setSuspended(true);
    expect(guard.decision()).toEqual({ allowed: false, reason: "device_unavailable" });
    guard.setSuspended(false);
    expect(guard.decision()).toEqual({ allowed: true });
  });
});

import { describe, expect, it } from "vitest";
import { LocalRunAdmission } from "../src/run-admission.js";

describe("local run admission", () => {
  it("prevents new claims while paused or offline without inventing queue authority", () => {
    const admission = new LocalRunAdmission();
    expect(admission.decision(true)).toEqual({ allowed: true });
    expect(admission.decision(false)).toEqual({ allowed: false, reason: "offline" });
    admission.setPaused(true);
    expect(admission.decision(true)).toEqual({ allowed: false, reason: "paused" });
    admission.setPaused(false);
    expect(admission.decision(true)).toEqual({ allowed: true });
  });

  it("keeps stop requested fail-closed until explicit resume", () => {
    const admission = new LocalRunAdmission();
    admission.requestStop();
    expect(admission.hasStopRequest).toBe(true);
    expect(admission.decision(true)).toEqual({ allowed: false, reason: "stop_requested" });
    admission.setPaused(false);
    expect(admission.hasStopRequest).toBe(false);
    expect(admission.decision(true)).toEqual({ allowed: true });
  });

  it("blocks new claims on an explicit unsafe device signal without changing manual pause", () => {
    const admission = new LocalRunAdmission();
    admission.setDeviceAvailable(false);
    expect(admission.decision(true)).toEqual({ allowed: false, reason: "device_unavailable" });
    expect(admission.isPaused).toBe(false);
    admission.setDeviceAvailable(true);
    expect(admission.decision(true)).toEqual({ allowed: true });
  });
});

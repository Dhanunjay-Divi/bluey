import { describe, expect, it } from "vitest";
import {
  cloudRunnerAccessCopy,
  lockedRunnerAvailability,
  localRunnerAccessCopy,
  runnerAvailabilityOrLocked,
  runnerLandingCopy,
} from "./runner-access";
import type { RunnerChannelAvailability } from "../types";

const runner = (
  status: RunnerChannelAvailability["status"],
  available: boolean,
  planIncluded: boolean,
): RunnerChannelAvailability => ({
  status,
  available,
  plan_included: planIncluded,
  distribution_enabled: available,
  reason: available
    ? "Runner is available."
    : planIncluded
      ? "Runner is included but still in invited beta."
      : "Runner is not included in this plan.",
  next_action: planIncluded ? "Review applications." : "View plans.",
});

describe("runner beta copy", () => {
  it("does not advertise unattended runners as generally available", () => {
    const copy = Object.values(runnerLandingCopy).join(" ");

    expect(copy).toContain("invited beta");
    expect(copy).not.toContain("let the cloud runner continue while your computer is off");
    expect(copy).not.toContain("local + cloud");
  });

  it("describes locked local and cloud access as invited beta", () => {
    expect(localRunnerAccessCopy(runner("invited_beta", false, true))).toMatchObject({
      badge: "Invited beta",
      action: "Review applications",
    });
    expect(cloudRunnerAccessCopy(runner("invited_beta", false, true))).toMatchObject({
      badge: "Invited beta",
      action: "Review applications",
    });
  });

  it("uses operational language only when the entitlement is enabled", () => {
    expect(localRunnerAccessCopy(runner("available", true, true)).description).toContain("run on your computer");
    expect(cloudRunnerAccessCopy(runner("available", true, true)).description).toContain("while your computer is off");
  });

  it("does not present local access as enabled without an exact release artifact", () => {
    expect(localRunnerAccessCopy(runner("available", true, true), {
      available: false,
      reason: "No supported Browser artifact is assigned to this computer.",
    })).toMatchObject({
      badge: "Browser unavailable",
      description: "No supported Browser artifact is assigned to this computer.",
      action: "Review applications",
    });
  });

  it("routes plan-locked runners to plans instead of a dead access request", () => {
    expect(localRunnerAccessCopy(runner("upgrade_required", false, false))).toMatchObject({
      badge: "Plan upgrade",
      action: "View plans",
    });
  });

  it("fails closed when an older API omits runner availability", () => {
    expect(runnerAvailabilityOrLocked(undefined)).toEqual(lockedRunnerAvailability);
    expect(runnerAvailabilityOrLocked(undefined).auto_submit_available).toBe(false);
  });
});

import { describe, expect, it } from "vitest";
import {
  cloudRunnerAccessCopy,
  lockedRunnerAvailability,
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
      ? "Runner is included but temporarily unavailable in this limited public beta."
      : "Runner is not included in this plan.",
  next_action: planIncluded ? "Review applications." : "View plans.",
});

describe("cloud automation access copy", () => {
  it("keeps public plans web-first and cloud beta truthful", () => {
    const copy = Object.values(runnerLandingCopy).join(" ");

    expect(copy).toContain("limited public beta");
    expect(copy).toContain("not cloud automation");
    expect(copy).toContain("available runner");
    expect(copy).not.toMatch(/automation then runs|admission (grants|authorizes)/i);
    expect(copy).toContain("web portal");
    expect(copy).toContain("job-site handoff");
    expect(copy).not.toMatch(/install|local browser|run locally/i);
    expect(copy).not.toContain("let the cloud runner continue while your computer is off");
    expect(copy).not.toContain("local + cloud");
  });

  it("describes unavailable cloud automation as limited public beta", () => {
    expect(cloudRunnerAccessCopy(runner("limited_beta", false, true))).toMatchObject({
      badge: "Limited public beta",
      action: "Review applications",
    });
  });

  it("uses operational language only when cloud access is enabled", () => {
    expect(cloudRunnerAccessCopy(runner("available", true, true))).toMatchObject({
      badge: "Enabled for this account",
      action: "Start cloud automation",
    });
  });

  it("routes plan-locked cloud access to plans instead of a dead request", () => {
    expect(cloudRunnerAccessCopy(runner("upgrade_required", false, false))).toMatchObject({
      badge: "Plan upgrade",
      action: "View plans",
    });
  });

  it("fails closed when an older API omits runner availability", () => {
    expect(runnerAvailabilityOrLocked(undefined)).toEqual(lockedRunnerAvailability);
    expect(runnerAvailabilityOrLocked(undefined).auto_submit_available).toBe(false);
  });

});

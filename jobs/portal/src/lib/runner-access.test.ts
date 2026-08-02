import { describe, expect, it } from "vitest";
import {
  cloudRunnerAccessCopy,
  localRunnerAccessCopy,
  runnerLandingCopy,
} from "./runner-access";

describe("runner beta copy", () => {
  it("does not advertise unattended runners as generally available", () => {
    const copy = Object.values(runnerLandingCopy).join(" ");

    expect(copy).toContain("invited beta");
    expect(copy).not.toContain("let the cloud runner continue while your computer is off");
    expect(copy).not.toContain("local + cloud");
  });

  it("describes locked local and cloud access as invited beta", () => {
    expect(localRunnerAccessCopy(false)).toMatchObject({
      badge: "Invited beta",
      action: "Request access",
    });
    expect(cloudRunnerAccessCopy(false)).toMatchObject({
      badge: "Invited beta",
      action: "Request access",
    });
  });

  it("uses operational language only when the entitlement is enabled", () => {
    expect(localRunnerAccessCopy(true).description).toContain("run on your computer");
    expect(cloudRunnerAccessCopy(true).description).toContain("while your computer is off");
  });
});

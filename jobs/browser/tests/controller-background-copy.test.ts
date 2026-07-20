import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import {
  backgroundAvailabilityDetail,
  backgroundAvailabilityStatus,
  backgroundAvailabilityTitle,
} from "../src/renderer/background-copy.js";

const browserRoot = join(import.meta.dirname, "..");

describe("Bluey Browser background availability copy", () => {
  it.each([
    {
      backgroundEnabled: false,
      loginItemSupported: true,
      expected: "Turn on to start quietly at sign-in and stay ready in the tray while this computer is awake.",
    },
    {
      backgroundEnabled: false,
      loginItemSupported: false,
      expected: "Turn on to stay ready in the tray while this computer is awake.",
    },
    {
      backgroundEnabled: true,
      loginItemSupported: true,
      expected: "Starts quietly at sign-in and stays ready in the tray while this computer is awake.",
    },
    {
      backgroundEnabled: true,
      loginItemSupported: false,
      expected: "Stays ready in the tray while this computer is awake. Start it again after signing in.",
    },
  ])("describes the real close behavior for enabled=$backgroundEnabled login=$loginItemSupported", (input) => {
    expect(backgroundAvailabilityDetail(input)).toBe(input.expected);
  });

  it("gives the background state a concise title and status", () => {
    expect(backgroundAvailabilityTitle({ backgroundEnabled: false, loginItemSupported: true }))
      .toBe("Start at sign-in");
    expect(backgroundAvailabilityTitle({ backgroundEnabled: true, loginItemSupported: true }))
      .toBe("Ready in the background");
    expect(backgroundAvailabilityStatus({ backgroundEnabled: false })).toBe("Window only");
    expect(backgroundAvailabilityStatus({ backgroundEnabled: true })).toBe("Background ready");
  });

  it("defaults the unchecked static controller to truthful copy before state hydration", async () => {
    const html = await readFile(join(browserRoot, "src", "renderer", "controller.html"), "utf8");
    expect(html).toContain("Turn on to stay ready in the tray while this computer is awake.");
    expect(html).toContain("Window only");
    expect(html).toMatch(/<input id="background-enabled"[^>]*>/);
    expect(html).not.toMatch(/<input id="background-enabled"[^>]*\bchecked\b/);
  });
});

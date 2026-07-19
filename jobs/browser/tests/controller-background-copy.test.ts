import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { backgroundAvailabilityDetail } from "../src/renderer/background-copy.js";

const browserRoot = join(import.meta.dirname, "..");

describe("Bluey Browser background availability copy", () => {
  it.each([
    {
      backgroundEnabled: false,
      loginItemSupported: true,
      expected: "Closing this window quits Bluey Browser. Turn this on to start quietly at sign-in and keep it available while this computer is awake.",
    },
    {
      backgroundEnabled: false,
      loginItemSupported: false,
      expected: "Closing this window quits Bluey Browser. Turn this on to keep it available while this computer is awake.",
    },
    {
      backgroundEnabled: true,
      loginItemSupported: true,
      expected: "Closing this window hides it to the tray. It starts quietly at sign-in and runs only while this computer is awake.",
    },
    {
      backgroundEnabled: true,
      loginItemSupported: false,
      expected: "Closing this window hides it to the tray. Start-at-sign-in is not managed on this system.",
    },
  ])("describes the real close behavior for enabled=$backgroundEnabled login=$loginItemSupported", (input) => {
    expect(backgroundAvailabilityDetail(input)).toBe(input.expected);
  });

  it("defaults the unchecked static controller to quit copy before state hydration", async () => {
    const html = await readFile(join(browserRoot, "src", "renderer", "controller.html"), "utf8");
    expect(html).toContain("Closing this window quits Bluey Browser.");
    expect(html).not.toContain("Closing this window then hides it to the tray.");
    expect(html).toMatch(/<input id="background-enabled"[^>]*>/);
    expect(html).not.toMatch(/<input id="background-enabled"[^>]*\bchecked\b/);
  });
});

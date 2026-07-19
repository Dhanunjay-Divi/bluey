import { describe, expect, it } from "vitest";
import {
  completedControllerState,
  failedUnknownControllerState,
  needsYouControllerState,
  readyControllerState,
  runningControllerState,
  safeDisplayText,
} from "../src/controller-state.js";

describe("Bluey Browser controller states", () => {
  it("uses truthful local background copy and a four-step progression", () => {
    const ready = readyControllerState({ backgroundEnabled: false, online: true });
    expect(ready.status).toBe("ready");
    expect(ready.modeLabel).toBe("Local");
    expect(ready.footnote).toBe("Runs quietly while this computer is awake.");
    expect(JSON.stringify(ready)).not.toMatch(/computer is off/i);

    const running = runningControllerState({
      currentRunCount: 1,
      progressStep: 1,
      backgroundEnabled: true,
      online: true,
    });
    expect(running.status).toBe("running");
    expect(running.progressStep).toBe(1);
    expect(running.activity).toHaveLength(4);
    expect(running.activity.map((item) => item.state)).toEqual(["done", "active", "pending", "pending"]);
  });

  it("gives user intervention and uncertain submit the highest safe priority", () => {
    const captcha = needsYouControllerState({
      currentRunCount: 1,
      interventionKind: "captcha",
      backgroundEnabled: true,
      online: true,
    });
    expect(captcha.status).toBe("needs_you");
    expect(captcha.title).toBe("CAPTCHA needs you");
    expect(captcha.primaryAction).toBe("continue");

    const unknown = failedUnknownControllerState({
      unknown: true,
      active: true,
      backgroundEnabled: true,
      online: true,
    });
    expect(unknown.status).toBe("failed_unknown");
    expect(unknown.detail).toMatch(/will not try again automatically/i);

    const locked = readyControllerState({ backgroundEnabled: true, online: true, paused: false });
    expect(locked.modeLabel).toBe("Local");
  });

  it("never carries resume, answer, email, or ticket data into display state", () => {
    const state = needsYouControllerState({
      currentRunCount: 1,
      interventionKind: "missing_fact",
      backgroundEnabled: false,
      online: true,
      context: {
        company: "<img src=x onerror=alert(1)>",
        role: "Engineer\u0000<script>steal()</script>",
        identityAvailable: true,
      },
    });
    const serialized = JSON.stringify(state);
    expect(state.company).toBe("<img src=x onerror=alert(1)>");
    expect(state.role).not.toContain("\u0000");
    expect(state.identityLabel).toBe("Verified application identity");
    expect(serialized).not.toMatch(/person@example|private answer|capability|ticket|resumePath/);
  });

  it("sanitizes controls and bounds display text without interpreting HTML", () => {
    expect(safeDisplayText("  hello\n\tworld  ", 20)).toBe("hello world");
    expect(safeDisplayText("a".repeat(400), 100)).toHaveLength(100);
    expect(completedControllerState({
      submitted: true,
      backgroundEnabled: false,
      online: true,
    }).progressStep).toBe(3);
  });
});

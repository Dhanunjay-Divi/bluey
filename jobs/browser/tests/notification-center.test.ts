import { describe, expect, it } from "vitest";
import { needsYouControllerState, readyControllerState } from "../src/controller-state.js";
import {
  ControllerNotificationCenter,
  notificationForStatus,
  type SafeNotification,
} from "../src/notification-center.js";

describe("controller notifications", () => {
  it("coalesces repeated states and emits only fixed PII-free copy", () => {
    const shown: SafeNotification[] = [];
    let now = 1_000;
    const center = new ControllerNotificationCenter({ show: (notice) => shown.push(notice) }, () => now, 30_000);
    const state = needsYouControllerState({
      currentRunCount: 1,
      interventionKind: "captcha",
      backgroundEnabled: true,
      online: true,
      context: { company: "Secret Corp", role: "person@example.test", identityAvailable: true },
    });

    expect(center.observe(state)).toBe(true);
    expect(center.observe(state)).toBe(false);
    center.observe(readyControllerState());
    now += 10_000;
    expect(center.observe(state)).toBe(false);
    center.observe(readyControllerState());
    now += 30_000;
    expect(center.observe(state)).toBe(true);

    expect(shown).toHaveLength(2);
    expect(JSON.stringify(shown)).not.toMatch(/Secret Corp|person@example|resume|answer/i);
  });

  it("does not notify for ready or running noise", () => {
    expect(notificationForStatus("ready")).toBeUndefined();
    expect(notificationForStatus("running")).toBeUndefined();
    expect(notificationForStatus("paused")?.body).toMatch(/No new local applications/i);
  });
});

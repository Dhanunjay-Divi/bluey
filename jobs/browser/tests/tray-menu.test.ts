import { describe, expect, it } from "vitest";
import {
  needsYouControllerState,
  pausedControllerState,
  readyControllerState,
  runningControllerState,
} from "../src/controller-state.js";
import { trayMenuView } from "../src/tray-menu.js";

describe("Bluey Browser tray menu", () => {
  it("shows an idle window-only controller without unavailable run actions", () => {
    const view = trayMenuView(readyControllerState());
    expect(view).toEqual({
      statusLabel: "Status: Ready",
      pauseLabel: "Pause applications",
      canPause: true,
      canOpenBrowser: false,
      backgroundEnabled: false,
    });
  });

  it("exposes the active application and persisted background state", () => {
    const view = trayMenuView(runningControllerState({
      currentRunCount: 1,
      backgroundEnabled: true,
      online: true,
    }));
    expect(view.statusLabel).toBe("Status: Running");
    expect(view.canOpenBrowser).toBe(true);
    expect(view.backgroundEnabled).toBe(true);
  });

  it("labels interventions and paused work clearly", () => {
    const intervention = trayMenuView(needsYouControllerState({
      currentRunCount: 1,
      backgroundEnabled: true,
      online: true,
    }));
    const paused = trayMenuView(pausedControllerState({
      backgroundEnabled: true,
      online: true,
      reason: "manual",
    }));
    expect(intervention.statusLabel).toBe("Status: Needs you");
    expect(paused.statusLabel).toBe("Status: Paused");
    expect(paused.pauseLabel).toBe("Resume applications");
  });
});

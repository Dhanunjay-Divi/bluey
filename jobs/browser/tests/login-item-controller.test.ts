import { describe, expect, it, vi } from "vitest";
import {
  BACKGROUND_LAUNCH_ARGUMENT,
  isBackgroundLoginLaunch,
  LoginItemController,
  shouldShowControllerOnReady,
} from "../src/login-item-controller.js";

describe("login item synchronization", () => {
  it("enables and disables launch-at-login only from explicit preference changes", async () => {
    const setLoginItemSettings = vi.fn();
    const controller = new LoginItemController({ setLoginItemSettings }, true);
    await controller.sync(false);
    await controller.sync(false);
    await controller.sync(true);
    await controller.sync(true);
    await controller.sync(false);
    expect(setLoginItemSettings.mock.calls).toEqual([
      [{ openAtLogin: false }],
      [{ openAtLogin: true }],
      [{ openAtLogin: false }],
    ]);
  });

  it("uses the same stable background argument when Windows enables or disables login start", async () => {
    const setLoginItemSettings = vi.fn();
    const controller = new LoginItemController(
      { setLoginItemSettings },
      true,
      [BACKGROUND_LAUNCH_ARGUMENT],
    );
    await controller.sync(true);
    await controller.sync(false);
    expect(setLoginItemSettings.mock.calls).toEqual([
      [{ openAtLogin: true, args: [BACKGROUND_LAUNCH_ARGUMENT] }],
      [{ openAtLogin: false, args: [BACKGROUND_LAUNCH_ARGUMENT] }],
    ]);
  });

  it("does not mutate OS login settings in an unpackaged development run", async () => {
    const setLoginItemSettings = vi.fn();
    const controller = new LoginItemController({ setLoginItemSettings }, false);
    await controller.sync(true);
    await controller.sync(false);
    expect(setLoginItemSettings).not.toHaveBeenCalled();
    expect(controller.isSupported).toBe(false);
  });

  it("does not rewrite a login item that already matches the persisted preference", async () => {
    const setLoginItemSettings = vi.fn();
    const controller = new LoginItemController(
      { setLoginItemSettings },
      true,
      [],
      false,
    );
    await controller.sync(false);
    expect(setLoginItemSettings).not.toHaveBeenCalled();
    await controller.sync(true);
    expect(setLoginItemSettings).toHaveBeenCalledWith({ openAtLogin: true });
  });

  it("distinguishes quiet login launches from visible manual launches by platform", () => {
    expect(isBackgroundLoginLaunch({
      platform: "darwin",
      argv: [],
      wasOpenedAtLogin: true,
    })).toBe(true);
    expect(isBackgroundLoginLaunch({
      platform: "darwin",
      argv: [BACKGROUND_LAUNCH_ARGUMENT],
      wasOpenedAtLogin: false,
    })).toBe(false);
    expect(isBackgroundLoginLaunch({
      platform: "win32",
      argv: ["Bluey Browser.exe", BACKGROUND_LAUNCH_ARGUMENT],
    })).toBe(true);
    expect(isBackgroundLoginLaunch({
      platform: "linux",
      argv: [BACKGROUND_LAUNCH_ARGUMENT],
    })).toBe(false);
  });

  it("starts hidden only for an enabled background preference launched by the OS", () => {
    expect(shouldShowControllerOnReady({
      backgroundEnabled: true,
      backgroundLoginLaunch: true,
    })).toBe(false);
    expect(shouldShowControllerOnReady({
      backgroundEnabled: false,
      backgroundLoginLaunch: true,
    })).toBe(true);
    expect(shouldShowControllerOnReady({
      backgroundEnabled: true,
      backgroundLoginLaunch: false,
    })).toBe(true);
  });
});

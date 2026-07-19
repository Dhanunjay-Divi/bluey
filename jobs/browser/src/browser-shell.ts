import { Notification } from "electron";
import { BrowserPreferenceStore } from "./background-preferences.js";
import { ControllerWindow } from "./controller-window.js";
import {
  readyControllerState,
  sanitizeControllerState,
  withShellFlags,
} from "./controller-state.js";
import type {
  ControllerCommand,
  ControllerViewState,
} from "./controller-contract.js";
import {
  ControllerNotificationCenter,
  type NotificationSink,
} from "./notification-center.js";
import { TrayController } from "./tray-controller.js";
import {
  LoginItemController,
  shouldShowControllerOnReady,
} from "./login-item-controller.js";

export interface BrowserShellCallbacks {
  onPrimary(action: ControllerViewState["primaryAction"]): void | Promise<void>;
  onOpenBrowser(): void | Promise<void>;
  onPauseChange(paused: boolean): void | Promise<void>;
  onStop(): void | Promise<void>;
  onOpenJobs(): void | Promise<void>;
  onQuit(): void | Promise<void>;
}

export class BrowserShell {
  private controllerWindow?: ControllerWindow;
  private tray?: TrayController;
  private preferenceStore?: BrowserPreferenceStore;
  private state = readyControllerState();
  private readonly notifications: ControllerNotificationCenter;
  private readonly loginItems?: LoginItemController;
  private readonly backgroundLoginLaunch: boolean;

  constructor(
    private readonly userDataDirectory: string,
    private readonly appRoot: string,
    private readonly callbacks: BrowserShellCallbacks,
    options: {
      notificationSink?: NotificationSink;
      loginItems?: LoginItemController;
      backgroundLoginLaunch?: boolean;
    } = {},
  ) {
    this.notifications = new ControllerNotificationCenter(
      options.notificationSink ?? new ElectronNotificationSink(() => this.show()),
    );
    this.loginItems = options.loginItems;
    this.backgroundLoginLaunch = Boolean(options.backgroundLoginLaunch);
  }

  async start(online: boolean): Promise<void> {
    this.preferenceStore = await BrowserPreferenceStore.open(this.userDataDirectory);
    let backgroundEnabled = this.preferenceStore.snapshot().backgroundEnabled;
    const requestedBackground = backgroundEnabled;
    try {
      await this.loginItems?.sync(backgroundEnabled);
    } catch {
      backgroundEnabled = false;
      if (requestedBackground) await this.loginItems?.sync(false).catch(() => undefined);
      await this.preferenceStore.setBackgroundEnabled(false);
    }
    this.state = sanitizeControllerState({
      ...readyControllerState({ backgroundEnabled, online }),
      loginItemSupported: this.loginItems?.isSupported ?? false,
    });
    this.controllerWindow = new ControllerWindow(
      this.state,
      {
        onCommand: (command) => this.handleCommand(command),
        onBackgroundEnabled: (enabled) => this.setBackgroundEnabled(enabled),
        onQuitRequest: () => this.callbacks.onQuit(),
      },
      this.appRoot,
      shouldShowControllerOnReady({
        backgroundEnabled,
        backgroundLoginLaunch: this.backgroundLoginLaunch,
      }),
    );
    this.tray = new TrayController(
      this.state,
      {
        onShow: () => this.show(),
        onTogglePause: () => this.callbacks.onPauseChange(!this.state.paused),
        onOpenJobs: () => this.callbacks.onOpenJobs(),
        onQuit: () => this.callbacks.onQuit(),
      },
      this.appRoot,
    );
    this.tray.create();
    await this.controllerWindow.create();
    this.publish(false);
  }

  update(state: ControllerViewState, notify = true): void {
    this.state = sanitizeControllerState({
      ...state,
      backgroundEnabled: this.backgroundEnabled,
      loginItemSupported: this.loginItems?.isSupported ?? false,
    });
    this.publish(notify);
  }

  updateConnectivity(online: boolean): void {
    if (this.state.online === online) return;
    this.state = withShellFlags(this.state, {
      backgroundEnabled: this.backgroundEnabled,
      online,
      paused: this.state.paused,
    });
    this.publish(false);
  }

  snapshot(): ControllerViewState {
    return structuredClone(this.state);
  }

  get backgroundEnabled(): boolean {
    return this.preferenceStore?.snapshot().backgroundEnabled ?? false;
  }

  get online(): boolean {
    return this.state.online;
  }

  get paused(): boolean {
    return this.state.paused;
  }

  show(): void {
    this.controllerWindow?.show();
  }

  hide(): void {
    this.controllerWindow?.hide();
  }

  beginQuit(): void {
    this.controllerWindow?.markQuitting();
  }

  dispose(): void {
    this.tray?.destroy();
    this.tray = undefined;
    this.controllerWindow?.dispose();
    this.controllerWindow = undefined;
  }

  private async setBackgroundEnabled(enabled: boolean): Promise<boolean> {
    const previous = this.backgroundEnabled;
    await this.loginItems?.sync(enabled);
    let saved;
    try {
      saved = await this.preferenceStore?.setBackgroundEnabled(enabled);
    } catch (error) {
      await this.loginItems?.sync(previous).catch(() => undefined);
      throw error;
    }
    if (!saved) return false;
    this.state = withShellFlags(this.state, {
      backgroundEnabled: saved.backgroundEnabled,
      online: this.state.online,
      paused: this.state.paused,
    });
    this.publish(false);
    return saved.backgroundEnabled;
  }

  private async handleCommand(command: ControllerCommand): Promise<void> {
    switch (command) {
      case "primary":
        await this.callbacks.onPrimary(this.state.primaryAction);
        break;
      case "open-browser":
        if (this.state.canOpenBrowser) await this.callbacks.onOpenBrowser();
        break;
      case "toggle-pause":
        if (this.state.canPause) await this.callbacks.onPauseChange(!this.state.paused);
        break;
      case "stop":
        if (this.state.canStop) await this.callbacks.onStop();
        break;
      case "open-jobs":
        await this.callbacks.onOpenJobs();
        break;
    }
  }

  private publish(notify: boolean): void {
    this.controllerWindow?.update(this.state);
    this.tray?.update(this.state);
    if (notify) this.notifications.observe(this.state);
  }
}

class ElectronNotificationSink implements NotificationSink {
  constructor(private readonly onClick: () => void = () => undefined) {}

  show(input: { title: string; body: string; urgency: "normal" | "critical" }): void {
    if (!Notification.isSupported()) return;
    const notification = new Notification({
      title: input.title,
      body: input.body,
      silent: input.urgency === "normal",
    });
    notification.on("click", this.onClick);
    notification.show();
  }
}

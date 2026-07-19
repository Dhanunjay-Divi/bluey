import {
  BrowserWindow,
  ipcMain,
  type IpcMainEvent,
  type IpcMainInvokeEvent,
} from "electron";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { closeDisposition } from "./background-preferences.js";
import {
  sanitizeControllerState,
} from "./controller-state.js";
import type {
  ControllerCommand,
  ControllerViewState,
} from "./controller-contract.js";

const IPC_GET_STATE = "bluey-browser:get-state";
const IPC_STATE = "bluey-browser:state";
const IPC_COMMAND = "bluey-browser:command";
const IPC_BACKGROUND = "bluey-browser:set-background";

const COMMANDS = new Set<ControllerCommand>([
  "primary",
  "open-browser",
  "toggle-pause",
  "stop",
  "open-jobs",
]);

export interface ControllerWindowCallbacks {
  onCommand(command: ControllerCommand): void | Promise<void>;
  onBackgroundEnabled(enabled: boolean): Promise<boolean>;
  onQuitRequest(): void | Promise<void>;
}

export class ControllerWindow {
  private window: BrowserWindow | null = null;
  private quitting = false;
  private registeredIpc = false;

  constructor(
    private state: ControllerViewState,
    private readonly callbacks: ControllerWindowCallbacks,
    private readonly appRoot: string,
    private readonly showOnReady = true,
  ) {}

  async create(): Promise<void> {
    if (this.window && !this.window.isDestroyed()) return;
    this.registerIpc();
    const moduleDirectory = dirname(fileURLToPath(import.meta.url));
    const preload = join(moduleDirectory, "controller-preload.cjs");
    const controllerHtml = join(moduleDirectory, "renderer", "controller.html");
    const icon = join(this.appRoot, "assets", "icon-512.png");
    const window = new BrowserWindow({
      width: 680,
      height: 720,
      minWidth: 480,
      minHeight: 560,
      show: false,
      title: "Bluey Browser",
      backgroundColor: "#06121a",
      icon,
      autoHideMenuBar: true,
      webPreferences: {
        preload,
        contextIsolation: true,
        nodeIntegration: false,
        nodeIntegrationInWorker: false,
        nodeIntegrationInSubFrames: false,
        sandbox: true,
        webSecurity: true,
        allowRunningInsecureContent: false,
        spellcheck: false,
      },
    });
    this.window = window;
    window.webContents.session.setPermissionRequestHandler((_webContents, _permission, callback) => {
      callback(false);
    });
    window.webContents.on("will-attach-webview", (event) => event.preventDefault());
    window.webContents.on("will-navigate", (event) => event.preventDefault());
    window.webContents.setWindowOpenHandler(() => ({ action: "deny" }));
    window.webContents.on("did-finish-load", () => this.publish());
    window.on("close", (event) => {
      const disposition = closeDisposition({
        backgroundEnabled: this.state.backgroundEnabled,
        quitting: this.quitting,
      });
      if (disposition === "close") return;
      event.preventDefault();
      if (disposition === "hide") {
        window.hide();
        return;
      }
      void this.callbacks.onQuitRequest();
    });
    window.on("closed", () => {
      if (this.window === window) this.window = null;
    });
    window.once("ready-to-show", () => {
      if (this.showOnReady) this.show();
    });
    await window.loadFile(controllerHtml);
  }

  update(state: ControllerViewState): void {
    this.state = sanitizeControllerState(state);
    this.publish();
  }

  snapshot(): ControllerViewState {
    return structuredClone(this.state);
  }

  show(): void {
    const window = this.window;
    if (!window || window.isDestroyed()) return;
    if (window.isMinimized()) window.restore();
    window.show();
    window.focus();
  }

  hide(): void {
    if (this.window && !this.window.isDestroyed()) this.window.hide();
  }

  markQuitting(): void {
    this.quitting = true;
  }

  closeForQuit(): void {
    this.markQuitting();
    if (this.window && !this.window.isDestroyed()) this.window.close();
  }

  dispose(): void {
    this.markQuitting();
    if (this.window && !this.window.isDestroyed()) {
      this.window.webContents.session.setPermissionRequestHandler(null);
    }
    this.closeForQuit();
    if (this.registeredIpc) {
      ipcMain.removeHandler(IPC_GET_STATE);
      ipcMain.removeHandler(IPC_BACKGROUND);
      ipcMain.removeListener(IPC_COMMAND, this.commandListener);
      this.registeredIpc = false;
    }
  }

  private registerIpc(): void {
    if (this.registeredIpc) return;
    ipcMain.handle(IPC_GET_STATE, (event) => {
      this.assertTrustedSender(event);
      return this.snapshot();
    });
    ipcMain.handle(IPC_BACKGROUND, async (event, enabled: unknown) => {
      this.assertTrustedSender(event);
      if (typeof enabled !== "boolean") throw new Error("Invalid background preference");
      return this.callbacks.onBackgroundEnabled(enabled);
    });
    ipcMain.on(IPC_COMMAND, this.commandListener);
    this.registeredIpc = true;
  }

  private readonly commandListener = (event: IpcMainEvent, command: unknown): void => {
    this.assertTrustedSender(event);
    if (!COMMANDS.has(command as ControllerCommand)) return;
    void this.callbacks.onCommand(command as ControllerCommand);
  };

  private assertTrustedSender(event: IpcMainEvent | IpcMainInvokeEvent): void {
    const window = this.window;
    if (!window || window.isDestroyed() || event.sender !== window.webContents) {
      throw new Error("Untrusted Bluey Browser controller sender");
    }
  }

  private publish(): void {
    const window = this.window;
    if (!window || window.isDestroyed() || window.webContents.isLoading()) return;
    window.webContents.send(IPC_STATE, this.snapshot());
  }
}

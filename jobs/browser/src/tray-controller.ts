import { Menu, Tray, nativeImage, type NativeImage } from "electron";
import { join } from "node:path";
import type { ControllerViewState } from "./controller-contract.js";

export interface TrayControllerCallbacks {
  onShow(): void;
  onTogglePause(): void | Promise<void>;
  onOpenJobs(): void | Promise<void>;
  onQuit(): void | Promise<void>;
}

export class TrayController {
  private tray?: Tray;

  constructor(
    private state: ControllerViewState,
    private readonly callbacks: TrayControllerCallbacks,
    private readonly appRoot: string,
  ) {}

  create(): void {
    if (this.tray) return;
    const tray = new Tray(this.icon());
    tray.setToolTip("Bluey Browser");
    tray.on("click", () => this.callbacks.onShow());
    tray.on("double-click", () => this.callbacks.onShow());
    this.tray = tray;
    this.rebuildMenu();
  }

  update(state: ControllerViewState): void {
    this.state = state;
    this.rebuildMenu();
  }

  destroy(): void {
    this.tray?.destroy();
    this.tray = undefined;
  }

  private icon(): NativeImage {
    const filename = process.platform === "darwin"
      ? "trayTemplate.png"
      : process.platform === "win32"
        ? "icon-32.png"
        : "icon-32.png";
    const image = nativeImage.createFromPath(join(this.appRoot, "assets", filename));
    if (process.platform === "darwin") image.setTemplateImage(true);
    return image;
  }

  private rebuildMenu(): void {
    if (!this.tray) return;
    const statusLabel = `Status: ${statusText(this.state)}`;
    const backgroundLabel = this.state.backgroundEnabled
      ? "Background: On (computer awake)"
      : "Background: Off";
    const menu = Menu.buildFromTemplate([
      { label: "Show Bluey Browser", click: () => this.callbacks.onShow() },
      {
        label: this.state.paused ? "Resume applications" : "Pause applications",
        enabled: this.state.canPause,
        click: () => void this.callbacks.onTogglePause(),
      },
      { label: "Open Bluey Jobs", click: () => void this.callbacks.onOpenJobs() },
      { type: "separator" },
      { label: statusLabel, enabled: false },
      { label: backgroundLabel, enabled: false },
      { type: "separator" },
      { label: "Quit Bluey Browser", click: () => void this.callbacks.onQuit() },
    ]);
    this.tray.setContextMenu(menu);
  }
}

function statusText(state: ControllerViewState): string {
  switch (state.status) {
    case "needs_you": return "Needs you";
    case "failed_unknown": return "Needs review";
    case "running": return state.paused ? "Finishing protected step" : "Running";
    case "completed": return "Completed";
    case "paused": return "Paused";
    default: return state.online ? "Ready" : "Offline";
  }
}

import { Menu, Tray, nativeImage, type NativeImage } from "electron";
import { join } from "node:path";
import type { ControllerViewState } from "./controller-contract.js";
import { statusText, trayMenuView } from "./tray-menu.js";

export interface TrayControllerCallbacks {
  onShow(): void;
  onTogglePause(): void | Promise<void>;
  onOpenBrowser(): void | Promise<void>;
  onBackgroundChange(enabled: boolean): void | Promise<void>;
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
    this.tray?.setToolTip(`Bluey Browser · ${statusText(state)}`);
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
    const view = trayMenuView(this.state);
    const menu = Menu.buildFromTemplate([
      { label: view.statusLabel, enabled: false },
      { type: "separator" },
      { label: "Show controller", click: () => this.callbacks.onShow() },
      {
        label: "Open current application",
        enabled: view.canOpenBrowser,
        click: () => void this.callbacks.onOpenBrowser(),
      },
      {
        label: view.pauseLabel,
        enabled: view.canPause,
        click: () => void this.callbacks.onTogglePause(),
      },
      {
        type: "checkbox",
        label: "Keep ready in background",
        checked: view.backgroundEnabled,
        click: (item) => void this.callbacks.onBackgroundChange(item.checked),
      },
      { label: "Open Bluey Jobs", click: () => void this.callbacks.onOpenJobs() },
      { type: "separator" },
      { label: "Quit Bluey Browser", click: () => void this.callbacks.onQuit() },
    ]);
    this.tray.setContextMenu(menu);
  }
}

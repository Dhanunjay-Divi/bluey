export interface LoginItemSettings {
  openAtLogin: boolean;
  args?: string[];
}

export interface LoginItemAdapter {
  setLoginItemSettings(settings: LoginItemSettings): void | Promise<void>;
}

export class LoginItemController {
  private lastEnabled?: boolean;

  constructor(
    private readonly adapter: LoginItemAdapter,
    private readonly supported: boolean,
    private readonly launchArguments: readonly string[] = [],
    initialEnabled?: boolean,
  ) {
    this.lastEnabled = initialEnabled;
  }

  async sync(enabled: boolean): Promise<void> {
    const next = Boolean(enabled);
    if (!this.supported || this.lastEnabled === next) return;
    await this.adapter.setLoginItemSettings({
      openAtLogin: next,
      ...(this.launchArguments.length > 0 ? { args: [...this.launchArguments] } : {}),
    });
    this.lastEnabled = next;
  }

  get isSupported(): boolean {
    return this.supported;
  }
}

export const BACKGROUND_LAUNCH_ARGUMENT = "--bluey-background";

export function isBackgroundLoginLaunch(input: {
  platform: NodeJS.Platform;
  argv: readonly string[];
  wasOpenedAtLogin?: boolean;
}): boolean {
  if (input.platform === "darwin") return input.wasOpenedAtLogin === true;
  if (input.platform === "win32") return input.argv.includes(BACKGROUND_LAUNCH_ARGUMENT);
  return false;
}

export function shouldShowControllerOnReady(input: {
  backgroundEnabled: boolean;
  backgroundLoginLaunch: boolean;
}): boolean {
  return !input.backgroundEnabled || !input.backgroundLoginLaunch;
}

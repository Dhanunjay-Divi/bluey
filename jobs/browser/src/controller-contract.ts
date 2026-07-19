export type ControllerStatus =
  | "ready"
  | "running"
  | "needs_you"
  | "paused"
  | "completed"
  | "failed_unknown";

export type ControllerMode = "local";

export type ControllerPrimaryAction =
  | "none"
  | "open_jobs"
  | "open_browser"
  | "continue"
  | "resume";

export type ControllerCommand =
  | "primary"
  | "open-browser"
  | "toggle-pause"
  | "stop"
  | "open-jobs";

export interface ControllerActivityItem {
  label: string;
  state: "pending" | "active" | "done" | "attention";
}

export interface ControllerViewState {
  version: 1;
  status: ControllerStatus;
  mode: ControllerMode;
  modeLabel: string;
  title: string;
  detail: string;
  footnote: string;
  company?: string;
  role?: string;
  identityLabel?: string;
  currentRunCount: number;
  progressStep: 0 | 1 | 2 | 3;
  primaryAction: ControllerPrimaryAction;
  primaryLabel?: string;
  canOpenBrowser: boolean;
  canPause: boolean;
  paused: boolean;
  canStop: boolean;
  backgroundEnabled: boolean;
  loginItemSupported: boolean;
  online: boolean;
  activity: ControllerActivityItem[];
}

export interface BlueyBrowserBridge {
  getState(): Promise<ControllerViewState>;
  command(command: ControllerCommand): void;
  setBackgroundEnabled(enabled: boolean): Promise<boolean>;
  onState(listener: (state: ControllerViewState) => void): () => void;
}

declare global {
  interface Window {
    blueyBrowser: BlueyBrowserBridge;
  }
}

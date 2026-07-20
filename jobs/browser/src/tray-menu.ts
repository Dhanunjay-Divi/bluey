import type { ControllerViewState } from "./controller-contract.js";

export interface TrayMenuView {
  statusLabel: string;
  pauseLabel: string;
  canPause: boolean;
  canOpenBrowser: boolean;
  backgroundEnabled: boolean;
}

export function trayMenuView(state: ControllerViewState): TrayMenuView {
  return {
    statusLabel: `Status: ${statusText(state)}`,
    pauseLabel: state.paused ? "Resume applications" : "Pause applications",
    canPause: state.canPause,
    canOpenBrowser: state.canOpenBrowser,
    backgroundEnabled: state.backgroundEnabled,
  };
}

export function statusText(state: ControllerViewState): string {
  switch (state.status) {
    case "needs_you": return "Needs you";
    case "failed_unknown": return "Needs review";
    case "running": return state.paused ? "Finishing protected step" : "Running";
    case "completed": return "Completed";
    case "paused": return "Paused";
    default: return state.online ? "Ready" : "Offline";
  }
}

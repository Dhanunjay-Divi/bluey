import type { BrowserShell } from "./browser-shell.js";
import {
  completedControllerState,
  failedUnknownControllerState,
  needsYouControllerState,
  pausedControllerState,
  readyControllerState,
  runningControllerState,
  type SafeJobContext,
} from "./controller-state.js";

export interface RunDisplay {
  runId: string;
  context?: SafeJobContext;
  interventionKind?: string;
}

export class RunControllerView {
  private focusedRunId?: string;

  constructor(
    private readonly shell: BrowserShell,
    private readonly activeRuns: () => RunDisplay[],
  ) {}

  showReady(paused: boolean): void {
    this.show(readyControllerState({
      backgroundEnabled: this.shell.backgroundEnabled,
      online: this.shell.online,
      paused,
    }));
  }

  showRunning(
    run: RunDisplay,
    currentRunCount: number,
    progressStep: 0 | 1 | 2,
    paused: boolean,
    detail?: string,
  ): void {
    this.focusedRunId = run.runId;
    this.show(runningControllerState({
      context: run.context,
      currentRunCount,
      progressStep,
      backgroundEnabled: this.shell.backgroundEnabled,
      online: this.shell.online,
      paused,
      detail,
    }));
  }

  showNeedsYou(run: RunDisplay, currentRunCount: number, recovered = false): void {
    this.focusedRunId = run.runId;
    this.show(needsYouControllerState({
      context: run.context,
      currentRunCount,
      interventionKind: run.interventionKind,
      backgroundEnabled: this.shell.backgroundEnabled,
      online: this.shell.online,
      recovered,
    }));
  }

  showPaused(
    reason: "manual" | "offline" | "unavailable" | "stop_requested" | "device_unavailable",
    run?: RunDisplay,
  ): void {
    if (run) this.focusedRunId = run.runId;
    this.show(pausedControllerState({
      backgroundEnabled: this.shell.backgroundEnabled,
      online: this.shell.online,
      reason,
      currentRunCount: this.activeRuns().length,
      context: run?.context ?? this.current()?.context,
    }));
  }

  showCompleted(run: RunDisplay, submitted: boolean, paused: boolean): void {
    this.clearFocus(run.runId);
    this.show(completedControllerState({
      submitted,
      backgroundEnabled: this.shell.backgroundEnabled,
      online: this.shell.online,
      paused,
      context: run.context,
    }));
  }

  showFailure(run: RunDisplay | undefined, unknown: boolean, active: boolean): void {
    if (run) this.focusedRunId = run.runId;
    this.show(failedUnknownControllerState({
      unknown,
      active,
      backgroundEnabled: this.shell.backgroundEnabled,
      online: this.shell.online,
      context: run?.context ?? this.current()?.context,
    }));
  }

  showPreparing(detail: string): void {
    this.show(runningControllerState({
      currentRunCount: Math.max(1, this.activeRuns().length),
      progressStep: 0,
      backgroundEnabled: this.shell.backgroundEnabled,
      online: this.shell.online,
      detail,
    }));
  }

  current(): RunDisplay | undefined {
    const active = this.activeRuns();
    return active.find((run) => run.runId === this.focusedRunId) ?? active[0];
  }

  clearFocus(runId: string): void {
    if (this.focusedRunId === runId) this.focusedRunId = undefined;
  }

  private show(state: Parameters<BrowserShell["update"]>[0]): void {
    this.shell.update(state);
  }
}

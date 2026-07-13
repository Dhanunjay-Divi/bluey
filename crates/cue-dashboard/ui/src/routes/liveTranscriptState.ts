export type AudioCaptureState =
  | "idle"
  | "planning"
  | "starting"
  | "capturing"
  | "paused"
  | "stopping"
  | "stopped"
  | "failed";

export type AudioSourceState =
  | "disabled"
  | "awaiting_device"
  | "ready"
  | "capturing"
  | "muted"
  | "stopped"
  | "failed";

interface AudioSourceStatusView {
  state?: AudioSourceState;
  last_error?: string | null;
}

interface AudioCaptureStatusView {
  state: AudioCaptureState;
  permission_denied_source?: "system" | "microphone" | null;
  last_error?: string | null;
  system?: AudioSourceStatusView;
  microphone?: AudioSourceStatusView;
}

/** The subset of cue-core's AudioPipelineStatus rendered by Live Transcript. */
export interface AudioPipelineStatusView {
  session_id: string | null;
  config?: {
    system?: { enabled?: boolean };
    microphone?: { enabled?: boolean };
  };
  capture: AudioCaptureStatusView;
  note?: string | null;
  updated_at?: string;
}

export type PendingListeningAction = "start" | "stop" | null;

export interface ListeningViewState {
  active: boolean;
  busy: boolean;
  buttonLabel: string;
  statusLabel: string;
}

export interface ListeningSourceSummary {
  coverage: "both" | "system_only" | "microphone_only" | "none";
  label: string;
  degraded: boolean;
  error: string | null;
}

function nonEmpty(value: string | null | undefined): string | null {
  const trimmed = value?.trim();
  return trimmed ? trimmed : null;
}

export function isListening(status: AudioPipelineStatusView | null): boolean {
  if (!status) return false;
  if (["starting", "capturing", "paused", "stopping"].includes(status.capture.state)) {
    return true;
  }
  return Boolean(
    status.session_id && status.capture.state !== "stopped" && status.capture.state !== "failed",
  );
}

function parsedUpdatedAt(status: AudioPipelineStatusView | null): number | null {
  const value = nonEmpty(status?.updated_at);
  if (!value) return null;
  const epochMs = Number(value);
  return Number.isFinite(epochMs) ? epochMs : null;
}

/** Reject only snapshots that are provably older than the current status. */
export function shouldAcceptAudioStatus(
  current: AudioPipelineStatusView | null,
  candidate: AudioPipelineStatusView,
): boolean {
  const currentUpdatedAt = parsedUpdatedAt(current);
  const candidateUpdatedAt = parsedUpdatedAt(candidate);
  if (currentUpdatedAt === null || candidateUpdatedAt === null) return true;
  return candidateUpdatedAt >= currentUpdatedAt;
}

export function listeningViewState(
  status: AudioPipelineStatusView | null,
  pendingAction: PendingListeningAction,
  statusLoading: boolean,
  hasError = false,
): ListeningViewState {
  const active = isListening(status);

  if (pendingAction === "start") {
    return {
      active,
      busy: true,
      buttonLabel: "Starting...",
      statusLabel: "Starting audio",
    };
  }

  if (pendingAction === "stop") {
    return {
      active: true,
      busy: true,
      buttonLabel: "Stopping...",
      statusLabel: "Stopping audio",
    };
  }

  if (statusLoading && !status) {
    return {
      active: false,
      busy: true,
      buttonLabel: "Checking...",
      statusLabel: "Checking audio",
    };
  }

  if (status?.capture.state === "stopping") {
    return {
      active: true,
      busy: true,
      buttonLabel: "Stopping...",
      statusLabel: "Stopping audio",
    };
  }

  if (status?.capture.state === "planning" || status?.capture.state === "starting") {
    return {
      active: true,
      busy: false,
      buttonLabel: "Stop Listening",
      statusLabel: "Starting audio",
    };
  }

  if (active) {
    return {
      active: true,
      busy: false,
      buttonLabel: "Stop Listening",
      statusLabel: hasError
        ? "Listening needs attention"
        : status?.capture.state === "paused"
          ? "Listening paused"
          : "Listening",
    };
  }

  if (status?.capture.state === "failed" || hasError) {
    return {
      active: false,
      busy: false,
      buttonLabel: "Retry Listening",
      statusLabel: "Listening needs attention",
    };
  }

  return {
    active: false,
    busy: false,
    buttonLabel: "Start Listening",
    statusLabel: "Not listening",
  };
}

function sourceIsHealthy(source: AudioSourceStatusView | undefined): boolean {
  if (nonEmpty(source?.last_error)) return false;
  return !source?.state || !["disabled", "stopped", "failed"].includes(source.state);
}

export function listeningSourceSummary(
  status: AudioPipelineStatusView | null,
): ListeningSourceSummary {
  if (!isListening(status)) {
    return {
      coverage: "both",
      label: "System audio and microphone",
      degraded: false,
      error: null,
    };
  }

  const systemActive = status?.config?.system?.enabled === true && sourceIsHealthy(status.capture.system);
  const microphoneActive =
    status?.config?.microphone?.enabled === true && sourceIsHealthy(status.capture.microphone);

  if (systemActive && microphoneActive) {
    return {
      coverage: "both",
      label: "System audio and microphone",
      degraded: false,
      error: null,
    };
  }

  if (systemActive) {
    return {
      coverage: "system_only",
      label: "System audio only",
      degraded: true,
      error:
        nonEmpty(status?.capture.microphone?.last_error) ??
        "Microphone capture is unavailable. Check audio settings and try again.",
    };
  }

  if (microphoneActive) {
    return {
      coverage: "microphone_only",
      label: "Microphone only",
      degraded: true,
      error:
        nonEmpty(status?.capture.system?.last_error) ??
        "System audio capture is unavailable. Check permissions and try again.",
    };
  }

  return {
    coverage: "none",
    label: "No active audio source",
    degraded: true,
    error:
      nonEmpty(status?.capture.system?.last_error) ??
      nonEmpty(status?.capture.microphone?.last_error) ??
      "No audio source is available. Check permissions and audio settings, then try again.",
  };
}

export function listeningError(
  status: AudioPipelineStatusView | null,
  boundaryError: string | null,
): string | null {
  const direct = nonEmpty(boundaryError);
  if (direct) return direct;
  if (!status) return null;

  const sourceSummary = listeningSourceSummary(status);
  if (sourceSummary.degraded && sourceSummary.error) return sourceSummary.error;

  const captureError = nonEmpty(status.capture.last_error);
  if (captureError) return captureError;

  const systemError = nonEmpty(status.capture.system?.last_error);
  if (systemError) return systemError;

  const microphoneError = nonEmpty(status.capture.microphone?.last_error);
  if (microphoneError) return microphoneError;

  if (status.capture.state === "failed" || status.capture.permission_denied_source) {
    return nonEmpty(status.note) ?? "Audio capture could not start. Check access and try again.";
  }

  return null;
}

export function errorMessage(error: unknown): string {
  if (error instanceof Error) return nonEmpty(error.message) ?? "Unable to update listening.";
  if (typeof error === "string") return nonEmpty(error) ?? "Unable to update listening.";
  if (error && typeof error === "object" && "message" in error) {
    const message = error.message;
    if (typeof message === "string") return nonEmpty(message) ?? "Unable to update listening.";
  }
  return "Unable to update listening.";
}

export function listeningShortcutLabel(isMac: boolean): string {
  return isMac ? "Control+Option+L" : "Ctrl+Alt+L";
}

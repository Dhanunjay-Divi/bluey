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

/** The subset of cue-core's AudioPipelineStatus that this screen renders. */
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
  pending: boolean;
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
  if (!status?.session_id) return false;
  return status.capture.state !== "stopped" && status.capture.state !== "failed";
}

function parsedUpdatedAt(status: AudioPipelineStatusView | null): number | null {
  const value = nonEmpty(status?.updated_at);
  if (!value) return null;
  const epochMs = Number(value);
  return Number.isFinite(epochMs) ? epochMs : null;
}

/** Prevent a delayed poll from replacing a newer event or command response. */
export function shouldAcceptAudioStatus(
  current: AudioPipelineStatusView | null,
  candidate: AudioPipelineStatusView,
): boolean {
  const currentUpdatedAt = parsedUpdatedAt(current);
  const candidateUpdatedAt = parsedUpdatedAt(candidate);

  // Partial or defensive payloads should still be usable. The normal Rust
  // contract always sends an epoch-ms string, so only reject a provably older
  // snapshot when both timestamps can be compared.
  if (currentUpdatedAt === null || candidateUpdatedAt === null) return true;
  return candidateUpdatedAt >= currentUpdatedAt;
}

export function listeningViewState(
  status: AudioPipelineStatusView | null,
  pendingAction: PendingListeningAction,
  hasError = false,
): ListeningViewState {
  const active = isListening(status);
  const captureState = status?.capture.state;
  const starting =
    pendingAction === "start" ||
    captureState === "starting" ||
    (captureState === "planning" && active);
  const stopping = pendingAction === "stop" || captureState === "stopping";

  if (starting) {
    return {
      active,
      pending: true,
      buttonLabel: "Starting…",
      statusLabel: "Starting audio…",
    };
  }

  if (stopping) {
    return {
      active,
      pending: true,
      buttonLabel: "Stopping…",
      statusLabel: "Stopping audio…",
    };
  }

  if (active && hasError) {
    return {
      active: true,
      pending: false,
      buttonLabel: "Stop Listening",
      statusLabel: "Listening needs attention",
    };
  }

  if (active) {
    return {
      active: true,
      pending: false,
      buttonLabel: "Stop Listening",
      statusLabel: captureState === "paused" ? "Listening paused" : "Listening",
    };
  }

  if (captureState === "failed" || hasError) {
    return {
      active: false,
      pending: false,
      buttonLabel: "Retry Listening",
      statusLabel: "Listening needs attention",
    };
  }

  return {
    active: false,
    pending: false,
    buttonLabel: "Start Listening",
    statusLabel: "Not listening",
  };
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

  const systemEnabled = status?.config?.system?.enabled === true;
  const microphoneEnabled = status?.config?.microphone?.enabled === true;
  const systemFailed =
    status?.capture.system?.state === "failed" ||
    nonEmpty(status?.capture.system?.last_error) !== null;
  const microphoneFailed =
    status?.capture.microphone?.state === "failed" ||
    nonEmpty(status?.capture.microphone?.last_error) !== null;

  if (systemEnabled && microphoneEnabled && !systemFailed && !microphoneFailed) {
    return {
      coverage: "both",
      label: "System audio and microphone",
      degraded: false,
      error: null,
    };
  }

  if (systemEnabled && !systemFailed && (!microphoneEnabled || microphoneFailed)) {
    return {
      coverage: "system_only",
      label: "System audio only",
      degraded: true,
      error:
        nonEmpty(status?.capture.microphone?.last_error) ??
        "Microphone capture is unavailable. Stop listening, fix the microphone, and retry.",
    };
  }

  if (microphoneEnabled && !microphoneFailed && (!systemEnabled || systemFailed)) {
    return {
      coverage: "microphone_only",
      label: "Microphone only",
      degraded: true,
      error:
        nonEmpty(status?.capture.system?.last_error) ??
        "System-audio capture is unavailable. Stop listening, fix Screen Recording access, and retry.",
    };
  }

  return {
    coverage: "none",
    label: "No active audio source",
    degraded: true,
    error:
      nonEmpty(status?.capture.system?.last_error) ??
      nonEmpty(status?.capture.microphone?.last_error) ??
      "Both system audio and microphone are unavailable. Stop listening, fix permissions, and retry.",
  };
}

/**
 * Prefer errors emitted by the command/event boundary, then the most specific
 * error recorded by the audio pipeline. A note is an error only for failed or
 * permission-denied states because active-pipeline notes can be informational.
 */
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
    return nonEmpty(status.note) ?? "Audio capture could not start. Check permissions and sign-in, then retry.";
  }

  return null;
}

export function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  if (error && typeof error === "object") {
    const message = "message" in error ? error.message : undefined;
    if (typeof message === "string") return message;
    const nested = "error" in error ? error.error : undefined;
    if (typeof nested === "string") return nested;
  }
  try {
    return JSON.stringify(error) ?? String(error);
  } catch {
    return String(error);
  }
}

export function listeningShortcutLabel(isMac: boolean): string {
  return isMac ? "Control+Option+L" : "Ctrl+Alt+L";
}

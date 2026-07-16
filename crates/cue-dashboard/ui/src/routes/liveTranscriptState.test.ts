import { describe, expect, it } from "vitest";
import {
  type AudioCaptureState,
  type AudioPipelineStatusView,
  isListening,
  listeningError,
  listeningShortcutLabel,
  listeningSourceSummary,
  listeningViewState,
  shouldAcceptAudioStatus,
} from "./liveTranscriptState";
import { shouldRunLivePollers, transcriptHistoryView } from "./LiveTranscript";

function status(
  state: AudioCaptureState,
  session_id: string | null = null,
  extras: Partial<AudioPipelineStatusView> = {},
): AudioPipelineStatusView {
  return {
    session_id,
    capture: { state },
    ...extras,
  };
}

describe("live transcript listening state", () => {
  it("renders explicit loading, error, empty, and populated history states", () => {
    expect(transcriptHistoryView("loading", 0)).toBe("loading");
    expect(transcriptHistoryView("error", 0)).toBe("error");
    expect(transcriptHistoryView("ready", 0)).toBe("empty");
    expect(transcriptHistoryView("loading", 2)).toBe("content");
    expect(transcriptHistoryView("error", 2)).toBe("content");
  });

  it("runs live pollers only for an available signed-in owner", () => {
    expect(
      shouldRunLivePollers({ owner_key: "account:a", signed_in: true, available: true }),
    ).toBe(true);
    expect(
      shouldRunLivePollers({ owner_key: "local", signed_in: false, available: true }),
    ).toBe(false);
    expect(
      shouldRunLivePollers({ owner_key: "unavailable", signed_in: false, available: false }),
    ).toBe(false);
    expect(shouldRunLivePollers(null)).toBe(false);
  });

  it("distinguishes initial loading from an idle pipeline", () => {
    expect(listeningViewState(null, null, true)).toEqual({
      active: false,
      busy: true,
      buttonLabel: "Checking...",
      statusLabel: "Checking audio",
    });
    expect(listeningViewState(status("idle"), null, false)).toEqual({
      active: false,
      busy: false,
      buttonLabel: "Start Listening",
      statusLabel: "Not listening",
    });
  });

  it("shows starting, active, stopping, and failed capture truthfully", () => {
    expect(isListening(status("planning"))).toBe(false);
    expect(isListening(status("planning", "session-1"))).toBe(true);
    expect(isListening(status("starting"))).toBe(true);
    expect(listeningViewState(status("starting"), null, false)).toEqual({
      active: true,
      busy: false,
      buttonLabel: "Stop Listening",
      statusLabel: "Starting audio",
    });
    expect(listeningViewState(status("capturing", "session-1"), null, false)).toMatchObject({
      active: true,
      busy: false,
      buttonLabel: "Stop Listening",
      statusLabel: "Listening",
    });
    expect(listeningViewState(status("stopping"), null, false)).toMatchObject({
      active: true,
      busy: true,
      buttonLabel: "Stopping...",
    });
    expect(listeningViewState(status("failed", "session-1"), null, false, true)).toMatchObject({
      active: false,
      busy: false,
      buttonLabel: "Retry Listening",
      statusLabel: "Listening needs attention",
    });
  });

  it("uses local pending actions for immediate loading labels", () => {
    expect(listeningViewState(status("idle"), "start", false).buttonLabel).toBe("Starting...");
    expect(
      listeningViewState(status("capturing", "session-1"), "stop", false).buttonLabel,
    ).toBe("Stopping...");
  });

  it("rejects only provably older status snapshots", () => {
    const current = status("capturing", "session-1", { updated_at: "2000" });
    expect(
      shouldAcceptAudioStatus(current, status("stopped", null, { updated_at: "1999" })),
    ).toBe(false);
    expect(
      shouldAcceptAudioStatus(current, status("stopped", null, { updated_at: "2000" })),
    ).toBe(true);
    expect(shouldAcceptAudioStatus(current, status("stopped"))).toBe(true);
  });

  it("reports actual source coverage instead of assuming dual capture", () => {
    const both = status("capturing", "session-1", {
      config: {
        system: { enabled: true },
        microphone: { enabled: true },
      },
      capture: {
        state: "capturing",
        system: { state: "capturing" },
        microphone: { state: "capturing" },
      },
    });
    expect(listeningSourceSummary(both)).toEqual({
      coverage: "both",
      label: "System audio and microphone",
      degraded: false,
      error: null,
    });

    const microphoneOnly = status("capturing", "session-1", {
      config: {
        system: { enabled: false },
        microphone: { enabled: true },
      },
      capture: {
        state: "capturing",
        system: { state: "disabled" },
        microphone: { state: "capturing" },
      },
    });
    expect(listeningSourceSummary(microphoneOnly)).toMatchObject({
      coverage: "microphone_only",
      label: "Microphone only",
      degraded: true,
    });
  });

  it("prioritizes command errors and surfaces a failed source", () => {
    const failedSystem = status("capturing", "session-1", {
      config: {
        system: { enabled: true },
        microphone: { enabled: true },
      },
      capture: {
        state: "capturing",
        system: { state: "failed", last_error: "System audio is unavailable." },
        microphone: { state: "capturing" },
      },
    });
    expect(listeningError(failedSystem, null)).toBe("System audio is unavailable.");
    expect(listeningError(failedSystem, "Bluey's local audio service is unavailable.")).toBe(
      "Bluey's local audio service is unavailable.",
    );
  });

  it("shows the shortcut that the desktop actually registers", () => {
    expect(listeningShortcutLabel(true)).toBe("Control+Option+L");
    expect(listeningShortcutLabel(false)).toBe("Ctrl+Alt+L");
  });
});

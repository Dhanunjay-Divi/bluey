import { describe, expect, it } from "vitest";
import {
  type AudioCaptureState,
  type AudioPipelineStatusView,
  isListening,
  listeningError,
  listeningSourceSummary,
  listeningShortcutLabel,
  listeningViewState,
  shouldAcceptAudioStatus,
} from "./liveTranscriptState";

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
  it("renders an inactive pipeline as ready to start", () => {
    expect(isListening(status("idle"))).toBe(false);
    expect(listeningViewState(status("planning"), null)).toEqual({
      active: false,
      pending: false,
      buttonLabel: "Start Listening",
      statusLabel: "Not listening",
    });
  });

  it("treats starting and capturing sessions as active without transcript data", () => {
    expect(listeningViewState(status("planning", "session-1"), null)).toEqual({
      active: true,
      pending: true,
      buttonLabel: "Starting…",
      statusLabel: "Starting audio…",
    });
    expect(listeningViewState(status("starting", "session-1"), null)).toEqual({
      active: true,
      pending: true,
      buttonLabel: "Starting…",
      statusLabel: "Starting audio…",
    });
    expect(listeningViewState(status("capturing", "session-1"), null)).toEqual({
      active: true,
      pending: false,
      buttonLabel: "Stop Listening",
      statusLabel: "Listening",
    });
  });

  it("never reports stopped or failed capture as listening, even with a stale session id", () => {
    expect(isListening(status("stopped", "session-1"))).toBe(false);
    expect(listeningViewState(status("stopped", "session-1"), null).buttonLabel).toBe(
      "Start Listening",
    );

    expect(isListening(status("failed", "session-1"))).toBe(false);
    expect(listeningViewState(status("failed", "session-1"), null)).toMatchObject({
      active: false,
      pending: false,
      buttonLabel: "Retry Listening",
      statusLabel: "Listening needs attention",
    });
  });

  it("uses local pending actions for immediate truthful labels", () => {
    expect(listeningViewState(status("idle"), "start").buttonLabel).toBe("Starting…");
    expect(listeningViewState(status("capturing", "session-1"), "stop").buttonLabel).toBe(
      "Stopping…",
    );
  });

  it("turns an inactive control into an explicit retry after a boundary error", () => {
    expect(listeningViewState(null, null, true)).toMatchObject({
      active: false,
      pending: false,
      buttonLabel: "Retry Listening",
      statusLabel: "Listening needs attention",
    });
  });

  it("rejects a provably older delayed snapshot without freezing on partial timestamps", () => {
    const current = status("capturing", "session-2", { updated_at: "2000" });

    expect(
      shouldAcceptAudioStatus(current, status("stopped", null, { updated_at: "1999" })),
    ).toBe(false);
    expect(
      shouldAcceptAudioStatus(current, status("stopped", null, { updated_at: "2000" })),
    ).toBe(true);
    expect(
      shouldAcceptAudioStatus(current, status("stopped", null, { updated_at: "2001" })),
    ).toBe(true);
    expect(shouldAcceptAudioStatus(current, status("stopped"))).toBe(true);
    expect(
      shouldAcceptAudioStatus(current, status("stopped", null, { updated_at: "unknown" })),
    ).toBe(true);
  });

  it("selects boundary, capture, source, and failed-state errors in order", () => {
    const failed = status("failed", null, {
      capture: {
        state: "failed",
        last_error: "Sign in to use Listen.",
        system: { last_error: "System audio unavailable." },
      },
      note: "Open the browser to sign in.",
    });
    expect(listeningError(failed, "Daemon is unavailable.")).toBe("Daemon is unavailable.");
    expect(listeningError(failed, null)).toBe("Sign in to use Listen.");

    const sourceFailure = status("capturing", "session-1", {
      capture: {
        state: "capturing",
        system: { last_error: "Screen Recording permission denied." },
      },
    });
    expect(listeningError(sourceFailure, null)).toBe("Screen Recording permission denied.");

    const permissionFailure = status("failed", null, {
      capture: { state: "failed", permission_denied_source: "microphone" },
      note: "Microphone permission is required.",
    });
    expect(listeningError(permissionFailure, null)).toBe("Microphone permission is required.");
  });

  it("reports the actual active source coverage and never claims silent dual capture", () => {
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

    const systemOnly = status("capturing", "session-1", {
      config: {
        system: { enabled: true },
        microphone: { enabled: false },
      },
      capture: {
        state: "capturing",
        system: { state: "capturing" },
        microphone: { state: "disabled" },
      },
    });
    expect(listeningSourceSummary(systemOnly)).toMatchObject({
      coverage: "system_only",
      label: "System audio only",
      degraded: true,
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

  it("surfaces a failed active source as degraded", () => {
    const failedSystem = status("capturing", "session-1", {
      config: {
        system: { enabled: true },
        microphone: { enabled: true },
      },
      capture: {
        state: "capturing",
        system: { state: "failed", last_error: "System audio permission is required." },
        microphone: { state: "capturing" },
      },
    });
    expect(listeningSourceSummary(failedSystem)).toEqual({
      coverage: "microphone_only",
      label: "Microphone only",
      degraded: true,
      error: "System audio permission is required.",
    });
    expect(listeningError(failedSystem, null)).toBe("System audio permission is required.");
    expect(listeningViewState(failedSystem, null, true)).toMatchObject({
      active: true,
      buttonLabel: "Stop Listening",
      statusLabel: "Listening needs attention",
    });
  });

  it("shows the shortcut that the desktop actually registers", () => {
    expect(listeningShortcutLabel(true)).toBe("Control+Option+L");
    expect(listeningShortcutLabel(false)).toBe("Ctrl+Alt+L");
  });
});

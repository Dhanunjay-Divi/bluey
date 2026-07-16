import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useNavigate } from "react-router-dom";
import {
  AudioLines,
  CircleAlert,
  CircleStop,
  Eye,
  ExternalLink,
  LoaderCircle,
  MonitorUp,
  PauseCircle,
  Play,
  RefreshCw,
  ScanText,
  Settings,
  Square,
} from "lucide-react";
import { invoke } from "../lib/tauri";
import {
  type AudioPipelineStatusView,
  type PendingListeningAction,
  errorMessage,
  listeningError,
  listeningSourceSummary,
  listeningViewState,
  shouldAcceptAudioStatus,
} from "../routes/liveTranscriptState";

interface DashboardOwnerView {
  signed_in: boolean;
  available: boolean;
}

interface ContextModeStatus {
  active: boolean;
  interval_secs: number | null;
  context_items: number;
}

interface ContextWatchSettingsView {
  interval_secs: number;
}

const STATUS_POLL_INTERVAL_MS = 5_000;

export function AssistantControlPanel() {
  const navigate = useNavigate();
  const currentStatus = useRef<AudioPipelineStatusView | null>(null);
  const [owner, setOwner] = useState<DashboardOwnerView | null>(null);
  const [status, setStatus] = useState<AudioPipelineStatusView | null>(null);
  const [loading, setLoading] = useState(true);
  const [pendingAction, setPendingAction] = useState<PendingListeningAction>(null);
  const [ending, setEnding] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [overlayMessage, setOverlayMessage] = useState("");
  const [contextStatus, setContextStatus] = useState<ContextModeStatus | null>(null);
  const [contextInterval, setContextInterval] = useState(12);
  const [contextBusy, setContextBusy] = useState<"start" | "stop" | "page" | null>(null);
  const [contextError, setContextError] = useState("");
  const [contextMessage, setContextMessage] = useState("");
  const [captureCountdown, setCaptureCountdown] = useState<number | null>(null);
  const captureArmGeneration = useRef(0);
  const contextPolicyLoaded = useRef(false);

  const acceptStatus = useCallback((next: AudioPipelineStatusView) => {
    if (!shouldAcceptAudioStatus(currentStatus.current, next)) return;
    currentStatus.current = next;
    setStatus(next);
    setError(null);
    setLoading(false);
  }, []);

  const refresh = useCallback(async () => {
    try {
      const [nextOwner, nextStatus] = await Promise.all([
        invoke<DashboardOwnerView>("get_dashboard_owner"),
        invoke<AudioPipelineStatusView>("daemon_listening_status"),
      ]);
      setOwner(nextOwner.available ? nextOwner : null);
      acceptStatus(nextStatus);
      try {
        const nextContext = await invoke<ContextModeStatus>("daemon_context_status");
        setContextStatus(nextContext);
        if (nextContext.interval_secs) {
          setContextInterval(nextContext.interval_secs);
          contextPolicyLoaded.current = true;
        } else if (!contextPolicyLoaded.current) {
          const policy = await invoke<ContextWatchSettingsView>("get_context_watch_settings");
          setContextInterval(policy.interval_secs);
          contextPolicyLoaded.current = true;
        }
        setContextError("");
      } catch (nextError) {
        setContextError(errorMessage(nextError));
      }
    } catch (nextError) {
      setError(errorMessage(nextError));
      setLoading(false);
    }
  }, [acceptStatus]);

  useEffect(() => {
    void refresh();
    const interval = window.setInterval(() => void refresh(), STATUS_POLL_INTERVAL_MS);
    const unlisteners = [
      listen<AudioPipelineStatusView>("audio_pipeline_status", (event) => {
        acceptStatus(event.payload);
      }),
      listen<unknown>("audio_pipeline_error", (event) => {
        setError(errorMessage(event.payload));
        setLoading(false);
      }),
      listen<DashboardOwnerView>("dashboard_owner_changed", (event) => {
        setOwner(event.payload.available ? event.payload : null);
      }),
    ];
    return () => {
      window.clearInterval(interval);
      for (const unlisten of unlisteners) void unlisten.then((fn) => fn());
    };
  }, [acceptStatus, refresh]);

  const currentError = listeningError(status, error);
  const view = listeningViewState(status, pendingAction, loading, currentError !== null);
  const sources = listeningSourceSummary(status);
  const signedIn = owner?.signed_in === true;

  async function toggleListening() {
    if (!signedIn || pendingAction || ending || view.busy) return;
    setPendingAction(view.active ? "stop" : "start");
    setError(null);
    try {
      acceptStatus(await invoke<AudioPipelineStatusView>("daemon_toggle_listening"));
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setPendingAction(null);
    }
  }

  async function endSession() {
    if (!signedIn || pendingAction || ending) return;
    setEnding(true);
    setError(null);
    try {
      acceptStatus(await invoke<AudioPipelineStatusView>("daemon_end_session"));
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setEnding(false);
    }
  }

  async function openOverlay() {
    setOverlayMessage("");
    try {
      const message = await invoke<string>("daemon_toggle_overlay");
      setOverlayMessage(message || "Overlay visibility updated.");
    } catch (nextError) {
      setError(errorMessage(nextError));
    }
  }

  async function startContextMode() {
    if (contextBusy) return;
    setContextBusy("start");
    setContextError("");
    setContextMessage("");
    try {
      const next = await invoke<ContextModeStatus>("daemon_context_start", {
        intervalSecs: contextInterval,
      });
      setContextStatus(next);
      setContextMessage(
        `Context mode is on. Bluey will check for changed readable page text every ${next.interval_secs ?? contextInterval} seconds until you pause it.`,
      );
    } catch (nextError) {
      setContextError(errorMessage(nextError));
    } finally {
      setContextBusy(null);
    }
  }

  async function stopContextMode() {
    if (contextBusy) return;
    setContextBusy("stop");
    setContextError("");
    setContextMessage("");
    try {
      setContextStatus(await invoke<ContextModeStatus>("daemon_context_stop"));
      setContextMessage("Context mode is paused. No new background page observations will be added.");
    } catch (nextError) {
      setContextError(errorMessage(nextError));
    } finally {
      setContextBusy(null);
    }
  }

  const performActivePageCapture = useCallback(async (generation: number) => {
    if (generation !== captureArmGeneration.current) return;
    setCaptureCountdown(null);
    setContextBusy("page");
    setContextError("");
    setContextMessage("Reading the active supported browser page now...");
    try {
      const next = await invoke<ContextModeStatus>("daemon_capture_active_page");
      setContextStatus(next);
      setContextMessage("Readable text from the active supported browser page was added to this session.");
    } catch (nextError) {
      setContextError(errorMessage(nextError));
    } finally {
      setContextBusy(null);
    }
  }, []);

  useEffect(() => {
    if (captureCountdown === null) return;
    const generation = captureArmGeneration.current;
    const timeout = window.setTimeout(() => {
      if (generation !== captureArmGeneration.current) return;
      if (captureCountdown > 1) {
        setCaptureCountdown(captureCountdown - 1);
      } else {
        void performActivePageCapture(generation);
      }
    }, 1_000);
    return () => window.clearTimeout(timeout);
  }, [captureCountdown, performActivePageCapture]);

  function armActivePageCapture() {
    if (contextBusy || captureCountdown !== null) return;
    captureArmGeneration.current += 1;
    setContextError("");
    setContextMessage("");
    setCaptureCountdown(3);
  }

  function cancelActivePageCapture() {
    captureArmGeneration.current += 1;
    setCaptureCountdown(null);
    setContextMessage("Page capture cancelled. Nothing was added.");
  }

  const contextLocked = contextBusy !== null || captureCountdown !== null;
  const statusTone = currentError
    ? "border-amber-400/30 bg-amber-400/10 text-amber-200"
    : view.active
      ? "border-emerald-400/30 bg-emerald-400/10 text-emerald-200"
      : "border-zinc-700 bg-zinc-900 text-zinc-300";

  return (
    <section
      aria-labelledby="assistant-control-title"
      className="overflow-hidden rounded-xl border border-zinc-800 bg-zinc-900 shadow-2xl shadow-black/20"
    >
      <div className="grid gap-5 p-5 lg:grid-cols-[minmax(0,1fr)_auto] lg:items-center">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <span className={`inline-flex items-center gap-2 rounded-full border px-3 py-1 text-xs font-semibold ${statusTone}`}>
              <span
                aria-hidden="true"
                className={`h-2 w-2 rounded-full ${
                  currentError ? "bg-amber-300" : view.active ? "bg-emerald-300" : "bg-zinc-500"
                }`}
              />
              {signedIn ? view.statusLabel : "Sign in required"}
            </span>
            {view.active ? (
              <span className="text-xs text-zinc-500">{sources.label}</span>
            ) : null}
          </div>
          <h2 id="assistant-control-title" className="mt-3 text-xl font-semibold text-zinc-50">
            Your live work assistant
          </h2>
          <p className="mt-1 max-w-2xl text-sm leading-6 text-zinc-400">
            Listening starts only when you use the control below or your shortcut. Screen captures,
            files, and cloud sync remain separate choices.
          </p>
        </div>

        <div className="flex flex-wrap gap-2 lg:justify-end">
          <button
            type="button"
            aria-busy={view.busy}
            aria-pressed={view.active}
            disabled={!signedIn || view.busy || ending}
            onClick={() => void toggleListening()}
            className={`inline-flex min-h-10 items-center justify-center gap-2 rounded-md px-4 text-sm font-semibold text-white transition focus-visible:ring-2 focus-visible:ring-offset-2 focus-visible:ring-offset-zinc-900 disabled:cursor-not-allowed disabled:opacity-50 ${
              view.active
                ? "bg-red-500 hover:bg-red-400 focus-visible:ring-red-300"
                : "bg-sky-500 hover:bg-sky-400 focus-visible:ring-sky-300"
            }`}
          >
            {view.busy ? (
              <LoaderCircle aria-hidden="true" className="animate-spin" size={16} />
            ) : view.active ? (
              <Square aria-hidden="true" fill="currentColor" size={14} />
            ) : (
              <Play aria-hidden="true" fill="currentColor" size={15} />
            )}
            {signedIn ? view.buttonLabel : "Start listening"}
          </button>
          <button
            type="button"
            onClick={() => void openOverlay()}
            className="inline-flex min-h-10 items-center justify-center gap-2 rounded-md border border-zinc-700 px-3 text-sm font-semibold text-zinc-200 hover:border-zinc-500 hover:bg-zinc-800"
          >
            <MonitorUp aria-hidden="true" size={16} />
            Show overlay
          </button>
          {view.active ? (
            <button
              type="button"
              disabled={ending || pendingAction !== null}
              onClick={() => void endSession()}
              className="inline-flex min-h-10 items-center justify-center gap-2 rounded-md border border-zinc-700 px-3 text-sm font-semibold text-zinc-300 hover:border-red-500/50 hover:bg-red-500/10 hover:text-red-200 disabled:opacity-50"
            >
              {ending ? (
                <LoaderCircle aria-hidden="true" className="animate-spin" size={16} />
              ) : (
                <CircleStop aria-hidden="true" size={16} />
              )}
              {ending ? "Ending..." : "End session"}
            </button>
          ) : null}
        </div>
      </div>

      <div className="grid gap-4 border-t border-zinc-800 bg-zinc-950/45 px-5 py-4 xl:grid-cols-[minmax(0,1fr)_auto] xl:items-center">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <span
              className={`inline-flex items-center gap-2 rounded-full border px-2.5 py-1 text-[11px] font-semibold ${
                contextStatus?.active
                  ? "border-violet-400/30 bg-violet-400/10 text-violet-200"
                  : "border-zinc-700 bg-zinc-900 text-zinc-400"
              }`}
            >
              <Eye aria-hidden="true" size={13} />
              {contextStatus?.active
                ? `Context mode on · every ${contextStatus.interval_secs ?? contextInterval}s`
                : "Context mode off"}
            </span>
            {contextStatus ? (
              <span className="text-[11px] text-zinc-600">
                {contextStatus.context_items} session context {contextStatus.context_items === 1 ? "item" : "items"}
              </span>
            ) : null}
          </div>
          <h3 className="mt-2 text-sm font-semibold text-zinc-100">Page-first context</h3>
          <p className="mt-1 max-w-3xl text-xs leading-5 text-zinc-500">
            Capture page reads text from the active supported browser once. Context mode is a
            separate, explicit opt-in that periodically checks for changed readable page text until
            you pause it. Screenshot fallback stays off unless it is separately enabled in policy.
            Saved observations become durable, searchable memory for the current session without
            starting listening or model training.
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2 xl:justify-end">
          <button
            type="button"
            disabled={contextLocked}
            onClick={armActivePageCapture}
            className="inline-flex min-h-9 items-center gap-2 rounded-md border border-zinc-700 px-3 text-xs font-semibold text-zinc-200 hover:border-violet-400/40 hover:bg-violet-400/10 disabled:opacity-50"
          >
            {contextBusy === "page" ? (
              <LoaderCircle aria-hidden="true" className="animate-spin" size={14} />
            ) : (
              <ScanText aria-hidden="true" size={14} />
            )}
            {contextBusy === "page"
              ? "Capturing active page..."
              : captureCountdown !== null
                ? `Capture in ${captureCountdown}s`
                : "Capture active page once"}
          </button>
          {!contextStatus?.active ? (
            <>
              <label>
                <span className="sr-only">Context observation interval</span>
                <select
                  value={contextInterval}
                  disabled={contextLocked}
                  onChange={(event) => setContextInterval(Number(event.target.value))}
                  className="min-h-9 rounded-md border border-zinc-700 bg-zinc-900 px-2 text-xs text-zinc-200"
                >
                  <option value={3}>Every 3 seconds</option>
                  <option value={12}>Every 12 seconds</option>
                  <option value={30}>Every 30 seconds</option>
                  <option value={60}>Every 60 seconds</option>
                  <option value={120}>Every 2 minutes</option>
                  <option value={300}>Every 5 minutes</option>
                </select>
              </label>
              <button
                type="button"
                disabled={contextLocked}
                onClick={() => void startContextMode()}
                className="inline-flex min-h-9 items-center gap-2 rounded-md bg-violet-400 px-3 text-xs font-semibold text-zinc-950 hover:bg-violet-300 disabled:opacity-50"
              >
                {contextBusy === "start" ? (
                  <LoaderCircle aria-hidden="true" className="animate-spin" size={14} />
                ) : (
                  <Eye aria-hidden="true" size={14} />
                )}
                Start context mode
              </button>
            </>
          ) : (
            <button
              type="button"
              disabled={contextLocked}
              onClick={() => void stopContextMode()}
              className="inline-flex min-h-9 items-center gap-2 rounded-md bg-violet-400 px-3 text-xs font-semibold text-zinc-950 hover:bg-violet-300 disabled:opacity-50"
            >
              {contextBusy === "stop" ? (
                <LoaderCircle aria-hidden="true" className="animate-spin" size={14} />
              ) : (
                <PauseCircle aria-hidden="true" size={14} />
              )}
              Pause context mode
            </button>
          )}
        </div>
        {captureCountdown !== null ? (
          <div
            role="status"
            className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-violet-400/30 bg-violet-400/10 px-3 py-2.5 text-xs text-violet-100 xl:col-span-2"
          >
            <span>
              Capture armed · {captureCountdown}s. Switch now to Chrome, Edge, Brave, Arc,
              Chromium, or Safari and leave the target tab frontmost. Bluey will read that page
              only when the countdown finishes.
            </span>
            <button
              type="button"
              onClick={cancelActivePageCapture}
              className="min-h-8 rounded-md border border-violet-300/35 px-3 font-semibold hover:bg-violet-300/10"
            >
              Cancel capture
            </button>
          </div>
        ) : null}
        {contextError ? (
          <p role="alert" className="text-xs text-amber-300 xl:col-span-2">
            Context could not be updated: {contextError}
          </p>
        ) : null}
        {contextMessage ? (
          <p role="status" className="text-xs text-violet-200 xl:col-span-2">
            {contextMessage}
          </p>
        ) : null}
      </div>

      {currentError ? (
        <div
          role="alert"
          className="flex flex-wrap items-center justify-between gap-3 border-t border-amber-500/25 bg-amber-500/10 px-5 py-3 text-sm text-amber-100"
        >
          <span className="flex min-w-0 items-start gap-2">
            <CircleAlert aria-hidden="true" className="mt-0.5 shrink-0" size={16} />
            <span>{currentError}</span>
          </span>
          <span className="flex gap-2">
            <button
              type="button"
              onClick={() => void refresh()}
              className="inline-flex min-h-8 items-center gap-1.5 rounded-md border border-amber-400/30 px-2.5 text-xs font-semibold hover:bg-amber-400/10"
            >
              <RefreshCw aria-hidden="true" size={13} />
              Recheck
            </button>
            <button
              type="button"
              onClick={() => navigate("/settings")}
              className="inline-flex min-h-8 items-center gap-1.5 rounded-md border border-amber-400/30 px-2.5 text-xs font-semibold hover:bg-amber-400/10"
            >
              <Settings aria-hidden="true" size={13} />
              Audio settings
            </button>
          </span>
        </div>
      ) : (
        <div className="flex flex-wrap items-center justify-between gap-3 border-t border-zinc-800 bg-zinc-950/50 px-5 py-3 text-xs text-zinc-500">
          <span className="inline-flex items-center gap-2">
            <AudioLines aria-hidden="true" size={14} />
            {view.active
              ? `Recording indicator on · ${sources.label}`
              : "Not recording · context is added only through visible controls"}
          </span>
          <button
            type="button"
            onClick={() => navigate("/live")}
            className="inline-flex items-center gap-1.5 font-semibold text-cyan-200 hover:text-cyan-100"
          >
            Open live transcript
            <ExternalLink aria-hidden="true" size={13} />
          </button>
        </div>
      )}
      {overlayMessage ? (
        <p role="status" className="sr-only">
          {overlayMessage}
        </p>
      ) : null}
    </section>
  );
}

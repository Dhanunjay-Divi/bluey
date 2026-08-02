import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "../lib/tauri";
import { listen } from "@tauri-apps/api/event";
import { CircleAlert, Radio } from "lucide-react";
import { LiveTranscriptList, TranscriptSegment } from "../components/LiveTranscriptList";
import {
  type AudioPipelineStatusView,
  type PendingListeningAction,
  errorMessage,
  isListening,
  listeningError,
  listeningSourceSummary,
  listeningShortcutLabel,
  listeningViewState,
  shouldAcceptAudioStatus,
} from "./liveTranscriptState";

const MAX_SEGMENTS = 200;
const STATUS_POLL_INTERVAL_MS = 5_000;

interface EndSessionResult {
  audio_status: AudioPipelineStatusView | null;
  message: string;
}

/** Unique key for dedup: (session_id, index) */
function segKey(seg: TranscriptSegment): string {
  return `${seg.session_id}:${seg.index ?? -1}`;
}

export function LiveTranscript() {
  // Map keyed by "session_id:index" for O(1) dedup on both catch-up and live.
  const segMapRef = useRef<Map<string, TranscriptSegment>>(new Map());
  const [segments, setSegments] = useState<TranscriptSegment[]>([]);
  const sessionRef = useRef<string | null>(null);
  const [audioStatus, setAudioStatus] = useState<AudioPipelineStatusView | null>(null);
  const audioStatusRef = useRef<AudioPipelineStatusView | null>(null);
  const statusGenerationRef = useRef(0);
  const [pendingAction, setPendingAction] = useState<PendingListeningAction>(null);
  const [endingSession, setEndingSession] = useState(false);
  const [sessionOpen, setSessionOpen] = useState(false);
  const [sessionEnded, setSessionEnded] = useState(false);
  const [sessionNotice, setSessionNotice] = useState<string | null>(null);
  const [boundaryError, setBoundaryError] = useState<string | null>(null);
  const [shortcut, setShortcut] = useState(() => {
    const isMac =
      typeof navigator !== "undefined" && /Mac|iPhone|iPad|iPod/.test(navigator.platform);
    return listeningShortcutLabel(isMac);
  });

  const clearTranscript = useCallback(() => {
    segMapRef.current.clear();
    sessionRef.current = null;
    setSegments([]);
  }, []);

  const rebuildFromMap = useCallback(() => {
    const sorted = Array.from(segMapRef.current.values()).sort(
      (a, b) => (a.index ?? 0) - (b.index ?? 0),
    );
    const tail = sorted.length > MAX_SEGMENTS ? sorted.slice(-MAX_SEGMENTS) : sorted;
    setSegments(tail);
  }, []);

  // Catch-up: load existing segments on mount.
  useEffect(() => {
    invoke<TranscriptSegment[]>("get_live_transcripts", { sinceIndex: 0 })
      .then((segs) => {
        if (segs.length === 0) return;
        setSessionOpen(true);
        setSessionEnded(false);
        sessionRef.current = segs[0].session_id;
        const map = segMapRef.current;
        for (const seg of segs) {
          map.set(segKey(seg), seg);
        }
        rebuildFromMap();
      })
      .catch((e) => console.warn("get_live_transcripts failed:", e));
  }, [rebuildFromMap]);

  // Subscribe to live events with {session_id, index} dedup.
  const handleEvent = useCallback((seg: TranscriptSegment) => {
    // Session change: clear map, start fresh.
    if (sessionRef.current && seg.session_id !== sessionRef.current) {
      segMapRef.current.clear();
      sessionRef.current = seg.session_id;
    }
    sessionRef.current = seg.session_id;
    setSessionOpen(true);
    setSessionEnded(false);
    setSessionNotice(null);

    const key = segKey(seg);
    const map = segMapRef.current;
    // Only insert if not already present (dedup).
    if (!map.has(key)) {
      map.set(key, seg);
      // Trim oldest if over limit.
      if (map.size > MAX_SEGMENTS) {
        const firstKey = map.keys().next().value;
        if (firstKey !== undefined) map.delete(firstKey);
      }
      rebuildFromMap();
    }
  }, [rebuildFromMap]);

  useEffect(() => {
    const unlisten = listen<TranscriptSegment>("live_transcript", (event) => {
      handleEvent(event.payload);
    });
    return () => { unlisten.then((fn) => fn()); };
  }, [handleEvent]);

  const acceptStatus = useCallback((status: AudioPipelineStatusView) => {
    if (!shouldAcceptAudioStatus(audioStatusRef.current, status)) return;
    audioStatusRef.current = status;
    setAudioStatus(status);
    if (isListening(status)) setSessionOpen(true);
    setBoundaryError(null);
  }, []);

  useEffect(() => {
    let disposed = false;
    invoke<string>("get_listening_shortcut")
      .then((label) => {
        const next = label.trim();
        if (!disposed && next) setShortcut(next);
      })
      .catch(() => {
        // The platform fallback remains truthful for older dashboard builds.
      });
    return () => {
      disposed = true;
    };
  }, []);

  // Reconcile status on mount and periodically so changes made by tray,
  // shortcut, or overlay stay truthful even if an event was missed.
  useEffect(() => {
    let disposed = false;
    let inFlight = false;

    const refresh = async () => {
      if (disposed || inFlight) return;
      inFlight = true;
      const generation = statusGenerationRef.current;
      try {
        const status = await invoke<AudioPipelineStatusView>("daemon_listening_status");
        if (!disposed && generation === statusGenerationRef.current) acceptStatus(status);
      } catch (error) {
        if (!disposed && generation === statusGenerationRef.current) {
          setBoundaryError(errorMessage(error));
        }
      } finally {
        inFlight = false;
      }
    };

    void refresh();
    const interval = window.setInterval(() => void refresh(), STATUS_POLL_INTERVAL_MS);
    return () => {
      disposed = true;
      window.clearInterval(interval);
    };
  }, [acceptStatus]);

  // Events make direct, shortcut, and overlay transitions visible immediately.
  useEffect(() => {
    let disposed = false;
    const unlisteners = [
      listen<AudioPipelineStatusView>("audio_pipeline_status", (event) => {
        if (!disposed) {
          statusGenerationRef.current += 1;
          acceptStatus(event.payload);
        }
      }),
      listen<unknown>("audio_pipeline_error", (event) => {
        if (!disposed) {
          statusGenerationRef.current += 1;
          setBoundaryError(errorMessage(event.payload));
        }
      }),
      listen<unknown>("live_session_ended", (event) => {
        if (disposed) return;
        statusGenerationRef.current += 1;
        setSessionOpen(false);
        setSessionEnded(true);
        setSessionNotice(errorMessage(event.payload));
      }),
    ];

    return () => {
      disposed = true;
      unlisteners.forEach((unlisten) => {
        void unlisten.then((fn) => fn());
      });
    };
  }, [acceptStatus]);

  const sourceSummary = listeningSourceSummary(audioStatus);
  const currentError = listeningError(audioStatus, boundaryError);
  const view = listeningViewState(audioStatus, pendingAction, currentError !== null);

  const toggleListening = useCallback(async () => {
    if (pendingAction || endingSession) return;

    const action: Exclude<PendingListeningAction, null> = view.active ? "stop" : "start";
    const startsAfterEnd = action === "start" && sessionEnded;
    const generation = statusGenerationRef.current + 1;
    statusGenerationRef.current = generation;
    setPendingAction(action);
    setBoundaryError(null);
    setSessionNotice(null);
    try {
      const status = await invoke<AudioPipelineStatusView>("daemon_toggle_listening");
      if (generation === statusGenerationRef.current) acceptStatus(status);
      if (action === "start" && isListening(status)) {
        if (startsAfterEnd) clearTranscript();
        setSessionOpen(true);
        setSessionEnded(false);
      } else if (action === "start" && status.capture.state === "failed") {
        // The daemon may have created a diagnostic-only meeting before a
        // source or account gate failed. Keep End Session available to cleanly
        // archive/delete that lifecycle record.
        setSessionOpen(true);
      }
    } catch (error) {
      if (action === "start") setSessionOpen(true);
      setBoundaryError(errorMessage(error));
    } finally {
      setPendingAction(null);
    }
  }, [acceptStatus, clearTranscript, endingSession, pendingAction, sessionEnded, view.active]);

  const endSession = useCallback(async () => {
    if (pendingAction || endingSession) return;
    setEndingSession(true);
    const generation = statusGenerationRef.current + 1;
    statusGenerationRef.current = generation;
    setBoundaryError(null);
    setSessionNotice(null);
    try {
      const result = await invoke<EndSessionResult>("daemon_end_session");
      if (result.audio_status && generation === statusGenerationRef.current) {
        acceptStatus(result.audio_status);
      }
      setSessionOpen(false);
      setSessionEnded(true);
      setSessionNotice(result.message || "Session ended and saved.");
    } catch (error) {
      setBoundaryError(errorMessage(error));
    } finally {
      setEndingSession(false);
    }
  }, [acceptStatus, endingSession, pendingAction]);

  const controlsPending = view.pending || endingSession;
  const showEndSession = sessionOpen && !sessionEnded;

  return (
    <div className="flex flex-1 flex-col overflow-hidden">
      <div className="border-b border-zinc-800 bg-zinc-950/70 px-4 py-3">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <Radio
                aria-hidden="true"
                size={17}
                className={
                  currentError ? "text-amber-400" : view.active ? "text-emerald-400" : "text-zinc-500"
                }
              />
              <h2 className="text-sm font-semibold text-zinc-200">Live Transcript</h2>
              <span
                aria-live="polite"
                className={`rounded-full px-2 py-0.5 text-[11px] font-medium ${
                  currentError
                    ? "bg-amber-500/15 text-amber-300"
                    : view.active
                    ? "bg-emerald-500/15 text-emerald-300"
                    : "bg-zinc-800 text-zinc-400"
                }`}
              >
                {view.statusLabel}
              </span>
            </div>
            <p className="mt-1 text-xs text-zinc-500">
              {view.active ? `Active sources: ${sourceSummary.label}` : "Starts system audio and microphone"}
              {` · Shortcut ${shortcut}`}
            </p>
          </div>
          <div className="flex flex-wrap justify-end gap-2">
            {showEndSession && (
              <button
                type="button"
                aria-label="End session, save the transcript, and create a recap"
                aria-busy={endingSession}
                disabled={controlsPending}
                onClick={() => void endSession()}
                className="rounded-md border border-zinc-700 bg-zinc-900 px-3 py-2 text-sm font-medium text-zinc-200 transition hover:bg-zinc-800 focus:outline-none focus-visible:ring-2 focus-visible:ring-zinc-400 disabled:cursor-wait disabled:opacity-60"
              >
                {endingSession ? "Ending Session…" : "End Session"}
              </button>
            )}
            <button
              type="button"
              aria-pressed={view.active}
              aria-busy={controlsPending}
              disabled={controlsPending}
              onClick={() => void toggleListening()}
              className={`min-w-36 rounded-md px-4 py-2 text-sm font-semibold shadow-sm transition focus:outline-none focus-visible:ring-2 focus-visible:ring-offset-2 focus-visible:ring-offset-zinc-950 disabled:cursor-wait disabled:opacity-65 ${
                view.active
                  ? "bg-red-500/90 text-white hover:bg-red-400 focus-visible:ring-red-400"
                  : "bg-blue-500 text-white hover:bg-blue-400 focus-visible:ring-blue-400"
              }`}
            >
              {view.buttonLabel}
            </button>
          </div>
        </div>
      </div>

      {sessionNotice && (
        <div
          role="status"
          aria-live="polite"
          className="mx-4 mt-3 rounded-md border border-emerald-500/25 bg-emerald-500/10 px-3 py-2 text-sm text-emerald-200"
        >
          {sessionNotice}
        </div>
      )}

      {currentError && (
        <div
          role="alert"
          className="mx-4 mt-3 flex items-start gap-2 rounded-md border border-amber-500/30 bg-amber-500/10 px-3 py-2 text-sm text-amber-200"
        >
          <CircleAlert aria-hidden="true" className="mt-0.5 shrink-0" size={16} />
          <div>
            <p>{currentError}</p>
            <p className="mt-1 text-xs text-amber-300/75">
              Fix the issue, then choose {view.active ? "Stop Listening" : "Retry Listening"}.
            </p>
          </div>
        </div>
      )}

      {segments.length === 0 ? (
        <div className="flex flex-1 items-center justify-center p-6 text-center">
          <div className="max-w-sm rounded-xl border border-zinc-800 bg-zinc-900/40 px-8 py-7">
            <Radio aria-hidden="true" className="mx-auto text-zinc-600" size={30} />
            <p className="mt-3 text-sm font-medium text-zinc-300">
              {view.active ? "Listening for speech…" : "Start a live transcript in one click"}
            </p>
            <p className="mt-1 text-xs leading-5 text-zinc-500">
              {view.active
                ? "Your first transcript segment will appear here and stay available after listening stops."
                : `Use Start Listening above or press ${shortcut}.`}
            </p>
          </div>
        </div>
      ) : (
        <LiveTranscriptList segments={segments} />
      )}
    </div>
  );
}

import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { AudioLines, CircleAlert, LoaderCircle, Play, Square } from "lucide-react";
import { LiveTranscriptList, TranscriptSegment } from "../components/LiveTranscriptList";
import { invoke } from "../lib/tauri";
import {
  type AudioPipelineStatusView,
  type PendingListeningAction,
  errorMessage,
  listeningError,
  listeningShortcutLabel,
  listeningSourceSummary,
  listeningViewState,
  shouldAcceptAudioStatus,
} from "./liveTranscriptState";

const MAX_SEGMENTS = 200;
const STATUS_POLL_INTERVAL_MS = 5_000;

/** Unique key for dedup: (session_id, index) */
function segKey(seg: TranscriptSegment): string {
  return `${seg.session_id}:${seg.index ?? -1}`;
}

export function LiveTranscript() {
  const segMapRef = useRef<Map<string, TranscriptSegment>>(new Map());
  const [segments, setSegments] = useState<TranscriptSegment[]>([]);
  const sessionRef = useRef<string | null>(null);
  const [audioStatus, setAudioStatus] = useState<AudioPipelineStatusView | null>(null);
  const audioStatusRef = useRef<AudioPipelineStatusView | null>(null);
  const statusGenerationRef = useRef(0);
  const [statusLoading, setStatusLoading] = useState(true);
  const [pendingAction, setPendingAction] = useState<PendingListeningAction>(null);
  const [boundaryError, setBoundaryError] = useState<string | null>(null);
  const [shortcut, setShortcut] = useState(() => {
    const isMac =
      typeof navigator !== "undefined" && /Mac|iPhone|iPad|iPod/.test(navigator.platform);
    return listeningShortcutLabel(isMac);
  });

  const rebuildFromMap = useCallback(() => {
    const sorted = Array.from(segMapRef.current.values()).sort(
      (a, b) => (a.index ?? 0) - (b.index ?? 0),
    );
    const tail = sorted.length > MAX_SEGMENTS ? sorted.slice(-MAX_SEGMENTS) : sorted;
    setSegments(tail);
  }, []);

  useEffect(() => {
    invoke<TranscriptSegment[]>("get_live_transcripts", { sinceIndex: 0 })
      .then((segs) => {
        if (segs.length === 0) return;
        sessionRef.current = segs[0].session_id;
        const map = segMapRef.current;
        for (const seg of segs) map.set(segKey(seg), seg);
        rebuildFromMap();
      })
      .catch((error) => console.warn("get_live_transcripts failed:", error));
  }, [rebuildFromMap]);

  const handleTranscript = useCallback(
    (seg: TranscriptSegment) => {
      if (sessionRef.current && seg.session_id !== sessionRef.current) {
        segMapRef.current.clear();
      }
      sessionRef.current = seg.session_id;

      const key = segKey(seg);
      const map = segMapRef.current;
      if (map.has(key)) return;

      map.set(key, seg);
      if (map.size > MAX_SEGMENTS) {
        const firstKey = map.keys().next().value;
        if (firstKey !== undefined) map.delete(firstKey);
      }
      rebuildFromMap();
    },
    [rebuildFromMap],
  );

  useEffect(() => {
    const unlisten = listen<TranscriptSegment>("live_transcript", (event) => {
      handleTranscript(event.payload);
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [handleTranscript]);

  const acceptStatus = useCallback((status: AudioPipelineStatusView) => {
    if (!shouldAcceptAudioStatus(audioStatusRef.current, status)) return;
    audioStatusRef.current = status;
    setAudioStatus(status);
    setStatusLoading(false);
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

  useEffect(() => {
    let disposed = false;
    const unlisteners = [
      listen<AudioPipelineStatusView>("audio_pipeline_status", (event) => {
        if (disposed) return;
        statusGenerationRef.current += 1;
        acceptStatus(event.payload);
      }),
      listen<unknown>("audio_pipeline_error", (event) => {
        if (disposed) return;
        statusGenerationRef.current += 1;
        setStatusLoading(false);
        setBoundaryError(errorMessage(event.payload));
      }),
    ];

    return () => {
      disposed = true;
      for (const unlisten of unlisteners) void unlisten.then((fn) => fn());
    };
  }, [acceptStatus]);

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
        if (!disposed && generation === statusGenerationRef.current) setStatusLoading(false);
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

  const sourceSummary = listeningSourceSummary(audioStatus);
  const currentError = listeningError(audioStatus, boundaryError);
  const view = listeningViewState(
    audioStatus,
    pendingAction,
    statusLoading,
    currentError !== null,
  );

  const toggleListening = useCallback(async () => {
    if (pendingAction || view.busy) return;

    const action: Exclude<PendingListeningAction, null> = view.active ? "stop" : "start";
    const generation = statusGenerationRef.current + 1;
    statusGenerationRef.current = generation;
    setPendingAction(action);
    setBoundaryError(null);
    try {
      const status = await invoke<AudioPipelineStatusView>("daemon_toggle_listening");
      if (generation === statusGenerationRef.current) acceptStatus(status);
    } catch (error) {
      if (generation === statusGenerationRef.current) setBoundaryError(errorMessage(error));
    } finally {
      setPendingAction(null);
    }
  }, [acceptStatus, pendingAction, view.active, view.busy]);

  const statusColor = currentError
    ? "text-amber-300"
    : view.active
      ? "text-emerald-300"
      : "text-zinc-400";
  const statusDot = currentError
    ? "bg-amber-400"
    : view.active
      ? "bg-emerald-400"
      : "bg-zinc-600";

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-hidden border border-zinc-800 bg-zinc-950">
      <div className="border-b border-zinc-800 px-4 py-3">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="min-w-0">
            <div className="flex items-center gap-3">
              <AudioLines aria-hidden="true" className="shrink-0 text-zinc-400" size={18} />
              <h2 className="text-sm font-semibold text-zinc-200">Live Transcript</h2>
              <span aria-live="polite" className={`flex items-center gap-1.5 text-xs ${statusColor}`}>
                <span aria-hidden="true" className={`h-1.5 w-1.5 rounded-full ${statusDot}`} />
                {view.statusLabel}
              </span>
            </div>
            <p className="mt-1 pl-[30px] text-xs text-zinc-500">
              {view.active
                ? `Active sources: ${sourceSummary.label}`
                : "Starts system audio and microphone"}
              {` · Shortcut ${shortcut}`}
            </p>
          </div>

          <button
            type="button"
            aria-pressed={view.active}
            aria-busy={view.busy}
            disabled={view.busy}
            onClick={() => void toggleListening()}
            className={`flex h-9 min-w-40 items-center justify-center gap-2 rounded-md px-4 text-sm font-semibold text-white transition focus:outline-none focus-visible:ring-2 focus-visible:ring-offset-2 focus-visible:ring-offset-zinc-950 disabled:cursor-wait disabled:opacity-65 ${
              view.active
                ? "bg-red-500 hover:bg-red-400 focus-visible:ring-red-400"
                : "bg-sky-500 hover:bg-sky-400 focus-visible:ring-sky-400"
            }`}
          >
            {view.busy ? (
              <LoaderCircle aria-hidden="true" className="animate-spin" size={16} />
            ) : view.active ? (
              <Square aria-hidden="true" fill="currentColor" size={14} />
            ) : (
              <Play aria-hidden="true" fill="currentColor" size={15} />
            )}
            {view.buttonLabel}
          </button>
        </div>
      </div>

      {currentError && (
        <div
          role="alert"
          className="flex items-start gap-2 border-b border-amber-500/30 bg-amber-500/10 px-4 py-2.5 text-sm text-amber-200"
        >
          <CircleAlert aria-hidden="true" className="mt-0.5 shrink-0" size={16} />
          <p>{currentError}</p>
        </div>
      )}

      {segments.length === 0 ? (
        <div className="flex flex-1 items-center justify-center p-6 text-center">
          <div className="max-w-sm">
            <AudioLines aria-hidden="true" className="mx-auto text-zinc-600" size={30} />
            <p className="mt-3 text-sm font-medium text-zinc-300">
              {view.active ? "Listening for speech" : "Start a live transcript in one click"}
            </p>
            <p className="mt-1 text-xs text-zinc-500">
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

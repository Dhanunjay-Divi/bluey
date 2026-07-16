import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  AudioLines,
  CircleAlert,
  CircleStop,
  LoaderCircle,
  Play,
  RefreshCw,
  Square,
} from "lucide-react";
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

export interface DashboardOwnerView {
  owner_key: string;
  signed_in: boolean;
  available: boolean;
}

export type TranscriptHistoryState = "loading" | "ready" | "error";
export type TranscriptHistoryView = "loading" | "error" | "empty" | "content";

export function transcriptHistoryView(
  state: TranscriptHistoryState,
  segmentCount: number,
): TranscriptHistoryView {
  if (segmentCount > 0) return "content";
  if (state === "loading") return "loading";
  if (state === "error") return "error";
  return "empty";
}

export function shouldRunLivePollers(owner: DashboardOwnerView | null): boolean {
  return Boolean(owner?.available && owner.signed_in);
}

/** Unique key for dedup: (session_id, index) */
function segKey(seg: TranscriptSegment): string {
  return `${seg.session_id}:${seg.index ?? -1}`;
}

export function LiveTranscript() {
  const segMapRef = useRef<Map<string, TranscriptSegment>>(new Map());
  const sessionRef = useRef<string | null>(null);
  const audioStatusRef = useRef<AudioPipelineStatusView | null>(null);
  const statusGenerationRef = useRef(0);
  const [segments, setSegments] = useState<TranscriptSegment[]>([]);
  const [owner, setOwner] = useState<DashboardOwnerView | null>(null);
  const [ownerLoading, setOwnerLoading] = useState(true);
  const [ownerError, setOwnerError] = useState<string | null>(null);
  const [historyState, setHistoryState] = useState<TranscriptHistoryState>("loading");
  const [historyError, setHistoryError] = useState<string | null>(null);
  const [historyReload, setHistoryReload] = useState(0);
  const [audioStatus, setAudioStatus] = useState<AudioPipelineStatusView | null>(null);
  const [statusLoading, setStatusLoading] = useState(true);
  const [pendingAction, setPendingAction] = useState<PendingListeningAction>(null);
  const [boundaryError, setBoundaryError] = useState<string | null>(null);
  const [endingSession, setEndingSession] = useState(false);
  const [endError, setEndError] = useState<string | null>(null);
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

  const clearVisibleSession = useCallback(() => {
    statusGenerationRef.current += 1;
    segMapRef.current.clear();
    sessionRef.current = null;
    audioStatusRef.current = null;
    setSegments([]);
    setAudioStatus(null);
    setStatusLoading(false);
    setPendingAction(null);
    setBoundaryError(null);
    setEndError(null);
    setEndingSession(false);
    setHistoryError(null);
    setHistoryState("loading");
  }, []);

  const applyOwner = useCallback(
    (next: DashboardOwnerView) => {
      clearVisibleSession();
      setOwner(next.available ? next : null);
      setOwnerError(
        next.available
          ? null
          : "Bluey could not verify the current account. Sign out and sign in again.",
      );
      setOwnerLoading(false);
    },
    [clearVisibleSession],
  );

  const loadOwner = useCallback(async () => {
    setOwnerLoading(true);
    setOwnerError(null);
    try {
      applyOwner(await invoke<DashboardOwnerView>("get_dashboard_owner"));
    } catch (error) {
      clearVisibleSession();
      setOwner(null);
      setOwnerError(errorMessage(error));
      setOwnerLoading(false);
    }
  }, [applyOwner, clearVisibleSession]);

  useEffect(() => {
    let disposed = false;
    const unlisten = listen<DashboardOwnerView>("dashboard_owner_changed", (event) => {
      if (!disposed) applyOwner(event.payload);
    });
    void loadOwner();
    return () => {
      disposed = true;
      void unlisten.then((fn) => fn());
    };
  }, [applyOwner, loadOwner]);

  useEffect(() => {
    if (!owner?.available) return;
    let disposed = false;
    segMapRef.current.clear();
    sessionRef.current = null;
    setSegments([]);
    setHistoryState("loading");
    setHistoryError(null);

    invoke<TranscriptSegment[]>("get_live_transcripts", { sinceIndex: 0 })
      .then((segs) => {
        if (disposed) return;
        if (segs.length > 0) {
          sessionRef.current = segs[0].session_id;
          for (const seg of segs) segMapRef.current.set(segKey(seg), seg);
          rebuildFromMap();
        }
        setHistoryState("ready");
      })
      .catch((error) => {
        if (disposed) return;
        setHistoryError(errorMessage(error));
        setHistoryState("error");
      });

    return () => {
      disposed = true;
    };
  }, [historyReload, owner?.available, owner?.owner_key, rebuildFromMap]);

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
      setHistoryState("ready");
      setHistoryError(null);
      rebuildFromMap();
    },
    [rebuildFromMap],
  );

  useEffect(() => {
    if (!shouldRunLivePollers(owner)) return;
    const unlisteners = [
      listen<TranscriptSegment>("live_transcript", (event) => {
        handleTranscript(event.payload);
      }),
      listen("live_session_ended", () => {
        segMapRef.current.clear();
        sessionRef.current = null;
        setSegments([]);
        setHistoryState("ready");
        setHistoryError(null);
      }),
    ];
    return () => {
      for (const unlisten of unlisteners) void unlisten.then((fn) => fn());
    };
  }, [handleTranscript, owner]);

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
    if (!shouldRunLivePollers(owner)) return;
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
  }, [acceptStatus, owner]);

  useEffect(() => {
    if (!shouldRunLivePollers(owner)) {
      setStatusLoading(false);
      return;
    }
    let disposed = false;
    let inFlight = false;
    setStatusLoading(true);

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
  }, [acceptStatus, owner]);

  const sourceSummary = listeningSourceSummary(audioStatus);
  const currentError = listeningError(audioStatus, boundaryError);
  const displayError = endError ?? currentError;
  const view = listeningViewState(
    audioStatus,
    pendingAction,
    statusLoading,
    currentError !== null,
  );

  const toggleListening = useCallback(async () => {
    if (!owner?.signed_in || pendingAction || view.busy || endingSession) return;

    const action: Exclude<PendingListeningAction, null> = view.active ? "stop" : "start";
    const generation = statusGenerationRef.current + 1;
    statusGenerationRef.current = generation;
    setPendingAction(action);
    setBoundaryError(null);
    setEndError(null);
    try {
      const status = await invoke<AudioPipelineStatusView>("daemon_toggle_listening");
      if (generation === statusGenerationRef.current) acceptStatus(status);
    } catch (error) {
      if (generation === statusGenerationRef.current) setBoundaryError(errorMessage(error));
    } finally {
      setPendingAction(null);
    }
  }, [acceptStatus, endingSession, owner?.signed_in, pendingAction, view.active, view.busy]);

  const endSession = useCallback(async () => {
    if (!owner?.signed_in || endingSession || pendingAction) return;
    const generation = statusGenerationRef.current + 1;
    statusGenerationRef.current = generation;
    setEndingSession(true);
    setBoundaryError(null);
    setEndError(null);
    try {
      const status = await invoke<AudioPipelineStatusView>("daemon_end_session");
      if (generation !== statusGenerationRef.current) return;
      acceptStatus(status);
      segMapRef.current.clear();
      sessionRef.current = null;
      setSegments([]);
      setHistoryState("ready");
      setHistoryError(null);
    } catch (error) {
      if (generation === statusGenerationRef.current) setEndError(errorMessage(error));
    } finally {
      setEndingSession(false);
    }
  }, [acceptStatus, endingSession, owner?.signed_in, pendingAction]);

  if (ownerLoading && !owner) {
    return (
      <div className="flex min-h-52 flex-1 items-center justify-center text-sm text-zinc-400">
        <LoaderCircle aria-hidden="true" className="mr-2 animate-spin" size={17} />
        Loading live session
      </div>
    );
  }

  if (ownerError && !owner) {
    return (
      <div className="flex min-h-52 flex-1 items-center justify-center p-6">
        <div className="max-w-md text-center" role="alert">
          <CircleAlert aria-hidden="true" className="mx-auto text-amber-300" size={26} />
          <p className="mt-3 text-sm font-medium text-zinc-200">Live session unavailable</p>
          <p className="mt-1 text-xs text-zinc-500">{ownerError}</p>
          <button
            type="button"
            onClick={() => void loadOwner()}
            className="mt-4 inline-flex h-9 items-center gap-2 rounded-md border border-zinc-700 px-3 text-sm font-medium text-zinc-200 hover:bg-zinc-900"
          >
            <RefreshCw aria-hidden="true" size={15} />
            Retry
          </button>
        </div>
      </div>
    );
  }

  const signedIn = owner?.signed_in === true;
  const statusColor = displayError
    ? "text-amber-300"
    : view.active
      ? "text-emerald-300"
      : "text-zinc-400";
  const statusDot = displayError
    ? "bg-amber-400"
    : view.active
      ? "bg-emerald-400"
      : "bg-zinc-600";
  const historyView = transcriptHistoryView(historyState, segments.length);

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
                {signedIn ? view.statusLabel : "Signed out"}
              </span>
            </div>
            <p className="mt-1 pl-[30px] text-xs text-zinc-500">
              {signedIn
                ? view.active
                  ? `Active sources: ${sourceSummary.label}`
                  : "System audio and microphone"
                : "Sign in to listen"}
              {signedIn && ` · Shortcut ${shortcut}`}
            </p>
          </div>

          <div className="flex items-center gap-2">
            {signedIn && (
              <button
                type="button"
                aria-busy={endingSession}
                disabled={endingSession || pendingAction !== null}
                onClick={() => void endSession()}
                className="flex h-9 items-center justify-center gap-2 rounded-md border border-zinc-700 px-3 text-sm font-medium text-zinc-200 transition hover:border-red-500/60 hover:bg-red-500/10 hover:text-red-200 focus:outline-none focus-visible:ring-2 focus-visible:ring-red-400 disabled:cursor-wait disabled:opacity-60"
              >
                {endingSession ? (
                  <LoaderCircle aria-hidden="true" className="animate-spin" size={16} />
                ) : (
                  <CircleStop aria-hidden="true" size={16} />
                )}
                {endingSession ? "Ending..." : "End Session"}
              </button>
            )}

            <button
              type="button"
              aria-pressed={view.active}
              aria-busy={view.busy}
              disabled={!signedIn || view.busy || endingSession}
              onClick={() => void toggleListening()}
              className={`flex h-9 min-w-40 items-center justify-center gap-2 rounded-md px-4 text-sm font-semibold text-white transition focus:outline-none focus-visible:ring-2 focus-visible:ring-offset-2 focus-visible:ring-offset-zinc-950 disabled:cursor-not-allowed disabled:opacity-60 ${
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
              {signedIn ? view.buttonLabel : "Start Listening"}
            </button>
          </div>
        </div>
      </div>

      {displayError && (
        <div
          role="alert"
          className="flex items-start gap-2 border-b border-amber-500/30 bg-amber-500/10 px-4 py-2.5 text-sm text-amber-200"
        >
          <CircleAlert aria-hidden="true" className="mt-0.5 shrink-0" size={16} />
          <p>{displayError}</p>
        </div>
      )}

      {historyView === "loading" && (
        <div className="flex flex-1 items-center justify-center text-sm text-zinc-500">
          <LoaderCircle aria-hidden="true" className="mr-2 animate-spin" size={16} />
          Loading transcript history
        </div>
      )}

      {historyView === "error" && (
        <div className="flex flex-1 items-center justify-center p-6 text-center">
          <div className="max-w-sm" role="alert">
            <CircleAlert aria-hidden="true" className="mx-auto text-amber-300" size={26} />
            <p className="mt-3 text-sm font-medium text-zinc-200">Transcript history unavailable</p>
            <p className="mt-1 text-xs text-zinc-500">{historyError}</p>
            <button
              type="button"
              onClick={() => setHistoryReload((value) => value + 1)}
              className="mt-4 inline-flex h-9 items-center gap-2 rounded-md border border-zinc-700 px-3 text-sm font-medium text-zinc-200 hover:bg-zinc-900"
            >
              <RefreshCw aria-hidden="true" size={15} />
              Retry
            </button>
          </div>
        </div>
      )}

      {historyView === "empty" && (
        <div className="flex flex-1 items-center justify-center p-6 text-center">
          <div className="max-w-sm">
            <AudioLines aria-hidden="true" className="mx-auto text-zinc-600" size={30} />
            <p className="mt-3 text-sm font-medium text-zinc-300">
              {signedIn
                ? view.active
                  ? "Listening for speech"
                  : "No transcript history yet"
                : "No local transcript history"}
            </p>
            <p className="mt-1 text-xs text-zinc-500">
              {signedIn
                ? view.active
                  ? "Speech will appear here as it is transcribed."
                  : `Start listening or press ${shortcut}.`
                : "Sign in to begin a live session."}
            </p>
          </div>
        </div>
      )}

      {historyView === "content" && <LiveTranscriptList segments={segments} />}
    </div>
  );
}

import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "../lib/tauri";
import { ExternalLink, LoaderCircle, Play, RefreshCw, X } from "lucide-react";
import {
  type AudioPipelineStatusView,
  isListening,
} from "../routes/liveTranscriptState";

type AudioSource = "microphone" | "system";
type PermissionAction = "status" | "listening";
type PermissionVerification = "allowed" | "denied" | "needs_listening";

interface PermissionDeniedPayload {
  source: AudioSource;
}

interface AudioPermissionPollPayload {
  sources: Array<{
    source: AudioSource;
    verification: PermissionVerification;
  }>;
}

const LABELS: Record<AudioSource, { title: string; body: string }> = {
  microphone: {
    title: "Microphone access denied",
    body: "Bluey needs microphone permission before it can transcribe your side of a conversation.",
  },
  system: {
    title: "System audio access denied",
    body: "Bluey needs system-audio or Screen Recording access before it can transcribe the other side.",
  },
};

export function PermissionBanner() {
  const [denied, setDenied] = useState<Set<AudioSource>>(new Set());
  const [checking, setChecking] = useState<{
    source: AudioSource;
    action: PermissionAction;
  } | null>(null);
  const [messages, setMessages] = useState<Partial<Record<AudioSource, string>>>({});

  useEffect(() => {
    const unlisteners = [
      listen<PermissionDeniedPayload>("audio_permission_denied", (event) => {
        setDenied((prev) => new Set(prev).add(event.payload.source));
      }),
      listen<PermissionDeniedPayload>("audio_permission_allowed", (event) => {
        clearDenied(event.payload.source);
      }),
    ];
    return () => {
      for (const unlisten of unlisteners) void unlisten.then((fn) => fn());
    };
  }, []);

  const setSourceMessage = (source: AudioSource, message: string) => {
    setMessages((previous) => ({ ...previous, [source]: message }));
  };

  const clearDenied = (source: AudioSource) => {
    setDenied((prev) => {
      const next = new Set(prev);
      next.delete(source);
      return next;
    });
    setMessages((previous) => {
      const next = { ...previous };
      delete next[source];
      return next;
    });
  };

  const dismiss = (source: AudioSource) => {
    clearDenied(source);
  };

  const openSettings = async (source: AudioSource) => {
    try {
      await invoke("open_privacy_settings", { source });
      setSourceMessage(
        source,
        "System settings opened. After allowing access, start listening so Bluey can verify the source.",
      );
    } catch (nextError) {
      setSourceMessage(source, `Bluey could not open system settings: ${String(nextError)}`);
    }
  };

  const applyVerification = (
    source: AudioSource,
    payload: AudioPermissionPollPayload,
  ): PermissionVerification => {
    const verification =
      payload.sources.find((candidate) => candidate.source === source)?.verification ??
      "needs_listening";
    if (verification === "allowed") {
      clearDenied(source);
    } else if (verification === "denied") {
      setSourceMessage(
        source,
        "Access is still denied. Change it in system settings, then start listening again.",
      );
    } else {
      setSourceMessage(
        source,
        "Bluey has not received this source successfully yet. Start listening to verify access.",
      );
    }
    return verification;
  };

  const checkStatus = async (source: AudioSource) => {
    if (checking) return;
    setChecking({ source, action: "status" });
    setSourceMessage(source, "");
    try {
      applyVerification(
        source,
        await invoke<AudioPermissionPollPayload>("poll_audio_permission"),
      );
    } catch (nextError) {
      setSourceMessage(source, `Bluey could not read audio status: ${String(nextError)}`);
    } finally {
      setChecking(null);
    }
  };

  const startListeningToVerify = async (source: AudioSource) => {
    if (checking) return;
    setChecking({ source, action: "listening" });
    setSourceMessage(source, "");
    try {
      const current = await invoke<AudioPipelineStatusView>("daemon_listening_status");
      if (!isListening(current)) {
        await invoke<AudioPipelineStatusView>("daemon_toggle_listening");
      }
      const verification = applyVerification(
        source,
        await invoke<AudioPermissionPollPayload>("poll_audio_permission"),
      );
      if (verification === "needs_listening") {
        setSourceMessage(
          source,
          "Listening started. This warning will clear only after Bluey successfully receives this source.",
        );
      }
    } catch (nextError) {
      setSourceMessage(source, `Bluey could not start listening: ${String(nextError)}`);
    } finally {
      setChecking(null);
    }
  };

  if (denied.size === 0) return null;

  return (
    <div className="fixed inset-x-4 top-3 z-50 flex flex-col gap-2 lg:left-60" aria-live="polite">
      {Array.from(denied).map((source) => {
        const { title, body } = LABELS[source];
        return (
          <div
            key={source}
            role="alert"
            className="flex items-start justify-between rounded-xl border border-amber-500/45 bg-amber-950/95 px-4 py-3 text-sm text-amber-100 shadow-2xl shadow-black/35 backdrop-blur"
          >
            <div className="min-w-0">
              <p className="font-semibold">{title}</p>
              <p className="mt-0.5 max-w-2xl text-xs leading-5 text-amber-200/80">{body}</p>
              <div className="mt-2 flex flex-wrap gap-2">
                <button
                  type="button"
                  onClick={() => void openSettings(source)}
                  className="inline-flex min-h-8 items-center gap-1.5 rounded-md bg-amber-400 px-3 text-xs font-semibold text-zinc-950 hover:bg-amber-300"
                >
                  Open system settings
                  <ExternalLink aria-hidden="true" size={12} />
                </button>
                <button
                  type="button"
                  disabled={checking !== null}
                  onClick={() => void checkStatus(source)}
                  className="inline-flex min-h-8 items-center gap-1.5 rounded-md border border-amber-400/35 px-3 text-xs font-semibold text-amber-100 hover:bg-amber-400/10 disabled:opacity-50"
                >
                  {checking?.source === source && checking.action === "status" ? (
                    <LoaderCircle aria-hidden="true" className="animate-spin" size={12} />
                  ) : (
                    <RefreshCw aria-hidden="true" size={12} />
                  )}
                  {checking?.source === source && checking.action === "status"
                    ? "Checking..."
                    : "Check audio status"}
                </button>
                <button
                  type="button"
                  disabled={checking !== null}
                  onClick={() => void startListeningToVerify(source)}
                  className="inline-flex min-h-8 items-center gap-1.5 rounded-md border border-amber-400/35 px-3 text-xs font-semibold text-amber-100 hover:bg-amber-400/10 disabled:opacity-50"
                >
                  {checking?.source === source && checking.action === "listening" ? (
                    <LoaderCircle aria-hidden="true" className="animate-spin" size={12} />
                  ) : (
                    <Play aria-hidden="true" size={12} />
                  )}
                  {checking?.source === source && checking.action === "listening"
                    ? "Starting..."
                    : "Start listening to verify"}
                </button>
              </div>
              {messages[source] ? (
                <p className="mt-2 text-xs text-amber-200/80">{messages[source]}</p>
              ) : null}
            </div>
            <button
              type="button"
              onClick={() => dismiss(source)}
              className="ml-3 grid h-8 w-8 shrink-0 place-items-center rounded-md text-amber-300 hover:bg-amber-400/10 hover:text-amber-100"
              aria-label={`Dismiss ${title.toLowerCase()}`}
            >
              <X aria-hidden="true" size={16} />
            </button>
          </div>
        );
      })}
    </div>
  );
}

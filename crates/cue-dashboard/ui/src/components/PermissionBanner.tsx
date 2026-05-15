import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";

type AudioSource = "microphone" | "system";

interface PermissionDeniedPayload {
  source: AudioSource;
}

const LABELS: Record<AudioSource, { title: string; body: string }> = {
  microphone: {
    title: "Microphone access denied",
    body: "Cue needs microphone permission to capture your voice.",
  },
  system: {
    title: "System audio access denied",
    body: "Cue needs Screen Recording permission to capture system audio.",
  },
};

export function PermissionBanner() {
  const [denied, setDenied] = useState<Set<AudioSource>>(new Set());

  useEffect(() => {
    const unlisten = listen<PermissionDeniedPayload>(
      "audio_permission_denied",
      (event) => {
        setDenied((prev) => new Set(prev).add(event.payload.source));
      }
    );
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const dismiss = (source: AudioSource) => {
    setDenied((prev) => {
      const next = new Set(prev);
      next.delete(source);
      return next;
    });
  };

  const openSettings = (source: AudioSource) => {
    invoke("open_privacy_settings", { source }).catch(console.error);
  };

  if (denied.size === 0) return null;

  return (
    <div className="flex flex-col gap-2 px-4 pt-3">
      {Array.from(denied).map((source) => {
        const { title, body } = LABELS[source];
        return (
          <div
            key={source}
            className="flex items-start justify-between rounded-lg border border-amber-300 bg-amber-50 px-4 py-3 text-sm text-amber-900 dark:border-amber-700 dark:bg-amber-950 dark:text-amber-200"
          >
            <div>
              <p className="font-semibold">{title}</p>
              <p className="mt-0.5 opacity-80">{body}</p>
              <button
                onClick={() => openSettings(source)}
                className="mt-2 rounded bg-amber-600 px-3 py-1 text-xs font-medium text-white hover:bg-amber-700"
              >
                Open Privacy Settings
              </button>
            </div>
            <button
              onClick={() => dismiss(source)}
              className="ml-3 text-amber-600 hover:text-amber-800 dark:text-amber-400"
              aria-label="Dismiss"
            >
              ✕
            </button>
          </div>
        );
      })}
    </div>
  );
}

import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { X } from "lucide-react";
import { invoke } from "../lib/tauri";

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
            className="glass flex items-start justify-between rounded-lg border border-warning/40 bg-warning/10 px-4 py-3 text-callout text-warning"
          >
            <div>
              <p className="font-semibold">{title}</p>
              <p className="mt-0.5 text-footnote text-text-secondary">{body}</p>
              <button
                onClick={() => openSettings(source)}
                className="mt-2 rounded-md bg-warning/15 border border-warning/30 px-3 py-1 text-footnote font-medium text-warning transition-colors duration-200 hover:bg-warning/25"
              >
                Open Privacy Settings
              </button>
            </div>
            <button
              onClick={() => dismiss(source)}
              className="ml-3 text-text-tertiary transition-colors duration-200 hover:text-text-primary"
              aria-label="Dismiss"
            >
              <X size={14} />
            </button>
          </div>
        );
      })}
    </div>
  );
}

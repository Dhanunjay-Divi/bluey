import { useEffect, useState } from "react";
import { invoke } from "../lib/tauri";
import { listen } from "@tauri-apps/api/event";
import { Eye, X } from "lucide-react";

/**
 * Codex Stage 18 commit 8 follow-up: dashboard subscriber for the
 * auto_disguise_offer event the daemon emits when a meeting-app is
 * first detected. Shows a one-time toast with Accept / Decline
 * buttons. Each action calls the corresponding Tauri command which
 * persists the choice in CueSettings.
 *
 * Designed to be a passive overlay that lives at the dashboard root
 * (App.tsx). Renders nothing until the event fires; auto-dismisses
 * after either button or 30s timeout.
 */

interface MeetingAppEvent {
  bundle_id: string;
  detected_at_unix_ms: number;
}

export function AutoDisguiseToast() {
  const [event, setEvent] = useState<MeetingAppEvent | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    const unlisten = listen<MeetingAppEvent>("auto_disguise_offer", (e) => {
      setEvent(e.payload);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  // Auto-dismiss after 30s if user ignores.
  useEffect(() => {
    if (!event) return;
    const t = setTimeout(() => setEvent(null), 30_000);
    return () => clearTimeout(t);
  }, [event]);

  if (!event) return null;

  async function accept() {
    setBusy(true);
    try {
      await invoke("auto_disguise_accept");
    } catch (e) {
      console.warn("auto_disguise_accept failed", e);
    }
    setEvent(null);
    setBusy(false);
  }

  async function decline() {
    setBusy(true);
    try {
      await invoke("auto_disguise_decline");
    } catch (e) {
      console.warn("auto_disguise_decline failed", e);
    }
    setEvent(null);
    setBusy(false);
  }

  const friendlyName = friendly(event.bundle_id);

  return (
    <div
      className="glass-strong fixed top-4 right-4 z-50 max-w-sm rounded-xl p-4 animate-in slide-in-from-top-4 duration-200"
      role="status"
      aria-live="polite"
    >
      <div className="flex items-start gap-3">
        <div className="shrink-0 mt-0.5 inline-flex h-8 w-8 items-center justify-center rounded-lg bg-accent-subtle text-accent-subtle-text">
          <Eye className="h-4 w-4" />
        </div>
        <div className="flex-1 min-w-0">
          <h4 className="text-callout font-semibold text-text-primary">
            Auto-disguise during meetings?
          </h4>
          <p className="mt-1 text-footnote text-text-tertiary leading-relaxed">
            We noticed{" "}
            <span className="text-text-primary font-medium">{friendlyName}</span>{" "}
            is in front. Bluey can disguise itself automatically when
            meeting apps are open so screen-shares don&apos;t reveal it.
          </p>
          <div className="mt-3 flex gap-2">
            <button
              disabled={busy}
              onClick={accept}
              className="text-footnote font-medium px-3 py-1.5 rounded-md bg-accent hover:bg-accent-hover text-white disabled:opacity-50"
            >
              Yes, auto-disguise
            </button>
            <button
              disabled={busy}
              onClick={decline}
              className="text-footnote font-medium px-3 py-1.5 rounded-md bg-bg-raised-2 hover:bg-bg-raised-2 text-text-primary disabled:opacity-50"
            >
              No thanks
            </button>
          </div>
        </div>
        <button
          onClick={decline}
          className="shrink-0 text-text-tertiary hover:text-text-secondary"
          aria-label="Dismiss"
        >
          <X className="h-4 w-4" />
        </button>
      </div>
    </div>
  );
}

function friendly(bundleId: string): string {
  if (bundleId.includes("zoom")) return "Zoom";
  if (bundleId.includes("teams")) return "Microsoft Teams";
  if (bundleId.includes("slack")) return "Slack";
  if (bundleId.includes("webex")) return "Cisco Webex";
  if (bundleId.includes("Chrome")) return "Google Meet (Chrome)";
  if (bundleId.includes("thebrowser.Browser")) return "Arc";
  return "a meeting app";
}

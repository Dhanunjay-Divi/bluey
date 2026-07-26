// The meeting-prep BANNER window — a small, standalone Tauri window (label
// "banner", loaded with ?banner) that renders ONLY the meeting-prep card. It's
// separate from the main overlay so it never races the overlay's boot state or
// hides behind the full screen (the shared-window approach's problem). The daemon
// shows this window when a meeting nears and hides it on warm/dismiss.

import { useEffect, useState } from "react";
import { getClient } from "./lib";
import type { MeetingBanner } from "./lib/types";
import { MeetingBannerCard } from "./components/MeetingBannerCard";

/** Hide the native banner window (reused for the next meeting, so hide ≠ close). */
async function hideBannerWindow() {
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("hide_banner");
  } catch {
    // Not in Tauri (dev/preview) — nothing to hide.
  }
}

export function BannerWindow() {
  const client = getClient();
  const [banner, setBanner] = useState<MeetingBanner | null>(null);

  useEffect(() => {
    // Keep the event path too (works once wired), but the RELIABLE path is the
    // pull below — event delivery to this NSPanel webview proved unreliable.
    const off = client.onMeetingBanner(setBanner);

    // PULL the pending banner on mount and re-poll briefly, since the webview may
    // mount slightly before the daemon stores the banner. Stops once we have it.
    let stop = false;
    let tries = 0;
    const pull = async () => {
      if (stop) return;
      try {
        const { invoke } = await import("@tauri-apps/api/core");
        const raw = await invoke<string | null>("get_pending_banner");
        if (raw) {
          const c = JSON.parse(raw) as {
            event_id: string;
            title: string;
            start_epoch_secs: number;
            end_epoch_secs: number;
            participant_count: number;
            accepted_count: number;
            online: boolean;
          };
          setBanner({
            eventId: c.event_id,
            title: c.title,
            startEpochSecs: c.start_epoch_secs,
            endEpochSecs: c.end_epoch_secs,
            participantCount: c.participant_count,
            acceptedCount: c.accepted_count,
            online: c.online,
          });
          return; // got it — stop polling
        }
      } catch {
        // not in Tauri / command missing — ignore
      }
      tries += 1;
      if (tries < 20 && !stop) setTimeout(pull, 300);
    };
    void pull();

    return () => {
      stop = true;
      off();
    };
  }, [client]);

  // Nothing to show yet — keep the (transparent) window blank until the pull
  // resolves. The native side keeps the window hidden until a banner is pending,
  // so this transparent state is only ever momentary.
  if (!banner) return null;

  return (
    <MeetingBannerCard
      banner={banner}
      onWarmUp={() => {
        client.respondMeetingPrep(banner.eventId, true);
        setBanner(null);
        void hideBannerWindow();
      }}
      onDismiss={() => {
        client.respondMeetingPrep(banner.eventId, false);
        setBanner(null);
        void hideBannerWindow();
      }}
    />
  );
}

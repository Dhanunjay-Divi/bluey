// The meeting-prep BANNER window — a small, standalone Tauri window (label
// "banner", loaded with ?banner) that renders ONLY the meeting-prep card. It's
// separate from the main overlay so it never races the overlay's boot state or
// hides behind the full screen (the shared-window approach's problem). The daemon
// shows this window when a meeting nears and hides it on warm/dismiss.

import { useEffect, useState } from "react";
import { getClient } from "./lib";
import type { MeetingBanner } from "./lib/types";
import { MeetingBannerCard } from "./components/MeetingBannerCard";

function parseBanner(raw: string): MeetingBanner | null {
  try {
    const c = JSON.parse(raw) as {
      event_id: string;
      title: string;
      start_epoch_secs: number;
      end_epoch_secs: number;
      participant_count: number;
      accepted_count: number;
      online: boolean;
    };
    return {
      eventId: c.event_id,
      title: c.title,
      startEpochSecs: c.start_epoch_secs,
      endEpochSecs: c.end_epoch_secs,
      participantCount: c.participant_count,
      acceptedCount: c.accepted_count,
      online: c.online,
    };
  } catch {
    return null;
  }
}

/** Advance the occurrence queue; the native window hides only when it is empty. */
async function advanceBannerWindow(
  eventId: string,
  startEpochSecs: number,
): Promise<MeetingBanner | null> {
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    const next = await invoke<string | null>("hide_banner", {
      eventId,
      startEpochSecs,
    });
    return next ? parseBanner(next) : null;
  } catch {
    // Not in Tauri (dev/preview) — nothing to hide.
    return null;
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
          const pending = parseBanner(raw);
          if (pending) {
            setBanner(pending);
            return; // got it — stop polling
          }
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
        client.respondMeetingPrep(banner.eventId, banner.startEpochSecs, true);
        setBanner(null);
        void advanceBannerWindow(banner.eventId, banner.startEpochSecs).then(
          setBanner,
        );
      }}
      onDismiss={() => {
        client.respondMeetingPrep(banner.eventId, banner.startEpochSecs, false);
        setBanner(null);
        void advanceBannerWindow(banner.eventId, banner.startEpochSecs).then(
          setBanner,
        );
      }}
    />
  );
}

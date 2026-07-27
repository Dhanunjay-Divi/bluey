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
    // PUSH, not poll. The native side (ipc.rs) emits `overlay://banner` to THIS
    // window in the same callback that orders the panel front — i.e. the moment
    // the banner becomes visible, so emit-to-a-hidden-webview (the old
    // unreliability) can't happen. We fill the slot only when it's empty; once a
    // card is on screen the warm/dismiss handlers (advanceBannerWindow) own the
    // transition, so a duplicate push must not overwrite or resurrect it.
    let stop = false;
    let unlisten: (() => void) | undefined;

    void (async () => {
      try {
        const { listen } = await import("@tauri-apps/api/event");
        const handle = await listen<string>("overlay://banner", (event) => {
          if (stop) return;
          const pending = parseBanner(event.payload);
          setBanner((current) => current ?? pending); // fill empty slot only
        });
        if (stop) handle();
        else unlisten = handle;
      } catch {
        // not in Tauri (dev/preview) — no native push.
      }

      // ONE fetch on mount covers the fresh-spawn race: if the daemon stored the
      // banner before this webview mounted (so its push fired into nothing), we
      // pick it up here. No repeated polling — the push handles everything after.
      try {
        const { invoke } = await import("@tauri-apps/api/core");
        const raw = await invoke<string | null>("get_pending_banner");
        if (!stop && raw) {
          const pending = parseBanner(raw);
          if (pending) setBanner((current) => current ?? pending);
        }
      } catch {
        // not in Tauri / command missing — ignore.
      }
    })();

    return () => {
      stop = true;
      unlisten?.();
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

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

  useEffect(() => client.onMeetingBanner(setBanner), [client]);

  // Nothing to show yet — keep the (transparent) window blank until a banner
  // arrives. The window itself is hidden by the native side until then.
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

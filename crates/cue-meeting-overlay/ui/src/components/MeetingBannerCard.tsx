// The meeting-prep BANNER — a compact, branded floating card shown ~3 min before
// a calendar meeting (the calendar trigger). NOT a native macOS notification:
// like Granola, it's our own window content so it carries the Bluey mark, the
// meeting title + time, and a real action button. "Warm up the meeting" expands
// the overlay to the full meeting UI and tells the daemon to warm the backend +
// build pre-context; dismiss just clears it.

import type { MeetingBanner } from "../lib/types";
import { SparkleIcon, CloseIcon } from "./icons";

/** Format an epoch-seconds start/end as a local "10:00–10:30" range (or just the
 *  start when the end is unknown). */
function timeRange(startSecs: number, endSecs: number): string {
  const fmt = (s: number) =>
    new Date(s * 1000).toLocaleTimeString([], {
      hour: "numeric",
      minute: "2-digit",
    });
  if (endSecs > startSecs) return `${fmt(startSecs)}–${fmt(endSecs)}`;
  return fmt(startSecs);
}

/** "in 3 min" / "now" until the meeting starts. */
function untilLabel(startSecs: number): string {
  const mins = Math.round((startSecs - Date.now() / 1000) / 60);
  if (mins <= 0) return "now";
  return `in ${mins} min`;
}

export function MeetingBannerCard({
  banner,
  onWarmUp,
  onDismiss,
}: {
  banner: MeetingBanner;
  onWarmUp: () => void;
  onDismiss: () => void;
}) {
  const range = timeRange(banner.startEpochSecs, banner.endEpochSecs);
  const meta: string[] = [untilLabel(banner.startEpochSecs)];
  if (banner.participantCount > 0) {
    const acc =
      banner.acceptedCount > 0 ? ` (${banner.acceptedCount} accepted)` : "";
    meta.push(`${banner.participantCount} invited${acc}`);
  }
  if (banner.online) meta.push("online");

  return (
    <div className="fp-mbanner" data-floorplan="" role="alertdialog" aria-label="Meeting starting">
      {/* Solid accent bar (calendar event) — Granola's cue for a scheduled item. */}
      <span className="fp-mbanner-bar" aria-hidden />
      <div className="fp-mbanner-body">
        <div className="fp-mbanner-title">{banner.title}</div>
        <div className="fp-mbanner-time">{range}</div>
        <div className="fp-mbanner-meta">{meta.join(" · ")}</div>
      </div>
      <button className="fp-mbanner-cta" onClick={onWarmUp}>
        <SparkleIcon size={14} />
        Warm up the meeting
      </button>
      <button
        className="fp-mbanner-dismiss"
        onClick={onDismiss}
        title="Dismiss"
        aria-label="Dismiss"
      >
        <CloseIcon size={13} />
      </button>
    </div>
  );
}

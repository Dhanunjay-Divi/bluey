// The collapsed pill — the ambient, always-there surface when the panel is
// closed. It is NOT just a "tap to open" button: it shows live state at a
// glance (listening status + the latest heard line) and offers one-tap controls
// (mic toggle, expand) so the user rarely needs to expand mid-meeting.

import { useEffect, useRef, useState, type RefObject } from "react";
import type { MeetingClient } from "../lib/client";
import type { AgentSummary, ListeningState } from "../lib/types";
import { Glass, Mark, Waveform } from "./primitives";
import { SystemAudioIcon, StopIcon } from "./icons";

// Status dot color + label per listening state.
function statusOf(state: ListeningState): {
  color: string;
  label: string;
  live: boolean;
} {
  switch (state) {
    case "listening":
      return { color: "var(--mint, #34d399)", label: "listening", live: true };
    case "connecting":
      return { color: "#f5b942", label: "connecting", live: false };
    case "paused":
      return { color: "#f5b942", label: "paused", live: false };
    case "failed":
      return { color: "#ef5a5a", label: "audio issue", live: false };
    default:
      return { color: "var(--ink-4)", label: "idle", live: false };
  }
}

export function Pill({
  client,
  attached,
  onExpand,
  dragRef,
}: {
  client: MeetingClient;
  attached: AgentSummary | null;
  onExpand: () => void;
  dragRef: RefObject<HTMLDivElement | null>;
}) {
  const [listen, setListen] = useState<ListeningState>("idle");
  const [latest, setLatest] = useState<string>("");
  // A brief flash when a new line lands — ambient "something happened" feedback.
  const [flash, setFlash] = useState(false);
  const flashTimer = useRef<number | null>(null);

  // Track source + last-arrival so we ACCUMULATE incremental ~560ms fragments
  // into one flowing line (the model streams "It held on" then " the west"),
  // resetting on speaker change or a >2.5s pause. Showing each lone fragment
  // would look like "missing words".
  const lineRef = useRef<{ source: string; text: string; at: number }>({
    source: "",
    text: "",
    at: 0,
  });
  useEffect(() => {
    const offState = client.onListeningState(setListen);
    const offLine = client.onTranscript((line) => {
      if (!line.text) return;
      const now = Date.now();
      const cur = lineRef.current;
      const sameSpeaker = cur.source === line.source && cur.text !== "";
      const paused = now - cur.at > 2500;
      const text =
        sameSpeaker && !paused ? cur.text + line.text : line.text;
      lineRef.current = { source: line.source, text, at: now };
      // Show the recent tail so the latest words stay visible in the small pill.
      const trimmed = text.replace(/^\s+/, "");
      setLatest(trimmed.length > 90 ? "…" + trimmed.slice(-90) : trimmed);
      setFlash(true);
      if (flashTimer.current) window.clearTimeout(flashTimer.current);
      flashTimer.current = window.setTimeout(() => setFlash(false), 700);
    });
    return () => {
      offState();
      offLine();
      if (flashTimer.current) window.clearTimeout(flashTimer.current);
    };
  }, [client]);

  const s = statusOf(listen);
  // v1 captures SYSTEM audio (not the mic) — the toggle reflects whether that
  // live capture is on, and routes start/stop through the same daemon path.
  const capturing = listen === "listening" || listen === "connecting";

  return (
    <div style={{ position: "fixed", inset: 4, display: "flex" }}>
      <Glass
        radius="var(--r-pill)"
        style={{
          flex: 1,
          display: "flex",
          alignItems: "center",
          gap: 10,
          padding: "0 8px 0 13px",
          transition: "box-shadow .2s",
          boxShadow: flash
            ? "0 0 0 1.5px var(--tint-ink, #34d399) inset"
            : undefined,
        }}
      >
        {/* status dot */}
        <span
          aria-label={s.label}
          title={s.label}
          style={{
            width: 8,
            height: 8,
            borderRadius: 999,
            background: s.color,
            flex: "none",
            boxShadow: s.live ? `0 0 7px ${s.color}` : undefined,
            animation: s.live
              ? "blueyPulse 1.6s ease-in-out infinite"
              : undefined,
          }}
        />

        {/* tap-to-expand body (also the drag handle) — brand + latest heard line */}
        <div
          ref={dragRef}
          onClick={onExpand}
          role="button"
          aria-label="Expand Bluey"
          tabIndex={0}
          onKeyDown={(e) => {
            if (e.key === "Enter" || e.key === " ") onExpand();
          }}
          style={{
            display: "flex",
            alignItems: "center",
            gap: 8,
            flex: 1,
            minWidth: 0,
            cursor: "grab",
          }}
        >
          {attached ? <Waveform /> : <Mark size={17} />}
          <span
            style={{
              fontSize: 12.5,
              color: latest ? "var(--ink-2)" : "var(--ink-3)",
              whiteSpace: "nowrap",
              overflow: "hidden",
              textOverflow: "ellipsis",
              flex: 1,
              minWidth: 0,
            }}
          >
            {latest || (s.live ? "listening…" : "Bluey · tap to open")}
          </span>
        </div>

        {/* listen toggle — start/stop system-audio capture without expanding */}
        <button
          aria-label={capturing ? "Stop listening" : "Listen to system audio"}
          title={capturing ? "Stop listening" : "Listen (system audio)"}
          onClick={(e) => {
            e.stopPropagation();
            if (capturing) client.stopListening();
            else client.startListening({ microphone: false, system: true });
          }}
          style={{
            width: 28,
            height: 28,
            borderRadius: 999,
            border: "none",
            background: capturing ? "var(--tint-wash)" : "transparent",
            color: capturing ? "var(--tint-ink)" : "var(--ink-3)",
            cursor: "pointer",
            flex: "none",
            display: "inline-flex",
            alignItems: "center",
            justifyContent: "center",
          }}
        >
          {capturing ? <StopIcon size={15} /> : <SystemAudioIcon size={15} />}
        </button>
      </Glass>
    </div>
  );
}

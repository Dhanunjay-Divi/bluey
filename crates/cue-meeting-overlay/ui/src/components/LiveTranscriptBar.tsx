// The live transcription strip — the panel's bottom row.
//
// Replaces the old static status footer ("Claude Code · runs on your machine ·
// ⌘↵ ask"): that line never changed, so it spent permanent vertical space on
// something the user reads once. The transcript is the thing that IS changing
// while a meeting runs, so it earns the slot.
//
// Collapsed (default) it shows ONLY the newest spoken line — an ambient caption,
// one line, ellipsised. Expanding turns it into a scrollable history that
// auto-follows the newest line unless the user scrolls up to read back.

import { useEffect, useRef, useState } from "react";
import { getClient } from "../lib";
import { useMeetingState } from "../lib/meetingState";
import type { TranscriptLine } from "../lib/types";
import { ChevronIcon, MicIcon, SystemAudioIcon } from "./icons";

/** Max height of the expanded scroller — tall enough to read a few exchanges,
 *  short enough that the answer feed above it stays the focus. */
const EXPANDED_MAX_H = 168;

export function LiveTranscriptBar() {
  // Reads the shared session state directly (MeetingProvider sits above <App/>),
  // so the bar stays a drop-in bottom row with no prop-drilling through the shell.
  const { transcript: current, history } = useMeetingState();
  // Live listening state — so the placeholder tells the truth: "Listening…" ONLY
  // while audio is actually being captured, not before the user starts anything
  // (the reported bug: it said "Listening…" with nothing running).
  const [listening, setListening] = useState(false);
  useEffect(() => {
    const client = getClient();
    return client.onListeningState((s) => setListening(s === "listening"));
  }, []);
  const [expanded, setExpanded] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);
  // Auto-follow the newest line while pinned to the bottom; stop yanking the
  // user back down once they scroll up to read earlier speech (same discipline
  // as the answer feed).
  const pinnedRef = useRef(true);

  useEffect(() => {
    if (!expanded || !pinnedRef.current) return;
    const el = scrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [expanded, history, current]);

  // Drag-up-to-expand / drag-down-to-collapse: press on the row and move the
  // pointer vertically. A small threshold distinguishes a drag from a click, so
  // the row ALSO toggles on a plain click. Works whether or not any transcript
  // exists yet — the affordance is the row itself, not a tiny chevron.
  const dragStartY = useRef<number | null>(null);
  const draggedRef = useRef(false);
  const onRowPointerDown = (e: React.PointerEvent) => {
    dragStartY.current = e.clientY;
    draggedRef.current = false;
  };
  const onRowPointerMove = (e: React.PointerEvent) => {
    if (dragStartY.current === null) return;
    const dy = dragStartY.current - e.clientY; // up = positive
    if (Math.abs(dy) < 6) return;
    draggedRef.current = true;
    if (dy > 0 && !expanded) setExpanded(true);
    else if (dy < 0 && expanded) setExpanded(false);
  };
  const onRowPointerUp = () => {
    if (!draggedRef.current) setExpanded((v) => !v); // treat as a click
    dragStartY.current = null;
    draggedRef.current = false;
  };

  return (
    <div style={{ borderTop: "1px solid var(--line)" }}>
      {/* The always-present caption row — the whole row is the expand toggle
          (click OR drag up/down). No dependency on transcript existing. */}
      <div
        onPointerDown={onRowPointerDown}
        onPointerMove={onRowPointerMove}
        onPointerUp={onRowPointerUp}
        role="button"
        aria-expanded={expanded}
        aria-label={expanded ? "Collapse transcript" : "Expand transcript"}
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          // Right padding clears the resize grip in the corner.
          padding: "8px 26px 8px 14px",
          minHeight: 34,
          boxSizing: "border-box",
          cursor: "ns-resize",
          userSelect: "none",
        }}
      >
        <SourceGlyph line={current} />
        {current?.text && (
          <span
            style={{
              flexShrink: 0,
              fontSize: 10.5,
              fontWeight: 600,
              letterSpacing: ".03em",
              color: "var(--ink-4)",
            }}
          >
            {speakerLabel(current)}
          </span>
        )}
        <span
          style={{
            flex: 1,
            minWidth: 0,
            fontSize: 12,
            lineHeight: 1.35,
            color: current?.text ? "var(--ink-2)" : "var(--ink-4)",
            // ONE line only when collapsed — the caption must never grow the
            // panel or push the feed around as speech streams in.
            whiteSpace: "nowrap",
            overflow: "hidden",
            textOverflow: "ellipsis",
            fontStyle: current && !current.final ? "italic" : undefined,
          }}
          title={current?.text ?? undefined}
        >
          {current?.text?.trim() ||
            (listening ? "Listening…" : "Not listening — start audio to transcribe")}
        </span>
        {/* ALWAYS-visible chevron affordance (decorative — the whole row is the
            toggle via its pointer handlers, so this is just a pointer-events:none
            glyph that flips when expanded). No `hasAny` gate: the expand hint is
            present even before any speech, so the control is discoverable. */}
        <span
          style={{
            display: "inline-flex",
            alignItems: "center",
            justifyContent: "center",
            width: 22,
            height: 22,
            color: "var(--ink-3)",
            flexShrink: 0,
            pointerEvents: "none",
            transform: expanded ? "rotate(180deg)" : undefined,
            transition: "transform .15s ease",
          }}
        >
          <ChevronIcon size={14} />
        </span>
      </div>

      {/* The expanded scroller — full history, newest last. */}
      {expanded && (
        <div
          ref={scrollRef}
          onScroll={(e) => {
            const el = e.currentTarget;
            pinnedRef.current =
              el.scrollHeight - el.scrollTop - el.clientHeight < 24;
          }}
          style={{
            maxHeight: EXPANDED_MAX_H,
            overflowY: "auto",
            padding: "2px 26px 10px 14px",
            borderTop: "1px solid var(--line-2)",
          }}
        >
          {history.length === 0 ? (
            <div style={{ fontSize: 12, color: "var(--ink-4)", padding: "6px 0" }}>
              Nothing transcribed yet.
            </div>
          ) : (
            history.map((line, i) => (
              <div
                key={line.id ?? `${i}-${line.text.slice(0, 12)}`}
                style={{
                  display: "flex",
                  gap: 7,
                  alignItems: "baseline",
                  padding: "3px 0",
                  fontSize: 12,
                  lineHeight: 1.4,
                  color: "var(--ink-2)",
                }}
              >
                <span
                  style={{
                    flexShrink: 0,
                    color:
                      line.source === "mic"
                        ? "var(--tint-ink)"
                        : "var(--ink-4)",
                    fontWeight: 540,
                  }}
                >
                  {speakerLabel(line)}
                </span>
                <span style={{ minWidth: 0, wordBreak: "break-word" }}>
                  {line.text}
                </span>
              </div>
            ))
          )}
        </div>
      )}
    </div>
  );
}

/** The label shown before a line: the diarized speaker when resolved ("Speaker
 *  2"), else a source-based fallback — "You" for the microphone, "They" for the
 *  call. Mirrors the daemon's default channel labels so a line always has a
 *  who, even before diarization catches up. */
function speakerLabel(line: TranscriptLine): string {
  if (line.speaker && line.speaker.trim()) return line.speaker;
  return line.source === "mic" ? "You" : "They";
}

/** Which source Bluey heard this line from — speaker (the call) vs mic (you). */
function SourceGlyph({ line }: { line: TranscriptLine | null }) {
  const isMic = line?.source === "mic";
  return (
    <span
      style={{
        display: "inline-flex",
        flexShrink: 0,
        color: line?.text ? "var(--ink-3)" : "var(--ink-4)",
      }}
      title={isMic ? "Your microphone" : "System audio (the call)"}
    >
      {isMic ? <MicIcon size={13} /> : <SystemAudioIcon size={13} />}
    </span>
  );
}

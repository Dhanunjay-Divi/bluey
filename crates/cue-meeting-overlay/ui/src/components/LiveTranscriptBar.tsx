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
import type { SpeakerCandidate, TranscriptLine } from "../lib/types";
import { ChevronIcon, MicIcon, SystemAudioIcon } from "./icons";

/** Max height of the expanded scroller — tall enough to read a few exchanges,
 *  short enough that the answer feed above it stays the focus. */
const EXPANDED_MAX_H = 168;

/** How many trailing characters of the live line the collapsed caption shows.
 *  The grouped line grows as one speaker keeps talking; a plain left-anchored
 *  ellipsis would then freeze on the START of the line and hide the words being
 *  spoken NOW. Keeping the TAIL means the caption always shows current speech. */
const LIVE_CAPTION_TAIL = 90;

/** The collapsed one-line caption: the newest words being spoken (the tail of
 *  the current line), or a listening/idle placeholder. Trimming to the tail on a
 *  word boundary keeps the live words visible instead of a stale line-start. */
function liveCaption(text: string | undefined, listening: boolean): string {
  const t = text?.trim();
  if (!t) return listening ? "Listening…" : "Not listening — start audio to transcribe";
  if (t.length <= LIVE_CAPTION_TAIL) return t;
  const tail = t.slice(t.length - LIVE_CAPTION_TAIL);
  // Start at the next word boundary so we don't slice mid-word; prefix an
  // ellipsis to signal there's earlier text (the full line is in the tooltip
  // and the expanded scroller).
  const sp = tail.indexOf(" ");
  return "…" + (sp >= 0 ? tail.slice(sp + 1) : tail);
}

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
  // Which speaker id is currently being renamed inline (null = none editing).
  const [editingSpeaker, setEditingSpeaker] = useState<number | null>(null);
  // Calendar attendees for the active meeting — tap-to-pick names in rename.
  const [candidates, setCandidates] = useState<SpeakerCandidate[]>([]);
  useEffect(() => getClient().onMeetingCandidates(setCandidates), []);
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
          {liveCaption(current?.text, listening)}
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
            <div
              style={{ fontSize: 12, color: "var(--ink-4)", padding: "6px 0" }}
            >
              Nothing transcribed yet.
            </div>
          ) : (
            (() => {
              // The FIRST line index of the speaker being edited. Only that one
              // line renders the input — otherwise every line of that speaker
              // renders an editor and each blur re-fires the rename (the 4× bug).
              const editIdx =
                editingSpeaker == null
                  ? -1
                  : history.findIndex((l) => l.speakerId === editingSpeaker);
              return history.map((line, i) => (
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
                <EditableSpeaker
                  line={line}
                  candidates={candidates}
                  editing={i === editIdx}
                  onStartEdit={() =>
                    line.speakerId != null && setEditingSpeaker(line.speakerId)
                  }
                  onDone={() => setEditingSpeaker(null)}
                />
                <span style={{ minWidth: 0, wordBreak: "break-word" }}>
                  {line.text}
                </span>
              </div>
              ));
            })()
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

/** The speaker label before a transcript line. When the line has a numeric
 *  `speakerId` (a diarized call speaker, not "You"/"They"), the label is
 *  click-to-rename inline: clicking swaps it for a small text field; Enter or
 *  blur persists via `renameSpeaker(speakerId, name)` — which relabels every
 *  line of that speaker AND enrolls their voiceprint for future meetings. */
function EditableSpeaker({
  line,
  candidates,
  editing,
  onStartEdit,
  onDone,
}: {
  line: TranscriptLine;
  candidates: SpeakerCandidate[];
  editing: boolean;
  onStartEdit: () => void;
  onDone: () => void;
}) {
  const [draft, setDraft] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);
  const renameable = line.speakerId != null;
  const label = speakerLabel(line);
  const color = line.source === "mic" ? "var(--tint-ink)" : "var(--ink-4)";

  useEffect(() => {
    if (editing) {
      setDraft(line.speaker ?? "");
      inputRef.current?.focus();
      inputRef.current?.select();
    }
  }, [editing, line.speaker]);

  const apply = (name: string) => {
    const trimmed = name.trim();
    if (renameable && trimmed && trimmed !== line.speaker) {
      getClient().renameSpeaker(line.speakerId as number, trimmed);
    }
    onDone();
  };

  if (editing) {
    // Candidates matching what's typed (empty draft shows all) — the invitee
    // list narrows as the user types, and a custom name is always allowed.
    const q = draft.trim().toLowerCase();
    const matches = candidates
      .filter((c) => !q || c.name.toLowerCase().includes(q))
      .slice(0, 5);
    return (
      <span
        style={{ position: "relative", flexShrink: 0, display: "inline-flex" }}
      >
        <input
          ref={inputRef}
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") apply(draft);
            if (e.key === "Escape") onDone();
          }}
          // Delay blur so a candidate click registers before the input closes.
          onBlur={() => setTimeout(() => apply(draft), 120)}
          placeholder="Name this speaker"
          style={{
            width: 130,
            fontSize: 12,
            fontWeight: 540,
            color,
            background: "var(--glass)",
            border: "1px solid var(--tint)",
            borderRadius: "var(--r-pill)",
            padding: "1px 7px",
            outline: "none",
          }}
        />
        {matches.length > 0 && (
          <div
            style={{
              position: "absolute",
              top: "calc(100% + 4px)",
              left: 0,
              zIndex: 10000,
              display: "flex",
              flexDirection: "column",
              minWidth: 130,
              background: "var(--glass)",
              border: "1px solid var(--tint)",
              borderRadius: 8,
              padding: 4,
              boxShadow: "0 4px 16px rgba(0,0,0,0.18)",
            }}
          >
            {matches.map((c) => (
              <button
                key={c.email || c.name}
                // onMouseDown (not onClick) so it fires before the input's blur.
                onMouseDown={(e) => {
                  e.preventDefault();
                  apply(c.name);
                }}
                title={c.email || undefined}
                style={{
                  textAlign: "left",
                  background: "transparent",
                  border: "none",
                  color: "var(--ink-2)",
                  fontSize: 12,
                  padding: "4px 8px",
                  borderRadius: 6,
                  cursor: "pointer",
                }}
              >
                {c.name}
              </button>
            ))}
          </div>
        )}
      </span>
    );
  }

  return (
    <span
      onClick={renameable ? onStartEdit : undefined}
      title={renameable ? "Click to name this speaker" : undefined}
      style={{
        flexShrink: 0,
        color,
        fontWeight: 540,
        cursor: renameable ? "pointer" : "default",
        borderBottom: renameable
          ? "1px dashed var(--ink-5, transparent)"
          : "none",
      }}
    >
      {label}
    </span>
  );
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

// The premium speaker editor — the click-to-edit popover on a transcript
// speaker label. Replaces the minimal inline input with a modern card that lets
// you, in one place:
//   • rename this speaker (free text),
//   • pick a real calendar attendee (SpeakerCandidate),
//   • reassign THIS line to a different existing speaker, or
//   • assign it to a NEW speaker.
//
// It reuses the daemon contracts: renameSpeaker(id, name) relabels every line of
// a speaker; reassignSpan([segmentId], speakerId, name?) moves specific lines to
// another (or new) speaker and re-enrols the voiceprint. All corrections feed
// diarization fine-tuning daemon-side. Only lines with a resolved speakerId are
// editable (the fallback You/Them lines have no id to key on yet).

import { useEffect, useMemo, useRef, useState } from "react";
import { getClient } from "../../lib";
import type { SpeakerCandidate, TranscriptLine } from "../../lib/types";
import { CheckIcon, CloseIcon, PlusIcon, UserIcon } from "../icons";

/** A speaker known in the meeting, for the "reassign to" list. */
export interface KnownSpeaker {
  id: number;
  label: string;
}

function fallbackLabel(line: TranscriptLine): string {
  if (line.speaker && line.speaker.trim()) return line.speaker;
  return line.source === "mic" ? "You" : "Them";
}

export function SpeakerEditor({
  line,
  candidates,
  knownSpeakers,
  editing,
  onStartEdit,
  onDone,
}: {
  line: TranscriptLine;
  candidates: SpeakerCandidate[];
  /** Other speakers seen in the meeting, offered as reassign targets. */
  knownSpeakers: KnownSpeaker[];
  editing: boolean;
  onStartEdit: () => void;
  onDone: () => void;
}) {
  const [draft, setDraft] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  const renameable = line.speakerId != null;
  const label = fallbackLabel(line);

  useEffect(() => {
    if (editing) {
      setDraft(line.speaker ?? "");
      // focus after the pop-in so the caret lands in the field
      const t = window.setTimeout(() => {
        inputRef.current?.focus();
        inputRef.current?.select();
      }, 40);
      return () => window.clearTimeout(t);
    }
  }, [editing, line.speaker]);

  // Close on outside click / Escape.
  useEffect(() => {
    if (!editing) return;
    const onDoc = (e: MouseEvent) => {
      if (!rootRef.current?.contains(e.target as Node)) onDone();
    };
    document.addEventListener("mousedown", onDoc, true);
    return () => document.removeEventListener("mousedown", onDoc, true);
  }, [editing, onDone]);

  // Rename THIS speaker everywhere (name follows the voice).
  const rename = (name: string) => {
    const trimmed = name.trim();
    if (renameable && trimmed && trimmed !== line.speaker) {
      getClient().renameSpeaker(line.speakerId as number, trimmed);
    }
    onDone();
  };

  // Reassign JUST this line to another (or new) speaker.
  const reassignLine = (speakerId: number, name?: string) => {
    if (line.id) {
      getClient().reassignSpan([line.id], speakerId, name);
    }
    onDone();
  };

  const q = draft.trim().toLowerCase();
  const matches = useMemo(
    () =>
      candidates
        .filter((c) => !q || c.name.toLowerCase().includes(q))
        .slice(0, 4),
    [candidates, q],
  );
  const otherSpeakers = useMemo(
    () => knownSpeakers.filter((s) => s.id !== line.speakerId).slice(0, 4),
    [knownSpeakers, line.speakerId],
  );
  // A fresh speaker id = one past the max known (the daemon treats an unseen id
  // as a new speaker and enrolls it).
  const newSpeakerId = useMemo(
    () => knownSpeakers.reduce((m, s) => Math.max(m, s.id), -1) + 1,
    [knownSpeakers],
  );

  if (!editing) {
    return (
      <span
        onClick={renameable ? onStartEdit : undefined}
        title={renameable ? "Edit speaker" : undefined}
        className={`fp-speaker${renameable ? " fp-speaker-renameable" : ""}`}
      >
        {label}
      </span>
    );
  }

  return (
    <span className="fp-speaker-edit" ref={rootRef}>
      <span className="fp-speaker-current">{label}</span>
      <div className="fp-speditor" role="dialog" aria-label="Edit speaker">
        <div className="fp-speditor-head">
          <UserIcon size={13} />
          <span>Who said this?</span>
          <button
            className="fp-speditor-x"
            aria-label="Close"
            onClick={onDone}
          >
            <CloseIcon size={13} />
          </button>
        </div>

        {/* Rename this speaker (free text + submit). */}
        <form
          className="fp-speditor-namerow"
          onSubmit={(e) => {
            e.preventDefault();
            rename(draft);
          }}
        >
          <input
            ref={inputRef}
            className="fp-speditor-input"
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") onDone();
            }}
            placeholder="Name this speaker…"
          />
          <button
            type="submit"
            className="fp-speditor-apply"
            aria-label="Apply name"
            disabled={!draft.trim()}
          >
            <CheckIcon size={14} />
          </button>
        </form>

        {/* Calendar attendees (real people on the call). */}
        {matches.length > 0 && (
          <div className="fp-speditor-group">
            <div className="fp-speditor-kicker">From this meeting</div>
            {matches.map((c) => (
              <button
                key={c.email || c.name}
                className="fp-speditor-row"
                title={c.email || undefined}
                onMouseDown={(e) => {
                  e.preventDefault();
                  rename(c.name);
                }}
              >
                <span className="fp-speditor-avatar" aria-hidden>
                  {c.name.slice(0, 1).toUpperCase()}
                </span>
                <span className="fp-speditor-rowname">{c.name}</span>
              </button>
            ))}
          </div>
        )}

        {/* Reassign this line to another existing speaker. */}
        {otherSpeakers.length > 0 && (
          <div className="fp-speditor-group">
            <div className="fp-speditor-kicker">This line is actually</div>
            {otherSpeakers.map((s) => (
              <button
                key={s.id}
                className="fp-speditor-row"
                onMouseDown={(e) => {
                  e.preventDefault();
                  reassignLine(s.id);
                }}
              >
                <span className="fp-speditor-avatar is-alt" aria-hidden>
                  {s.label.slice(0, 1).toUpperCase()}
                </span>
                <span className="fp-speditor-rowname">{s.label}</span>
              </button>
            ))}
          </div>
        )}

        {/* Assign this line to a brand-new speaker. */}
        <button
          className="fp-speditor-new"
          onMouseDown={(e) => {
            e.preventDefault();
            reassignLine(newSpeakerId, draft.trim() || undefined);
          }}
        >
          <PlusIcon size={13} />
          Assign to a new speaker
        </button>
      </div>
    </span>
  );
}

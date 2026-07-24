// Inline click-to-rename speaker label — extracted from LiveTranscriptBar so
// both the glass transcript and the Open Floor Plan timeline share the ONE
// correct implementation (with the hard-won gotchas intact):
//   • only the first line of a speaker renders the editor (the "4× rename" bug),
//     enforced by the caller passing `editing` for exactly one line;
//   • candidate clicks use onMouseDown (fire before the input's blur);
//   • blur is delayed so a candidate click registers before the input closes;
//   • rename relabels ALL that speaker's lines + enrolls their voiceprint,
//     handled daemon-side by renameSpeaker(speakerId, name).

import { useEffect, useRef, useState } from "react";
import { getClient } from "../../lib";
import type { SpeakerCandidate, TranscriptLine } from "../../lib/types";

function speakerLabel(line: TranscriptLine): string {
  if (line.speaker && line.speaker.trim()) return line.speaker;
  // No diarized label yet: fall back to the capture channel, which is always
  // known. The mic is you; system audio is everyone else on the call. "Them"
  // reads far better than "They" as a per-line chip, and once diarization
  // resolves a real label it replaces this in place.
  return line.source === "mic" ? "You" : "Them";
}

export function EditableSpeaker({
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
    const q = draft.trim().toLowerCase();
    const matches = candidates
      .filter((c) => !q || c.name.toLowerCase().includes(q))
      .slice(0, 5);
    return (
      <span className="fp-speaker-edit">
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
          className="fp-speaker-input"
        />
        {matches.length > 0 && (
          <div className="fp-speaker-menu">
            {matches.map((c) => (
              <button
                key={c.email || c.name}
                // onMouseDown (not onClick) so it fires before the input's blur.
                onMouseDown={(e) => {
                  e.preventDefault();
                  apply(c.name);
                }}
                title={c.email || undefined}
                className="fp-speaker-cand"
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
      className={`fp-speaker${renameable ? " fp-speaker-renameable" : ""}`}
    >
      {label}
    </span>
  );
}

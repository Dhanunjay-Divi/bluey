// Pure transcript grouping — shared by the LIVE meeting view (MeetingProvider)
// and the READ-ONLY past-meeting viewer (MeetingsScreen), so both render a
// meeting's transcript byte-for-byte identically.
//
// Parakeet streams INCREMENTAL ~560ms fragments ("It held on" then " the
// west"). Same-speaker fragments ACCUMULATE into one flowing line; a speaker
// change or a >PAUSE_MS live gap starts a new one. Reconciliation is by segment
// id: a fragment whose id is already folded in is a duplicate and skipped, so an
// async seed and the live stream can interleave in any order without dropping or
// double-counting a line.
//
// This module is PURE (no React). It exposes:
//   - the grouping constants (CAP / MAX_LINES / PAUSE_MS),
//   - tailCap / toHistory helpers,
//   - createTranscriptGrouper(): a stateful-but-plain accumulator whose
//     foldSegment(seg, paused) returns the fresh { history, caption } after
//     folding one segment. The LIVE provider drives it with real pause timing;
//     the read-only viewer folds its whole snapshot with paused=false.

import type { TranscriptLine } from "./types";

// The ambient caption shows only the tail of the current line so the 2-line
// clamp displays the newest words, not the start of a long stretch.
export const CAP = 240;
// Full history is kept un-capped per line but bounded in line COUNT so a very
// long meeting can't grow the DOM unbounded.
export const MAX_LINES = 400;
// A speaker's line ends after this idle gap; the next LIVE fragment starts a new
// one. (A seed has no per-segment timing, so it groups on speaker change only —
// the daemon persists consecutive same-speaker fragments that ARE one line.)
export const PAUSE_MS = 2500;

// One grouped transcript line, plus the set of segment ids folded into it. The
// ids are the reconciliation key: a live/seed segment already listed here is a
// duplicate and skipped. The public `history` projects these to plain
// `TranscriptLine`s (ids stripped) for rendering.
export interface GroupedLine extends TranscriptLine {
  /** The daemon segment ids folded into this grouped line, in arrival order. */
  ids: string[];
}

// Tail-cap a line for the ambient caption: keep only the last CAP chars, and
// drop a leading partial word so the 2-line clamp shows clean, current words.
export function tailCap(text: string): string {
  if (text.length <= CAP) return text;
  const tail = text.slice(-CAP);
  const sp = tail.indexOf(" ");
  return sp > 0 ? tail.slice(sp + 1) : tail;
}

// Project the internal grouped lines to the public render shape (drop `ids`).
export function toHistory(lines: GroupedLine[]): TranscriptLine[] {
  return lines.map(({ ids: _ids, ...line }) => line);
}

/** The result of folding one segment: the fresh full history and the ambient
 *  caption tail (the current line, tail-capped) — or null caption when the
 *  segment was a duplicate and nothing changed. */
export interface FoldResult {
  history: TranscriptLine[];
  caption: TranscriptLine | null;
}

/** A plain accumulator over grouped transcript lines. Owns the grouped lines and
 *  the seen-id dedup set; NOT React-aware. Both the live provider and the
 *  read-only viewer construct one and drive it through `foldSegment`. */
export interface TranscriptGrouper {
  /** Fold one finalized segment into the grouped history. `paused` forces a new
   *  line on a live gap; a seed passes `paused=false` so consecutive
   *  same-speaker persisted fragments group into one flowing line. A segment
   *  whose id is already folded in is a no-op ({ history, caption:null }). */
  foldSegment(seg: TranscriptLine, paused: boolean): FoldResult;
  /** Patch the diarized speaker label of the grouped line containing
   *  `segmentId` (labels arrive AFTER the text — the daemon's live diarize
   *  tick pushes upgrades by segment id). Returns the fresh history, or null
   *  when no line contains that id (stale id, line rotated out) or the label
   *  is already set — callers skip the re-render then. */
  setSpeaker(segmentId: string, speaker: string): TranscriptLine[] | null;
  /** The current grouped history projected to the render shape. */
  history(): TranscriptLine[];
}

export function createTranscriptGrouper(): TranscriptGrouper {
  let lines: GroupedLine[] = [];
  const seen = new Set<string>();

  const foldSegment = (seg: TranscriptLine, paused: boolean): FoldResult => {
    if (seg.id && seen.has(seg.id)) {
      return { history: toHistory(lines), caption: null };
    }
    if (seg.id) seen.add(seg.id);

    const last = lines.length > 0 ? lines[lines.length - 1] : null;
    // Same channel, no live gap, and no CONFLICTING diarized speaker labels.
    // (Seeded past-meeting lines carry labels on the segments themselves; two
    // different voices on the same channel must not fold into one line.)
    const continues =
      last != null &&
      last.source === seg.source &&
      !paused &&
      !(last.speaker && seg.speaker && last.speaker !== seg.speaker);

    let next: GroupedLine[];
    if (continues && last) {
      // Extend the current line; RAW concat preserves the model's leading-space
      // word boundaries (re-spacing would split words).
      const merged: GroupedLine = {
        ...last,
        text: last.text + seg.text,
        // Adopt the segment's label when the line has none yet (a seed whose
        // first fragment predates the diarize tick that labeled the rest).
        speaker: last.speaker ?? seg.speaker,
        ids: seg.id ? [...last.ids, seg.id] : last.ids,
      };
      next = [...lines.slice(0, -1), merged];
    } else {
      next = [...lines, { ...seg, ids: seg.id ? [seg.id] : [] }];
    }
    if (next.length > MAX_LINES) {
      next = next.slice(-MAX_LINES);
    }
    lines = next;

    // Ambient caption = the tail of the CURRENT (possibly just-extended) line.
    const currentText = next[next.length - 1].text;
    return {
      history: toHistory(next),
      caption: { ...seg, text: tailCap(currentText) },
    };
  };

  const setSpeaker = (
    segmentId: string,
    speaker: string,
  ): TranscriptLine[] | null => {
    const idx = lines.findIndex((l) => l.ids.includes(segmentId));
    if (idx < 0 || lines[idx].speaker === speaker) return null;
    const next = [...lines];
    next[idx] = { ...next[idx], speaker };
    lines = next;
    return toHistory(next);
  };

  return {
    foldSegment,
    setSpeaker,
    history: () => toHistory(lines),
  };
}

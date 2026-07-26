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
// Sentence-boundary break: once a grouped line reaches this length AND ends on
// sentence-final punctuation, the NEXT fragment starts a fresh line so the
// transcript reads as paragraphs, not one ever-growing block. The model DOES
// emit punctuation (verified end-to-end: it produces ". ? ," from prosody), so
// this fires at the model's real sentence ends. The length floor stops
// abbreviations / "U.S." / "3.5" from shattering the line into fragments.
export const SENTENCE_MIN_CHARS = 160;
// True when `text` ends on sentence-final punctuation (optionally followed by a
// closing quote/bracket and trailing space) — the break point for a new line.
function endsSentence(text: string): boolean {
  return /[.!?]["')\]]?\s*$/.test(text);
}
// Punctuation glue: the streaming model emits punctuation as its OWN space-
// prefixed token and each token lands as a SEPARATE segment ("well " then ". "),
// so concatenating segments for display yields detached "well . These". Pull a
// space that sits directly before a punctuation mark back onto the preceding
// word. Safe: a space before "." "," … is never a real word boundary, so this
// never merges two words — it only removes the model's stray pre-punctuation gap.
function gluePunctuation(text: string): string {
  return text.replace(/\s+([.,?!;:%)\]}])/g, "$1");
}

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

// Project the internal grouped lines to the public render shape. `ids` (the
// internal reconciliation key) is surfaced as `memberIds` so callers can resolve
// an ATTACHMENT ANCHOR (a segment id) to the grouped line that contains it — the
// UI groups many segments into one line, so an attachment anchored to segment N
// must find WHICH line holds N. The renderer otherwise ignores it.
export function toHistory(lines: GroupedLine[]): TranscriptLine[] {
  return lines.map(({ ids, ...line }) => ({ ...line, memberIds: ids }));
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
  setSpeaker(
    segmentId: string,
    speaker: string,
    speakerId?: number | null,
  ): TranscriptLine[] | null;
  /** The current grouped history projected to the render shape. */
  history(): TranscriptLine[];
}

export function createTranscriptGrouper(): TranscriptGrouper {
  let lines: GroupedLine[] = [];
  // segment id -> the FULL accumulated text currently shown for that id.
  //
  // WIRE REALITY (verified against the daemon, app.rs:9436-9494): the daemon
  // COALESCES a same-speaker run into ONE stored segment and re-broadcasts under
  // that segment's STABLE id. Two DIFFERENT shapes reach us under the same id:
  //   • LIVE: each card carries only the NEW fragment ("To the", " west side")
  //     via `text_raw` — the delta, not the whole line.
  //   • SEED (remount / rehydrate snapshot): one card carries the segment's FULL
  //     accumulated text at once ("To the west side of the building").
  // The id is deliberately stable (setSpeaker, attachment anchors, span assign
  // all address segments by id), so the wire id MUST NOT change — a repeat id is
  // a CONTINUATION, not a duplicate. The old `Set<id>` dropped every repeat, so
  // only the first fragment of each turn ever rendered.
  //
  // Unifying rule: keep the full accumulated text per id and reconcile the line
  // to it. A card is either (a) an exact resend of what we already have (drop),
  // (b) a full-text seed that is a superset of / equal to the accumulation
  // (adopt the longer of the two), or (c) a live delta to append. This is
  // idempotent under out-of-order seed↔live interleave AND under a delta that
  // repeats an EARLIER (not just the last) fragment.
  const accumById = new Map<string, string>();

  const foldSegment = (seg: TranscriptLine, paused: boolean): FoldResult => {
    const segId = seg.id;
    if (segId) {
      const acc = accumById.get(segId);
      if (acc !== undefined) {
        if (seg.text === acc) {
          // Exact resend of the whole accumulation (remount / seam replay of the
          // same card). Nothing new — drop. NOTE: we deliberately do NOT drop a
          // delta merely because it equals a tail already present — a speaker
          // genuinely repeats phrases ("save you twenty thousand dollars ...
          // twenty thousand dollars"), and dropping the repeat would silently
          // eat real speech.
          return { history: toHistory(lines), caption: null };
        }
        // The NEW text this card contributes: a full-text seed (remount snapshot)
        // supersedes the accumulation, so the delta is the suffix past `acc`; a
        // live card carries just the delta already.
        const isSeedSuperset =
          seg.text.length > acc.length && seg.text.startsWith(acc);
        const delta = isSeedSuperset ? seg.text.slice(acc.length) : seg.text;
        accumById.set(segId, gluePunctuation(acc + delta));

        // The LAST line carrying this id — a long coalesced turn may already have
        // been split into several paragraph lines that ALL share the id, and the
        // continuation must extend the most recent one, not the first (using
        // findIndex here appended every later delta to the FIRST paragraph and
        // never broke again — the "one giant top line" bug).
        let idx = -1;
        for (let i = lines.length - 1; i >= 0; i--) {
          if (lines[i].ids.includes(segId)) {
            idx = i;
            break;
          }
        }
        if (idx < 0) {
          // Owning line aged out past MAX_LINES — record, drop the delta.
          return { history: toHistory(lines), caption: null };
        }
        const owner = lines[idx];
        // PARAGRAPH BREAK inside a coalesced turn: once the owning line has
        // reached the length floor AND ends on sentence-final punctuation, the
        // continuation starts a NEW line — otherwise a long same-speaker turn
        // (all one segment id) grows into a single ever-expanding block pinned at
        // the top (the "transcript populating on the top" bug). The new line
        // carries the SAME id, so attachment / Q&A anchors that resolve by id
        // still find a line — every line of the turn contains the id.
        const isLastLine = idx === lines.length - 1;
        const breakHere =
          isLastLine &&
          owner.text.length >= SENTENCE_MIN_CHARS &&
          endsSentence(owner.text);

        let next: GroupedLine[];
        let currentText: string;
        if (breakHere) {
          const fresh: GroupedLine = {
            ...seg,
            // Drop a leading space so a fresh paragraph doesn't start indented.
            text: gluePunctuation(delta).replace(/^\s+/, ""),
            speaker: owner.speaker ?? seg.speaker,
            speakerId: owner.speakerId ?? seg.speakerId,
            ids: seg.id ? [seg.id] : [],
          };
          next = [...lines, fresh];
          currentText = fresh.text;
        } else {
          const grown: GroupedLine = {
            ...owner,
            text: gluePunctuation(owner.text + delta),
            // Adopt a label the line didn't have yet (diarize can land between the
            // first fragment and a later coalesced delta).
            speaker: owner.speaker ?? seg.speaker,
            speakerId: owner.speakerId ?? seg.speakerId,
          };
          next = [...lines];
          next[idx] = grown;
          currentText = grown.text;
        }
        if (next.length > MAX_LINES) next = next.slice(-MAX_LINES);
        lines = next;
        return {
          history: toHistory(next),
          caption: { ...seg, text: tailCap(currentText) },
        };
      }
      accumById.set(segId, seg.text);
    }

    const last = lines.length > 0 ? lines[lines.length - 1] : null;
    // Break a long line at a sentence end so the transcript reads as paragraphs,
    // not one ever-growing block. The length floor (SENTENCE_MIN_CHARS) keeps
    // abbreviations / decimals ("U.S.", "3.5") from shattering a line; the break
    // only ever ENDS the current line before appending the next fragment, so it
    // is safe on the seed path too (a persisted line is never re-split — the
    // boundary just decides where the FOLLOWING fragment starts). Live and seed
    // fold identically, preserving the byte-for-byte-identical render guarantee.
    const sentenceBreak =
      last != null &&
      last.text.length >= SENTENCE_MIN_CHARS &&
      endsSentence(last.text);
    // Same channel, no live gap, no CONFLICTING diarized speaker labels, and not
    // just past a sentence boundary. (Seeded past-meeting lines carry labels on
    // the segments themselves; two different voices on the same channel must not
    // fold into one line.)
    const continues =
      last != null &&
      last.source === seg.source &&
      !paused &&
      !sentenceBreak &&
      !(last.speaker && seg.speaker && last.speaker !== seg.speaker) &&
      // Break on a diarized speaker_id change too — otherwise a segment the user
      // just REASSIGNED to a different speaker would merge back into the adjacent
      // line (the "my reassigned block globbed into the live transcript" bug).
      // Two known-but-different ids never share a line.
      !(
        last.speakerId != null &&
        seg.speakerId != null &&
        last.speakerId !== seg.speakerId
      );

    let next: GroupedLine[];
    if (continues && last) {
      // Extend the current line; RAW concat preserves the model's leading-space
      // word boundaries (re-spacing would split words).
      const merged: GroupedLine = {
        ...last,
        // RAW concat preserves the model's leading-space word boundaries, then
        // gluePunctuation pulls back the stray space before a punctuation segment
        // ("well " + ". These" -> "well. These") — the detached-punctuation fix.
        text: gluePunctuation(last.text + seg.text),
        // Adopt the segment's label/id when the line has none yet (a seed whose
        // first fragment predates the diarize tick that labeled the rest).
        speaker: last.speaker ?? seg.speaker,
        speakerId: last.speakerId ?? seg.speakerId,
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
    speakerId?: number | null,
  ): TranscriptLine[] | null => {
    // Rename echo: empty segmentId + a speakerId → relabel EVERY line of that
    // speaker at once (a user rename applies to the whole person, not one line).
    if (!segmentId && speakerId != null) {
      let changed = false;
      const next = lines.map((l) => {
        if (l.speakerId === speakerId && l.speaker !== speaker) {
          changed = true;
          return { ...l, speaker };
        }
        return l;
      });
      if (!changed) return null;
      lines = next;
      return toHistory(next);
    }
    // Live diarize upgrade: relabel the single line holding this segment id, and
    // record its numeric speakerId so the label becomes clickable-to-rename.
    const idx = lines.findIndex((l) => l.ids.includes(segmentId));
    if (idx < 0) return null;
    if (lines[idx].speaker === speaker && lines[idx].speakerId === speakerId) {
      return null;
    }
    const next = [...lines];
    next[idx] = {
      ...next[idx],
      speaker,
      speakerId: speakerId ?? next[idx].speakerId,
    };
    lines = next;
    return toHistory(next);
  };

  return {
    foldSegment,
    setSpeaker,
    history: () => toHistory(lines),
  };
}

// The MEETING is the source of truth; the overlay is a VIEW of it (never the
// owner). This provider is the single, never-unmounted home for the meeting's
// live session state — transcript, scrollable history, the Q&A feed, and the
// detected for-me question. It sits ABOVE <App/> (see main.tsx), so collapsing
// to the pill, first-run onboarding, or a tab switch cannot unmount it and wipe
// the session (the two bugs this fixes).
//
// Two complementary layers keep the view faithful to the meeting:
//   Fix A — because this state lives above the collapsing panel, a
//           collapse/expand no longer destroys it; React useState survives.
//   Fix B — on mount we fetch the active meeting's persisted transcript + Q&A
//           once (client.meetingState()) and SEED this state, so even a full
//           overlay process restart (JS memory gone; daemon still holds it)
//           rehydrates the view.
//
// The live subscriptions (onTranscript, onForMeQuestion) also live HERE, as the
// single owner — never in a view — so ingestion continues while collapsed and
// there is no double-subscribe/double-append.
//
// Seed↔live reconciliation is by SEGMENT ID, not by text/source. Every finalized
// segment has one stable daemon id: the rehydrate snapshot carries it
// (MeetingTranscriptLine.id) and the live push carries the SAME id
// (TranscriptLine.id — the daemon sets the live card's id to the segment id). A
// live segment whose id is already folded into history is skipped, so the async
// seed and the live stream can interleave in any order without dropping or
// duplicating a line, and grouping stays consistent across the seed/live seam.

import {
  createContext,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { getClient } from ".";
import type { AnswerState } from "../components/AnswerCard";
import type { AnswerStatusStep, TranscriptLine } from "./types";

/** One Q&A exchange in the conversation feed: the question asked + the streamed
 *  answer + the live status steps for that turn. Past turns stay on screen so
 *  the meeting builds a scrollable history instead of each ask replacing the
 *  last. */
export interface Turn {
  id: number;
  question: string;
  answer: AnswerState;
  statusSteps: AnswerStatusStep[];
  statusDone: boolean;
}

/** A daemon-detected for-me question (master doc §6). */
export interface DetectedQuestion {
  text: string;
  title?: string;
}

interface MeetingStateValue {
  /** Ambient caption tail — the newest speech only, tail-capped. */
  transcript: TranscriptLine | null;
  /** Full scrollable transcript history (one entry per grouped line). */
  history: TranscriptLine[];
  /** The conversation feed — every asked question + its answer, in order. */
  turns: Turn[];
  /** The current detected for-me question, or null when none/dismissed. */
  detectedQ: DetectedQuestion | null;
  setDetectedQ: (q: DetectedQuestion | null) => void;
  /** Append a fresh turn to the feed (used by runAsk when a new ask starts). */
  appendTurn: (turn: Turn) => void;
  /** Patch an existing turn by id as its answer streams in. */
  patchTurn: (
    id: number,
    patch: Pick<Turn, "answer" | "statusSteps" | "statusDone">,
  ) => void;
  /** Monotonic turn-id source. runAsk does `++turnSeq.current` to mint an id;
   *  seeded on rehydrate so a live ask never collides with a persisted turn. */
  turnSeq: React.MutableRefObject<number>;
  /** false until the Fix-B snapshot seed has applied once. */
  rehydrated: boolean;
}

const MeetingStateContext = createContext<MeetingStateValue | null>(null);

// The ambient caption shows only the tail of the current line so the 2-line
// clamp displays the newest words, not the start of a long stretch.
const CAP = 240;
// Full history is kept un-capped per line but bounded in line COUNT so a very
// long meeting can't grow the DOM unbounded.
const MAX_LINES = 400;
// A speaker's line ends after this idle gap; the next LIVE fragment starts a new
// one. (The seed has no per-segment timing, so it groups on speaker change only
// — the daemon persists consecutive same-speaker fragments that ARE one line.)
const PAUSE_MS = 2500;

// One grouped transcript line, plus the set of segment ids folded into it. The
// ids are the reconciliation key: a live/seed segment already listed here is a
// duplicate and skipped, so the async seed and the live stream can interleave
// without dropping or double-counting a line. The public `history` projects
// these to plain `TranscriptLine`s (ids stripped) for rendering.
interface GroupedLine extends TranscriptLine {
  /** The daemon segment ids folded into this grouped line, in arrival order. */
  ids: string[];
}

// Tail-cap a line for the ambient caption: keep only the last CAP chars, and
// drop a leading partial word so the 2-line clamp shows clean, current words.
function tailCap(text: string): string {
  if (text.length <= CAP) return text;
  const tail = text.slice(-CAP);
  const sp = tail.indexOf(" ");
  return sp > 0 ? tail.slice(sp + 1) : tail;
}

// Project the internal grouped lines to the public render shape (drop `ids`).
function toHistory(lines: GroupedLine[]): TranscriptLine[] {
  return lines.map(({ ids: _ids, ...line }) => line);
}

export function MeetingProvider({ children }: { children: ReactNode }) {
  const client = getClient();

  const [transcript, setTranscript] = useState<TranscriptLine | null>(null);
  const [history, setHistory] = useState<TranscriptLine[]>([]);
  const [turns, setTurns] = useState<Turn[]>([]);
  const [detectedQ, setDetectedQ] = useState<DetectedQuestion | null>(null);
  const [rehydrated, setRehydrated] = useState(false);

  // The authoritative grouped history, keyed by member segment ids. This ref is
  // the single writer of the `history` state; both the seed and the live sub go
  // through `foldSegment` below so seed↔live reconciliation is one code path.
  const linesRef = useRef<GroupedLine[]>([]);
  // Every segment id already folded into a line — the O(1) dedup guard that
  // makes the async seed and live stream idempotent regardless of arrival order.
  const seenIdsRef = useRef<Set<string>>(new Set());
  // Wall-clock of the last LIVE fold, for the >PAUSE_MS new-line rule. The seed
  // doesn't touch this (it has no timing); it stays 0 until the first live line.
  const lastLiveAtRef = useRef(0);
  const turnSeq = useRef(0);

  const appendTurn = (turn: Turn) => setTurns((prev) => [...prev, turn]);
  const patchTurn = (
    id: number,
    patch: Pick<Turn, "answer" | "statusSteps" | "statusDone">,
  ) =>
    setTurns((prev) => prev.map((t) => (t.id === id ? { ...t, ...patch } : t)));

  // Fold one finalized segment into the grouped history and refresh the caption.
  // Shared by the seed (no id-dedup needed but harmless) and the live sub (where
  // it makes the two paths idempotent). `paused` forces a new line on a live
  // gap; the seed passes `paused=false` so consecutive same-speaker persisted
  // fragments group into one flowing line (not the raw ~560ms fragments — the
  // rehydrate-fidelity fix). A segment with no id (should not happen for finals)
  // is treated as un-dedupable and always appended.
  const foldSegment = (seg: TranscriptLine, paused: boolean) => {
    if (seg.id && seenIdsRef.current.has(seg.id)) return;
    if (seg.id) seenIdsRef.current.add(seg.id);

    const lines = linesRef.current;
    const last = lines.length > 0 ? lines[lines.length - 1] : null;
    const continues = last != null && last.source === seg.source && !paused;

    let next: GroupedLine[];
    if (continues && last) {
      // Extend the current line; RAW concat preserves the model's leading-space
      // word boundaries (re-spacing would split words).
      const merged: GroupedLine = {
        ...last,
        text: last.text + seg.text,
        ids: seg.id ? [...last.ids, seg.id] : last.ids,
      };
      next = [...lines.slice(0, -1), merged];
    } else {
      next = [...lines, { ...seg, ids: seg.id ? [seg.id] : [] }];
    }
    if (next.length > MAX_LINES) {
      next = next.slice(-MAX_LINES);
    }
    linesRef.current = next;
    setHistory(toHistory(next));

    // Ambient caption = the tail of the CURRENT (possibly just-extended) line.
    const currentText = next[next.length - 1].text;
    setTranscript({ ...seg, text: tailCap(currentText) });
  };

  // ---- Fix B: seed once from the active meeting's persisted snapshot ----
  // Runs before live data matters. We fold each persisted segment through the
  // SAME grouping path the live stream uses (paused=false → group consecutive
  // same-speaker fragments), so a rehydrated meeting shows the same flowing
  // lines as a live one, and every seeded id is registered so a live push of the
  // same segment (the same-tick fetch/push race) is deduped by id.
  useEffect(() => {
    let live = true;
    client
      .meetingState()
      .then((snap) => {
        if (!live) return;
        for (const l of snap.transcript) {
          foldSegment(
            {
              id: l.id,
              source: l.source,
              speaker: l.speaker,
              text: l.text,
              final: true,
            },
            false,
          );
        }

        // Seed the Q&A feed. Numeric ids 1..N in order; turnSeq set to N so the
        // next runAsk `++turnSeq.current` yields N+1 and can't collide.
        const seededTurns: Turn[] = snap.conversation.map((t, i) => ({
          id: i + 1,
          question: t.question,
          answer: {
            agentLabel: "Bluey",
            text: t.answer,
            sources: [],
            tools: [],
            done: true,
            cost: undefined,
          },
          statusSteps: [],
          statusDone: true,
        }));
        setTurns(seededTurns);
        turnSeq.current = snap.conversation.length;

        // Treat the seed as "just heard" so the FIRST live fragment continuing
        // the same speaker within PAUSE_MS joins the seeded tail into one flowing
        // line instead of being forced onto a new line. A genuinely new segment
        // (new id) is still appended; only the pause-split timing is primed here.
        lastLiveAtRef.current = Date.now();
        setRehydrated(true);
      })
      .catch(() => {
        // No meeting / daemon unreachable: still mark rehydrated so the live
        // path takes over cleanly (empty view is the correct empty state).
        if (!live) return;
        lastLiveAtRef.current = Date.now();
        setRehydrated(true);
      });
    return () => {
      live = false;
    };
    // foldSegment closes only over refs + stable setters, so it needn't be a dep.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [client]);

  // ---- live transcript accumulation (single owner) ----
  // Parakeet streams INCREMENTAL ~560ms fragments ("It held on" then " the
  // west"). Same-speaker fragments ACCUMULATE into one flowing line; a speaker
  // change or a >PAUSE_MS gap starts a new one. Reconciliation is by segment id
  // (foldSegment), so a fragment already seeded from the snapshot is dropped —
  // no text/source heuristic, and no clobber of live lines that arrived during
  // the async seed (they carry ids and are re-merged, not replaced).
  useEffect(
    () =>
      client.onTranscript((l) => {
        if (!l.final) return;
        const now = Date.now();
        const paused = now - lastLiveAtRef.current > PAUSE_MS;
        lastLiveAtRef.current = now;
        foldSegment(l, paused);
      }),
    [client],
  );

  // ---- detected for-me question (single owner) ----
  useEffect(() => client.onForMeQuestion((q) => setDetectedQ(q)), [client]);

  const value: MeetingStateValue = {
    transcript,
    history,
    turns,
    detectedQ,
    setDetectedQ,
    appendTurn,
    patchTurn,
    turnSeq,
    rehydrated,
  };

  return (
    <MeetingStateContext.Provider value={value}>
      {children}
    </MeetingStateContext.Provider>
  );
}

/** Read the meeting session state. Must be called under <MeetingProvider>. */
export function useMeetingState(): MeetingStateValue {
  const ctx = useContext(MeetingStateContext);
  if (!ctx) {
    throw new Error("useMeetingState must be used within a MeetingProvider");
  }
  return ctx;
}

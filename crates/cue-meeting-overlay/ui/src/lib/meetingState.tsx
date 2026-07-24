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
import {
  createTranscriptGrouper,
  PAUSE_MS,
  type TranscriptGrouper,
} from "./transcriptGrouping";
import type {
  AnswerStatusStep,
  ContextItem,
  FixProposal,
  ListeningState,
  MeetingDecision,
  MeetingState,
  TranscriptLine,
} from "./types";

/** One Q&A exchange in the conversation feed: the question asked + the streamed
 *  answer + the live status steps for that turn. Past turns stay on screen so
 *  the meeting builds a scrollable history instead of each ask replacing the
 *  last. */
export interface Turn {
  id: number;
  /** The id of the last transcript SEGMENT present when this question was asked —
   *  the turn's ANCHOR into the timeline. The Open Floor document interleaves each
   *  Q&A directly after the transcript LINE that contains this segment, so the
   *  exchange stays pinned where it happened even as the grouper merges later
   *  fragments (a raw count would DRIFT as grouping changes; a segment id is
   *  stable — the same anchoring attachments use). Absent → renders at the tail. */
  anchorSegmentId?: string;
  /** What is SHOWN in the feed for this turn. */
  question: string;
  /** What is SENT to the agent on (re)ask — differs from `question` only for the
   *  "ask recent" path, where the sent text is an internal instruction and the
   *  shown text is a readable label. Absent → send `question` verbatim. */
  sendQuestion?: string;
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
  /** The active meeting's Key Decisions ledger (empty until decisions land). */
  decisions: MeetingDecision[];
  /** The current detected for-me question, or null when none/dismissed. */
  detectedQ: DetectedQuestion | null;
  setDetectedQ: (q: DetectedQuestion | null) => void;
  /** The current review-gated fix proposal, or null when none/dismissed. In
   *  beta this is preview-only (the card's Apply is disabled). */
  fixProposal: FixProposal | null;
  setFixProposal: (p: FixProposal | null) => void;
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
  /** Attached context (screenshots/files). Seeded from the meeting snapshot on
   *  mount/reseed (so reopening reloads them) AND kept live by onContextItems.
   *  Each item's `anchorIndex` pins it inline in the timeline. */
  context: ContextItem[];
  /** The live capture state (idle | connecting | listening | paused | failed |
   *  permission_denied). Lives HERE (not in a screen) so it survives a
   *  collapse→expand: the screen unmounts on collapse and would otherwise reset
   *  to "idle" while the daemon keeps capturing — the toggle then lies. */
  listenState: ListeningState;
  /** Whether OUR mic is being captured (in addition to system audio). Also kept
   *  here so it survives the collapse→expand remount. */
  micInputOn: boolean;
  setMicInputOn: (on: boolean) => void;
}

const MeetingStateContext = createContext<MeetingStateValue | null>(null);

export function MeetingProvider({ children }: { children: ReactNode }) {
  const client = getClient();

  const [transcript, setTranscript] = useState<TranscriptLine | null>(null);
  const [history, setHistory] = useState<TranscriptLine[]>([]);
  const [turns, setTurns] = useState<Turn[]>([]);
  const [decisions, setDecisions] = useState<MeetingDecision[]>([]);
  const [detectedQ, setDetectedQ] = useState<DetectedQuestion | null>(null);
  const [fixProposal, setFixProposal] = useState<FixProposal | null>(null);
  const [rehydrated, setRehydrated] = useState(false);
  // Capture state lives at the provider (never-unmounted) so it survives a
  // collapse→expand. The daemon pushes listening_state_changed on every
  // transition; we hold the latest so a remounted screen reads the TRUTH.
  const [listenState, setListenState] = useState<ListeningState>("idle");
  const [micInputOn, setMicInputOn] = useState(false);
  // Attached context, seeded from the snapshot (reopen/reseed) and kept live by
  // onContextItems. Lives here so it survives a collapse→expand and reloads on
  // reopen — the live push alone never re-delivers already-attached artifacts.
  const [context, setContext] = useState<ContextItem[]>([]);

  // The authoritative grouped history, keyed by member segment ids. This grouper
  // is the single writer of the `history` state; both the seed and the live sub
  // go through its foldSegment so seed↔live reconciliation is one code path. It
  // owns the grouped lines + the seen-id dedup set (the O(1) guard that makes the
  // async seed and live stream idempotent regardless of arrival order).
  const grouperRef = useRef<TranscriptGrouper | null>(null);
  if (grouperRef.current === null) {
    grouperRef.current = createTranscriptGrouper();
  }
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
  // rehydrate-fidelity fix). The pure grouping lives in transcriptGrouping.ts
  // (shared verbatim with the read-only past-meeting viewer); this wrapper only
  // pushes the result into React state. A duplicate segment yields caption:null
  // and leaves state untouched.
  const foldSegment = (seg: TranscriptLine, paused: boolean) => {
    const grouper = grouperRef.current;
    if (!grouper) return;
    const { history: next, caption } = grouper.foldSegment(seg, paused);
    if (caption === null) return;
    setHistory(next);
    setTranscript(caption);
  };

  // ---- Continue-past-meeting reseed (single mutation path) ----
  // A daemon-PUSHED active set_meeting_state (meeting_id absent) arrives when a
  // past meeting is continued into the active slot. REPLACE the whole session
  // with the target's snapshot — a fresh grouper (so the previous meeting's
  // grouped lines + seen-id set are discarded), the transcript refolded through
  // the SAME path the mount seed uses (paused=false), the Q&A feed rebuilt like
  // the seed, and the detected question cleared (a new meeting carries none).
  const reseed = (snap: MeetingState) => {
    const grouper = createTranscriptGrouper();
    grouperRef.current = grouper;
    lastLiveAtRef.current = Date.now();

    let folded: TranscriptLine[] = [];
    let lastCaption: TranscriptLine | null = null;
    for (const l of snap.transcript) {
      const { history: next, caption } = grouper.foldSegment(
        {
          id: l.id,
          source: l.source,
          speaker: l.speaker,
          speakerId: l.speakerId,
          text: l.text,
          final: true,
        },
        false,
      );
      if (caption !== null) {
        folded = next;
        lastCaption = caption;
      }
    }
    // STEP 5 (UI): how the re-broadcast re-grouped. If the reassigned block MERGED
    // with adjacent live segments (fewer grouped lines than distinct speakers), we
    // see it here — the re-fold groups consecutive same-speaker segments together.
    console.log("[SPAN-DEBUG] UI reseed re-grouped", {
      snapshotSegments: snap.transcript.length,
      groupedLines: folded.length,
      lines: folded.map((l) => ({
        id: l.id,
        speaker: l.speaker,
        speakerId: l.speakerId,
        text: l.text.slice(0, 40),
      })),
    });
    setHistory(folded);
    setTranscript(lastCaption);

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
    setDecisions(snap.decisions ?? []);
    // Reload persisted attachments inline (the live push never re-delivers
    // already-attached artifacts to a fresh mount).
    setContext(snap.context ?? []);
    setDetectedQ(null);
    setFixProposal(null);
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
        setDecisions(snap.decisions ?? []);
        setContext(snap.context ?? []);

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

  // Capture state — subscribed ONCE at the provider so the latest daemon-pushed
  // state persists across a collapse→expand (the screen that used to own this
  // reset it to "idle" on every remount, desyncing the toggle from reality).
  useEffect(() => client.onListeningState(setListenState), [client]);

  // Live context push (attach/screenshot/remove). Seeded separately from the
  // snapshot (below) so a reopen reloads persisted context; this keeps it fresh.
  useEffect(() => client.onContextItems(setContext), [client]);

  // ---- diarized speaker-label upgrades (single owner) ----
  // Labels lag lines: the daemon's live diarize tick resolves "who said it" a
  // few seconds after the text streamed in, then pushes (segmentId, label).
  // Patch the grouped line containing that segment in place; a null return
  // (stale id / label unchanged) skips the re-render.
  useEffect(
    () =>
      client.onSpeakerUpdate((segmentId, speaker, speakerId) => {
        const next = grouperRef.current?.setSpeaker(
          segmentId,
          speaker,
          speakerId,
        );
        if (next) setHistory(next);
      }),
    [client],
  );

  // ---- detected for-me question (single owner) ----
  useEffect(() => client.onForMeQuestion((q) => setDetectedQ(q)), [client]);

  // ---- proposed fix (single owner) ----
  // The daemon PUSHES a fix proposal after driving the attached agent in
  // propose-only mode (the reply to a requestFix). Store it in shared state so
  // it survives a collapse/tab switch and there's no double-subscribe. In beta
  // this is preview-only; the card's Apply is disabled.
  useEffect(() => client.onFixProposal((p) => setFixProposal(p)), [client]);

  // ---- continue-past-meeting reseed (single owner) ----
  // Apply a daemon-pushed active reseed ONLY after the initial rehydrate seed
  // has run, so the meetingState() reply that shares this bus shape (the mount
  // seed) doesn't trigger a redundant second reseed. reseed closes over refs +
  // stable setters, so it needn't be a dep.
  useEffect(
    () =>
      client.onMeetingReseed((snap) => {
        if (!rehydrated) return;
        reseed(snap);
      }),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [client, rehydrated],
  );

  const value: MeetingStateValue = {
    transcript,
    history,
    turns,
    decisions,
    detectedQ,
    setDetectedQ,
    fixProposal,
    setFixProposal,
    appendTurn,
    patchTurn,
    turnSeq,
    rehydrated,
    context,
    listenState,
    micInputOn,
    setMicInputOn,
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

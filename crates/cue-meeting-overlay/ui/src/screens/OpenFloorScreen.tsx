// The Open Floor Plan — Bluey's warm-paper "meeting document".
//
// A living Granola-style document, not a chat panel: a centered reading column
// of the meeting transcript with the Q&A woven inline where each question was
// asked, a Key Decisions ledger at the top, and the floating control stack +
// detected-question dock hovering over the paper. Every surface reads the
// [data-floorplan] tokens (warm ink, serif headings, the single --fp-live red),
// so light/dark flip for free.
//
// This screen OWNS the ask lifecycle (runAsk streaming contract) and its own
// listening/context/candidate subscriptions, but the durable session state
// (transcript history, turns, decisions, the detected question) lives in the
// never-unmounted MeetingProvider via useMeetingState — a collapse/expand or a
// tab switch cannot wipe it.

import { useEffect, useMemo, useRef, useState } from "react";
import { getClient } from "../lib";
import { useDragHeader } from "../lib/useDragHeader";
import type {
  AgentConnectorInfo,
  AgentSummary,
  AnswerSource,
  AnswerStatusStep,
  ContextItem,
  MeetingDecision,
  SpeakerCandidate,
  TranscriptLine,
} from "../lib/types";
import type { AnswerState } from "../components/AnswerCard";
import { useMeetingState, type Turn } from "../lib/meetingState";
import { AgentLogo } from "../components/AgentLogo";
import { StatusFeed } from "../components/StatusFeed";
import { Markdown } from "../components/Markdown";
import { repairStreamingMarkdown } from "../components/repairStreamingMarkdown";
import {
  AttachIcon,
  CloseIcon,
  CopyIcon,
  LayersIcon,
  PlusIcon,
  SparkleIcon,
} from "../components/icons";
import { FloatingStack } from "../components/floorplan/FloatingStack";
import { DetectedDock } from "../components/floorplan/DetectedDock";
import {
  SpeakerEditor,
  type KnownSpeaker,
} from "../components/floorplan/SpeakerEditor";
import { SelectionToolbar } from "../components/floorplan/SelectionToolbar";
import { Waveform } from "../components/primitives";

/** The instruction actually SENT to the agent when the user taps "Ask the
 *  detected question" — a self-contained directive, since the raw detected text
 *  alone isn't a well-formed prompt. */
const ASK_RECENT_QUESTION =
  "Answer the most recent question or request raised in the meeting transcript. If the last lines contain no question, briefly answer what would be most useful about what was just discussed.";
/** The readable label SHOWN in the feed for that same turn. */
const ASK_RECENT_LABEL = "Answering the question just asked in the meeting.";

/** The local phase of an in-flight ask (drives the detected dock visibility). */
type Phase = "idle" | "thinking" | "answering";

export function OpenFloorScreen({
  agent,
  meetingTitle,
  onTurnOff,
  onCollapse,
  onOpenDrawer,
  connectors,
}: {
  agent: AgentSummary | null;
  meetingTitle?: string;
  onTurnOff: () => void;
  onCollapse: () => void;
  onOpenDrawer: () => void;
  /** The FULL connector list (name/ready/authTier) — the bar shows the first
   *  couple and the "+N" chip opens a popover over the rest. */
  connectors: AgentConnectorInfo[];
}) {
  const client = getClient();
  const {
    transcript,
    history,
    turns,
    decisions,
    detectedQ,
    setDetectedQ,
    appendTurn,
    patchTurn,
    turnSeq,
    context: contextItems,
    listenState,
    micInputOn,
    setMicInputOn,
  } = useMeetingState();

  // ---- local (view-owned) state -------------------------------------------
  const [phase, setPhase] = useState<Phase>("idle");
  const [candidates, setCandidates] = useState<SpeakerCandidate[]>([]);
  // Exactly ONE transcript line renders its speaker editor at a time (the
  // "4× rename" guard) — the timeline row whose id is `editingId`.
  const [editingId, setEditingId] = useState<string | null>(null);

  const askRef = useRef<{ cancel(): void } | null>(null);
  const canvasRef = useRef<HTMLDivElement>(null);
  // Drag the frameless panel by its top bar (CSS -webkit-app-region:drag is not
  // honored on the borderless NSPanel — this hooks the proven startDragging path).
  const topbarRef = useRef<HTMLDivElement>(null);
  useDragHeader(topbarRef);

  // ---- subscriptions (this view is the single owner of these) -------------
  // (listenState + context are owned by MeetingProvider now, so they survive a
  // collapse→expand and reload from the snapshot on reopen.)
  useEffect(() => client.onMeetingCandidates(setCandidates), [client]);

  // Follow the tail as new content streams in, but DO NOT auto-scroll to the bottom
  // on initial rehydration so mid-conversation Q&A and pinned cards are visible.
  const hasMountedRef = useRef(false);
  useEffect(() => {
    const el = canvasRef.current;
    if (!el) return;
    if (!hasMountedRef.current) {
      hasMountedRef.current = true;
      el.scrollTop = 0; // Start at top on mount so decisions, header & Q&A are visible
      return;
    }
    const nearBottom =
      el.scrollHeight - el.scrollTop - el.clientHeight < 160;
    if (nearBottom) el.scrollTop = el.scrollHeight;
  }, [history, turns, transcript]);

  // ---- the verified streaming ask contract --------------------------------
  const runAsk = (question: string, displayQuestion?: string) => {
    askRef.current?.cancel();
    setDetectedQ(null);
    setPhase("thinking");
    const id = ++turnSeq.current;
    const draft: AnswerState = {
      agentLabel: agent?.displayName ?? "Bluey",
      text: "",
      sources: [],
      tools: [],
      done: false,
      cost: undefined,
      error: undefined,
    };
    let steps: AnswerStatusStep[] = [];
    let statusDone = false;
    appendTurn({
      id,
      // Pin this exchange after whatever transcript is on screen right now, so
      // it stays anchored where it was asked and later lines flow below it.
      anchorSegmentId: history.length > 0 ? history[history.length - 1].id : undefined,
      question: displayQuestion ?? question,
      sendQuestion: displayQuestion ? question : undefined,
      answer: { ...draft },
      statusSteps: steps,
      statusDone,
    });
    const patch = () =>
      patchTurn(id, { answer: { ...draft }, statusSteps: steps, statusDone });
    askRef.current = client.ask(question, (c) => {
      if (c.status) steps = c.status;
      if (c.statusDone !== undefined) statusDone = c.statusDone;
      if (c.text) {
        draft.text += c.text;
        setPhase("answering");
      }
      if (c.tool) draft.tools = [...draft.tools, c.tool];
      if (c.source) draft.sources = [...draft.sources, c.source];
      if (c.done) {
        draft.done = true;
        if (c.error) draft.error = true;
        setPhase("idle");
      }
      patch();
    });
  };

  // Ask the detected/most-recent meeting question via the internal directive.
  const askDetected = () => {
    if (!detectedQ?.text && !transcript?.text) return;
    setDetectedQ(null);
    runAsk(ASK_RECENT_QUESTION, ASK_RECENT_LABEL);
  };

  // ---- control-stack handlers ---------------------------------------------
  const [systemInputOn, setSystemInputOn] = useState(true);

  const toggleSystemAudio = () => {
    const nextSystem = !systemInputOn;
    setSystemInputOn(nextSystem);
    if (!nextSystem && !micInputOn) {
      client.stopListening();
    } else {
      client.startListening({ system: nextSystem, microphone: micInputOn });
    }
  };
  const toggleMic = () => {
    const nextMic = !micInputOn;
    setMicInputOn(nextMic);
    if (!nextMic && !systemInputOn) {
      client.stopListening();
    } else {
      client.startListening({ system: systemInputOn, microphone: nextMic });
    }
  };

  const cleanup = () => askRef.current?.cancel();
  useEffect(() => cleanup, []);

  const speakerCount = countSpeakers(history);
  const isEmpty =
    history.length === 0 && turns.length === 0 && contextItems.length === 0;
  const showDock = detectedQ !== null && phase === "idle";
  const capturing =
    listenState === "listening" || listenState === "connecting";

  // Interleave the Q&A turns INTO the transcript at the point each was asked, so
  // an exchange stays pinned where it happened and later lines flow below it.
  // Each turn's `anchor` is the history length at ask time → it renders right
  // after history line #anchor. Turns with no anchor (seeded) fall to the end.
  const timeline = useMemo(
    () => buildTimeline(history, turns, contextItems),
    [history, turns, contextItems],
  );


  // Distinct diarized speakers seen so far — the "reassign this line to…" targets
  // in the speaker editor. Labelled by the first line that named each speaker id.
  const knownSpeakers = useMemo<KnownSpeaker[]>(() => {
    const seen = new Map<number, string>();
    for (const l of history) {
      if (l.speakerId != null && !seen.has(l.speakerId)) {
        seen.set(
          l.speakerId,
          l.speaker?.trim() || `Speaker ${l.speakerId + 1}`,
        );
      }
    }
    return [...seen.entries()].map(([id, label]) => ({ id, label }));
  }, [history]);

  return (
    <div className="fp-root" data-floorplan="">
      {/* ---- top bar ---- */}
      <div className="fp-topbar" ref={topbarRef}>
        <div className="fp-topbar-left">
          <div className="fp-dots">
            <button
              className="fp-dot fp-dot-red"
              title="Close (Bluey keeps running in the background)"
              aria-label="Close overlay; Bluey keeps running in the background"
              onClick={onTurnOff}
            />
            <button
              className="fp-dot fp-dot-amber"
              title="Minimize to pill"
              aria-label="Minimize to pill"
              onClick={onCollapse}
            />
          </div>
          <span className="fp-title">{meetingTitle ?? "Bluey"}</span>
          {capturing && (
            <span className="fp-listening">
              <span className="fp-wave" aria-hidden>
                <Waveform />
              </span>
              {listenState === "connecting" ? "connecting" : "listening"}
            </span>
          )}
        </div>
        <div className="fp-topbar-right">
          <span className="fp-speaker-count">
            {speakerCount} {speakerCount === 1 ? "speaker" : "speakers"}
          </span>
          <button
            className="fp-iconbtn"
            title="New meeting session"
            aria-label="New meeting session"
            onClick={() => {
              // Archives the current meeting + starts fresh. Guard when there's
              // real content so a stray click can't wipe a live transcript.
              if (history.length > 0 && !confirm("Start a new meeting? The current transcript will be saved to History.")) {
                return;
              }
              client.newMeeting();
            }}
          >
            <PlusIcon size={16} />
          </button>
          <button
            className="fp-iconbtn"
            title="Meetings & Agents"
            aria-label="Meetings & Agents"
            onClick={onOpenDrawer}
          >
            <LayersIcon size={16} />
          </button>
        </div>
      </div>

      {/* ---- agent bar (floating pill) ---- */}
      <div className="fp-agentbar">
        <div className="fp-agentbar-left">
          <span className="fp-agent-logo">
            <AgentLogo kind={agent?.kind} size={18} />
          </span>
          <div className="fp-agent-meta">
            <div className="fp-agent-name">
              {agent ? (
                <>
                  {agent.displayName}
                  <span className="fp-agent-sub"> · your session</span>
                </>
              ) : (
                "No agent attached"
              )}
            </div>
            {agent && (
              <div className="fp-agent-detail">
                {agent.readyConnectorCount}/{agent.connectorCount} connectors
                {agent.sessionCount != null
                  ? ` · ${agent.sessionCount} sessions`
                  : ""}
              </div>
            )}
          </div>
        </div>
        {agent && connectors.length > 0 && (
          <ConnectorRail connectors={connectors} />
        )}
      </div>

      {/* ---- the document ---- */}
      <div className="fp-canvas" ref={canvasRef}>
        <div className="fp-doc">
          {isEmpty ? (
            <EmptyState />
          ) : (
            <>
              {decisions.length > 0 && <DecisionLedger decisions={decisions} />}

              <div className="fp-timeline">
                <div className="fp-kicker">Timeline</div>

                {timeline.map((item, i) => {
                  if (item.kind === "line") {
                    return (
                      <TranscriptRow
                        key={item.line.id ?? `h${item.index}`}
                        line={item.line}
                        // Mark the newest line live while capturing, so it
                        // carries a LIVE badge instead of a SEPARATE duplicate
                        // caption row (the old redundant-live-line bug).
                        live={capturing && item.index === history.length - 1}
                        candidates={candidates}
                        knownSpeakers={knownSpeakers}
                        editing={
                          editingId === (item.line.id ?? `h${item.index}`)
                        }
                        onStartEdit={() =>
                          setEditingId(item.line.id ?? `h${item.index}`)
                        }
                        onDone={() => setEditingId(null)}
                      />
                    );
                  }
                  if (item.kind === "attach") {
                    return (
                      <Attachments
                        key={`a${i}`}
                        items={item.items}
                        onRemove={(id) => client.removeContextItem(id)}
                      />
                    );
                  }
                  return (
                    <QaBlock
                      key={`q${item.turn.id}`}
                      turn={item.turn}
                      onFix={(text) => client.requestFix(text)}
                      onRetry={(t) =>
                        runAsk(
                          t.sendQuestion ?? t.question,
                          t.sendQuestion ? t.question : undefined,
                        )
                      }
                    />
                  );
                })}

                {/* When nothing has been transcribed yet but capture is on, show
                    a single live placeholder so the user sees it's listening.
                    (Attachments now interleave in the timeline above, anchored
                    where each was added — no separate tail block.) */}
                {history.length === 0 && capturing && (
                  <div className="fp-live-hint">
                    <Waveform />
                    <span>Listening…</span>
                  </div>
                )}
              </div>
            </>
          )}
        </div>
      </div>

      {/* ---- selection toolbar (assign a highlighted span to a speaker) ---- */}
      <SelectionToolbar knownSpeakers={knownSpeakers} />

      {/* ---- floating control stack ---- */}
      <FloatingStack
        listenState={listenState}
        micInputOn={micInputOn}
        askStreaming={phase !== "idle"}
        agentName={agent?.displayName}
        onToggleSystemAudio={toggleSystemAudio}
        onToggleMic={toggleMic}
        onScreenshot={() => client.captureScreenshot()}
        onAttach={() => client.openAttachPicker()}
        onCapturePage={() => client.capturePage()}
        onAsk={(q) => runAsk(q)}
        onAskRecent={askDetected}
      />

      {/* ---- detected-question dock ---- */}
      {showDock && detectedQ && (
        <DetectedDock
          question={detectedQ.text}
          title={detectedQ.title}
          agentName={agent?.displayName}
          onAsk={askDetected}
          onDismiss={() => setDetectedQ(null)}
        />
      )}
    </div>
  );
}

// ==========================================================================
// The calm empty state — a small mark ~40% down the column.
// ==========================================================================
function EmptyState() {
  return (
    <div className="fp-empty">
      <span className="fp-empty-mark" aria-hidden>
        <SparkleIcon size={30} />
      </span>
      <div className="fp-empty-text">Start listening, or ask a question.</div>
    </div>
  );
}

// ==========================================================================
// The Key Decisions ledger — a left-ruled inline block, rendered only when the
// meeting has verified decisions.
// ==========================================================================
function DecisionLedger({ decisions }: { decisions: MeetingDecision[] }) {
  return (
    <>
      <div className="fp-ledger">
        <div className="fp-kicker">Key decisions</div>
        {decisions.map((d) => (
          <div className="fp-ledger-item" key={d.id}>
            {d.text}
          </div>
        ))}
      </div>
      <hr className="fp-rule" />
    </>
  );
}

// ==========================================================================
// One transcript line: speaker (click-to-rename) + optional LIVE badge, then
// text. When `live`, this IS the in-progress tail — it carries the LIVE badge
// in place of rendering a separate duplicate caption row.
// ==========================================================================
function TranscriptRow({
  line,
  live,
  candidates,
  knownSpeakers,
  editing,
  onStartEdit,
  onDone,
}: {
  line: TranscriptLine;
  live?: boolean;
  candidates: SpeakerCandidate[];
  knownSpeakers: KnownSpeaker[];
  editing: boolean;
  onStartEdit: () => void;
  onDone: () => void;
}) {
  return (
    <div className={`fp-line${live ? " is-live" : ""}`}>
      <div className="fp-line-head">
        <SpeakerEditor
          line={line}
          candidates={candidates}
          knownSpeakers={knownSpeakers}
          editing={editing}
          onStartEdit={onStartEdit}
          onDone={onDone}
        />
        {live && (
          <span className="fp-live-badge">
            <span className="fp-live-dot" aria-hidden />
            LIVE
          </span>
        )}
      </div>
      {/* data-line-id lets the selection toolbar map a text selection back to the
          segment(s) to split/reassign. */}
      <div
        className="fp-line-text"
        data-line-id={line.id ?? ""}
        data-member-ids={(line.memberIds ?? []).join(",")}
      >
        {line.text}
      </div>
    </div>
  );
}

// ==========================================================================
// The connector rail — compact ready-dot chips (first 2) + a "+N" button that
// opens a popover over the FULL connector list. Replaces the old static "+N"
// span that couldn't expand.
// ==========================================================================
function ConnectorRail({ connectors }: { connectors: AgentConnectorInfo[] }) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const shown = connectors.slice(0, 2);
  const rest = connectors.length - shown.length;

  // Close on outside click / Escape.
  useEffect(() => {
    if (!open) return;
    const onDoc = (e: MouseEvent) => {
      if (!rootRef.current?.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    document.addEventListener("mousedown", onDoc, true);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDoc, true);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  return (
    <div className="fp-connectors" ref={rootRef}>
      {shown.map((c) => (
        <span
          className={`fp-connector${c.ready ? "" : " is-off"}`}
          key={c.name}
          title={c.ready ? c.name : `${c.name} · not ready`}
        >
          <span className="fp-connector-dot" aria-hidden />
          {c.name}
        </span>
      ))}
      {rest > 0 && (
        <button
          className="fp-connector-more"
          aria-label={`Show all ${connectors.length} connectors`}
          aria-expanded={open}
          onClick={() => setOpen((v) => !v)}
        >
          +{rest}
        </button>
      )}
      {open && (
        <div className="fp-connector-pop" role="menu">
          <div className="fp-connector-pop-kicker">
            {connectors.length} connectors
          </div>
          {connectors.map((c) => (
            <div className="fp-connector-pop-row" key={c.name} role="menuitem">
              <span
                className={`fp-connector-dot${c.ready ? "" : " is-off"}`}
                aria-hidden
              />
              <span className="fp-connector-pop-name">{c.name}</span>
              {!c.ready && (
                <span className="fp-connector-pop-note">not ready</span>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

// ==========================================================================
// A Q&A dialog block — the question, the agent's reasoning (StatusFeed) INSIDE
// the same card, then the streamed answer, all indented under the question so
// the reasoning reads as part of this exchange (the reference layout).
// ==========================================================================
function QaBlock({
  turn,
  onFix,
  onRetry,
}: {
  turn: Turn;
  onFix: (text: string) => void;
  onRetry: (turn: Turn) => void;
}) {
  const a = turn.answer;
  return (
    <div className="fp-qa">
      <div className="fp-qa-q">
        <span className="fp-qa-avatar" aria-hidden>
          You
        </span>
        <div className="fp-qa-question">{turn.question}</div>
      </div>

      {/* The agent's live reasoning + tool calls, indented under the text. */}
      {turn.statusSteps.length > 0 && (
        <div className="fp-qa-indent fp-qa-status">
          <StatusFeed steps={turn.statusSteps} done={turn.statusDone} />
        </div>
      )}

      {a.error ? (
        <div className="fp-qa-indent">
          <div className="fp-qa-error">
            <div className="fp-qa-error-kicker">Couldn't answer</div>
            <div className="fp-qa-error-text">{a.text}</div>
            <button
              className="fp-qa-error-retry"
              onClick={() => onRetry(turn)}
            >
              Try again
            </button>
          </div>
        </div>
      ) : (
        <div className="fp-qa-indent">
          <div className="fp-qa-a md-body">
            <Markdown source={repairStreamingMarkdown(a.text)} />
            {!a.done && <span className="fp-cursor" aria-hidden />}
          </div>

          {a.sources.length > 0 && <SourceRows sources={a.sources} />}

          {a.done && (
            <div className="fp-qa-actions">
              <button
                className="fp-qa-action"
                onClick={() => navigator.clipboard.writeText(a.text)}
              >
                <CopyIcon size={14} />
                Copy
              </button>
              {/* "Fix this" only on a successful answer. */}
              {!a.error && (
                <button
                  className="fp-qa-action"
                  onClick={() => onFix(a.text)}
                >
                  <SparkleIcon size={14} />
                  Fix this
                </button>
              )}
              {a.cost && <span className="fp-qa-cost">{a.cost}</span>}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

// Grounding source rows under an answer.
function SourceRows({ sources }: { sources: AnswerSource[] }) {
  return (
    <div className="fp-sources">
      {sources.map((s, i) => (
        <div className="fp-source" key={`${s.kind}-${i}`}>
          <span className="fp-source-dot" aria-hidden />
          <span>{s.label}</span>
          {s.note && <span className="fp-source-note">{s.note}</span>}
        </div>
      ))}
    </div>
  );
}

// ==========================================================================
// Inline attachments — screenshots/files queued for the next question, rendered
// as a block IN the document (at the tail, where the next ask lands) instead of
// floating on the control stack. Image kinds show a preview; others a glyph.
// ==========================================================================
function Attachments({
  items,
  onRemove,
}: {
  items: ContextItem[];
  onRemove: (id: string) => void;
}) {
  return (
    <div className="fp-attach">
      <div className="fp-attach-kicker">
        <AttachIcon size={13} />
        Attached · {items.length}
      </div>
      <div className="fp-attach-grid">
        {items.map((item) => {
          const isImage =
            (item.kind === "image" || item.kind === "diagram") &&
            !!item.thumbnail;
          return (
            <div
              className={`fp-attach-item${isImage ? " is-image" : ""}`}
              key={item.id}
              title={item.path ?? item.title}
            >
              {isImage ? (
                <img
                  className="fp-attach-thumb"
                  src={item.thumbnail}
                  alt={item.title}
                />
              ) : (
                <span className="fp-attach-glyph" aria-hidden>
                  <AttachKindGlyph kind={item.kind} />
                </span>
              )}
              <span className="fp-attach-title">{item.title}</span>
              <button
                className="fp-attach-x"
                aria-label={`Remove ${item.title}`}
                onClick={() => onRemove(item.id)}
              >
                <CloseIcon size={12} />
              </button>
            </div>
          );
        })}
      </div>
    </div>
  );
}

// A small vector glyph for a non-image attachment, per kind. Vector-only.
function AttachKindGlyph({ kind }: { kind: string }) {
  const common = {
    width: 16,
    height: 16,
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 1.6,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
    "aria-hidden": true,
  };
  switch (kind) {
    case "code":
      return (
        <svg {...common}>
          <path d="M9 8l-4 4 4 4" />
          <path d="M15 8l4 4-4 4" />
        </svg>
      );
    case "text":
      return (
        <svg {...common}>
          <path d="M5 7h14" />
          <path d="M5 12h14" />
          <path d="M5 17h9" />
        </svg>
      );
    default:
      return (
        <svg {...common}>
          <path d="M7 3h7l4 4v14H7z" />
          <path d="M14 3v4h4" />
        </svg>
      );
  }
}

// ==========================================================================
// helpers
// ==========================================================================

/** A rendered timeline item: a transcript line (with its history index), a Q&A
 *  turn, or an attachment — all interleaved in reading order. */
type TimelineItem =
  | { kind: "line"; line: TranscriptLine; index: number }
  | { kind: "qa"; turn: Turn }
  | { kind: "attach"; items: ContextItem[] };

/** Interleave Q&A turns AND attachments into the transcript by their anchor (the
 *  history length when they were created): each renders right AFTER history line
 *  #(anchor-1), so an exchange or a screenshot stays pinned where it happened
 *  and later transcript flows below it. Within one anchor, turns are ordered by
 *  id and attachments are grouped into a single block. A missing/over-large
 *  anchor falls to the end. */
function buildTimeline(
  history: TranscriptLine[],
  turns: Turn[],
  attachments: ContextItem[],
): TimelineItem[] {
  // Both turns and attachments anchor by a SEGMENT ID (the UI groups many raw
  // segments into one line). Resolve it to the index of the grouped line whose
  // memberIds contain that segment. Not found → tail.
  const segToLineIndex = new Map<string, number>();
  history.forEach((line, i) => {
    if (line.id) segToLineIndex.set(line.id, i);
    for (const sid of line.memberIds ?? []) segToLineIndex.set(sid, i);
  });
  const anchorIndexOf = (segId: string | undefined): number => {
    if (!segId) return history.length - 1; // no anchor → tail
    const i = segToLineIndex.get(segId);
    return i == null ? history.length - 1 : i;
  };

  // Bucket turns by the line that contains their anchor segment (same stable
  // segment-id resolution as attachments — a raw count would drift as the
  // grouper merges live fragments).
  const turnsByAfter = new Map<number, Turn[]>();
  for (const t of turns) {
    const after = anchorIndexOf(t.anchorSegmentId);
    (turnsByAfter.get(after) ?? turnsByAfter.set(after, []).get(after)!).push(t);
  }
  for (const arr of turnsByAfter.values()) arr.sort((a, b) => a.id - b.id);

  // Bucket attachments by the line that contains their anchor segment.
  const attachByAfter = new Map<number, ContextItem[]>();
  for (const item of attachments) {
    const after = anchorIndexOf(item.anchorSegmentId);
    (
      attachByAfter.get(after) ?? attachByAfter.set(after, []).get(after)!
    ).push(item);
  }

  const emitAfter = (idx: number, out: TimelineItem[]) => {
    for (const t of turnsByAfter.get(idx) ?? []) out.push({ kind: "qa", turn: t });
    const att = attachByAfter.get(idx);
    if (att && att.length) out.push({ kind: "attach", items: att });
  };

  const out: TimelineItem[] = [];
  emitAfter(-1, out); // anchored before any transcript
  history.forEach((line, index) => {
    out.push({ kind: "line", line, index });
    emitAfter(index, out);
  });
  return out;
}

// Count distinct speakers heard in the meeting so far. Falls back to the
// source ("mic"/"system") when a line has no resolved speaker label yet.
function countSpeakers(history: TranscriptLine[]): number {
  const seen = new Set<string>();
  for (const l of history) {
    const key =
      l.speaker?.trim() ||
      (l.speakerId != null ? `#${l.speakerId}` : l.source);
    if (key) seen.add(key);
  }
  return seen.size;
}

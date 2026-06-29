// The Ask tab — the live loop. Transcript line → question-detection trigger →
// the agent drives (slow → ThinkingState) → streamed answer with grounded
// source rows → actions. The whole product happens here.

import { useEffect, useRef, useState } from "react";
import { getClient } from "../lib";
import type {
  AgentSummary,
  AnswerSource,
  AnswerStatusStep,
  AskMode,
  ListeningState,
  TranscriptLine,
} from "../lib/types";
import { AgentBar } from "../components/AgentBar";
import { AnswerCard, type AnswerState } from "../components/AnswerCard";
import { Composer } from "../components/Composer";
import { ThinkingState } from "../components/primitives";
import { StatusFeed } from "../components/StatusFeed";

type Phase = "idle" | "detected" | "thinking" | "answering";

/** One Q&A exchange in the conversation feed: the question asked + the streamed
 *  answer + the live status steps for that turn. Past turns stay on screen so
 *  the meeting builds a scrollable history instead of each ask replacing the
 *  last. */
interface Turn {
  id: number;
  question: string;
  answer: AnswerState;
  statusSteps: AnswerStatusStep[];
  statusDone: boolean;
}

export function AskScreen({ agent }: { agent: AgentSummary | null }) {
  const client = getClient();
  const [transcript, setTranscript] = useState<TranscriptLine | null>(null);
  const [phase, setPhase] = useState<Phase>("idle");
  // The conversation feed — every asked question + its answer, in order.
  const [turns, setTurns] = useState<Turn[]>([]);
  const [connectors, setConnectors] = useState<string[]>([]);
  const [listenState, setListenState] = useState<ListeningState>("idle");
  // The answer-speed preset, forwarded to the daemon as `mode`. Defaults to
  // "balanced" so an untouched composer behaves exactly as before.
  const [mode, setMode] = useState<AskMode>("balanced");
  const askRef = useRef<{ cancel(): void } | null>(null);
  const turnSeq = useRef(0);
  const feedEndRef = useRef<HTMLDivElement>(null);

  useEffect(
    () => client.onTranscript((l) => l.final && setTranscript(l)),
    [client],
  );
  // A daemon-detected for-me question (master doc §6) — the distinct signal that
  // rises into the "They asked…" hero card. Cleared once asked or superseded.
  const [detectedQ, setDetectedQ] = useState<{
    text: string;
    title?: string;
  } | null>(null);
  useEffect(
    () => client.onForMeQuestion((q) => setDetectedQ(q)),
    [client],
  );
  useEffect(() => client.onListeningState(setListenState), [client]);
  // Keep the newest turn / streaming text in view as the feed grows.
  useEffect(() => {
    feedEndRef.current?.scrollIntoView({ behavior: "smooth", block: "end" });
  }, [turns]);

  // Real connectors for the attached agent — replaces the old hardcoded
  // "Jira / GitHub / +2" demo chips. Empty when no agent is attached.
  useEffect(() => {
    let live = true;
    if (!agent) {
      setConnectors([]);
      return;
    }
    client
      .connectors(agent.kind)
      .then(
        (cs) =>
          live && setConnectors(cs.filter((c) => c.ready).map((c) => c.name)),
      )
      .catch(() => live && setConnectors([]));
    return () => {
      live = false;
    };
  }, [client, agent]);

  // Show the first 2 connector chips inline; the rest collapse into "+N".
  const connectorNames = connectors.slice(0, 2);
  const extraConnectors = Math.max(0, connectors.length - 2);

  const runAsk = (question: string) => {
    askRef.current?.cancel();
    setPhase("thinking");

    // Append a NEW turn (don't wipe prior ones). We mutate this turn's draft as
    // chunks stream, and patch the matching turn by id so earlier answers stay.
    const id = ++turnSeq.current;
    const draft: AnswerState = {
      agentLabel: agent?.displayName ?? "Bluey",
      text: "",
      sources: [],
      tools: [],
      done: false,
      cost: undefined,
    };
    let steps: AnswerStatusStep[] = [];
    let statusDone = false;
    setTurns((prev) => [
      ...prev,
      { id, question, answer: { ...draft }, statusSteps: steps, statusDone },
    ]);

    const patch = () =>
      setTurns((prev) =>
        prev.map((t) =>
          t.id === id
            ? { ...t, answer: { ...draft }, statusSteps: steps, statusDone }
            : t,
        ),
      );

    askRef.current = client.ask(
      question,
      (c) => {
        if (c.status) steps = c.status;
        if (c.statusDone !== undefined) statusDone = c.statusDone;
        if (c.text) {
          draft.text += c.text;
          setPhase("answering");
        }
        if (c.tool) draft.tools = [...draft.tools, c.tool];
        if (c.source)
          draft.sources = [...draft.sources, c.source as AnswerSource];
        if (c.done) draft.done = true;
        patch();
      },
      { mode },
    );
  };

  const askDetected = () => {
    const q = detectedQ?.text ?? transcript?.text;
    if (!q) return;
    setDetectedQ(null);
    runAsk(q);
  };

  return (
    <div
      style={{
        flex: 1,
        display: "flex",
        flexDirection: "column",
        minHeight: 0,
      }}
    >
      <AgentBar
        agent={agent}
        connectorNames={connectorNames}
        extraCount={extraConnectors}
      />

      <div
        style={{
          flex: 1,
          minHeight: 0,
          padding: "6px 0 2px",
          overflowY: "auto",
        }}
      >
        {/* THE TRIGGER MOMENT (master doc §6): a detected for-me question rises
            into a hero card — the heart of the product. It is NOT styled as
            "transcript"; it's the actionable prompt. Shown only when idle (while
            answering, the answer owns the screen). */}
        {detectedQ && phase === "idle" && (
          <div style={heroWrap}>
            <div style={heroCard}>
              <div style={heroEyebrow}>
                <span style={heroDot} />
                {detectedQ.title ?? "Looks like a question for you"}
              </div>
              <div style={heroQ}>{detectedQ.text}</div>
              <div style={{ display: "flex", gap: 8, marginTop: 11 }}>
                <button onClick={askDetected} style={heroAsk}>
                  Ask {agent?.displayName ?? "your agent"}
                </button>
                <button onClick={() => setDetectedQ(null)} style={heroDismiss}>
                  Dismiss
                </button>
              </div>
            </div>
          </div>
        )}

        {/* The conversation feed: every asked question + its answer, oldest at
            the top. Past turns stay on screen — a new ask appends, never wipes. */}
        {turns.map((turn) => {
          const live = turn.id === turnSeq.current && !turn.answer.done;
          return (
            <div key={turn.id}>
              <div style={trig}>
                <div style={ln} />
                <span style={trigLbl}>✦ {turn.question}</span>
                <div style={ln} />
              </div>

              {/* Timer affordance only for the live turn, until its first real
                  status step / answer token arrives. */}
              {live &&
                turn.statusSteps.length === 0 &&
                turn.answer.text === "" && (
                  <ThinkingState
                    detail={`asking ${agent?.displayName ?? "your agent"}…`}
                  />
                )}

              {/* The agent's real reasoning + tool calls for this turn. */}
              {turn.statusSteps.length > 0 && (
                <StatusFeed steps={turn.statusSteps} done={turn.statusDone} />
              )}

              {(turn.answer.text !== "" || turn.answer.done) && (
                <AnswerCard
                  answer={turn.answer}
                  onCopy={() =>
                    navigator.clipboard?.writeText(turn.answer.text)
                  }
                />
              )}
            </div>
          );
        })}

        {turns.length === 0 && !detectedQ && (
          <div
            style={{
              padding: "28px 16px",
              textAlign: "center",
              color: "var(--ink-3)",
              fontSize: 13,
            }}
          >
            {listenState === "listening"
              ? "Listening — a question will surface here, or just ask."
              : "Start listening, or ask a question."}
          </div>
        )}

        <div ref={feedEndRef} />
      </div>

      {/* AMBIENT CAPTION (master doc §4/§12 — "felt, not read"): a single quiet
          live line proving Bluey hears you, pinned above the composer. NOT a
          transcript wall. Hidden while answering (the answer owns the screen)
          and while a detected question is being shown (that's the focus). */}
      {transcript && phase === "idle" && !detectedQ && (
        <div style={captionWrap} title={transcript.text}>
          <span style={captionDot} />
          <span style={captionWho}>
            {transcript.source === "mic" ? "You" : "They"}
          </span>
          <span style={captionText}>{transcript.text}</span>
        </div>
      )}

      <Composer
        placeholder="Ask a follow-up while Bluey listens…"
        contextLabel={transcript ? "live transcript · in context" : undefined}
        onSubmit={runAsk}
        mode={mode}
        onModeChange={setMode}
        onMic={() => {
          // Debounce: ignore clicks while a start is mid-flight (connecting),
          // so a non-responsive moment doesn't fire a burst of start events.
          if (listenState === "connecting") return;
          if (listenState === "listening") client.stopListening();
          else client.startListening();
        }}
        listenState={listenState}
      />
    </div>
  );
}

// ---- The trigger-moment hero card (a detected for-me question) ----
const heroWrap = { padding: "10px 14px 4px" } as const;
const heroCard = {
  borderRadius: "var(--r-lg)",
  border: "1px solid var(--tint-wash)",
  background:
    "linear-gradient(180deg, var(--tint-wash), rgba(255,255,255,0.4))",
  padding: "13px 15px",
  boxShadow: "0 6px 22px -14px var(--tint)",
} as const;
const heroEyebrow = {
  fontSize: 11,
  fontWeight: 600,
  color: "var(--tint-ink)",
  display: "flex",
  alignItems: "center",
  gap: 7,
  marginBottom: 6,
} as const;
const heroDot = {
  width: 7,
  height: 7,
  borderRadius: 999,
  background: "var(--tint-ink)",
  boxShadow: "0 0 8px var(--tint)",
  animation: "blueyPulse 1.6s ease-in-out infinite",
} as const;
const heroQ = {
  fontSize: 15,
  lineHeight: 1.45,
  fontWeight: 540,
  color: "var(--ink)",
  letterSpacing: "-.01em",
} as const;
const heroAsk = {
  fontSize: 12.5,
  fontWeight: 600,
  color: "#fff",
  background: "var(--tint)",
  border: "none",
  borderRadius: "var(--r-pill)",
  padding: "7px 15px",
  cursor: "pointer",
} as const;
const heroDismiss = {
  fontSize: 12.5,
  color: "var(--ink-3)",
  background: "transparent",
  border: "none",
  borderRadius: "var(--r-pill)",
  padding: "7px 12px",
  cursor: "pointer",
} as const;

// ---- The ambient caption (a single quiet live line — "felt, not read") ----
const captionWrap = {
  display: "flex",
  alignItems: "center",
  gap: 8,
  padding: "7px 16px",
  borderTop: "1px solid var(--line)",
  minWidth: 0,
} as const;
const captionDot = {
  width: 6,
  height: 6,
  borderRadius: 999,
  background: "var(--mint)",
  flex: "none",
  animation: "blueyPulse 1.8s ease-in-out infinite",
} as const;
const captionWho = {
  fontSize: 10.5,
  fontWeight: 600,
  color: "var(--ink-4)",
  letterSpacing: ".04em",
  flex: "none",
} as const;
const captionText = {
  fontSize: 12,
  color: "var(--ink-3)",
  whiteSpace: "nowrap",
  overflow: "hidden",
  textOverflow: "ellipsis",
  flex: 1,
  minWidth: 0,
} as const;
const trig = {
  display: "flex",
  alignItems: "center",
  gap: 10,
  padding: "5px 16px",
} as const;
const ln = { flex: 1, height: 1, background: "var(--line-2)" } as const;
const trigLbl = {
  fontSize: 10.5,
  color: "var(--tint-ink)",
  fontWeight: 540,
  background: "var(--tint-wash)",
  padding: "3px 10px",
  borderRadius: "var(--r-pill)",
} as const;

// The Ask tab — the live loop. Transcript line → question-detection trigger →
// the agent drives (slow → ThinkingState) → streamed answer with grounded
// source rows → actions. The whole product happens here.

import { useEffect, useRef, useState } from "react";
import { getClient } from "../lib";
import type {
  AgentSummary,
  AnswerSource,
  AnswerStatusStep,
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
  const askRef = useRef<{ cancel(): void } | null>(null);
  const turnSeq = useRef(0);
  const feedEndRef = useRef<HTMLDivElement>(null);

  useEffect(() => client.onTranscript((l) => l.final && setTranscript(l)), [client]);
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
      .then((cs) => live && setConnectors(cs.filter((c) => c.ready).map((c) => c.name)))
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

    askRef.current = client.ask(question, (c) => {
      if (c.status) steps = c.status;
      if (c.statusDone !== undefined) statusDone = c.statusDone;
      if (c.text) {
        draft.text += c.text;
        setPhase("answering");
      }
      if (c.tool) draft.tools = [...draft.tools, c.tool];
      if (c.source) draft.sources = [...draft.sources, c.source as AnswerSource];
      if (c.done) draft.done = true;
      patch();
    });
  };

  const askDetected = () => transcript && runAsk(transcript.text);

  return (
    <div style={{ flex: 1, display: "flex", flexDirection: "column", minHeight: 0 }}>
      <AgentBar agent={agent} connectorNames={connectorNames} extraCount={extraConnectors} />

      <div style={{ flex: 1, minHeight: 0, padding: "6px 0 2px", overflowY: "auto" }}>
        {transcript && (
          <div style={{ padding: "9px 16px" }}>
            <div style={role}>
              TRANSCRIPT
              <span style={meta}>
                {transcript.speaker ?? transcript.source} · heard
              </span>
            </div>
            <div style={{ fontSize: 13.5, lineHeight: 1.6, color: "var(--ink-2)" }}>{transcript.text}</div>
            {phase === "idle" && (
              <button onClick={askDetected} style={detectBtn}>
                ✦ Ask your agent about this
              </button>
            )}
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
              {live && turn.statusSteps.length === 0 && turn.answer.text === "" && (
                <ThinkingState detail={`asking ${agent?.displayName ?? "your agent"}…`} />
              )}

              {/* The agent's real reasoning + tool calls for this turn. */}
              {turn.statusSteps.length > 0 && (
                <StatusFeed steps={turn.statusSteps} done={turn.statusDone} />
              )}

              {(turn.answer.text !== "" || turn.answer.done) && (
                <AnswerCard
                  answer={turn.answer}
                  onCopy={() => navigator.clipboard?.writeText(turn.answer.text)}
                />
              )}
            </div>
          );
        })}

        {turns.length === 0 && !transcript && (
          <div style={{ padding: "28px 16px", textAlign: "center", color: "var(--ink-3)", fontSize: 13 }}>
            Listening — ask a question, or one will be detected from the call.
          </div>
        )}

        <div ref={feedEndRef} />
      </div>

      <Composer
        placeholder="Ask a follow-up while Bluey listens…"
        contextLabel={transcript ? "in context · live transcript" : undefined}
        onSubmit={runAsk}
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

const role = {
  fontSize: 10,
  fontWeight: 680,
  letterSpacing: ".1em",
  color: "var(--ink-3)",
  marginBottom: 5,
  display: "flex",
  alignItems: "center",
  gap: 8,
} as const;
const meta = { fontWeight: 430, letterSpacing: 0, textTransform: "none", color: "var(--ink-4)" } as const;
const detectBtn = {
  marginTop: 9,
  fontSize: 11.5,
  color: "var(--tint-ink)",
  background: "var(--tint-wash)",
  border: "none",
  borderRadius: "var(--r-pill)",
  padding: "5px 12px",
  cursor: "pointer",
} as const;
const trig = { display: "flex", alignItems: "center", gap: 10, padding: "5px 16px" } as const;
const ln = { flex: 1, height: 1, background: "var(--line-2)" } as const;
const trigLbl = {
  fontSize: 10.5,
  color: "var(--tint-ink)",
  fontWeight: 540,
  background: "var(--tint-wash)",
  padding: "3px 10px",
  borderRadius: "var(--r-pill)",
} as const;

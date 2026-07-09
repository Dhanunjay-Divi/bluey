// The Ask tab — the live loop. Transcript line → question-detection trigger →
// the agent drives (slow → ThinkingState) → streamed answer with grounded
// source rows → actions. The whole product happens here.

import { useEffect, useRef, useState } from "react";
import { getClient } from "../lib";
import { useDataStore } from "../lib/dataStore";
import { useMeetingState } from "../lib/meetingState";
import type {
  AgentSummary,
  AnswerSource,
  AnswerStatusStep,
  AskMode,
  ListeningState,
} from "../lib/types";
import { AgentBar } from "../components/AgentBar";
import {
  AgentInstallCard,
  type AgentInstallOffer,
} from "../components/AgentInstallCard";
import { AnswerCard, type AnswerState } from "../components/AnswerCard";
import { Composer } from "../components/Composer";
import { ThinkingState } from "../components/primitives";
import { StatusFeed } from "../components/StatusFeed";

type Phase = "idle" | "detected" | "thinking" | "answering";

// Canonical prompt for the manual "ask about what was just said" button — the
// fallback when question-detection misses. The daemon's context assembly
// already attaches the rolling summary + decisions ledger + recent transcript
// to every ask, so this needs no extra payload: the question itself just points
// the agent at the tail of the transcript.
const ASK_RECENT_QUESTION =
  "Answer the most recent question or request raised in the meeting " +
  "transcript. If the last lines contain no question, briefly answer what " +
  "would be most useful about what was just discussed.";

export function AskScreen({ agent }: { agent: AgentSummary | null }) {
  const client = getClient();
  // Session state (transcript, history, Q&A feed, detected question) lives in
  // MeetingProvider above <App/> so it survives collapse/onboarding/tab switches
  // and a full overlay restart (rehydrated once from the daemon). AskScreen is a
  // pure VIEW that reads it; the live onTranscript/onForMeQuestion subscriptions
  // live in the provider (single owner), not here.
  const {
    transcript,
    history,
    turns,
    detectedQ,
    setDetectedQ,
    appendTurn,
    patchTurn,
    turnSeq,
  } = useMeetingState();
  const captionScrollRef = useRef<HTMLDivElement>(null);
  const captionPinnedRef = useRef(true);
  // Auto-scroll the transcript panel to the newest line, UNLESS the user has
  // scrolled up to read earlier history (captionPinnedRef tracks that).
  useEffect(() => {
    const el = captionScrollRef.current;
    if (el && captionPinnedRef.current) el.scrollTop = el.scrollHeight;
  }, [history]);
  const [phase, setPhase] = useState<Phase>("idle");
  const [connectors, setConnectors] = useState<string[]>([]);
  const [listenState, setListenState] = useState<ListeningState>("idle");
  // Daemon offer to install a missing agent CLI (push_agent_install). Shown as a
  // card with Install / Not now; the daemon reports the install result as a card.
  const [installOffer, setInstallOffer] = useState<AgentInstallOffer | null>(
    null,
  );
  useEffect(
    () => client.onAgentInstall((offer) => setInstallOffer(offer)),
    [client],
  );
  // The answer-speed preset, forwarded to the daemon as `mode`. Defaults to
  // "balanced" so an untouched composer behaves exactly as before.
  const [mode, setMode] = useState<AskMode>("balanced");
  // The attached agent's selectable models ("auto" is always element [0]) and
  // the current pick. Fetched when the agent changes; the Composer shows the
  // picker beside the speed pills only when there is more than one choice.
  const [selectedModel, setSelectedModel] = useState("auto");
  const agentKind = agent?.kind ?? null;
  // Models come from the SHARED store (cached per kind + revalidated in the
  // background), so the Composer's picker appears INSTANTLY on re-attach instead
  // of after a per-mount CLI round-trip. `modelsFor` returns the cached list (or
  // null until first load); `ensureModels` lazily loads a kind once.
  const { modelsFor, ensureModels } = useDataStore();
  const models = modelsFor(agentKind) ?? [];
  useEffect(() => {
    // Reset the pick to "auto" whenever the agent changes so a model chosen for
    // the previous agent never lingers as a stale value the new one lacks.
    setSelectedModel("auto");
    if (agentKind) ensureModels(agentKind);
  }, [agentKind, ensureModels]);
  const askRef = useRef<{ cancel(): void } | null>(null);
  const feedEndRef = useRef<HTMLDivElement>(null);

  // onListeningState stays HERE — listenState is view-local. (onTranscript and
  // onForMeQuestion moved to MeetingProvider as the single owner.)
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
    appendTurn({
      id,
      question,
      answer: { ...draft },
      statusSteps: steps,
      statusDone,
    });

    const patch = () =>
      patchTurn(id, {
        answer: { ...draft },
        statusSteps: steps,
        statusDone,
      });

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
        if (c.done) {
          draft.done = true;
          // Answer finished → return to idle so the live caption (gated on
          // phase==="idle") reappears. The answer itself persists in the feed
          // above (turns are appended, not cleared). Without this, phase stayed
          // "answering" forever and the transcript vanished after the first ask.
          setPhase("idle");
        }
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

  // True while an ask is streaming: the newest turn exists and hasn't received
  // its `done` chunk yet. (The turn's `done` flag is the authoritative signal;
  // `phase` returns to "idle" the instant `done` arrives so the live caption
  // reappears.) Used to disable the manual ask-recent button so taps can't
  // stack asks mid-stream.
  const lastTurn = turns.length > 0 ? turns[turns.length - 1] : undefined;
  const askStreaming =
    phase !== "idle" && lastTurn !== undefined && !lastTurn.answer.done;

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

      {installOffer && (
        <AgentInstallCard
          offer={installOffer}
          onInstall={() => {
            client.respondAgentInstall(installOffer.kind, true);
            setInstallOffer(null);
          }}
          onCancel={() => {
            client.respondAgentInstall(installOffer.kind, false);
            setInstallOffer(null);
          }}
        />
      )}

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
          transcript wall. Kept visible even when a question is detected — the
          live caption reassures the user that Bluey is still hearing them (a
          detected question used to HIDE it, which looked like transcription had
          stopped). Only hidden while answering (the answer owns the screen). */}
      {history.length > 0 && phase === "idle" && (
        <div
          ref={captionScrollRef}
          style={captionScroll}
          onScroll={(e) => {
            // Track whether the user is pinned to the bottom. If they scroll up
            // to read history, we stop auto-scrolling so we don't yank them back.
            const el = e.currentTarget;
            captionPinnedRef.current =
              el.scrollHeight - el.scrollTop - el.clientHeight < 24;
          }}
        >
          {history.map((line, i) => (
            <div key={i} style={captionWrap}>
              <span style={captionDot} />
              <span style={captionWho}>
                {line.source === "mic" ? "You" : "They"}
              </span>
              <span style={captionText}>{line.text.replace(/^\s+/, "")}</span>
            </div>
          ))}
        </div>
      )}

      <Composer
        placeholder="Ask a follow-up while Bluey listens…"
        contextLabel={transcript ? "live transcript · in context" : undefined}
        onSubmit={runAsk}
        onAskRecent={() => runAsk(ASK_RECENT_QUESTION)}
        askRecentDisabled={askStreaming}
        mode={mode}
        onModeChange={setMode}
        models={models}
        selectedModel={selectedModel}
        onModelChange={(m) => {
          setSelectedModel(m);
          // Re-attach the SAME agent carrying the model override ("auto" clears
          // it). Same mechanism the header attach uses; the daemon persists
          // attached_model and applies it on the next answer.
          if (agentKind) {
            void client.attach(
              agentKind,
              undefined,
              m === "auto" ? undefined : m,
            );
          }
        }}
        onMic={() => {
          // Debounce: ignore clicks while a start is mid-flight (connecting),
          // so a non-responsive moment doesn't fire a burst of start events.
          if (listenState === "connecting") return;
          // Permission denied → the mic click opens Settings (the recovery the
          // old "+"-menu "Grant Screen Recording…" item used to offer).
          if (listenState === "permission_denied") {
            client.openPermissionSettings("screen_recording");
            return;
          }
          if (listenState === "listening") client.stopListening();
          // v1 captures SYSTEM audio (the other people in the call — the
          // question trigger), matching the intent of the old "+"-menu Listen
          // item that this outside mic button replaces.
          else client.startListening({ microphone: false, system: true });
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
// Scrollable transcript panel: bounded height, scrolls vertically so the full
// history is readable. Border-top separates it from the feed above; the pinned
// composer sits below it.
const captionScroll = {
  maxHeight: 108,
  overflowY: "auto",
  overflowX: "hidden",
  borderTop: "1px solid var(--line)",
} as const;
const captionWrap = {
  display: "flex",
  alignItems: "flex-start",
  gap: 8,
  padding: "5px 16px",
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
  // Inside the scrollable panel each line shows in FULL — wrap freely (the
  // panel scrolls), and break any pathological unbroken run so nothing overflows
  // horizontally.
  whiteSpace: "pre-wrap",
  overflowWrap: "anywhere",
  wordBreak: "break-word",
  lineHeight: 1.4,
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

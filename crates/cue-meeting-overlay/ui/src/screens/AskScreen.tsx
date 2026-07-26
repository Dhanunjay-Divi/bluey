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
  ContextItem,
  ListeningState,
} from "../lib/types";
import { AgentBar } from "../components/AgentBar";
import {
  AgentInstallCard,
  type AgentInstallOffer,
} from "../components/AgentInstallCard";
import { AnswerCard, type AnswerState } from "../components/AnswerCard";
import { Composer } from "../components/Composer";
import { FixProposalCard } from "../components/FixProposalCard";
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

// The human-readable label shown in the feed for an "ask recent" turn — the
// ASK_RECENT_QUESTION text above is an internal instruction to the agent and
// must NEVER be shown as if the user asked it. Kept in sync with the daemon's
// visible_question_for_source mapping.
const ASK_RECENT_LABEL = "Answering the question just asked in the meeting.";

export function AskScreen({ agent }: { agent: AgentSummary | null }) {
  const client = getClient();
  // Session state (transcript, history, Q&A feed, detected question) lives in
  // MeetingProvider above <App/> so it survives collapse/onboarding/tab switches
  // and a full overlay restart (rehydrated once from the daemon). AskScreen is a
  // pure VIEW that reads it; the live onTranscript/onForMeQuestion subscriptions
  // live in the provider (single owner), not here.
  const {
    transcript,
    turns,
    detectedQ,
    setDetectedQ,
    fixProposal,
    setFixProposal,
    appendTurn,
    patchTurn,
    turnSeq,
  } = useMeetingState();
  // (The caption scroll refs went with the ambient-caption block — the bottom
  // `LiveTranscriptBar` owns transcript scrolling + auto-follow now.)
  const [phase, setPhase] = useState<Phase>("idle");
  const [connectors, setConnectors] = useState<string[]>([]);
  const [listenState, setListenState] = useState<ListeningState>("idle");
  // Microphone capture (YOUR voice) is an independent source from the system
  // audio the listen button toggles. Tracked here so the two buttons can be
  // toggled separately and the current pair is re-sent on every change (the
  // daemon's start takes both flags at once).
  const [micInputOn, setMicInputOn] = useState(false);
  // Attached context artifacts (the "+" menu: files, screenshots, pages), pushed
  // by the daemon as set_context_items. Rendered as ChatGPT-style chips above the
  // composer input; the strip is a pure mirror of the daemon's list.
  const [contextItems, setContextItems] = useState<ContextItem[]>([]);
  useEffect(() => client.onContextItems(setContextItems), [client]);
  // Daemon offer to install a missing agent CLI (push_agent_install). Shown as a
  // card with Install / Not now; the daemon reports the install result as a card.
  const [installOffer, setInstallOffer] = useState<AgentInstallOffer | null>(
    null,
  );
  useEffect(
    () => client.onAgentInstall((offer) => setInstallOffer(offer)),
    [client],
  );
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
  const feedScrollRef = useRef<HTMLDivElement>(null);
  // Whether the user is pinned to the bottom of the feed. Same guard the
  // caption uses: while true we auto-follow new content; once the user scrolls
  // UP (to read the reasoning above a streaming answer) we STOP yanking them
  // back down. Without this, the feed re-pinned to the answer's tail on every
  // streamed token, shoving the reasoning off the top — the "reply at the
  // bottom, thinking scrolls up" symptom.
  const feedPinnedRef = useRef(true);

  // onListeningState stays HERE — listenState is view-local. (onTranscript and
  // onForMeQuestion moved to MeetingProvider as the single owner.)
  useEffect(() => client.onListeningState(setListenState), [client]);
  // Follow the newest turn as the feed grows — but ONLY when pinned to the
  // bottom, so scrolling up to read reasoning is not fought. No smooth
  // behavior: the per-token animation read as a "snap/jump" on each chunk.
  useEffect(() => {
    if (feedPinnedRef.current) {
      feedEndRef.current?.scrollIntoView({ block: "end" });
    }
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

  // `question` is what we SEND the agent; `displayQuestion` (when given) is what
  // we SHOW in the feed. They differ for the "ask recent" path, where the sent
  // text is an internal instruction that must never surface as the user's words.
  const runAsk = (question: string, displayQuestion?: string) => {
    askRef.current?.cancel();
    // Asking ANYTHING (typed, ask-recent, or a detected tap) makes any pending
    // detected-question suggestion moot — clear the hero so it can't linger on
    // screen next to the new turn. (askDetected already nulls it before calling
    // us; this covers the typed-Composer path, which previously left a stale
    // hero card up with its Ask/Dismiss buttons.)
    setDetectedQ(null);
    setPhase("thinking");
    // A fresh ask always follows initially (re-pin), even if the user had
    // scrolled up while reading a prior answer.
    feedPinnedRef.current = true;

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
      question: displayQuestion ?? question,
      // Remember the real send-text so a retry re-sends the instruction, not the
      // display label (only meaningfully differs for the ask-recent path).
      sendQuestion: displayQuestion ? question : undefined,
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
          if (c.error) draft.error = true;
          // Answer finished → return to idle so the live caption (gated on
          // phase==="idle") reappears. The answer itself persists in the feed
          // above (turns are appended, not cleared). Without this, phase stayed
          // "answering" forever and the transcript vanished after the first ask.
          setPhase("idle");
        }
        patch();
      },
    );
  };

  const askDetected = () => {
    // The hero card DISPLAYS the detected question (readable), but what we ASK
    // is ASK_RECENT_QUESTION — the detected text is only a FRAGMENT of the
    // spoken question (STT splits one question across several finalized
    // segments), so asking it verbatim ships the agent a truncated "are there
    // any" and it replies "your message looks cut off". The context envelope
    // already carries the recent transcript, so pointing the agent at the
    // transcript tail lets it read the COMPLETE question itself. Falls back to
    // asking nothing when there's neither a detected question nor a caption.
    if (!detectedQ?.text && !transcript?.text) return;
    setDetectedQ(null);
    runAsk(ASK_RECENT_QUESTION, ASK_RECENT_LABEL);
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
        ref={feedScrollRef}
        onScroll={(e) => {
          // Pin-to-bottom guard: while the user is at the bottom we auto-follow
          // streaming answers; the moment they scroll UP to read the reasoning,
          // we stop yanking them back. (Padding-tolerant threshold.)
          const el = e.currentTarget;
          feedPinnedRef.current =
            el.scrollHeight - el.scrollTop - el.clientHeight < 28;
        }}
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
                  onFix={
                    // "Fix this" only on a completed, non-error answer — a fix
                    // proposes against the diagnosis text. Fires requestFix; the
                    // daemon drives the agent in propose-only mode and PUSHES a
                    // proposal that MeetingProvider stores as `fixProposal`. In
                    // beta the resulting card previews only (Apply disabled).
                    turn.answer.done && !turn.answer.error
                      ? () => client.requestFix(turn.answer.text)
                      : undefined
                  }
                  onRetry={
                    turn.answer.error
                      ? () => runAsk(turn.sendQuestion ?? turn.question, turn.sendQuestion ? turn.question : undefined)
                      : undefined
                  }
                />
              )}
            </div>
          );
        })}

        {/* The proposed fix (Fix-button slice F3): the agent's diagnosis +
            reasoning + diff, pushed after a Fix click. BETA: preview-only — the
            card's Apply button is disabled; Dismiss clears the proposal. */}
        {fixProposal && (
          <FixProposalCard
            proposal={fixProposal}
            onDismiss={() => setFixProposal(null)}
          />
        )}

        {turns.length === 0 && !detectedQ && !fixProposal && (
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

      {/* The ambient caption that used to live here was REMOVED: the panel's
          bottom row (`LiveTranscriptBar`) now owns live transcription — current
          line by default, expandable to the scrollable history. Keeping this
          block rendered the SAME history twice (reported: the transcript
          appearing both above the composer and in the bottom bar). One surface
          for transcription, and it is the bottom bar. */}

      <Composer
        placeholder="Ask a follow-up while Bluey listens…"
        contextLabel={transcript ? "live transcript · in context" : undefined}
        contextItems={contextItems}
        onRemoveContext={(id) => client.removeContextItem(id)}
        onSubmit={(q) => runAsk(q)}
        onAskRecent={() => runAsk(ASK_RECENT_QUESTION, ASK_RECENT_LABEL)}
        askRecentDisabled={askStreaming}
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
          // This button is SYSTEM audio (the other people in the call — the
          // question trigger). The microphone is the separate button beside it,
          // whose current state rides along so toggling one never silently
          // drops the other.
          else client.startListening({ microphone: micInputOn, system: true });
        }}
        onToggleMicInput={() => {
          const nextMic = !micInputOn;
          setMicInputOn(nextMic);
          if (!nextMic && listenState === "listening") {
            client.startListening({ microphone: false, system: true });
          } else {
            client.startListening({ microphone: nextMic, system: true });
          }
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

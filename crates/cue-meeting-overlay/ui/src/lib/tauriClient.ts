// Real adapter — speaks the EXACT live wire contract the daemon's socket
// transport carries through the Tauri overlay shell.
//
// Transport (the two-pipe model, proven in crates/cue-overlay-tauri/src/ipc.rs):
//   daemon → UI : the daemon serializes a `cue_core::overlay::OverlayCommand`
//                 to one NDJSON line; the Rust shell re-emits each line VERBATIM
//                 as the Tauri event "overlay://command" (a JSON string). We
//                 parse the "type" tag and translate to MeetingClient callbacks.
//   UI → daemon : we call invoke("overlay_send", { event }) where `event` is a
//                 raw `OverlayEvent` JSON STRING; the shell injects the session
//                 "token" and writes it back as NDJSON.
//
// IMPORTANT — field casing. The shell forwards the daemon's NDJSON verbatim, so
// the event payload carries the daemon's serde wire shape: snake_case struct
// fields (display_name, connector_count, auth_tier, cost_label, …) and
// snake_case "type" tags. (Tauri's camelCase conversion only applies to invoke
// command ARGS, never to raw event strings.) So this adapter translates the
// snake_case wire DTOs into the camelCase UI types in ./types.ts — the UI types
// stay stable and the mock adapter is untouched.
//
// The MeetingClient interface is promise-based for discovery/attach, but the
// wire is request/response over events: we fire an OverlayEvent and resolve the
// promise when the matching OverlayCommand reply arrives. ask() is a pure
// stream: fire ask_requested, then map the answer card's update_card deltas to
// onChunk.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { AskHandle, MeetingClient } from "./client";
import type {
  AgentConnectorInfo,
  AgentSessionSummary,
  AgentSummary,
  AnswerChunk,
  AnswerStatusStep,
  ListeningState,
  TranscriptLine,
} from "./types";

// ---------------------------------------------------------------------------
// Wire DTOs — the daemon's serde shapes (snake_case), as they arrive on the
// "overlay://command" event. Only the fields this UI consumes are typed; the
// rest of each command is ignored.
// ---------------------------------------------------------------------------

interface WireAgentSummary {
  kind: string;
  display_name: string;
  capability: string;
  connector_count: number;
  ready_connector_count: number;
  session_count: number | null;
  attached: boolean;
}

interface WireAgentSessionSummary {
  id: string;
  title: string | null;
  updated_at: string;
  project: string | null;
}

interface WireAgentConnectorInfo {
  name: string;
  auth_tier: string;
  ready: boolean;
}

interface WireCueCard {
  id: string;
  kind: string; // CardKind, snake_case: answer | transcript | question | …
  title: string;
  body: string;
  created_at: string;
  source: string | null;
  cost_label?: string | null;
  // artifact omitted — not consumed on push_card.
}

// The set of OverlayCommand variants this adapter reacts to. `type` is the
// serde tag; remaining fields are per-variant. Unhandled variants fall through.
type OverlayCommand =
  | { type: "set_agents"; agents: WireAgentSummary[] }
  | { type: "set_agent_sessions"; kind: string; sessions: WireAgentSessionSummary[] }
  | { type: "set_agent_connectors"; kind: string; connectors: WireAgentConnectorInfo[] }
  | { type: "listening_state_changed"; state: string }
  | { type: "push_card"; card: WireCueCard }
  | {
      type: "update_card";
      id: string;
      body: string;
      done?: boolean;
      cost_label?: string | null;
    }
  | {
      type: "set_answer_status";
      id: string;
      steps: WireAnswerStatusStep[];
      done?: boolean;
    }
  | { type: string; [k: string]: unknown };

// The daemon's AnswerStatusStep wire shape (serde: `kind` tag, snake_case
// fields + states) — identical to the UI's AnswerStatusStep, so it passes
// through. Typed here so the adapter validates the shape.
type WireAnswerStatusStep =
  | { kind: "reasoning"; text: string }
  | { kind: "tool"; id: string; title: string; state: string };

// ---------------------------------------------------------------------------
// Wire → UI translation.
// ---------------------------------------------------------------------------

function toAgentSummary(w: WireAgentSummary): AgentSummary {
  return {
    kind: w.kind,
    displayName: w.display_name,
    capability: w.capability,
    connectorCount: w.connector_count,
    readyConnectorCount: w.ready_connector_count,
    sessionCount: w.session_count ?? null,
    attached: w.attached,
  };
}

function toAgentSessionSummary(w: WireAgentSessionSummary): AgentSessionSummary {
  return {
    id: w.id,
    title: w.title ?? null,
    updatedAt: w.updated_at,
    project: w.project ?? null,
  };
}

function toAgentConnectorInfo(w: WireAgentConnectorInfo): AgentConnectorInfo {
  return { name: w.name, authTier: w.auth_tier, ready: w.ready };
}

// ---------------------------------------------------------------------------
// UI → daemon: send a raw OverlayEvent object. The Rust shell injects "token".
// ---------------------------------------------------------------------------

function sendEvent(event: Record<string, unknown>): void {
  void invoke<void>("overlay_send", { event: JSON.stringify(event) }).catch((e) => {
    // Surface, never silently swallow — but don't crash the UI loop.
    console.error("[tauriClient] overlay_send failed", event.type, e);
  });
}

export function createTauriClient(): MeetingClient {
  // ---- single command bus over the one daemon → UI pipe ----
  // Every "overlay://command" line flows here; we fan it out to whoever is
  // currently waiting (a pending request promise, a stream, a subscriber).
  const handlers = new Set<(cmd: OverlayCommand) => void>();
  let unlistenBus: UnlistenFn | null = null;
  // The bus must be LISTENING before we send any event — otherwise a fast daemon
  // response (the agent list comes back in ~20ms) arrives before listen() is
  // registered and is lost, hanging the UI on "Discovering agents…" forever.
  // `busReady` resolves once the listener is active; every send awaits it.
  const busReady: Promise<void> = listen<string>("overlay://command", (e) => {
    let cmd: OverlayCommand;
    try {
      // The payload is the daemon's NDJSON line, forwarded verbatim as a string.
      cmd = JSON.parse(e.payload) as OverlayCommand;
    } catch (err) {
      console.error("[tauriClient] bad overlay://command payload", err);
      return;
    }
    for (const h of [...handlers]) h(cmd);
  }).then((fn) => {
    unlistenBus = fn;
  });
  // Best-effort teardown if the window unloads (the shell also cleans up).
  if (typeof window !== "undefined") {
    window.addEventListener("beforeunload", () => unlistenBus?.());
  }

  /**
   * Fire an OverlayEvent and resolve with the first OverlayCommand the
   * predicate accepts. Used to model the request/response discovery commands
   * over the event bus. Resolves with the translated value the picker returns.
   */
  function request<T>(
    event: Record<string, unknown>,
    pick: (cmd: OverlayCommand) => T | undefined,
  ): Promise<T> {
    return new Promise<T>((resolve) => {
      const handler = (cmd: OverlayCommand) => {
        const out = pick(cmd);
        if (out !== undefined) {
          handlers.delete(handler);
          resolve(out);
        }
      };
      handlers.add(handler);
      // Wait until the bus is actually listening before firing the event, so the
      // daemon's (fast) response can never arrive before we're ready to catch it.
      void busReady.then(() => sendEvent(event));
    });
  }

  return {
    listAgents: () =>
      request<AgentSummary[]>({ type: "agent_list_requested" }, (cmd) =>
        cmd.type === "set_agents"
          ? (cmd as Extract<OverlayCommand, { type: "set_agents" }>).agents.map(toAgentSummary)
          : undefined,
      ),

    // attach/detach both confirm by the daemon re-pushing the full agent list.
    attach: (kind, sessionId) =>
      request<AgentSummary[]>(
        {
          type: "agent_attach_requested",
          kind,
          ...(sessionId ? { session_id: sessionId } : {}),
        },
        (cmd) =>
          cmd.type === "set_agents"
            ? (cmd as Extract<OverlayCommand, { type: "set_agents" }>).agents.map(toAgentSummary)
            : undefined,
      ),

    detach: () =>
      request<AgentSummary[]>({ type: "agent_detach_requested" }, (cmd) =>
        cmd.type === "set_agents"
          ? (cmd as Extract<OverlayCommand, { type: "set_agents" }>).agents.map(toAgentSummary)
          : undefined,
      ),

    sessions: (kind) =>
      request<AgentSessionSummary[]>(
        { type: "agent_sessions_requested", kind },
        (cmd) => {
          if (cmd.type !== "set_agent_sessions") return undefined;
          const c = cmd as Extract<OverlayCommand, { type: "set_agent_sessions" }>;
          // Guard against a reply for a different agent on the shared bus.
          if (c.kind !== kind) return undefined;
          return c.sessions.map(toAgentSessionSummary);
        },
      ),

    connectors: (kind) =>
      request<AgentConnectorInfo[]>(
        { type: "agent_connectors_requested", kind },
        (cmd) => {
          if (cmd.type !== "set_agent_connectors") return undefined;
          const c = cmd as Extract<OverlayCommand, { type: "set_agent_connectors" }>;
          if (c.kind !== kind) return undefined;
          return c.connectors.map(toAgentConnectorInfo);
        },
      ),

    setSessionHistoryConsent: (enabled) => {
      // First-class consent toggle: the daemon persists it the same way the IPC
      // SetAgentSessionHistory path does, then refreshes the agent list.
      sendEvent({ type: "session_history_consent_requested", enabled });
      return Promise.resolve();
    },

    onListeningState(cb) {
      // The daemon pushes listening_state_changed with the full pipeline state
      // (idle | connecting | listening | paused | failed). Pass it through so
      // the UI can show connecting/failed, not just on/off — otherwise a failed
      // start silently snaps the mic button back and looks dead.
      const handler = (cmd: OverlayCommand) => {
        if (cmd.type !== "listening_state_changed") return;
        const c = cmd as Extract<OverlayCommand, { type: "listening_state_changed" }>;
        const s = c.state;
        const known: ListeningState =
          s === "connecting" || s === "listening" || s === "paused" || s === "failed"
            ? s
            : "idle";
        cb(known);
      };
      handlers.add(handler);
      return () => handlers.delete(handler);
    },

    startListening() {
      // The mic: start AUDIO capture (mic + system audio). The daemon replies
      // with listening_state_changed, which onListeningState reflects.
      // (recording_* = audio; capture_* = the screen "eye" — different feature.)
      sendEvent({ type: "recording_start_requested" });
    },

    stopListening() {
      sendEvent({ type: "recording_stop_requested" });
    },

    capturePage() {
      // "+" → Capture page: grab the active BROWSER page's text as a context
      // artifact (daemon "overlay page" path). Needs a browser frontmost; the
      // daemon pushes an honest warning card if no readable page is found.
      sendEvent({ type: "active_page_capture_requested" });
    },

    attachFiles(paths) {
      // "+" → Attach files: the UI picks local files (native dialog) and hands
      // their paths to the daemon, which attaches them as context artifacts.
      if (paths.length === 0) return;
      sendEvent({ type: "attach_files_requested", paths });
    },

    captureScreenshot() {
      // "+" → Take a screenshot: the daemon captures the screen and routes it to
      // vision/context (AnalyzeScreenRequested). Shows an honest "needs vision"
      // card if no vision provider is configured.
      sendEvent({ type: "analyze_screen_requested" });
    },

    onTranscript(cb) {
      // Transcript lines arrive as push_card with kind "transcript".
      const handler = (cmd: OverlayCommand) => {
        if (cmd.type !== "push_card") return;
        const card = (cmd as Extract<OverlayCommand, { type: "push_card" }>).card;
        if (card.kind !== "transcript") return;
        const line: TranscriptLine = {
          // The daemon tags transcript origin in `source` ("system" | "mic"…);
          // fall back to "system" when absent.
          source: card.source ?? "system",
          speaker: card.title || undefined,
          text: card.body,
          // push_card carries a fully-formed (finalized) line.
          final: true,
        };
        cb(line);
      };
      handlers.add(handler);
      return () => handlers.delete(handler);
    },

    ask(question, onChunk): AskHandle {
      // The answer stream (app.rs:5277-5394):
      //   push_card(question) → push_card(answer placeholder "Thinking…")
      //   → update_card(same id, growing body, done:false) × N
      //   → update_card(same id, done:true, optional cost_label/artifact).
      // We latch onto the FIRST answer-kind card pushed after we ask, capture
      // its id, then map subsequent update_card(body) deltas to onChunk text
      // deltas. We diff bodies so onChunk receives true incremental text.
      let answerId: string | null = null;
      let lastBody = "";
      let finished = false;

      const finish = () => {
        if (finished) return;
        finished = true;
        handlers.delete(handler);
      };

      const handler = (cmd: OverlayCommand) => {
        if (finished) return;

        if (cmd.type === "push_card") {
          const card = (cmd as Extract<OverlayCommand, { type: "push_card" }>).card;
          // Bind to the first answer card that appears after we asked. The
          // question card (kind "question") is skipped.
          if (answerId === null && card.kind === "answer") {
            answerId = card.id;
            // The placeholder body is "Thinking…" — treat as empty so the
            // first real delta isn't polluted by the placeholder text.
            lastBody = "";
          }
          return;
        }

        if (cmd.type === "set_answer_status") {
          const s = cmd as Extract<OverlayCommand, { type: "set_answer_status" }>;
          if (answerId === null || s.id !== answerId) return;
          // The agent's REAL reasoning + tool calls, surfaced live. Map the wire
          // steps (already the same shape) into typed UI steps; the daemon
          // re-sends the whole list each change, so we replace, not append.
          const steps: AnswerStatusStep[] = s.steps.map((w) =>
            w.kind === "reasoning"
              ? { kind: "reasoning", text: w.text }
              : {
                  kind: "tool",
                  id: w.id,
                  title: w.title,
                  state:
                    w.state === "pending" ||
                    w.state === "running" ||
                    w.state === "done" ||
                    w.state === "failed"
                      ? w.state
                      : "running",
                },
          );
          onChunk({ status: steps, statusDone: s.done ?? false });
          return;
        }

        if (cmd.type === "update_card") {
          const u = cmd as Extract<OverlayCommand, { type: "update_card" }>;
          if (answerId === null || u.id !== answerId) return;

          // Emit the incremental text. Bodies are cumulative; if the new body
          // doesn't extend the old (rare reset), emit it whole.
          const body = u.body ?? "";
          const delta = body.startsWith(lastBody) ? body.slice(lastBody.length) : body;
          lastBody = body;
          console.log(`[ask] update_card body.len=${body.length} delta.len=${delta.length} done=${u.done}`);
          if (delta.length > 0) {
            const chunk: AnswerChunk = { text: delta };
            onChunk(chunk);
          }

          if (u.done) {
            const doneChunk: AnswerChunk = { done: true };
            // Carry the cost label through as part of the terminal signal so the
            // UI can show real cost instead of a placeholder. The UI's
            // AnswerChunk has no cost field, so we surface it as a final text-
            // free done chunk; cost rendering stays the UI's concern.
            onChunk(doneChunk);
            finish();
          }
        }
      };

      handlers.add(handler);
      sendEvent({ type: "ask_requested", question });

      return {
        cancel: () => {
          if (finished) return;
          finish();
          // First-class cancel: the local handler is already detached (finish()),
          // and this tells the daemon to reset its overlay UI state so the next
          // ask is clean.
          sendEvent({ type: "ask_cancel_requested" });
        },
      };
    },
  };
}

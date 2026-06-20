// Real adapter — speaks the EXACT IPC the dashboard already uses (agent_list /
// agent_attach / agent_detach / agent_sessions / agent_connectors /
// set_agent_session_history), plus a streaming `ask` over Tauri events the
// meeting Tauri shell forwards from the daemon's AnswerStream + transcript feed.
// Implements the same MeetingClient as the mock, so the UI is unchanged.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { AskHandle, MeetingClient } from "./client";
import type {
  AgentConnectorInfo,
  AgentSessionSummary,
  AgentSummary,
  AnswerChunk,
  TranscriptLine,
} from "./types";

let askSeq = 0;

export function createTauriClient(): MeetingClient {
  return {
    listAgents: () => invoke<AgentSummary[]>("agent_list"),
    attach: (kind, sessionId) =>
      invoke<AgentSummary[]>("agent_attach", { kind, sessionId }),
    detach: () => invoke<AgentSummary[]>("agent_detach"),
    sessions: (kind) => invoke<AgentSessionSummary[]>("agent_sessions", { kind }),
    connectors: (kind) =>
      invoke<AgentConnectorInfo[]>("agent_connectors", { kind }),
    setSessionHistoryConsent: (enabled) =>
      invoke<void>("set_agent_session_history", { enabled }),

    onTranscript(cb) {
      // The Tauri shell re-emits the daemon's transcript events as
      // `meeting://transcript`. listen() resolves to an unlisten fn async; we
      // return a sync unsubscribe that awaits it.
      let unlisten: (() => void) | null = null;
      void listen<TranscriptLine>("meeting://transcript", (e) => cb(e.payload)).then(
        (fn) => (unlisten = fn),
      );
      return () => unlisten?.();
    },

    ask(question, onChunk): AskHandle {
      const id = `ask-${++askSeq}`;
      let unlisten: (() => void) | null = null;
      void listen<AnswerChunk>(`meeting://answer/${id}`, (e) => {
        onChunk(e.payload);
        if (e.payload.done) unlisten?.();
      }).then((fn) => (unlisten = fn));
      // Fire-and-forget; chunks arrive over the event channel above.
      void invoke<void>("meeting_ask", { id, question });
      return {
        cancel: () => {
          unlisten?.();
          void invoke<void>("meeting_ask_cancel", { id }).catch(() => {});
        },
      };
    },
  };
}

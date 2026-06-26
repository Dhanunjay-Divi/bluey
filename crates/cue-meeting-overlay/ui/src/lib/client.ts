// The single boundary between the meeting UI and the backend.
//
// The UI talks ONLY to this interface — never directly to Tauri or the daemon.
// That keeps the whole frontend testable/demoable without a running daemon (the
// mock adapter), and lets the real Tauri→daemon adapter (which speaks the SAME
// IPC the dashboard already uses: agent_list / agent_attach / ask …) be swapped
// in with zero UI change. New output surfaces (Slack/Teams, per the v0.6 plan)
// also plug in here, never in the views.

import type {
  AgentConnectorInfo,
  AgentSessionSummary,
  AgentSummary,
  AnswerChunk,
  AskOptions,
  ListeningState,
  TranscriptLine,
} from "./types";

export interface AskHandle {
  /** Cancel an in-flight ask. */
  cancel(): void;
}

export interface MeetingClient {
  // ---- discovery / attach (the proven agent-bridge commands) ----
  listAgents(): Promise<AgentSummary[]>;
  attach(kind: string, sessionId?: string): Promise<AgentSummary[]>;
  detach(): Promise<AgentSummary[]>;
  sessions(kind: string): Promise<AgentSessionSummary[]>;
  connectors(kind: string): Promise<AgentConnectorInfo[]>;
  setSessionHistoryConsent(enabled: boolean): Promise<void>;

  // ---- the live loop ----
  /** Subscribe to live transcript lines; returns an unsubscribe fn. */
  onTranscript(cb: (line: TranscriptLine) => void): () => void;
  /**
   * Ask the attached agent a question; streams chunks (text / tool-fired /
   * source / done) to `onChunk`. The agent answer is SLOW by nature (driving a
   * real CLI/ACP agent + MCP round-trips) — callers render a thinking state
   * until the first chunk arrives.
   *
   * `opts` carries optional answer-shaping fields (mode / provider / model)
   * forwarded verbatim to the daemon's AskRequested. Omitted fields let the
   * daemon keep its own defaults.
   */
  ask(
    question: string,
    onChunk: (chunk: AnswerChunk) => void,
    opts?: AskOptions,
  ): AskHandle;

  // ---- listening (mic / system audio capture) ----
  /** Subscribe to the daemon's listening state; returns an unsubscribe fn.
   *  State mirrors the daemon: idle | connecting | listening | paused | failed. */
  onListeningState(cb: (state: ListeningState) => void): () => void;
  /** Start audio capture (the mic/listen toggle). */
  startListening(): void;
  /** Stop audio capture. */
  stopListening(): void;

  // ---- context (the "+" menu: capture page / attach files / screenshot) ----
  /** Capture the active browser page's text as a context artifact. */
  capturePage(): void;
  /** Open the DAEMON-owned native file picker to attach files as context.
   *  (The overlay is an accessory app and cannot open NSOpenPanel itself.) */
  openAttachPicker(): void;
  /** Capture a screenshot of the screen and route it to context/vision. */
  captureScreenshot(): void;
}

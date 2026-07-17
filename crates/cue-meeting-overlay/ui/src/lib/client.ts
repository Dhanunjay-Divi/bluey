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
  SourceCoverageInfo,
  AgentSessionSummary,
  AgentSummary,
  AnswerChunk,
  AskOptions,
  ContinueResult,
  FixProposal,
  ListeningState,
  MeetingState,
  MeetingSummary,
  MeetingViewState,
  TranscriptLine,
} from "./types";

export interface AskHandle {
  /** Cancel an in-flight ask. */
  cancel(): void;
}

export interface MeetingClient {
  // ---- discovery / attach (the proven agent-bridge commands) ----
  listAgents(): Promise<AgentSummary[]>;
  /** Subscribe to daemon-PUSHED agent lists (set_agents); returns an unsubscribe
   *  fn. The daemon pushes set_agents not only as the reply to a one-shot
   *  {@link listAgents} request but also from the background full-refresh (SWR)
   *  and from attach/detach. This persistent subscription keeps a shared store
   *  live where listAgents() cannot — it resolves on the first reply and detaches.
   *  Single owner: subscribe ONCE, in the DataProvider. */
  onAgents(cb: (agents: AgentSummary[]) => void): () => void;
  attach(
    kind: string,
    sessionId?: string,
    model?: string,
  ): Promise<AgentSummary[]>;
  detach(): Promise<AgentSummary[]>;
  sessions(kind: string): Promise<AgentSessionSummary[]>;
  /** List the attached agent's selectable model ids (element [0] is always
      "auto" = no override). Live-scraped for Cursor/Antigravity, curated for
      the rest. */
  models(kind: string): Promise<string[]>;
  connectors(kind: string): Promise<AgentConnectorInfo[]>;
  /** Meeting-relevant source coverage for the ATTACHED agent (the onboarding
      coverage meter). Empty when no agent is attached. */
  sourceCoverage(): Promise<SourceCoverageInfo[]>;
  setSessionHistoryConsent(enabled: boolean): Promise<void>;

  /** Fetch the active meeting's transcript + Q&A once, to rehydrate on mount
   *  (Fix B). The MEETING is the source of truth (the daemon persists it); the
   *  overlay is a view that seeds itself from this snapshot so a collapse-remount
   *  or a full process restart never loses the transcript or prior exchanges.
   *  Resolves with empty arrays when no meeting is active. */
  meetingState(): Promise<MeetingState>;

  // ---- the MEETINGS lens ("my past meetings") ----
  /** List past meetings, newest-first, for the MEETINGS lens. Distinct from
   *  {@link sessions} (the AGENT-SESSION lens): these are meetings Bluey
   *  recorded, not coding-agent threads. Resolves with an empty list when the
   *  store holds none. */
  meetings(): Promise<MeetingSummary[]>;
  /** Open a past meeting for READ-ONLY viewing: returns its persisted transcript
   *  + Q&A snapshot. This NEVER activates the meeting or touches the live one —
   *  the daemon serves a pure read and sets `readOnly` true whenever a live or
   *  different active meeting would otherwise be clobbered. */
  openMeeting(id: string): Promise<MeetingViewState>;
  /** Continue a past meeting: make it the ACTIVE meeting so the Ask screen
   *  resumes in it. The daemon archives the current (idle) active meeting and
   *  activates the target, then pushes the active-rehydrate set_meeting_state
   *  that MeetingProvider reseeds from. BLOCKED (resolves { blocked:true }) when
   *  a recording is live and the target differs — the live meeting is untouched
   *  and a guidance card is shown. Resolves { ok:true } once activation is
   *  confirmed. */
  continueMeeting(id: string): Promise<ContinueResult>;
  /** Subscribe to daemon-pushed ACTIVE meeting-state reseeds (meeting_id
   *  absent) — emitted when a past meeting is continued into the active slot.
   *  Returns an unsubscribe fn. Read-only VIEW replies (meeting_id present)
   *  are NOT delivered here. */
  onMeetingReseed(cb: (state: MeetingState) => void): () => void;

  // ---- the live loop ----
  /** Subscribe to live transcript lines; returns an unsubscribe fn. */
  onTranscript(cb: (line: TranscriptLine) => void): () => void;
  /** Subscribe to diarization speaker-label upgrades for already-delivered
   *  transcript lines. Labels lag lines: the daemon's live diarize tick
   *  resolves "who said it" a few seconds after the text streamed in, then
   *  pushes (segmentId, label) so the UI patches the line in place. */
  onSpeakerUpdate(cb: (segmentId: string, speaker: string) => void): () => void;
  /** Subscribe to daemon-detected "for-me" questions (master doc §6) — the
   *  distinct signal that drives the Ask view's "They asked…" hero card.
   *  Returns an unsubscribe fn. */
  onForMeQuestion(
    cb: (q: { text: string; title?: string }) => void,
  ): () => void;
  /** Subscribe to daemon offers to install a missing agent CLI. Returns an
   *  unsubscribe fn. The daemon pushes this when the attached agent's CLI isn't
   *  on PATH but has a vetted install recipe. */
  onAgentInstall(
    cb: (offer: {
      kind: string;
      displayName: string;
      command: string;
      prerequisite?: string;
    }) => void,
  ): () => void;
  /** Answer an install offer. `approved` runs the vetted recipe on the daemon
   *  (which reports the outcome as a card); Bluey never signs the user in. */
  respondAgentInstall(kind: string, approved: boolean): void;
  /** Ask the daemon to propose a fix for an agent answer (Fix-button slice F3).
   *  The daemon drives the ATTACHED agent in PROPOSE-ONLY mode and replies by
   *  PUSHING an {@link onFixProposal} — nothing is applied at this step.
   *  `cardId` optionally references the source answer card the Fix launched
   *  from. Fire-and-subscribe: the proposal arrives asynchronously (driving a
   *  real agent is slow), so callers listen via {@link onFixProposal}. */
  requestFix(question: string, cardId?: string): void;
  /** Subscribe to daemon-PUSHED fix proposals (push_fix_proposal); returns an
   *  unsubscribe fn. Mirrors {@link onForMeQuestion}: the single owner
   *  subscribes once and stores the proposal in shared state. Nothing is
   *  applied — the UI previews the diagnosis + diff for review. */
  onFixProposal(cb: (proposal: FixProposal) => void): () => void;
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
  /** Start audio capture. Optionally select sources (Audio tab toggles);
   *  omitted = both mic + system (the daemon's defaults). */
  startListening(sources?: { microphone?: boolean; system?: boolean }): void;
  /** Stop audio capture. */
  stopListening(): void;
  /** Ask the daemon to open a macOS privacy settings pane (so the user can
   *  grant Screen Recording / Microphone access). */
  openPermissionSettings(pane: "screen_recording" | "microphone"): void;
  /** Start system-audio capture via the macOS content-sharing picker — the user
   *  chooses which app to capture (e.g. their Zoom call). */
  pickSystemAudio(): void;

  // ---- context (the "+" menu: capture page / attach files / screenshot) ----
  /** Capture the active browser page's text as a context artifact. */
  capturePage(): void;
  /** Open the DAEMON-owned native file picker to attach files as context.
   *  (The overlay is an accessory app and cannot open NSOpenPanel itself.) */
  openAttachPicker(): void;
  /** Capture a screenshot of the screen and route it to context/vision. */
  captureScreenshot(): void;
}

// Wire DTOs — mirror the daemon's agent_ui.rs structs exactly (snake_case Rust
// fields arrive camelCased through Tauri v2, so these match the JS shape).

/** One discovered coding agent, summarized for the discovery/attach list. */
export interface AgentSummary {
  kind: string;
  displayName: string;
  /** "drive" | "read_only" | "needs_trust" | "needs_reauth" | "cloud_blocked" */
  capability: string;
  connectorCount: number;
  readyConnectorCount: number;
  /** null when session-history consent is off. */
  sessionCount: number | null;
  attached: boolean;
}

/** One inherited MCP connector (shape + readiness only — never secrets). */
export interface AgentConnectorInfo {
  name: string;
  /** "env_auth" | "hosted_oauth" | "none" */
  authTier: string;
  ready: boolean;
}

/** Coverage of one meeting-relevant context source for the attached agent —
 *  the onboarding coverage meter's row ("Calendar connected via gcal-mcp";
 *  "Slack missing — connect it in your agent"). */
export interface SourceCoverageInfo {
  /** Stable id: "calendar" | "slack" | "email" | "tickets" | "bluey_memory" */
  source: string;
  label: string;
  connected: boolean;
  /** Matched connector name when connected. */
  via: string | null;
  /** Guided connect command/snippet when missing (authorization happens in
   *  the user's agent — Bluey never holds credentials). */
  connectHint: string | null;
}

/** The connection state of ONE cloud-calendar provider (the "Connect Google /
 *  Microsoft Calendar" buttons). Mirrors the daemon's cue-core
 *  `CalendarConnection` wire DTO. Carries only non-secret metadata — tokens live
 *  in the OS keychain, never here. All fields are single-word so the daemon's
 *  snake_case serde already matches this shape. */
export interface CalendarConnection {
  /** "google" | "microsoft" */
  provider: string;
  /** true when tokens for this provider are stored on-device. */
  connected: boolean;
  /** connected account email for the label (empty when unknown/disconnected). */
  email: string;
}

/** One prior session, summarized for the resume picker. */
export interface AgentSessionSummary {
  id: string;
  title: string | null;
  /** RFC3339 or epoch-seconds string, best-effort per source. */
  updatedAt: string;
  /** full project path if recorded, else null. */
  project: string | null;
}

/** Coarse listening-pipeline state, mirrored from the daemon's ListeningState. */
export type ListeningState =
  | "idle"
  | "connecting"
  | "listening"
  | "paused"
  | "failed"
  // A required macOS permission (Screen Recording for system audio, or
  // Microphone) is not granted — the overlay shows a "grant access" flow.
  | "permission_denied";

/** A normalized transcript line surfaced during a live meeting. */
/** A calendar attendee offered as a tap-to-pick name in the speaker-rename UI. */
export interface SpeakerCandidate {
  name: string;
  email: string;
}

export interface TranscriptLine {
  /** The daemon's stable per-segment id (a Uuid string), carried on the live
   *  push path so a live line can be reconciled by id against the same segment
   *  already present in the rehydration snapshot (Fix B seam). Absent only for
   *  synthetic/partial lines that never reach history. */
  id?: string;
  /** "system" | "mic" — who Bluey heard. */
  source: string;
  /** optional speaker label, when diarization provides one. */
  speaker?: string;
  /** The numeric diarized speaker index behind `speaker`, when known — used to
   *  offer inline rename (click the label → `renameSpeaker(speakerId, name)`). */
  speakerId?: number | null;
  text: string;
  /** true once the line is finalized (not a partial). */
  final: boolean;
}

/** A persisted transcript line from the active meeting's rehydration snapshot
 *  (Fix B). Same shape as a live {@link TranscriptLine} plus the daemon's stable
 *  segment id — the dedup key when reconciling the seeded snapshot against the
 *  live onTranscript stream. */
export interface MeetingTranscriptLine extends TranscriptLine {
  /** TranscriptSegment.id (Uuid) as string — the stable per-segment key. */
  id: string;
}

/** A persisted Q&A turn from the active meeting's rehydration snapshot (Fix B). */
export interface MeetingConversationTurn {
  /** ConversationTurn.id (Uuid) as string. */
  id: string;
  question: string;
  answer: string;
  /** Grounding hint carried through from the daemon, when present. */
  source?: string;
}

/** The active meeting's read-only snapshot, fetched once on mount to rehydrate
 *  the overlay after a collapse-remount or a full process restart (Fix B). Both
 *  arrays are empty when no meeting is active. */
export interface MeetingState {
  transcript: MeetingTranscriptLine[];
  conversation: MeetingConversationTurn[];
}

/** One past meeting summarized for the MEETINGS lens (the "my past meetings"
 *  list). Distinct from the AGENT-SESSION lens ({@link AgentSessionSummary}):
 *  this is a meeting Bluey recorded, not a coding-agent thread. */
/** One attached context artifact shown as a composer chip (the "+" menu:
 *  files, screenshots, pages). Mirrors the daemon's `OverlayContextItem`. */
export interface ContextItem {
  /** ContextArtifact.id (Uuid) as string — the id to remove by. */
  id: string;
  title: string;
  /** stringified ContextKind: "image" | "diagram" | "code" | "document" |
   *  "text" | "other". Drives the chip glyph and thumbnail treatment. */
  kind: string;
  /** absolute file path, when known. */
  path?: string;
  /** a small inline `data:image/...` thumbnail for image/diagram kinds, so the
   *  chip shows a ChatGPT-style preview. Absent for non-image kinds. */
  thumbnail?: string;
}

export interface MeetingSummary {
  /** MeetingRecord.id (Uuid) as string. */
  id: string;
  title: string;
  /** epoch-ms string, as stored. */
  startedAt: string;
  /** epoch-ms string; absent while the meeting is still active. */
  endedAt?: string;
  /** meeting.transcript.len() — a cheap count for the row; the VIEW re-filters
   *  to final segments only. */
  transcriptCount: number;
  /** meeting.conversation.len() — the number of Q&A turns. */
  turnCount: number;
  /** one line: the first non-empty transcript text, trimmed; absent if none. */
  preview?: string;
  /** true for the currently-active meeting (its id == the daemon's active id). */
  isActive: boolean;
  /** the agent session this meeting was chained to, when recorded — drives the
   *  "Resume agent thread →" affordance. Absent when the meeting never chained a
   *  thread; the frontend derives `hasAgentSession = agentSessionId != null`. */
  agentSessionId?: string;
  /** the agent KIND ("claude_code", "cursor", …) that owns `agentSessionId`.
   *  Resume targets THIS agent, not whatever is currently attached. Absent for
   *  links recorded before the kind was stored → fall back to the attached agent. */
  agentKind?: string;
}

/** Result of continuing a past meeting. `blocked` is true when a live
 *  recording prevented the switch (the daemon kept recording and pushed a
 *  guidance card); the caller must NOT switch to Ask in that case. */
export interface ContinueResult {
  ok: boolean;
  blocked: boolean;
}

/** The read-only view of a PAST meeting opened from the MEETINGS lens: the same
 *  transcript + Q&A as a live {@link MeetingState}, plus the meeting id it
 *  belongs to and whether the daemon served it read-only (true whenever opening
 *  it would clobber a live/active meeting). The viewer NEVER subscribes to the
 *  live stream, so this snapshot is the whole of what it renders. */
export interface MeetingViewState extends MeetingState {
  meetingId: string;
  readOnly: boolean;
}

/** The run state of a tool step in the live status feed (mirrors the agent's
 *  real ACP tool-call status — never fabricated). */
export type AnswerStatusState = "pending" | "running" | "done" | "failed";

/** One row in the live status feed shown while the agent works: either its
 *  reasoning, or a tool/connector call with run state. Real ACP events. */
export type AnswerStatusStep =
  | { kind: "reasoning"; text: string }
  | { kind: "tool"; id: string; title: string; state: AnswerStatusState };

/** One streamed chunk of an agent answer. */
export interface AnswerChunk {
  /** plain text delta. */
  text?: string;
  /** an MCP tool fired mid-answer (the grounding proof) — e.g. "perplexity_ask". */
  tool?: string;
  /** a resolved source the answer is grounded in (Jira/GitHub/etc.). */
  source?: AnswerSource;
  /** the full live status feed for this answer (replaces, not appends) — the
   *  agent's real reasoning + tool calls as it works. */
  status?: AnswerStatusStep[];
  /** true once the status feed is complete (the work is done). */
  statusDone?: boolean;
  /** true on the terminal chunk. */
  done?: boolean;
  /** true when the terminal body is an ERROR (provider/agent failure, policy
   *  block), not an answer — the UI renders a retryable error state. */
  error?: boolean;
}

/** Answer speed/depth, mapped 1:1 to the daemon's optional `mode` field on
 *  AskRequested. "fast" trades depth for latency; "deep" the reverse. The wire
 *  carries it as a lowercase string; an unset value lets the daemon pick. */
export type AskMode = "fast" | "balanced" | "deep";

/** Optional answer-shaping fields forwarded with an ask. All optional so the
 *  daemon falls back to its own defaults when the UI doesn't pin them. Mirrors
 *  the daemon's OverlayEvent::AskRequested { provider, model, mode } fields. */
export interface AskOptions {
  /** Coarse speed/depth selector. */
  mode?: AskMode;
  /** Override the answering provider (e.g. an agent kind), when chosen. */
  provider?: string;
  /** Override the specific model, when chosen. */
  model?: string;
}

/** A review-gated fix proposal pushed by the daemon in response to a Fix click
 *  (Fix-button slice F3). Mirrors the daemon's OverlayCommand::PushFixProposal
 *  wire shape — the daemon drove the attached agent in PROPOSE-ONLY mode: this
 *  is a diagnosis + reasoning + the raw fix text + an optional renderable diff.
 *  Nothing has been applied. In beta the UI previews this proposal only; the
 *  Apply action is disabled (no agent may edit a tester's repo). */
export interface FixProposal {
  /** PushFixProposal.proposal_id (Uuid) as string — the id the daemon expects
   *  echoed back on approval. Carried so a future (non-beta) apply path can
   *  reference the still-pending proposal. */
  id: string;
  /** What's wrong: the agent's diagnosis of the problem. */
  diagnosis: string;
  /** Why this fix: the agent's reasoning for the proposed change. */
  reasoning: string;
  /** The full fix text as the agent produced it (the FIX section). */
  fix: string;
  /** A renderable unified diff extracted from the fix, when the fix contained
   *  one. Absent for commands-only / prose-only fixes. */
  diff?: string;
  /** true when the producing agent CAN be driven to apply (has a CLI, etc.).
   *  In beta the Apply button stays disabled regardless — this flag is carried
   *  for the post-beta apply path and for honest labeling. */
  applySupported: boolean;
}

/** A grounding source row shown under an answer. */
export interface AnswerSource {
  /** "jira" | "github" | "supabase" | … — drives the dot color + label. */
  kind: string;
  label: string;
  /** optional status note, e.g. "resolved 3d ago". */
  note?: string;
}

/** One first-run setup prerequisite (see daemon `setup_status`). */
export interface SetupItem {
  /** "ready" | "working" | "missing" | "needs_login" | "failed" */
  state: string;
  detail: string;
  /** 0-100 while state === "working". */
  percent: number | null;
  /** Agent kind when agent-specific, for install/login actions. */
  kind: string | null;
}

/** First-run setup status: the REAL state of every prerequisite. */
export interface SetupStatus {
  model: SetupItem;
  agent: SetupItem;
  /** True only when every required item is ready — onboarding gates on this. */
  allReady: boolean;
}

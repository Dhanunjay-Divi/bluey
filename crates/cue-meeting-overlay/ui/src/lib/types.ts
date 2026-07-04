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

/** A grounding source row shown under an answer. */
export interface AnswerSource {
  /** "jira" | "github" | "supabase" | … — drives the dot color + label. */
  kind: string;
  label: string;
  /** optional status note, e.g. "resolved 3d ago". */
  note?: string;
}

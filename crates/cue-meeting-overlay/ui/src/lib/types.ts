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

/** A normalized transcript line surfaced during a live meeting. */
export interface TranscriptLine {
  /** "system" | "mic" — who Bluey heard. */
  source: string;
  /** optional speaker label, when diarization provides one. */
  speaker?: string;
  text: string;
  /** true once the line is finalized (not a partial). */
  final: boolean;
}

/** One streamed chunk of an agent answer. */
export interface AnswerChunk {
  /** plain text delta. */
  text?: string;
  /** an MCP tool fired mid-answer (the grounding proof) — e.g. "perplexity_ask". */
  tool?: string;
  /** a resolved source the answer is grounded in (Jira/GitHub/etc.). */
  source?: AnswerSource;
  /** true on the terminal chunk. */
  done?: boolean;
}

/** A grounding source row shown under an answer. */
export interface AnswerSource {
  /** "jira" | "github" | "supabase" | … — drives the dot color + label. */
  kind: string;
  label: string;
  /** optional status note, e.g. "resolved 3d ago". */
  note?: string;
}

// Wire DTOs for the agent-bridge UI surface.
//
// These mirror the Rust DTOs in `crates/cue-core/src/agent_ui.rs`
// (`AgentSummary`, `AgentConnectorInfo`, `AgentSessionSummary`) one-to-one and
// are the contract the dashboard's agent Tauri commands return:
//   - `agent_list` / `agent_attach` / `agent_detach` -> `AgentSummary[]`
//   - `agent_sessions` -> `AgentSessionSummary[]`
//   - `agent_connectors` -> `AgentConnectorInfo[]`
//
// Field names match serde's output exactly (the structs carry no rename, so
// fields stay snake_case). serde maps Rust `None` to JSON `null`, hence the
// `T | null` optionals below (not `T | undefined`).
//
// Security: shape only. Connectors carry a name, auth tier, and readiness flag
// — never an env value, token, or credentialed URL. Session summaries carry an
// id, optional title, timestamp, and optional project path — never message
// bodies.

/** One discovered coding agent, summarized for the discovery list. */
export interface AgentSummary {
  /** snake_case agent kind, e.g. "claude_code". Stable id sent back on attach. */
  kind: string;
  /** Friendly, human-facing name, e.g. "Claude Code". */
  display_name: string;
  /**
   * snake_case capability: "drive" / "read_only" / "needs_trust" /
   * "needs_reauth" / "cloud_blocked".
   */
  capability: string;
  /** Total inherited MCP connectors (0 when no config was located). */
  connector_count: number;
  /** How many of those connectors are usable without a re-login. */
  ready_connector_count: number;
  /**
   * Best-effort recent-session count, or null when not cheaply countable or
   * when session-history consent is off.
   */
  session_count: number | null;
  /** True when this is the currently attached agent. */
  attached: boolean;
}

/** One inherited MCP connector, shape + readiness only (never secrets). */
export interface AgentConnectorInfo {
  name: string;
  /** snake_case auth tier: "env_auth" / "hosted_oauth" / "none". */
  auth_tier: string;
  /**
   * True when usable as-is (env-auth or no auth); false for hosted-OAuth
   * connectors that need a re-login first.
   */
  ready: boolean;
}

/** One prior agent session, summarized for a picker (no body decode). */
export interface AgentSessionSummary {
  id: string;
  title: string | null;
  /** Best-effort last-updated marker (RFC3339 or epoch string, per source). */
  updated_at: string;
  /**
   * The project/workspace path the session belongs to when the source records
   * it; null otherwise.
   */
  project: string | null;
}

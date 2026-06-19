// Typed API surface for the agent-bridge Tauri commands.
//
// Thin wrappers around the daemon's agent commands — one exported async
// function per backend command, each returning the proper typed promise.
// Centralizing the `invoke` calls here keeps the Tauri arg-naming rules in a
// single place: Tauri v2 camelCases snake_case Rust params, so the Rust
// `session_id` becomes `sessionId` on the JS side, while single-word params
// (`kind`, `enabled`) pass through unchanged.
//
// The backend commands are:
//   - `agent_list`                 -> AgentSummary[]
//   - `agent_attach`               -> AgentSummary[]
//   - `agent_detach`               -> AgentSummary[]
//   - `agent_sessions`             -> AgentSessionSummary[]
//   - `agent_connectors`           -> AgentConnectorInfo[]
//   - `set_agent_session_history`  -> void

import { invoke } from "./tauri";
import type {
  AgentConnectorInfo,
  AgentSessionSummary,
  AgentSummary,
} from "./agentTypes";

// Re-export the wire DTOs so callers can import types and API from one module.
export type { AgentSummary, AgentConnectorInfo, AgentSessionSummary };

/** List every discovered coding agent, summarized for the discovery list. */
export function listAgents(): Promise<AgentSummary[]> {
  return invoke<AgentSummary[]>("agent_list");
}

/**
 * Attach the given agent kind (optionally resuming a prior session) and return
 * the refreshed agent list with the new `attached` flags applied.
 */
export function attachAgent(
  kind: string,
  sessionId?: string,
): Promise<AgentSummary[]> {
  return invoke<AgentSummary[]>("agent_attach", { kind, sessionId });
}

/** Detach the currently attached agent and return the refreshed agent list. */
export function detachAgent(): Promise<AgentSummary[]> {
  return invoke<AgentSummary[]>("agent_detach");
}

/** List prior sessions for an agent kind, summarized for a resume picker. */
export function listAgentSessions(
  kind: string,
): Promise<AgentSessionSummary[]> {
  return invoke<AgentSessionSummary[]>("agent_sessions", { kind });
}

/** List an agent's inherited MCP connectors (shape + readiness only). */
export function listAgentConnectors(
  kind: string,
): Promise<AgentConnectorInfo[]> {
  return invoke<AgentConnectorInfo[]>("agent_connectors", { kind });
}

/** Set whether the daemon may read prior agent session history for counts/pickers. */
export function setSessionHistoryConsent(enabled: boolean): Promise<void> {
  return invoke<void>("set_agent_session_history", { enabled });
}

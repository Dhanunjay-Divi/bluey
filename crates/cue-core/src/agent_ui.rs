//! Wire DTOs for the agent-bridge UI surface (Slice 5a).
//!
//! These are the serializable shapes a UI (the Swift overlay, later) renders
//! when discovering coding agents, inspecting their MCP connectors, and listing
//! their prior sessions. They are intentionally **decoupled from the bridge's
//! internal types** (`DiscoveredAgent`, `Connector`, `SessionRef`): the daemon
//! maps bridge types onto these DTOs so the wire format can evolve independently
//! of discovery internals, and so this crate carries no dependency on
//! `cue-agent-bridge`.
//!
//! Security: these DTOs carry **shape only** — a connector's name, auth tier,
//! and a readiness flag, never an env value, token, or URL with embedded
//! credentials. Session summaries carry an id, optional title, and a timestamp
//! string, never message bodies.

use serde::{Deserialize, Serialize};

/// One discovered agent, summarized for the discovery list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentSummary {
    /// `snake_case` [`AgentKind`](../../cue_agent_bridge/enum.AgentKind.html)
    /// label, e.g. `"claude_code"`. The stable id the UI sends back on attach.
    pub kind: String,
    /// Friendly, human-facing name, e.g. `"Claude Code"`.
    pub display_name: String,
    /// `snake_case` capability: `drive` / `read_only` / `needs_trust` /
    /// `needs_reauth` / `cloud_blocked`.
    pub capability: String,
    /// Total inherited MCP connectors (0 when no config was located).
    pub connector_count: usize,
    /// How many of those connectors are usable without a re-login.
    pub ready_connector_count: usize,
    /// Best-effort recent-session count, or `None` when not cheaply countable
    /// or when session-history consent is off.
    pub session_count: Option<usize>,
    /// True when this is the currently attached agent.
    pub attached: bool,
}

/// One inherited MCP connector, shape + readiness only (never secrets).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentConnectorInfo {
    pub name: String,
    /// `snake_case` auth tier: `env_auth` / `hosted_oauth` / `none`.
    pub auth_tier: String,
    /// True when the connector is usable as-is (env-auth or no auth); false for
    /// hosted-OAuth connectors that need a re-login first.
    pub ready: bool,
}

/// Coverage of one meeting-relevant context SOURCE for the attached agent —
/// the data behind the onboarding coverage meter ("your agent reaches
/// calendar+tickets; Slack is missing — connect it"). Unlike
/// [`AgentConnectorInfo`] (raw inherited connectors), this classifies against
/// the sources a MEETING needs and carries the guided connect recipe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceCoverageInfo {
    /// Stable source id: `calendar` / `slack` / `email` / `tickets` /
    /// `bluey_memory`.
    pub source: String,
    /// Human label ("Calendar", "Slack", …).
    pub label: String,
    /// True when the attached agent can already reach this source.
    pub connected: bool,
    /// The matched connector's name when connected (e.g. "gcal-mcp").
    #[serde(default)]
    pub via: Option<String>,
    /// Guided connect instruction for THIS agent when not connected (the
    /// exact command or config snippet the user runs — authorization happens
    /// in their agent; Bluey never holds credentials). `None` = no vetted
    /// recipe yet for this agent+source.
    #[serde(default)]
    pub connect_hint: Option<String>,
}

/// One prior agent session, summarized for a picker (no body decode).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentSessionSummary {
    pub id: String,
    pub title: Option<String>,
    /// Best-effort last-updated marker (RFC3339 or epoch string, per source).
    pub updated_at: String,
    /// The project/workspace path the session belongs to, when the source
    /// records it (e.g. Cursor's per-workspace store, Claude's cwd, Antigravity's
    /// index). `None` when the source has no project association. Defaults to
    /// `None` on older serialized payloads.
    #[serde(default)]
    pub project: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_summary_roundtrips_with_snake_case_fields() {
        let summary = AgentSummary {
            kind: "claude_code".to_string(),
            display_name: "Claude Code".to_string(),
            capability: "drive".to_string(),
            connector_count: 3,
            ready_connector_count: 2,
            session_count: Some(12),
            attached: true,
        };
        let json = serde_json::to_string(&summary).expect("serialize summary");
        assert!(json.contains(r#""kind":"claude_code""#));
        assert!(json.contains(r#""display_name":"Claude Code""#));
        assert!(json.contains(r#""ready_connector_count":2"#));
        let parsed: AgentSummary = serde_json::from_str(&json).expect("deserialize summary");
        assert_eq!(parsed, summary);
    }

    #[test]
    fn agent_summary_session_count_none_serializes_as_null() {
        let summary = AgentSummary {
            kind: "cursor".to_string(),
            display_name: "Cursor".to_string(),
            capability: "read_only".to_string(),
            connector_count: 0,
            ready_connector_count: 0,
            session_count: None,
            attached: false,
        };
        let json = serde_json::to_string(&summary).expect("serialize");
        assert!(json.contains(r#""session_count":null"#));
    }

    #[test]
    fn agent_connector_info_roundtrips() {
        let info = AgentConnectorInfo {
            name: "filesystem".to_string(),
            auth_tier: "env_auth".to_string(),
            ready: true,
        };
        let json = serde_json::to_string(&info).expect("serialize connector");
        assert_eq!(
            json,
            r#"{"name":"filesystem","auth_tier":"env_auth","ready":true}"#
        );
        let parsed: AgentConnectorInfo = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed, info);
    }

    #[test]
    fn agent_session_summary_roundtrips_with_optional_title() {
        let with_title = AgentSessionSummary {
            id: "abc123".to_string(),
            title: Some("System design prep".to_string()),
            updated_at: "1717000000".to_string(),
            project: Some("/Users/me/Developer/Bluey".to_string()),
        };
        let json = serde_json::to_string(&with_title).expect("serialize");
        let parsed: AgentSessionSummary = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed, with_title);

        let without_title = AgentSessionSummary {
            id: "def456".to_string(),
            title: None,
            updated_at: "0".to_string(),
            project: None,
        };
        let json = serde_json::to_string(&without_title).expect("serialize");
        assert!(json.contains(r#""title":null"#));

        // Older payloads without a `project` field still deserialize (serde
        // default), so the wire format stays backward-compatible.
        let legacy: AgentSessionSummary =
            serde_json::from_str(r#"{"id":"x","title":null,"updated_at":"0"}"#)
                .expect("legacy payload deserializes");
        assert_eq!(legacy.project, None);
    }
}

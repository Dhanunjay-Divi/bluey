//! Session-list assembly: read an agent's stored sessions, de-duplicate the
//! Claude CLI ↔ app overlap, and fill a registry-driven fallback title for
//! sessions whose store exposes no readable name.
//!
//! This is the spine's **session-read** surface (it used to live in the daemon
//! as `list_agent_sessions` / `app_claimed_claude_session_ids`). It returns the
//! neutral [`SessionRef`] — the daemon maps that to its own UI DTO, keeping the
//! `cue-agent-bridge → cue-core` dependency boundary intact (the bridge knows
//! nothing about the UI layer).
//!
//! All reads go through [`reader_for`], so the security invariants documented on
//! [`crate::sessions`] (read-only, bounded, fail-soft, no secrets) apply here
//! unchanged. Every fallible read is fail-soft: a store that can't be read
//! contributes nothing rather than erroring the whole list.

use std::collections::HashSet;

use crate::sessions::reader_for;
use crate::{AgentKind, DiscoveredAgent, SessionRef};

/// List `agent`'s most-recent sessions (up to `cap`), enriched and de-duplicated.
///
/// `all_agents` is the full discovery set (from [`crate::discover_agents`]); it
/// is only consulted for the Claude CLI↔app de-dup below — every other agent
/// ignores it. Returns enriched [`SessionRef`]s:
///
/// - **Fallback title.** A session whose store has no readable title gets a
///   generic `"{display} session {short-id}"` label, where `{display}` is the
///   agent's registry display name (never hardcoded per agent) and `{short-id}`
///   is the id up to its first `-`.
/// - **Claude de-dup.** The Claude CLI store and the Claude-app stores point at
///   the SAME transcript files (the app indexes them by `cliSessionId`, which is
///   the CLI file stem). When listing the CLI row we drop any session already
///   surfaced by an app row, so one conversation never appears twice — the app
///   row keeps it (its pre-computed title is richer). A no-op for every other
///   agent.
///
/// Fail-soft: an unreadable store yields an empty list (logged by the caller via
/// the returned emptiness, not here — this stays dependency-light).
#[must_use]
pub fn list_for_agent(
    agent: &DiscoveredAgent,
    all_agents: &[DiscoveredAgent],
    cap: usize,
) -> Vec<SessionRef> {
    let Some(store) = agent.session_store.as_ref() else {
        return Vec::new();
    };

    // Generic, registry-driven fallback label, computed ONCE from the agent's
    // display name — never hardcoded per agent/reader.
    let display =
        crate::registry::display_name_for(&agent.kind).unwrap_or_else(|| "Session".to_string());

    let claimed_by_app = if matches!(agent.kind, AgentKind::ClaudeCode) {
        claude_app_claimed_ids(all_agents, cap)
    } else {
        HashSet::new()
    };

    let Ok(refs) = reader_for(store.format).list(store, cap) else {
        return Vec::new();
    };

    refs.into_iter()
        .filter(|r| !claimed_by_app.contains(&r.id))
        .map(|mut r| {
            if r.title.is_none() {
                let short: &str = r.id.split('-').next().unwrap_or(&r.id);
                r.title = Some(format!("{display} session {short}"));
            }
            r
        })
        .collect()
}

/// The set of Claude session ids (`cliSessionId`s) claimed by the Claude-app
/// stores (Code mode + agent mode), used to de-duplicate the CLI row's listing
/// against the richer app rows. Fail-soft: a store that can't be read simply
/// contributes nothing (so at worst a session shows under both rows, never fewer
/// than it should). Each id is a JSONL file stem shared across stores.
fn claude_app_claimed_ids(all_agents: &[DiscoveredAgent], cap: usize) -> HashSet<String> {
    let mut claimed = HashSet::new();
    for agent in all_agents.iter().filter(|a| {
        matches!(
            a.kind,
            AgentKind::ClaudeCodeApp | AgentKind::ClaudeCodeAgent
        )
    }) {
        let Some(store) = agent.session_store.as_ref() else {
            continue;
        };
        if let Ok(refs) = reader_for(store.format).list(store, cap) {
            claimed.extend(refs.into_iter().map(|r| r.id));
        }
    }
    claimed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Capability, SessionFormat, SessionStore};
    use std::io::Write;
    use std::path::PathBuf;

    /// A Claude-kind agent whose JSONL store lives in `dir`.
    fn agent(kind: AgentKind, dir: PathBuf) -> DiscoveredAgent {
        DiscoveredAgent {
            kind,
            install_evidence: vec![],
            capability: Capability::Drive,
            connector_config_path: None,
            session_store: Some(SessionStore {
                path: dir,
                format: SessionFormat::Jsonl,
            }),
        }
    }

    /// Write a minimal one-turn JSONL session file named `<id>.jsonl`.
    fn write_session(dir: &std::path::Path, id: &str, text: &str) {
        let mut f = std::fs::File::create(dir.join(format!("{id}.jsonl"))).expect("create");
        writeln!(
            f,
            r#"{{"type":"user","message":{{"role":"user","content":"{text}"}}}}"#
        )
        .unwrap();
    }

    #[test]
    fn claude_cli_listing_drops_sessions_already_claimed_by_an_app_row() {
        // CLI store has two sessions; the app store re-indexes one of them by the
        // SAME id (the cliSessionId). Listing the CLI row must drop the shared id.
        let cli_dir = tempfile::tempdir().expect("tempdir");
        let app_dir = tempfile::tempdir().expect("tempdir");
        write_session(cli_dir.path(), "shared-1111", "shared conversation");
        write_session(cli_dir.path(), "cli-only-2222", "cli only");
        write_session(app_dir.path(), "shared-1111", "shared conversation");

        let cli = agent(AgentKind::ClaudeCode, cli_dir.path().to_path_buf());
        let app = agent(AgentKind::ClaudeCodeApp, app_dir.path().to_path_buf());
        let all = vec![cli.clone(), app];

        let listed = list_for_agent(&cli, &all, 40);
        let ids: Vec<&str> = listed.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, vec!["cli-only-2222"], "shared id de-duplicated away");
    }

    #[test]
    fn non_claude_agent_never_dedups_against_app_rows() {
        // A non-Claude agent ignores the app stores entirely: same id present in
        // an app store must NOT be filtered out of this agent's listing.
        let dir = tempfile::tempdir().expect("tempdir");
        let app_dir = tempfile::tempdir().expect("tempdir");
        write_session(dir.path(), "shared-1111", "hi");
        write_session(app_dir.path(), "shared-1111", "hi");

        let codex = agent(AgentKind::Codex, dir.path().to_path_buf());
        let app = agent(AgentKind::ClaudeCodeApp, app_dir.path().to_path_buf());
        let all = vec![codex.clone(), app];

        let listed = list_for_agent(&codex, &all, 40);
        assert_eq!(listed.len(), 1, "codex listing untouched by app dedup");
        assert_eq!(listed[0].id, "shared-1111");
    }

    #[test]
    fn agent_without_a_store_lists_nothing() {
        let mut no_store = agent(AgentKind::Gemini, PathBuf::new());
        no_store.session_store = None;
        assert!(list_for_agent(&no_store, &[], 40).is_empty());
    }
}

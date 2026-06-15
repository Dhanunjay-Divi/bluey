//! Compute a coarse [`Capability`] for a discovered agent.
//!
//! Slice 1 distinguishes only the two states it has evidence for:
//! - a drivable CLI binary on `PATH` → [`Capability::Drive`];
//! - a readable session store / connector config but no CLI → [`Capability::ReadOnly`].
//!
//! `NeedsTrust`, `NeedsReauth`, and `CloudBlocked` are deferred:
//! - `NeedsTrust` needs an interactive trust probe (Cursor headless) — not run here.
//! - `NeedsReauth` is fundamentally **per-connector** and is derived from
//!   [`crate::AuthTier::HostedOauth`] in `connectors.rs`, not at agent grain.
//! - `CloudBlocked` needs a cloud-only marker we do not yet collect.

use crate::{Capability, DiscoveredAgent};

/// Classify a discovered agent. Pure function over already-gathered evidence;
/// performs no IO so it stays cheap and deterministic.
pub fn compute_capability(agent: &DiscoveredAgent) -> Capability {
    if agent.has_cli_evidence() {
        // A binary on PATH (or a bin-dir path) means we can drive it later.
        Capability::Drive
    } else if agent.session_store.is_some() || agent.connector_config_path.is_some() {
        // Footprint only: we can read history/connectors, not drive.
        Capability::ReadOnly
    } else {
        // Evidence exists (we would not have a DiscoveredAgent otherwise) but
        // it is neither a CLI nor a readable store — treat as read-only rather
        // than claim drivability. TODO(slice-2+): NeedsTrust probe.
        Capability::ReadOnly
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AgentKind, SessionFormat, SessionStore};
    use std::path::PathBuf;

    fn agent_with(evidence: Vec<PathBuf>, store: bool, config: bool) -> DiscoveredAgent {
        DiscoveredAgent {
            kind: AgentKind::Unknown,
            install_evidence: evidence,
            capability: Capability::ReadOnly,
            connector_config_path: config.then(|| PathBuf::from("/tmp/mcp.json")),
            session_store: store.then(|| SessionStore {
                path: PathBuf::from("/tmp/store"),
                format: SessionFormat::Jsonl,
            }),
        }
    }

    #[test]
    fn test_cli_binary_is_drivable() {
        let a = agent_with(vec![PathBuf::from("/usr/local/bin/claude")], false, false);
        assert_eq!(compute_capability(&a), Capability::Drive);
    }

    #[test]
    fn test_bare_binary_name_is_drivable() {
        let a = agent_with(vec![PathBuf::from("claude")], false, false);
        assert_eq!(compute_capability(&a), Capability::Drive);
    }

    #[test]
    fn test_data_dir_only_is_read_only() {
        let a = agent_with(
            vec![PathBuf::from("/Users/x/Library/Application Support/Cursor")],
            true,
            false,
        );
        assert_eq!(compute_capability(&a), Capability::ReadOnly);
    }

    #[test]
    fn test_config_only_is_read_only() {
        let a = agent_with(vec![PathBuf::from("/Users/x/.cursor")], false, true);
        assert_eq!(compute_capability(&a), Capability::ReadOnly);
    }
}

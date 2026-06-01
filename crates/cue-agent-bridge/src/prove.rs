//! Capability matrix — prove, per agent, what actually works **on this machine
//! right now**, at the highest level of proof available, and report it honestly.
//!
//! "Test against all applications" cannot mean "pretend they all work." An agent
//! that is not installed here cannot be live-driven; saying otherwise would be a
//! lie. So each capability is reported at one of three honest levels:
//!
//! - **Live** — actually exercised against the real installed agent on this
//!   machine (real discovery / real read / real binary present).
//! - **Fixture** — verified against a real captured sample of that agent's
//!   on-disk format (proves parsing without the app installed). *(Reserved for
//!   when fixtures are wired; not yet populated.)*
//! - **Skipped** — not testable here, with the concrete reason (e.g. "CLI not
//!   installed", "no session store on disk").
//!
//! This module does **read-only / non-destructive** probing only — it never
//! installs anything and never drives an agent (driving costs money / hits the
//! user's quota). It reports what *would* work and what is *proven present*.
//! The live drive proof is a separate, explicit, consent-gated action.

use crate::{discover_agents, read_connectors, reader_for, registry, AgentKind, DiscoveredAgent};

/// The proof level achieved for one capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProofLevel {
    /// Exercised live against the real installed agent.
    Live(String),
    /// Verified against a real captured fixture of the agent's format.
    Fixture(String),
    /// Not testable on this machine, with the reason.
    Skipped(String),
}

impl ProofLevel {
    pub fn marker(&self) -> &'static str {
        match self {
            ProofLevel::Live(_) => "LIVE ✅",
            ProofLevel::Fixture(_) => "FIXTURE 🟡",
            ProofLevel::Skipped(_) => "skip ⬜",
        }
    }
    pub fn detail(&self) -> &str {
        match self {
            ProofLevel::Live(s) | ProofLevel::Fixture(s) | ProofLevel::Skipped(s) => s,
        }
    }
}

/// The per-agent capability report.
#[derive(Debug, Clone)]
pub struct AgentProof {
    pub display_name: &'static str,
    pub kind: AgentKind,
    /// Is the agent present at all (CLI binary or GUI/data footprint)?
    pub discovered: ProofLevel,
    /// Can we install its CLI if missing (is there a recipe + are prereqs met)?
    pub installable: ProofLevel,
    /// Can we read its MCP connectors?
    pub connectors: ProofLevel,
    /// Can we read its session history?
    pub sessions: ProofLevel,
    /// Can we drive it (CLI present on PATH)?
    pub drivable: ProofLevel,
}

/// Build the full capability matrix: every registry agent, each capability at
/// its highest honest proof level, using only read-only probing of this
/// machine. Never installs, never drives.
pub fn prove_all() -> Vec<AgentProof> {
    let discovered = discover_agents();
    registry::REGISTRY
        .iter()
        .map(|entry| prove_one(entry, &discovered))
        .collect()
}

fn prove_one(entry: &registry::AgentEntry, discovered: &[DiscoveredAgent]) -> AgentProof {
    let kind = entry.kind_tag.to_agent_kind();
    let found = discovered.iter().find(|d| d.kind == kind);

    // Discovered?
    let discovered_lvl = match found {
        Some(d) => ProofLevel::Live(format!("found via {:?}", d.install_evidence)),
        None => ProofLevel::Skipped("not found on this machine".to_string()),
    };

    // Installable? (recipe present + prereq available → could install live)
    let installable = match crate::provision::plan_install(&kind) {
        Some(plan) => {
            let pre = crate::provision::preflight(&plan);
            if pre.already_installed {
                ProofLevel::Live("CLI already installed".to_string())
            } else if let Some(missing) = pre.missing_prerequisite {
                ProofLevel::Skipped(format!("recipe exists but prereq '{missing}' missing"))
            } else if let Some(obs) = &pre.obstruction {
                ProofLevel::Live(format!(
                    "installable after remedy: {}",
                    obs.remedy().description
                ))
            } else {
                ProofLevel::Live(format!("ready to install: {}", plan.human_command))
            }
        }
        None => ProofLevel::Skipped("no install recipe (no official CLI installer)".to_string()),
    };

    // Connectors — read for real if a config path was found.
    let connectors = match found.and_then(|d| d.connector_config_path.as_ref()) {
        Some(path) => {
            let conns = read_connectors(path);
            ProofLevel::Live(format!("{} connector(s) read", conns.len()))
        }
        None => ProofLevel::Skipped("no connector config located".to_string()),
    };

    // Sessions — actually list (bounded) if a store was found.
    let sessions = match found.and_then(|d| d.session_store.as_ref()) {
        Some(store) => {
            let reader = reader_for(store.format);
            match reader.list(store, 100_000) {
                Ok(refs) => {
                    let titled = refs.iter().filter(|s| s.title.is_some()).count();
                    ProofLevel::Live(format!(
                        "{} session(s) listed, {titled} titled [{:?}]",
                        refs.len(),
                        store.format
                    ))
                }
                Err(e) => ProofLevel::Skipped(format!("store present but list failed: {e}")),
            }
        }
        None => ProofLevel::Skipped("no session store on disk".to_string()),
    };

    // Drivable — is a CLI binary actually on PATH? (real check, no spawn)
    let drivable = if found.map(|d| d.has_cli_evidence()).unwrap_or(false) {
        ProofLevel::Live("CLI binary on PATH (drive verified separately)".to_string())
    } else {
        ProofLevel::Skipped("no CLI on PATH (install it to drive)".to_string())
    };

    AgentProof {
        display_name: entry.display_name,
        kind,
        discovered: discovered_lvl,
        installable,
        connectors,
        sessions,
        drivable,
    }
}

/// Render the matrix as a human-readable report string.
pub fn render_report(proofs: &[AgentProof]) -> String {
    let mut out = String::new();
    out.push_str("Agent capability matrix (this machine, read-only probe)\n");
    out.push_str(&"=".repeat(60));
    out.push('\n');
    for p in proofs {
        out.push_str(&format!("\n{} [{:?}]\n", p.display_name, p.kind));
        for (label, lvl) in [
            ("discovered ", &p.discovered),
            ("installable", &p.installable),
            ("connectors ", &p.connectors),
            ("sessions   ", &p.sessions),
            ("drivable   ", &p.drivable),
        ] {
            out.push_str(&format!(
                "  {label} {:<12} {}\n",
                lvl.marker(),
                lvl.detail()
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prove_all_covers_every_registry_agent() {
        let proofs = prove_all();
        // One report per registry row.
        assert_eq!(proofs.len(), registry::REGISTRY.len());
        // Every agent has a non-empty display name and a report for each cap.
        for p in &proofs {
            assert!(!p.display_name.is_empty());
            // Each capability is one of the three honest levels (always set).
            for lvl in [
                &p.discovered,
                &p.installable,
                &p.connectors,
                &p.sessions,
                &p.drivable,
            ] {
                assert!(
                    !lvl.detail().is_empty(),
                    "{}: a capability had no detail",
                    p.display_name
                );
            }
        }
    }

    #[test]
    fn report_renders_all_agents() {
        let proofs = prove_all();
        let report = render_report(&proofs);
        for p in &proofs {
            assert!(
                report.contains(p.display_name),
                "report missing {}",
                p.display_name
            );
        }
    }

    #[test]
    fn levels_have_distinct_markers() {
        assert_eq!(ProofLevel::Live("x".into()).marker(), "LIVE ✅");
        assert_eq!(ProofLevel::Fixture("x".into()).marker(), "FIXTURE 🟡");
        assert_eq!(ProofLevel::Skipped("x".into()).marker(), "skip ⬜");
    }
}

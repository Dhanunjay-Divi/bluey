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

/// GUI app and CLI are **distinct surfaces** — an agent can have one, both, or
/// neither, at different versions. This separates them honestly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceStatus {
    /// The GUI app's version (from its bundle), if the app is installed.
    pub gui_version: Option<String>,
    /// The CLI binary's version (from `--version`), if the CLI is on PATH AND
    /// actually runs. `None` if no CLI, or the CLI is present but won't run.
    pub cli_version: Option<String>,
    /// True if a CLI binary is on PATH but `--version` failed — a present but
    /// **broken** CLI (e.g. a dangling symlink, a bad install). Distinct from
    /// "no CLI at all".
    pub cli_present_but_broken: bool,
    /// The binary name we actually drive through (may differ from the agent —
    /// e.g. Antigravity drives via `gemini`). `None` if not drivable.
    pub drives_via: Option<String>,
}

/// The per-agent capability report.
#[derive(Debug, Clone)]
pub struct AgentProof {
    pub display_name: &'static str,
    pub kind: AgentKind,
    /// GUI-vs-CLI surfaces + their versions (the thing a bare presence check
    /// misses). See [`SurfaceStatus`].
    pub surfaces: SurfaceStatus,
    /// Is the agent present at all (CLI binary or GUI/data footprint)?
    pub discovered: ProofLevel,
    /// Can we install its CLI if missing (is there a recipe + are prereqs met)?
    pub installable: ProofLevel,
    /// Can we read its MCP connectors?
    pub connectors: ProofLevel,
    /// Can we read its session history?
    pub sessions: ProofLevel,
    /// Can we drive it — CLI present on PATH AND it actually runs?
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

/// Probe a CLI binary's version by actually running `<binary> --version`.
/// Returns `Some(version)` if it runs and we can extract a version-ish token;
/// `Some(raw)` trimmed if it runs but the format is unfamiliar; `None` if the
/// binary is absent or fails to run. Bounded, read-only, never panics.
///
/// Real-world formats vary: `2.0.42 (Claude Code)`, bare `0.44.1`,
/// `codex-cli 0.135.0` — so extraction is tolerant (first dotted-number token).
fn probe_cli_version(binary: &str) -> Option<String> {
    let output = std::process::Command::new(binary)
        .arg("--version")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.lines().next().unwrap_or("").trim();
    if line.is_empty() {
        return None;
    }
    // Extract the first token that looks like a version (digits + dots).
    let version = line
        .split_whitespace()
        .find(|tok| {
            let core = tok.trim_start_matches('v');
            !core.is_empty()
                && core.chars().next().is_some_and(|c| c.is_ascii_digit())
                && core.contains('.')
        })
        .map(|t| t.trim_start_matches('v').to_string())
        .unwrap_or_else(|| line.to_string());
    Some(version)
}

/// Whether a binary name resolves on `PATH` (via `which`). Read-only.
fn binary_on_path(binary: &str) -> bool {
    std::process::Command::new("which")
        .arg(binary)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Read a macOS `.app` bundle's version from its `Info.plist`
/// (`CFBundleShortVersionString`), if the bundle exists. Read-only; macOS-only
/// path scheme (returns `None` elsewhere).
fn gui_app_version(app_bundles: &[&str]) -> Option<String> {
    for bundle in app_bundles {
        let plist = format!("/Applications/{bundle}/Contents/Info.plist");
        if !std::path::Path::new(&plist).exists() {
            continue;
        }
        let info = format!("/Applications/{bundle}/Contents/Info");
        if let Ok(out) = std::process::Command::new("defaults")
            .args(["read", &info, "CFBundleShortVersionString"])
            .output()
        {
            if out.status.success() {
                let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !v.is_empty() {
                    return Some(v);
                }
            }
        }
    }
    None
}

/// Assess GUI-vs-CLI surfaces for an agent: the GUI app version (from its
/// bundle), the CLI version (by actually running it), whether a present CLI is
/// broken, and which binary we drive through.
fn assess_surfaces(entry: &registry::AgentEntry) -> SurfaceStatus {
    let gui_version = gui_app_version(entry.app_bundles);

    // The agent's own CLI candidates (first that resolves is the drive binary).
    let mut cli_version = None;
    let mut cli_present_but_broken = false;
    let mut drives_via = None;
    for binary in entry.binary_candidates {
        if binary_on_path(binary) {
            match probe_cli_version(binary) {
                Some(v) => {
                    cli_version = Some(v);
                    drives_via = Some((*binary).to_string());
                    break;
                }
                None => {
                    // On PATH but won't run → broken (e.g. dangling symlink).
                    cli_present_but_broken = true;
                }
            }
        }
    }

    SurfaceStatus {
        gui_version,
        cli_version,
        cli_present_but_broken,
        drives_via,
    }
}

fn prove_one(entry: &registry::AgentEntry, discovered: &[DiscoveredAgent]) -> AgentProof {
    let kind = entry.kind_tag.to_agent_kind();
    let found = discovered.iter().find(|d| d.kind == kind);
    let surfaces = assess_surfaces(entry);

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

    // Drivable — a CLI must be on PATH AND actually run (`--version` succeeded).
    // This catches a present-but-broken CLI (dangling symlink / bad install)
    // that a bare PATH check would wrongly call drivable.
    let drivable = match (&surfaces.cli_version, &surfaces.drives_via) {
        (Some(v), Some(bin)) => {
            ProofLevel::Live(format!("{bin} v{v} runs (drive verified separately)"))
        }
        _ if surfaces.cli_present_but_broken => {
            ProofLevel::Skipped("CLI on PATH but won't run — broken (reinstall to fix)".to_string())
        }
        _ => ProofLevel::Skipped("no working CLI on PATH (install it to drive)".to_string()),
    };

    AgentProof {
        display_name: entry.display_name,
        kind,
        surfaces,
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
        // GUI-vs-CLI surfaces + versions (separate, honest).
        let gui = p
            .surfaces
            .gui_version
            .as_deref()
            .map(|v| format!("GUI v{v}"))
            .unwrap_or_else(|| "GUI —".to_string());
        let cli = match (&p.surfaces.cli_version, p.surfaces.cli_present_but_broken) {
            (Some(v), _) => format!("CLI v{v}"),
            (None, true) => "CLI present-but-broken".to_string(),
            (None, false) => "CLI —".to_string(),
        };
        let via = p
            .surfaces
            .drives_via
            .as_deref()
            .map(|b| format!(" (drives via {b})"))
            .unwrap_or_default();
        out.push_str(&format!("  surfaces    {gui} | {cli}{via}\n"));
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

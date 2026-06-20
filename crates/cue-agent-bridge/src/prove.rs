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

use crate::{discover_agents, read_connectors, registry, AgentKind, DiscoveredAgent};

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
    /// actually runs — either on the active runtime, or under a satisfying
    /// runtime Bluey auto-selected (see [`runtime_resolution`](Self::runtime_resolution)).
    /// `None` only when there is no CLI, or it is genuinely broken.
    pub cli_version: Option<String>,
    /// True if a CLI binary is on PATH but `--version` failed **and no satisfying
    /// runtime could make it run** — a genuinely **broken** CLI (e.g. a dangling
    /// symlink, a bad install). A mere runtime-version mismatch that Bluey can
    /// resolve is *not* broken (see [`runtime_resolution`](Self::runtime_resolution)).
    pub cli_present_but_broken: bool,
    /// The binary name we actually drive through (may differ from the agent —
    /// e.g. Antigravity drives via `gemini`). `None` if not drivable.
    pub drives_via: Option<String>,
    /// Present when the bare `<binary> --version` failed on the *active* runtime
    /// but Bluey located a satisfying runtime under which the CLI runs (so it is
    /// drivable-via-runtime, not broken). Carries the resolved runtime + version
    /// for honest reporting. `None` when the active runtime worked, or when the
    /// CLI is absent / genuinely broken.
    pub runtime_resolution: Option<crate::runtime_resolve::RuntimeResolution>,
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

/// The outcome of probing `<binary> --version`, carrying enough detail to tell
/// "ran fine" from "won't run, and here's why" — the error text is what lets us
/// ask the runtime resolver whether the failure is a satisfiable runtime-version
/// mismatch rather than a genuine breakage.
enum VersionProbe {
    /// `--version` exited 0; the extracted (tolerant) version string.
    Ran(String),
    /// `--version` exited non-zero; the captured launch-error text (stderr, or
    /// stdout when stderr was empty — CLIs split this inconsistently).
    Failed(String),
    /// The binary could not be spawned at all (absent / not executable).
    Absent,
}

/// Probe a CLI binary's version by running `<binary> --version`, optionally under
/// a `path_override` (prepended runtime bin dir) supplied for the child process
/// only — the parent env is never mutated. Bounded, read-only, never panics.
///
/// Real-world formats vary: `2.0.42 (Claude Code)`, bare `0.44.1`,
/// `codex-cli 0.135.0` — so extraction is tolerant (first dotted-number token).
fn probe_cli_version_inner(binary: &str, path_override: Option<&str>) -> VersionProbe {
    let mut cmd = std::process::Command::new(binary);
    cmd.arg("--version");
    if let Some(path) = path_override {
        cmd.env("PATH", path);
    }
    let Ok(output) = cmd.output() else {
        return VersionProbe::Absent;
    };
    if !output.status.success() {
        // Capture why it refused to launch. Some CLIs print to stderr, some to
        // stdout (the Copilot node-version error goes to stdout), so fall back.
        let mut err_text = String::from_utf8_lossy(&output.stderr).into_owned();
        if err_text.trim().is_empty() {
            err_text = String::from_utf8_lossy(&output.stdout).into_owned();
        }
        return VersionProbe::Failed(err_text);
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.lines().next().unwrap_or("").trim();
    if line.is_empty() {
        return VersionProbe::Failed(String::new());
    }
    VersionProbe::Ran(extract_version_token(line))
}

/// Extract the first whitespace token that looks like a version (digits + dots),
/// falling back to the whole line when the format is unfamiliar.
fn extract_version_token(line: &str) -> String {
    line.split_whitespace()
        .find(|tok| {
            let core = tok.trim_start_matches('v');
            !core.is_empty()
                && core.chars().next().is_some_and(|c| c.is_ascii_digit())
                && core.contains('.')
        })
        .map(|t| t.trim_start_matches('v').to_string())
        .unwrap_or_else(|| line.to_string())
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
/// genuinely broken, which binary we drive through, and — when the bare probe
/// failed only because the *active* runtime is too old — the satisfying runtime
/// Bluey would drive it under.
///
/// The runtime step is generic and data-driven: a failed `--version` hands its
/// raw error text to [`crate::runtime_resolve`], which decides (from a small
/// runtime table, never a per-agent branch) whether the failure is a satisfiable
/// runtime-version mismatch and, if so, locates a satisfying install. We then
/// re-probe `--version` under that runtime's bin dir (prepended to the child
/// `PATH` only, exactly as the drive layer does) to *prove* it actually runs
/// before reporting it as drivable-via-runtime.
fn assess_surfaces(entry: &registry::AgentEntry) -> SurfaceStatus {
    let gui_version = gui_app_version(entry.app_bundles);

    // The agent's own CLI candidates (first that resolves is the drive binary).
    let mut cli_version = None;
    let mut cli_present_but_broken = false;
    let mut drives_via = None;
    let mut runtime_resolution = None;
    for binary in entry.binary_candidates {
        if !binary_on_path(binary) {
            continue;
        }
        match probe_cli_version_inner(binary, None) {
            VersionProbe::Ran(v) => {
                cli_version = Some(v);
                drives_via = Some((*binary).to_string());
                break;
            }
            VersionProbe::Failed(err_text) => {
                // On PATH but `--version` failed. Before calling it broken, ask
                // the runtime resolver whether this is just a too-old *active*
                // runtime we can satisfy from an installed one. Generic: the
                // resolver returns None for auth/network/any non-runtime error.
                match resolve_via_runtime(binary, &err_text) {
                    Some((version, resolution)) => {
                        cli_version = Some(version);
                        drives_via = Some((*binary).to_string());
                        runtime_resolution = Some(resolution);
                        break;
                    }
                    None => {
                        // No satisfying runtime (or not a runtime error at all)
                        // → genuinely broken. Keep scanning later candidates.
                        cli_present_but_broken = true;
                    }
                }
            }
            VersionProbe::Absent => {
                // `which` said it was on PATH but spawning failed (race / odd
                // perms) → treat as broken, like a failed launch.
                cli_present_but_broken = true;
            }
        }
    }

    SurfaceStatus {
        gui_version,
        cli_version,
        cli_present_but_broken,
        drives_via,
        runtime_resolution,
    }
}

/// Given a binary whose bare `--version` failed with `err_text`, decide whether
/// Bluey can drive it under a satisfying runtime. Returns the real version
/// (re-probed *under* that runtime) and the resolution, or `None` when the
/// failure is not a satisfiable runtime-version mismatch.
///
/// Generic by construction — no agent or runtime name appears here. The decision
/// is delegated to [`crate::runtime_resolve::resolve_for_launch_error`], and we
/// confirm by actually re-running `--version` with the runtime's bin dir
/// prepended to the child `PATH` (parent env untouched), so we never claim
/// "drivable" without proving the CLI runs.
fn resolve_via_runtime(
    binary: &str,
    err_text: &str,
) -> Option<(String, crate::runtime_resolve::RuntimeResolution)> {
    let resolution = crate::runtime_resolve::resolve_for_launch_error(err_text)?;
    let current = std::env::var("PATH").unwrap_or_default();
    let child_path = crate::runtime_resolve::prepend_path(&resolution.bin_dir, &current);
    // Re-probe under the satisfying runtime. Only a clean run proves drivability.
    match probe_cli_version_inner(binary, Some(&child_path)) {
        VersionProbe::Ran(version) => Some((version, resolution)),
        // Resolver pointed at a runtime but the CLI still won't run → broken.
        _ => None,
    }
}

/// Decide the `drivable` proof level from assessed surfaces. Pure (no I/O), so
/// the three-way decision is unit-testable without spawning processes:
///
/// - working CLI **+ a runtime resolution** → drivable *via* that runtime (the
///   case the old code wrongly reported as broken),
/// - working CLI **on the active runtime** → drivable directly,
/// - present-but-broken with **no** satisfying runtime → broken,
/// - nothing on PATH → not drivable.
///
/// A CLI must be on PATH AND actually run (`--version` succeeded), either on the
/// active runtime or under a satisfying runtime Bluey auto-selected. A bare
/// `--version` failure with NO satisfying runtime is genuinely broken; this
/// distinguishes the two so a mere runtime-version mismatch (e.g. a CLI needing
/// newer Node than the active one) is reported as drivable, matching what the
/// live drive matrix actually does.
fn drivable_from_surfaces(surfaces: &SurfaceStatus) -> ProofLevel {
    match (
        &surfaces.cli_version,
        &surfaces.drives_via,
        &surfaces.runtime_resolution,
    ) {
        (Some(v), Some(bin), Some(res)) => ProofLevel::Live(format!(
            "{bin} v{v} runs under {} v{} (auto-selected; your shell stays put)",
            res.req.kind.label(),
            res.found_version,
        )),
        (Some(v), Some(bin), None) => {
            ProofLevel::Live(format!("{bin} v{v} runs (drive verified separately)"))
        }
        _ if surfaces.cli_present_but_broken => {
            ProofLevel::Skipped("CLI on PATH but won't run — broken (reinstall to fix)".to_string())
        }
        _ => ProofLevel::Skipped("no working CLI on PATH (install it to drive)".to_string()),
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

    // Sessions — actually list (bounded) if a store was found. Use the
    // health-checked wrapper so a silently-drifted store (0 parsed of N raw)
    // emits one structured `unrecognized_format` warning instead of looking like
    // an empty store.
    let sessions = match found.and_then(|d| d.session_store.as_ref()) {
        Some(store) => {
            match crate::sessions::list_with_health_check(store.format, store, 100_000) {
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

    let drivable = drivable_from_surfaces(&surfaces);

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
        let cli = match (
            &p.surfaces.cli_version,
            &p.surfaces.runtime_resolution,
            p.surfaces.cli_present_but_broken,
        ) {
            (Some(v), Some(res), _) => {
                format!(
                    "CLI v{v} (via {} v{})",
                    res.req.kind.label(),
                    res.found_version
                )
            }
            (Some(v), None, _) => format!("CLI v{v}"),
            (None, _, true) => "CLI present-but-broken".to_string(),
            (None, _, false) => "CLI —".to_string(),
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
    use crate::runtime_resolve;

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

    // ---- version-token extraction ---------------------------------------

    #[test]
    fn extract_version_token_handles_real_cli_formats() {
        // Bare, prefixed-with-name, and v-prefixed.
        assert_eq!(extract_version_token("0.44.1"), "0.44.1");
        assert_eq!(extract_version_token("codex-cli 0.135.0"), "0.135.0");
        assert_eq!(extract_version_token("2.0.42 (Claude Code)"), "2.0.42");
        // The exact Copilot-under-v24 line — proves we pull the version, not the
        // prose. (Trailing dot is Copilot's own output quirk, preserved as-is.)
        assert_eq!(
            extract_version_token("GitHub Copilot CLI 1.0.62."),
            "1.0.62."
        );
        // Unfamiliar format falls back to the whole line, never panics.
        assert_eq!(extract_version_token("nightly-build"), "nightly-build");
    }

    // ---- the core decision: drivable vs broken --------------------------

    fn node_resolution(found: runtime_resolve::Version) -> runtime_resolve::RuntimeResolution {
        runtime_resolve::RuntimeResolution {
            req: runtime_resolve::RuntimeReq {
                kind: runtime_resolve::RuntimeKind::Node,
                min_version: runtime_resolve::Version::major_only(24),
            },
            bin_dir: std::path::PathBuf::from("/Users/x/.nvm/versions/node/v24.13.0/bin"),
            found_version: found,
        }
    }

    /// The regression this whole fix targets: `--version` failed on the active
    /// runtime, but a satisfying runtime was located and the CLI runs under it.
    /// This must be reported as **drivable (via runtime)**, never broken.
    #[test]
    fn version_fails_but_runtime_resolves_is_drivable_not_broken() {
        let surfaces = SurfaceStatus {
            gui_version: None,
            cli_version: Some("1.0.62".to_string()),
            // Crucially NOT broken: a runtime made it run.
            cli_present_but_broken: false,
            drives_via: Some("copilot".to_string()),
            runtime_resolution: Some(node_resolution(runtime_resolve::Version {
                major: 24,
                minor: 13,
                patch: 0,
            })),
        };
        let lvl = drivable_from_surfaces(&surfaces);
        assert!(
            matches!(lvl, ProofLevel::Live(_)),
            "runtime-resolved CLI must be Live (drivable), got {lvl:?}"
        );
        let detail = lvl.detail();
        // Names the agent binary, the runtime, and the satisfying version.
        assert!(detail.contains("copilot"), "detail: {detail}");
        assert!(detail.contains("Node.js"), "detail: {detail}");
        assert!(detail.contains("24.13.0"), "detail: {detail}");
        // Must NOT carry the old "broken / reinstall" language.
        assert!(!detail.contains("broken"), "detail: {detail}");
        assert!(!detail.contains("reinstall"), "detail: {detail}");
    }

    /// The genuinely-broken case is unchanged: `--version` failed and NO
    /// satisfying runtime exists (so `assess_surfaces` left `cli_version` empty
    /// and flagged broken). It must still report broken.
    #[test]
    fn version_fails_with_no_runtime_is_still_broken() {
        let surfaces = SurfaceStatus {
            gui_version: None,
            cli_version: None,
            cli_present_but_broken: true,
            drives_via: None,
            runtime_resolution: None,
        };
        let lvl = drivable_from_surfaces(&surfaces);
        assert!(
            matches!(lvl, ProofLevel::Skipped(_)),
            "genuinely broken CLI must be Skipped, got {lvl:?}"
        );
        assert!(lvl.detail().contains("broken"), "detail: {}", lvl.detail());
    }

    /// A CLI that runs on the *active* runtime (no resolution needed) stays a
    /// plain "runs" report — the runtime wording is reserved for the resolved
    /// case so the two are distinguishable.
    #[test]
    fn version_runs_on_active_runtime_is_drivable_without_runtime_wording() {
        let surfaces = SurfaceStatus {
            gui_version: None,
            cli_version: Some("0.44.1".to_string()),
            cli_present_but_broken: false,
            drives_via: Some("gemini".to_string()),
            runtime_resolution: None,
        };
        let lvl = drivable_from_surfaces(&surfaces);
        let detail = lvl.detail();
        assert!(matches!(lvl, ProofLevel::Live(_)));
        assert!(detail.contains("gemini"), "detail: {detail}");
        assert!(
            !detail.contains("auto-selected"),
            "active-runtime case must not claim a runtime was selected: {detail}"
        );
    }

    /// No CLI on PATH at all → not drivable, with the install hint (unchanged).
    #[test]
    fn no_cli_on_path_is_not_drivable() {
        let surfaces = SurfaceStatus {
            gui_version: None,
            cli_version: None,
            cli_present_but_broken: false,
            drives_via: None,
            runtime_resolution: None,
        };
        let lvl = drivable_from_surfaces(&surfaces);
        assert!(matches!(lvl, ProofLevel::Skipped(_)));
        assert!(lvl.detail().contains("install"), "detail: {}", lvl.detail());
    }

    // ---- resolve_via_runtime short-circuits (no spawn) ------------------

    #[test]
    fn resolve_via_runtime_rejects_non_runtime_errors() {
        // Auth / network failures are not runtime-version errors, so the
        // resolver returns None *before* any re-probe — these can never be
        // misreported as drivable-via-runtime.
        assert!(resolve_via_runtime("copilot", "Error: not signed in.").is_none());
        assert!(resolve_via_runtime("copilot", "401 Unauthorized").is_none());
        assert!(resolve_via_runtime("anything", "Cannot find module 'foo'").is_none());
    }

    #[test]
    fn resolve_via_runtime_rejects_unsatisfiable_runtime_requirement() {
        // A real runtime-version error, but for a version no install can satisfy
        // → locate_runtime returns None, so we report broken (None), never a
        // false "drivable". (No node v9999 exists anywhere.)
        let err = "This CLI requires Node.js v9999 or higher. Currently using v23.7.0.";
        assert!(resolve_via_runtime("definitely-not-a-real-binary", err).is_none());
    }
}

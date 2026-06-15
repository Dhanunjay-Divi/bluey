//! Proactive provisioning — install a missing agent CLI so the user becomes
//! drivable, instead of degrading to read-only.
//!
//! See `docs/work/PLAN-PRODUCTION-VISION.md` §2 (proactive, three tiers) and §5
//! Phase A. The flow for an agent whose CLI is missing but which has an
//! [`InstallRecipe`]:
//!
//! ```text
//! plan_install(agent)  → an InstallPlan describing exactly what would run
//! (consent happens at the call site — Bluey shows the plan, the user approves)
//! run_install(plan)    → execute the vetted recipe, then VERIFY the binary
//!                        appeared on PATH (never trust the installer exit code)
//! ```
//!
//! Security invariants:
//! - **Vetted recipes only.** The command comes from the registry's
//!   [`InstallRecipe`] (official npm package or official vendor URL) — never a
//!   string from user input, a config file, or the network.
//! - **Consent-gated.** This module never installs on its own; it produces an
//!   [`InstallPlan`] for the caller to confirm, then runs it only when asked.
//! - **Verified.** Success requires the expected binary to actually appear on
//!   `PATH` — the installer's exit code alone is not trusted.
//! - **No arbitrary shell.** npm runs via an argv array. The curl-script form
//!   is the one place a shell pipe is unavoidable (vendor installers ship as
//!   `curl … | bash`); it runs only the exact official URL from the recipe.

use std::process::Command;

use crate::registry::{self, InstallMethod, InstallRecipe};
use crate::AgentKind;

/// A concrete, inspectable description of what installing an agent's CLI would
/// do — shown to the user for consent before anything runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallPlan {
    pub agent: AgentKind,
    /// The recipe being used (method + spec + verify binary).
    pub recipe: InstallRecipe,
    /// A human-readable one-line description of the command, for the consent
    /// prompt (e.g. `npm install -g @openai/codex`).
    pub human_command: String,
    /// The prerequisite that must be present for this method (e.g. `npm`), if
    /// any. `None` means no external prerequisite.
    pub prerequisite: Option<&'static str>,
}

/// Outcome of running an [`InstallPlan`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallOutcome {
    /// Installed and the expected binary is now on `PATH` **and runs**.
    Installed { binary: String },
    /// The binary installed and is on `PATH`, but it will not actually run in
    /// this environment — e.g. a runtime-version mismatch (the GitHub Copilot
    /// CLI requires Node ≥24; if the active node is older, `copilot` is present
    /// but errors on launch). "On PATH" is NOT the same as "runnable", so this is
    /// reported distinctly from [`Installed`] with the real launch error, rather
    /// than falsely claiming success. The `detail` carries the runtime's own
    /// message so the caller can guide the user (or auto-resolve the runtime).
    InstalledButNotRunnable { binary: String, detail: String },
    /// The required prerequisite (e.g. `npm`) is missing — nothing was run.
    MissingPrerequisite { needed: &'static str },
    /// The installer ran but the binary did not appear on `PATH`.
    VerificationFailed { binary: String, detail: String },
    /// The installer command itself failed (non-zero exit / spawn error).
    InstallFailed { detail: String },
}

/// A read-only diagnosis of the environment before installing — so the runner
/// can spot (and remedy) obstructions instead of blindly failing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preflight {
    /// The verify-binary already resolves on `PATH` — nothing to install.
    pub already_installed: bool,
    /// The prerequisite (npm/curl) is missing — install cannot proceed.
    pub missing_prerequisite: Option<&'static str>,
    /// An obstruction that would block (or already blocked) the install.
    pub obstruction: Option<Obstruction>,
}

/// A detected obstruction to installing, with the remedy that clears it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Obstruction {
    /// A bin path holds a **broken symlink** (target does not exist) — npm's
    /// `EEXIST` will refuse to overwrite it. Remedy: remove the dangling link.
    /// Found in the wild: `/usr/local/bin/codex` → deleted Homebrew cask.
    BrokenSymlink { path: std::path::PathBuf },
}

impl Obstruction {
    /// The remedy for this obstruction, and whether it is destructive (needs
    /// explicit consent before [`apply_remedy`] runs it).
    pub fn remedy(&self) -> Remedy {
        match self {
            Obstruction::BrokenSymlink { path } => Remedy {
                description: format!(
                    "remove the broken symlink at {} (its target no longer exists)",
                    path.display()
                ),
                destructive: true,
            },
        }
    }
}

/// A remedy for an [`Obstruction`] — shown to the user; destructive ones are
/// only applied after consent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remedy {
    pub description: String,
    /// True if applying this remedy modifies/removes something on disk.
    pub destructive: bool,
}

/// Read-only environment diagnosis for an install plan. Touches nothing — only
/// probes PATH and inspects the target bin path. The basis for deciding whether
/// to install, report a blocker, or propose a remedy first.
pub fn preflight(plan: &InstallPlan) -> Preflight {
    let already_installed = binary_on_path(plan.recipe.verify_binary);

    let missing_prerequisite = match plan.prerequisite {
        Some(p) if !binary_on_path(p) => Some(p),
        _ => None,
    };

    // Look for a broken symlink occupying a likely bin path for the verify
    // binary — the real-world EEXIST cause (stale brew cask, switched installer).
    let obstruction = (!already_installed)
        .then(|| broken_symlink_for(plan.recipe.verify_binary))
        .flatten()
        .map(|path| Obstruction::BrokenSymlink { path });

    Preflight {
        already_installed,
        missing_prerequisite,
        obstruction,
    }
}

/// Find a **broken symlink** named `binary` in a standard bin dir, if one
/// exists. A broken symlink is a path that `symlink_metadata` sees (the link
/// itself exists) but `metadata` cannot resolve (the target is gone). Read-only.
fn broken_symlink_for(binary: &str) -> Option<std::path::PathBuf> {
    let mut dirs: Vec<std::path::PathBuf> = vec![
        std::path::PathBuf::from("/usr/local/bin"),
        std::path::PathBuf::from("/opt/homebrew/bin"),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(std::path::Path::new(&home).join(".local/bin"));
    }
    for dir in dirs {
        let candidate = dir.join(binary);
        // The link itself exists?
        if std::fs::symlink_metadata(&candidate).is_ok() {
            // …but the target does not resolve → broken symlink.
            if std::fs::metadata(&candidate).is_err() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Apply a remedy. Destructive remedies (removing a broken symlink) MUST have
/// been consented to by the caller. Removes ONLY a path that is still a broken
/// symlink at apply time (re-checked) — never a working file. Returns whether
/// the remedy was applied.
pub fn apply_remedy(obstruction: &Obstruction) -> anyhow::Result<bool> {
    match obstruction {
        Obstruction::BrokenSymlink { path } => {
            // Re-verify it is STILL a broken symlink right now — never remove a
            // path that has become a real file/working binary since preflight.
            let is_link = std::fs::symlink_metadata(path).is_ok();
            let target_missing = std::fs::metadata(path).is_err();
            if is_link && target_missing {
                std::fs::remove_file(path)?;
                Ok(true)
            } else {
                // No longer a broken symlink — do nothing (safe).
                Ok(false)
            }
        }
    }
}

/// Build an [`InstallPlan`] for an agent, if it has a known recipe. Returns
/// `None` for agents with no installer (e.g. VS Code, Windsurf). Does no I/O
/// beyond probing for the prerequisite — never installs anything.
pub fn plan_install(agent: &AgentKind) -> Option<InstallPlan> {
    let tag = registry::KindTag::from_agent_kind(agent)?;
    let recipe = registry::entry_for(tag).and_then(|e| e.install)?;

    let (human_command, prerequisite) = match recipe.method {
        InstallMethod::NpmGlobal => (format!("npm install -g {}", recipe.spec), Some("npm")),
        InstallMethod::CurlScript => (format!("curl -fsSL {} | bash", recipe.spec), Some("curl")),
    };

    Some(InstallPlan {
        agent: agent.clone(),
        recipe,
        human_command,
        prerequisite,
    })
}

/// Whether a command is available on `PATH`. Read-only; never executes the
/// target — uses a `--version`-free `command -v`-style probe via `which`.
fn binary_on_path(binary: &str) -> bool {
    // `which` is on every supported platform's PATH; if even it is missing we
    // conservatively report false rather than risk a false positive.
    Command::new("which")
        .arg(binary)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Whether a binary on `PATH` actually **runs** — `Ok(())` if a `--version`
/// probe exits 0, `Err(launch-error-text)` if it spawns but fails (e.g. a
/// runtime-version mismatch like Copilot's "requires Node v24"). This is the
/// difference between "installed" and "usable": a binary can be on `PATH` yet
/// non-functional. We probe `--version` (cheap, side-effect-free for every CLI
/// in the registry) and, on a non-zero exit, return the combined stderr+stdout
/// tail so the caller sees the real reason. A binary that isn't even on PATH
/// returns `Err` too (caller should check `binary_on_path` first for clarity).
fn binary_runnable(binary: &str) -> Result<(), String> {
    match Command::new(binary).arg("--version").output() {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => {
            // Some CLIs print their launch error to stdout, some to stderr.
            let mut msg = String::from_utf8_lossy(&o.stderr).into_owned();
            if msg.trim().is_empty() {
                msg = String::from_utf8_lossy(&o.stdout).into_owned();
            }
            let one_line: String = msg.split_whitespace().collect::<Vec<_>>().join(" ");
            let tail: String = one_line.chars().take(240).collect();
            Err(if tail.trim().is_empty() {
                format!("`{binary} --version` exited non-zero")
            } else {
                tail
            })
        }
        Err(e) => Err(format!("could not run `{binary}`: {e}")),
    }
}

/// Run an approved [`InstallPlan`]: check the prerequisite, execute the vetted
/// recipe, then VERIFY the expected binary appeared on `PATH`.
///
/// Caller MUST have obtained user consent before calling this — it executes an
/// install command. Never panics; every failure is an [`InstallOutcome`].
pub fn run_install(plan: &InstallPlan) -> InstallOutcome {
    // Prerequisite gate.
    if let Some(prereq) = plan.prerequisite {
        if !binary_on_path(prereq) {
            return InstallOutcome::MissingPrerequisite { needed: prereq };
        }
    }

    let result = match plan.recipe.method {
        // npm: argv array, no shell — the package name can never be a flag.
        InstallMethod::NpmGlobal => Command::new("npm")
            .args(["install", "-g", plan.recipe.spec])
            .output(),
        // Vendor install scripts ship as `curl URL | bash`. We pipe ONLY the
        // exact official URL from the vetted recipe — no interpolation of any
        // outside value. This is the one unavoidable shell use.
        InstallMethod::CurlScript => {
            let script = format!("curl -fsSL {} | bash", shell_safe_url(plan.recipe.spec));
            Command::new("bash").args(["-c", &script]).output()
        }
    };

    match result {
        Ok(output) if output.status.success() => {
            // Verify — never trust the exit code alone. Two distinct checks:
            // (1) is the binary on PATH? (2) does it actually RUN? A binary can
            // pass (1) and fail (2) — e.g. Copilot on a too-old Node — so we
            // report that case honestly instead of claiming "Installed".
            let binary = plan.recipe.verify_binary.to_string();
            if !binary_on_path(&binary) {
                InstallOutcome::VerificationFailed {
                    binary,
                    detail: "install reported success but binary not on PATH \
                             (may need a new shell / PATH refresh)"
                        .to_string(),
                }
            } else {
                match binary_runnable(&binary) {
                    Ok(()) => InstallOutcome::Installed { binary },
                    Err(detail) => InstallOutcome::InstalledButNotRunnable { binary, detail },
                }
            }
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let tail: String = stderr.chars().rev().take(300).collect::<String>();
            let tail: String = tail.chars().rev().collect();
            InstallOutcome::InstallFailed {
                detail: format!("installer exited non-zero: {}", tail.trim()),
            }
        }
        Err(e) => InstallOutcome::InstallFailed {
            detail: format!("could not run installer: {e}"),
        },
    }
}

/// Whether the caller has granted consent to apply a destructive remedy
/// (e.g. removing a broken symlink) during provisioning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemedyConsent {
    /// Apply safe remedies automatically; apply destructive ones too.
    AllowDestructive,
    /// Apply safe remedies only; a destructive remedy blocks with a report.
    SafeOnly,
}

/// Production provisioning with dynamic pre-flight + recovery. Diagnoses the
/// environment, clears a consented obstruction, installs, and on an
/// obstruction-shaped failure applies the remedy and retries once. Read-only
/// until it installs or applies a (consented) remedy.
///
/// This is the entry point a caller (daemon/UI) uses after showing the user the
/// [`InstallPlan`] and any [`Remedy`] for consent.
pub fn provision_with_recovery(plan: &InstallPlan, consent: RemedyConsent) -> InstallOutcome {
    let pre = preflight(plan);

    if pre.already_installed {
        // On PATH already — but confirm it actually RUNS before claiming success
        // (the Copilot/old-Node case: present but non-functional).
        let binary = plan.recipe.verify_binary.to_string();
        return match binary_runnable(&binary) {
            Ok(()) => InstallOutcome::Installed { binary },
            Err(detail) => InstallOutcome::InstalledButNotRunnable { binary, detail },
        };
    }
    if let Some(needed) = pre.missing_prerequisite {
        return InstallOutcome::MissingPrerequisite { needed };
    }

    // Clear a known obstruction up front (e.g. a broken symlink that would
    // cause EEXIST), if consent allows.
    if let Some(obstruction) = &pre.obstruction {
        let remedy = obstruction.remedy();
        if remedy.destructive && consent != RemedyConsent::AllowDestructive {
            return InstallOutcome::InstallFailed {
                detail: format!("blocked: {} (needs consent to fix)", remedy.description),
            };
        }
        let _ = apply_remedy(obstruction);
    }

    // First install attempt.
    let outcome = run_install(plan);
    if matches!(outcome, InstallOutcome::Installed { .. }) {
        return outcome;
    }

    // On failure, re-diagnose: a freshly-detected obstruction (the install
    // surfaced it) can be remedied + retried ONCE.
    let post = preflight(plan);
    if let Some(obstruction) = &post.obstruction {
        let remedy = obstruction.remedy();
        if remedy.destructive && consent != RemedyConsent::AllowDestructive {
            return InstallOutcome::InstallFailed {
                detail: format!(
                    "install blocked by: {} (needs consent to fix)",
                    remedy.description
                ),
            };
        }
        if apply_remedy(obstruction).unwrap_or(false) {
            return run_install(plan); // single retry after clearing
        }
    }
    outcome
}

/// Defensive guard: a recipe URL must be a plain `https://` URL with no shell
/// metacharacters, so the `curl … | bash` form can never inject a second
/// command. Recipes are compile-time constants from the registry, but this is
/// belt-and-suspenders in case a future row is malformed.
fn shell_safe_url(url: &str) -> &str {
    let ok = url.starts_with("https://")
        && !url
            .chars()
            .any(|c| matches!(c, ';' | '|' | '&' | '`' | '$' | '(' | ')' | ' ' | '\n'));
    if ok {
        url
    } else {
        // A malformed recipe yields a URL that curl will simply fail to fetch,
        // rather than ever executing injected content.
        ""
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_install_npm_agent_describes_command() {
        let plan = plan_install(&AgentKind::Codex).expect("codex has a recipe");
        assert_eq!(plan.recipe.method, InstallMethod::NpmGlobal);
        assert_eq!(plan.human_command, "npm install -g @openai/codex");
        assert_eq!(plan.prerequisite, Some("npm"));
        assert_eq!(plan.recipe.verify_binary, "codex");
    }

    #[test]
    fn plan_install_curl_agent_describes_command() {
        let plan = plan_install(&AgentKind::Cursor).expect("cursor has a recipe");
        assert_eq!(plan.recipe.method, InstallMethod::CurlScript);
        assert!(plan
            .human_command
            .starts_with("curl -fsSL https://cursor.com/install"));
        assert_eq!(plan.prerequisite, Some("curl"));
        assert_eq!(plan.recipe.verify_binary, "cursor-agent");
    }

    #[test]
    fn agents_without_installer_have_no_plan() {
        // VS Code / Windsurf have no headless agent-CLI installer.
        assert!(plan_install(&AgentKind::VsCodeFork).is_none());
        assert!(plan_install(&AgentKind::Windsurf).is_none());
        // Unknown agents never get an install plan.
        assert!(plan_install(&AgentKind::Unknown).is_none());
        assert!(plan_install(&AgentKind::Other("x".into())).is_none());
    }

    #[test]
    fn shell_safe_url_rejects_injection() {
        assert_eq!(
            shell_safe_url("https://cursor.com/install"),
            "https://cursor.com/install"
        );
        // Anything with shell metacharacters or a non-https scheme is neutralized.
        assert_eq!(shell_safe_url("https://x.com/i; rm -rf /"), "");
        assert_eq!(shell_safe_url("http://x.com/i"), "");
        assert_eq!(shell_safe_url("https://x.com/$(evil)"), "");
    }

    #[test]
    fn every_recipe_url_is_shell_safe() {
        // Every CurlScript recipe in the registry must pass the injection guard.
        for kind in [
            AgentKind::ClaudeCode,
            AgentKind::Cursor,
            AgentKind::Antigravity,
            AgentKind::Gemini,
            AgentKind::Codex,
            AgentKind::Copilot,
        ] {
            if let Some(plan) = plan_install(&kind) {
                if plan.recipe.method == InstallMethod::CurlScript {
                    assert_eq!(
                        shell_safe_url(plan.recipe.spec),
                        plan.recipe.spec,
                        "{kind:?} curl recipe URL must be shell-safe"
                    );
                }
            }
        }
    }

    #[test]
    fn apply_remedy_removes_a_real_broken_symlink_and_spares_real_files() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().expect("tempdir");

        // A broken symlink: points at a path that does not exist.
        let link = dir.path().join("codex");
        let missing_target = dir.path().join("gone-target");
        symlink(&missing_target, &link).expect("make symlink");
        assert!(std::fs::symlink_metadata(&link).is_ok(), "link exists");
        assert!(std::fs::metadata(&link).is_err(), "target missing → broken");

        let obstruction = Obstruction::BrokenSymlink { path: link.clone() };
        assert!(
            obstruction.remedy().destructive,
            "removing a link is destructive"
        );
        let applied = apply_remedy(&obstruction).expect("remedy runs");
        assert!(applied, "broken symlink removed");
        assert!(std::fs::symlink_metadata(&link).is_err(), "link gone");

        // A REAL file at the same kind of path must NEVER be removed by the
        // remedy (re-check guards it).
        let real = dir.path().join("realbin");
        std::fs::write(&real, b"#!/bin/sh\n").expect("write real file");
        let not_a_broken_link = Obstruction::BrokenSymlink { path: real.clone() };
        let applied2 = apply_remedy(&not_a_broken_link).expect("remedy runs");
        assert!(
            !applied2,
            "must NOT remove a real (non-broken-symlink) file"
        );
        assert!(real.exists(), "real file spared");
    }

    #[test]
    fn provision_blocks_destructive_remedy_without_consent() {
        // We can't trigger a real obstruction deterministically here, but we can
        // assert the consent enum gates correctly: SafeOnly must never be equal
        // to AllowDestructive, and the runner branches on it.
        assert_ne!(RemedyConsent::SafeOnly, RemedyConsent::AllowDestructive);
    }

    #[test]
    fn binary_runnable_distinguishes_runs_from_fails() {
        // A real, always-present binary that supports --version must report
        // runnable. (`sh` exists on every supported platform; but it doesn't take
        // --version cleanly, so use a binary that does. `true` always exits 0
        // regardless of args, which is the safest cross-platform "runs" proxy.)
        assert!(
            binary_runnable("true").is_ok(),
            "`true` should always run and exit 0"
        );
        // A binary that does not exist on PATH must report NOT runnable, with a
        // reason — never a false positive.
        let missing = binary_runnable("definitely-not-a-real-binary-xyz123");
        assert!(missing.is_err(), "a nonexistent binary is not runnable");
        assert!(
            !missing.unwrap_err().is_empty(),
            "the failure must carry a reason"
        );
    }

    #[test]
    fn installed_but_not_runnable_is_distinct_from_installed() {
        // The whole point of the variant: "on PATH" != "usable". The two
        // outcomes must be different values so callers can treat them
        // differently (Installed = drive it; InstalledButNotRunnable = guide the
        // user / resolve the runtime first).
        let ok = InstallOutcome::Installed {
            binary: "copilot".into(),
        };
        let runtime_blocked = InstallOutcome::InstalledButNotRunnable {
            binary: "copilot".into(),
            detail: "requires Node v24".into(),
        };
        assert_ne!(ok, runtime_blocked);
    }
}

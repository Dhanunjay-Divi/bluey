//! Runtime resolver — drive an agent under a satisfying runtime when its CLI is
//! present but fails to launch on the *active* runtime version.
//!
//! ## The problem this solves (found by real testing, not theory)
//!
//! Bluey can install an agent's CLI successfully and still not be able to drive
//! it. The GitHub Copilot CLI (`copilot`) is the live case: it is on `PATH`, it
//! is signed in, but it refuses to launch because it requires Node.js ≥ 24 and
//! the user's *active* node is older:
//!
//! ```text
//! GitHub Copilot CLI requires Node.js v24 or higher. Currently using v23.7.0.
//! ```
//!
//! The user *has* a satisfying node (`~/.nvm/versions/node/v24.13.0/bin`), it is
//! just not the active one. Telling the user to "go run `nvm use 24`" is a punt.
//! A production system resolves it: it detects the mismatch, finds a satisfying
//! installed runtime, and drives the agent under that runtime by **prepending
//! that runtime's bin dir to the child process's `PATH` for that spawn only**.
//!
//! ## Invariants
//!
//! - **Per-spawn `PATH` only.** This module produces a `PATH` *string* for one
//!   child process. It never calls `nvm`, never writes a shell profile, never
//!   sets a global default, never mutates `std::env`. The parent process and the
//!   user's shell are untouched.
//! - **Read-only locating.** Finding a runtime only inspects the filesystem
//!   (well-known version-manager dirs) and runs `<candidate> --version` to learn
//!   a version. It installs nothing.
//! - **Data-driven detection.** Whether a launch failure is a *runtime-version*
//!   failure is decided by parsing the CLI's own error text against a small,
//!   generic table of runtimes — not by `if agent == "copilot"`. A new runtime
//!   is a new [`RuntimeKind`] row, not a new code path.
//! - **Fail-soft.** Every step degrades to `None`/"no resolution"; if no
//!   satisfying runtime is found the caller falls back to today's honest error.
//!   Nothing here panics.
//!
//! ## Autonomy model: propose + approve
//!
//! Detecting and *applying* the fix are separate. [`resolve_for_launch_error`]
//! and [`RuntimeResolution`] describe a fix; whether to apply it is the caller's
//! decision after the user approves (see the CLI's `agent prove` consent prompt
//! and the daemon's `OverlayCommand::PushFixProposal` for the GUI hook). Bluey
//! does the work — it never tells the user to do it themselves.

use std::path::{Path, PathBuf};
use std::process::Command;

/// A language runtime an agent CLI may depend on. Generic and data-driven: each
/// variant carries the markers needed to (a) recognize it in a launch-error
/// string and (b) locate installed copies of it. Adding a runtime is a new
/// variant + a new [`RuntimeKind::all`] row — never a per-agent branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeKind {
    /// Node.js — the runtime behind the JS/TS agent CLIs (Copilot, Gemini,
    /// Cursor, Codex, Claude Code all ship as npm packages launched by `node`).
    Node,
    /// CPython — the runtime behind Python agent CLIs (e.g. Aider).
    Python,
}

impl RuntimeKind {
    /// Every runtime the resolver knows how to detect and locate. Detection
    /// walks this list and the first whose [`name_aliases`](Self::name_aliases)
    /// appears in the error text wins.
    pub fn all() -> &'static [RuntimeKind] {
        &[RuntimeKind::Node, RuntimeKind::Python]
    }

    /// Lower-cased substrings that name this runtime in a CLI's launch-error
    /// text. Matched case-insensitively. e.g. Copilot says "Node.js"; a future
    /// CLI might say "node". Kept deliberately specific so an unrelated mention
    /// of the word in a stack trace does not over-match (detection *also*
    /// requires a version token nearby — see [`detect_requirement`]).
    pub fn name_aliases(&self) -> &'static [&'static str] {
        match self {
            RuntimeKind::Node => &["node.js", "nodejs", "node"],
            RuntimeKind::Python => &["python"],
        }
    }

    /// The executable name this runtime is invoked as on `PATH` (and the file
    /// we probe with `--version` inside a candidate bin dir).
    pub fn binary(&self) -> &'static str {
        match self {
            RuntimeKind::Node => "node",
            RuntimeKind::Python => "python3",
        }
    }

    /// Human label for the proposal/consent surface.
    pub fn label(&self) -> &'static str {
        match self {
            RuntimeKind::Node => "Node.js",
            RuntimeKind::Python => "Python",
        }
    }

    /// Candidate bin directories where versioned installs of this runtime live,
    /// in **no particular priority** (the locator probes every one and picks the
    /// highest *satisfying* version, so order does not matter). All paths are
    /// `$HOME`-anchored or well-known system prefixes; none are derived from
    /// untrusted input. Read-only scan targets.
    fn candidate_bin_dirs(&self, home: &Path) -> Vec<PathBuf> {
        match self {
            RuntimeKind::Node => {
                let mut dirs = Vec::new();
                // nvm: ~/.nvm/versions/node/v<ver>/bin
                push_versioned(&mut dirs, home.join(".nvm/versions/node"), "bin");
                // fnm: ~/.fnm/node-versions/v<ver>/installation/bin  AND the
                // newer XDG location ~/.local/share/fnm/node-versions/...
                push_versioned(
                    &mut dirs,
                    home.join(".fnm/node-versions"),
                    "installation/bin",
                );
                push_versioned(
                    &mut dirs,
                    home.join(".local/share/fnm/node-versions"),
                    "installation/bin",
                );
                // n: ~/n/n/versions/node/<ver>/bin and /usr/local/n/versions/node/<ver>/bin
                push_versioned(&mut dirs, home.join("n/versions/node"), "bin");
                push_versioned(
                    &mut dirs,
                    PathBuf::from("/usr/local/n/versions/node"),
                    "bin",
                );
                // volta: a single shimmed bin (~/.volta/bin) — one row, no glob.
                dirs.push(home.join(".volta/bin"));
                // Homebrew versioned formulae: /opt/homebrew/opt/node@NN/bin (Apple
                // silicon) and /usr/local/opt/node@NN/bin (Intel), plus the
                // unversioned `node` formula.
                push_brew_node(&mut dirs, "/opt/homebrew/opt");
                push_brew_node(&mut dirs, "/usr/local/opt");
                dirs
            }
            RuntimeKind::Python => {
                let mut dirs = Vec::new();
                // pyenv: ~/.pyenv/versions/<ver>/bin
                push_versioned(&mut dirs, home.join(".pyenv/versions"), "bin");
                // Homebrew python formulae.
                push_brew_python(&mut dirs, "/opt/homebrew/opt");
                push_brew_python(&mut dirs, "/usr/local/opt");
                dirs
            }
        }
    }
}

/// Push `<root>/<each child>/<suffix>` for every immediate subdirectory of
/// `root`, if `root` is a readable directory. Fail-soft: a missing or
/// unreadable `root` contributes nothing.
fn push_versioned(out: &mut Vec<PathBuf>, root: PathBuf, suffix: &str) {
    let Ok(entries) = std::fs::read_dir(&root) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            out.push(p.join(suffix));
        }
    }
}

/// Push every `<prefix>/node@*/bin` plus the unversioned `<prefix>/node/bin`
/// Homebrew layout, when present. Fail-soft.
fn push_brew_node(out: &mut Vec<PathBuf>, prefix: &str) {
    push_brew_formula(out, prefix, "node");
}

/// Push every `<prefix>/python@*/bin` plus `<prefix>/python/bin`. Fail-soft.
fn push_brew_python(out: &mut Vec<PathBuf>, prefix: &str) {
    push_brew_formula(out, prefix, "python");
}

/// Shared Homebrew layout scanner: `<prefix>/<formula>/bin` and every
/// `<prefix>/<formula>@<ver>/bin`.
fn push_brew_formula(out: &mut Vec<PathBuf>, prefix: &str, formula: &str) {
    let prefix = Path::new(prefix);
    // Unversioned formula.
    out.push(prefix.join(formula).join("bin"));
    // Versioned `formula@NN` formulae.
    let Ok(entries) = std::fs::read_dir(prefix) else {
        return;
    };
    let versioned = format!("{formula}@");
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(&versioned) {
            out.push(entry.path().join("bin"));
        }
    }
}

/// A dotted version (`major.minor.patch`), each component optional after the
/// first. Comparison is numeric and component-wise, with a missing component
/// treated as `0` (so `24` == `24.0.0` for ordering against `24.0.1`). Only the
/// leading numeric core is parsed; any pre-release/build suffix (`-rc1`,
/// `+meta`) is ignored, which is the right call for a "is this new enough?"
/// gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl Version {
    /// A bare major version (`v24` → `24.0.0`).
    pub fn major_only(major: u64) -> Self {
        Self {
            major,
            minor: 0,
            patch: 0,
        }
    }

    /// Parse a version from a string that may carry a leading `v`/`V`, trailing
    /// junk, or a pre-release suffix. Returns `None` if there is no leading
    /// numeric major component.
    ///
    /// Accepts: `v24`, `24.13.0`, `v24.13.0`, `Python 3.11.4` (when handed the
    /// `3.11.4` token), `24.13.0-nightly`. The first run of digits is the major;
    /// `.`-separated digit runs after it are minor/patch.
    pub fn parse(s: &str) -> Option<Version> {
        let s = s.trim();
        // Drop a single leading v/V.
        let s = s.strip_prefix(['v', 'V']).unwrap_or(s);
        // Take the leading "x.y.z"-shaped core: digits and dots only.
        let core: String = s
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        let mut parts = core.split('.').filter(|p| !p.is_empty());
        let major: u64 = parts.next()?.parse().ok()?;
        let minor: u64 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
        let patch: u64 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
        Some(Version {
            major,
            minor,
            patch,
        })
    }

    /// True when `self` meets or exceeds `required` (the satisfies check).
    pub fn satisfies(&self, required: &Version) -> bool {
        (self.major, self.minor, self.patch) >= (required.major, required.minor, required.patch)
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// A parsed runtime requirement extracted from a CLI's launch-error text: which
/// runtime, and the minimum version it demands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeReq {
    pub kind: RuntimeKind,
    pub min_version: Version,
}

/// A located runtime that satisfies a [`RuntimeReq`]: the bin directory to
/// prepend to a child's `PATH`, the version found there, and the requirement it
/// satisfies. This is the *proposal* — what Bluey would do, surfaced for
/// approval before it drives under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeResolution {
    /// The requirement that triggered this resolution.
    pub req: RuntimeReq,
    /// The bin dir to PREPEND to the child process `PATH` (this child only).
    pub bin_dir: PathBuf,
    /// The runtime version found in `bin_dir` (≥ `req.min_version`).
    pub found_version: Version,
}

impl RuntimeResolution {
    /// A one-line, human-readable description of the fix for a consent prompt,
    /// naming the agent binary it would unblock. Example:
    /// `run copilot under Node.js v24.13.0 (found at …/v24.13.0/bin); your shell stays on its current node`.
    pub fn proposal_line(&self, agent_binary: &str) -> String {
        format!(
            "run {agent_binary} under {} v{} (found at {}); your shell stays on its current {}",
            self.req.kind.label(),
            self.found_version,
            self.bin_dir.display(),
            self.req.kind.binary(),
        )
    }
}

/// Detect a runtime-version requirement from a CLI's launch-error text.
///
/// This is the **"did it fail because of a runtime version?"** discriminator,
/// and it is deliberately data-driven: it walks [`RuntimeKind::all`] and looks
/// for the co-occurrence of (a) a runtime name alias and (b) a version token
/// near a "requires/needs/or higher/or newer/at least"-style phrase. Both must
/// be present, so an unrelated error that merely mentions "node" (e.g. a DOM
/// "node" in a stack trace) does not falsely trigger.
///
/// Returns the strongest (highest) requirement found, or `None` when the text
/// is not a recognizable runtime-version error. Pure; no I/O.
///
/// Recognized shapes (case-insensitive), e.g.:
/// - `GitHub Copilot CLI requires Node.js v24 or higher. Currently using v23.7.0.`
/// - `requires Node.js version 24 or newer`
/// - `needs Python >= 3.11`
/// - `Node.js 24+ is required`
pub fn detect_requirement(error_text: &str) -> Option<RuntimeReq> {
    let lower = error_text.to_lowercase();

    // A requirement phrase must be present — this is what separates a
    // *version* failure from any other error that happens to name a runtime.
    const REQUIRE_MARKERS: &[&str] = &[
        "requires",
        "required",
        "require ",
        "needs",
        "need ",
        "must be",
        "or higher",
        "or newer",
        "or above",
        "or later",
        "at least",
        ">=",
        "is required",
    ];
    if !REQUIRE_MARKERS.iter().any(|m| lower.contains(m)) {
        return None;
    }

    let mut best: Option<RuntimeReq> = None;
    for kind in RuntimeKind::all() {
        // Find where this runtime is named.
        let Some(alias) = kind
            .name_aliases()
            .iter()
            .find(|a| lower.contains(**a))
            .copied()
        else {
            continue;
        };
        // Parse the *required* version: the first version-shaped token that
        // appears AFTER the runtime name. Anchoring after the name avoids
        // grabbing the "Currently using vX" version (which appears later) or an
        // unrelated leading number. We scan tokens from just after the alias.
        let Some(idx) = lower.find(alias) else {
            continue;
        };
        let after = &lower[idx + alias.len()..];
        let Some(min_version) = first_version_token(after) else {
            continue;
        };
        let req = RuntimeReq {
            kind: *kind,
            min_version,
        };
        // Keep the highest min_version if several runtimes somehow match.
        best = match best {
            Some(b) if b.min_version.satisfies(&min_version) => Some(b),
            _ => Some(req),
        };
    }
    best
}

/// Find the first version-shaped token in `s`: a run beginning with an optional
/// `v`/`V` then a digit. Handles `v24`, `24`, `24.13.0`, `24+`, `version 24`.
/// Returns the parsed [`Version`] of that first token.
fn first_version_token(s: &str) -> Option<Version> {
    // Split on anything that is not part of a version token. We want runs that
    // contain digits, dots, and a possible leading v.
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        let is_start = c.is_ascii_digit() || ((c == 'v' || c == 'V') && next_is_digit(bytes, i));
        if is_start {
            // Consume the token: [vV]? then digits/dots.
            let start = i;
            if c == 'v' || c == 'V' {
                i += 1;
            }
            while i < bytes.len() && ((bytes[i] as char).is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            if let Some(ver) = Version::parse(&s[start..i]) {
                return Some(ver);
            }
        } else {
            i += 1;
        }
    }
    None
}

/// Whether the byte after index `i` is an ASCII digit (used to require a `v` be
/// a version prefix, not a stray letter).
fn next_is_digit(bytes: &[u8], i: usize) -> bool {
    bytes
        .get(i + 1)
        .is_some_and(|b| (*b as char).is_ascii_digit())
}

/// Locate an installed runtime that satisfies `req`, returning the highest
/// satisfying version found and the bin dir to prepend. Read-only and
/// fail-soft: scans well-known version-manager dirs plus the runtime's plain
/// `PATH` location, probes each candidate's `--version`, and keeps the best one
/// that meets `req.min_version`. Returns `None` when nothing satisfies (the
/// caller then falls back to the honest launch error).
pub fn locate_runtime(req: &RuntimeReq) -> Option<RuntimeResolution> {
    let home = home_dir()?;
    let mut candidates = req.kind.candidate_bin_dirs(&home);
    // Also consider whatever bin dir the runtime currently resolves to on PATH
    // — a system/global install may already be new enough on another machine.
    if let Some(dir) = which_bin_dir(req.kind.binary()) {
        candidates.push(dir);
    }

    let mut best: Option<RuntimeResolution> = None;
    let mut seen: Vec<PathBuf> = Vec::new();
    for bin_dir in candidates {
        // Dedup (several roots can resolve to the same dir).
        if seen.contains(&bin_dir) {
            continue;
        }
        seen.push(bin_dir.clone());

        let exe = bin_dir.join(req.kind.binary());
        let Some(found) = probe_version(&exe) else {
            continue;
        };
        if !found.satisfies(&req.min_version) {
            continue;
        }
        let better = match &best {
            None => true,
            Some(cur) => version_gt(&found, &cur.found_version),
        };
        if better {
            best = Some(RuntimeResolution {
                req: *req,
                bin_dir,
                found_version: found,
            });
        }
    }
    best
}

/// Convenience: detect a runtime requirement from a launch-error string and, if
/// one is found, locate a satisfying runtime. `None` when the error is not a
/// runtime-version error or no satisfying runtime exists. This is the one-call
/// entry the drive layer uses.
pub fn resolve_for_launch_error(error_text: &str) -> Option<RuntimeResolution> {
    let req = detect_requirement(error_text)?;
    locate_runtime(&req)
}

/// Build the `PATH` value for a child process by **prepending** `bin_dir` to
/// `current_path` (using the platform path separator), de-duplicating so the
/// same dir is not added twice. Pure: returns the new string, mutates nothing.
///
/// This is the whole mechanism — the child sees the satisfying runtime first on
/// its `PATH`, the parent/global env is never touched.
pub fn prepend_path(bin_dir: &Path, current_path: &str) -> String {
    let sep = if cfg!(windows) { ';' } else { ':' };
    let bin_str = bin_dir.to_string_lossy();
    // Already first? Leave it.
    if current_path
        .split(sep)
        .next()
        .map(|first| Path::new(first) == bin_dir)
        .unwrap_or(false)
    {
        return current_path.to_string();
    }
    if current_path.is_empty() {
        return bin_str.into_owned();
    }
    format!("{bin_str}{sep}{current_path}")
}

/// `Ordering`-free "is `a` strictly greater than `b`" for [`Version`].
fn version_gt(a: &Version, b: &Version) -> bool {
    (a.major, a.minor, a.patch) > (b.major, b.minor, b.patch)
}

/// Run `<exe> --version` and parse a [`Version`] from its output (stdout or
/// stderr — runtimes vary). Returns `None` if the exe is absent or prints
/// nothing version-shaped. Read-only probe; does not execute agent code.
fn probe_version(exe: &Path) -> Option<Version> {
    if !exe.exists() {
        return None;
    }
    let out = Command::new(exe).arg("--version").output().ok()?;
    let text = if !out.stdout.is_empty() {
        String::from_utf8_lossy(&out.stdout).into_owned()
    } else {
        String::from_utf8_lossy(&out.stderr).into_owned()
    };
    // node prints `v24.13.0`; python prints `Python 3.11.4`. Grab the first
    // version token from whichever line carries it.
    first_version_token(&text)
}

/// The bin directory currently holding `binary` on `PATH`, via `which`. Used so
/// a satisfying *global* install is considered alongside version-manager dirs.
/// Returns the *parent* directory of the resolved path. Fail-soft.
fn which_bin_dir(binary: &str) -> Option<PathBuf> {
    let out = Command::new("which").arg(binary).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&out.stdout);
    let path = path.trim();
    if path.is_empty() {
        return None;
    }
    Path::new(path).parent().map(|p| p.to_path_buf())
}

/// The user's home directory, fail-soft (no `dirs` dependency — this crate
/// resolves `$HOME` directly, matching `provision.rs`).
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Version parse + satisfies --------------------------------------

    #[test]
    fn version_parse_handles_v_prefix_and_partials() {
        assert_eq!(Version::parse("v24"), Some(Version::major_only(24)));
        assert_eq!(
            Version::parse("24.13.0"),
            Some(Version {
                major: 24,
                minor: 13,
                patch: 0
            })
        );
        assert_eq!(
            Version::parse("v24.13.0"),
            Some(Version {
                major: 24,
                minor: 13,
                patch: 0
            })
        );
        // Pre-release/build suffixes are ignored (leading core only).
        assert_eq!(
            Version::parse("24.13.0-nightly+abc"),
            Some(Version {
                major: 24,
                minor: 13,
                patch: 0
            })
        );
        // Garbage / no leading digit → None.
        assert_eq!(Version::parse("latest"), None);
        assert_eq!(Version::parse(""), None);
    }

    #[test]
    fn version_satisfies_is_component_wise() {
        let need = Version::major_only(24);
        assert!(Version::parse("v24.13.0").unwrap().satisfies(&need));
        assert!(Version::parse("v24.0.0").unwrap().satisfies(&need));
        assert!(Version::parse("v25.0.0").unwrap().satisfies(&need));
        // The exact real-world failure: v23.7.0 must NOT satisfy node ≥ 24.
        assert!(!Version::parse("v23.7.0").unwrap().satisfies(&need));

        // Minor/patch precision.
        let need_minor = Version {
            major: 3,
            minor: 11,
            patch: 0,
        };
        assert!(Version::parse("3.11.4").unwrap().satisfies(&need_minor));
        assert!(!Version::parse("3.10.9").unwrap().satisfies(&need_minor));
    }

    // ---- Requirement detection (data-driven, from real error text) ------

    #[test]
    fn detects_the_real_copilot_node_error() {
        // The EXACT string the GitHub Copilot CLI prints on a too-old node.
        let err = "GitHub Copilot CLI requires Node.js v24 or higher. \
                   Currently using v23.7.0.";
        let req = detect_requirement(err).expect("should detect a node requirement");
        assert_eq!(req.kind, RuntimeKind::Node);
        // It must grab the REQUIRED version (24), not the "currently using" 23.
        assert_eq!(req.min_version, Version::major_only(24));
    }

    #[test]
    fn detects_assorted_phrasings() {
        for (text, want_major) in [
            ("requires Node.js version 24 or newer", 24),
            ("needs Python >= 3.11", 3),
            ("Node.js 24+ is required to run this tool", 24),
            ("This CLI requires nodejs v20 or above", 20),
        ] {
            let req = detect_requirement(text).unwrap_or_else(|| panic!("missed: {text:?}"));
            assert_eq!(req.min_version.major, want_major, "for {text:?}");
        }
    }

    #[test]
    fn does_not_trigger_without_a_requirement_phrase() {
        // Mentions a runtime + a version but is NOT a version-requirement error
        // (e.g. a normal stack trace) → must not falsely detect.
        assert!(detect_requirement("TypeError at node v24 internal/modules").is_none());
        assert!(detect_requirement("Cannot find module 'foo'").is_none());
        // Auth / network failures must never be read as runtime failures.
        assert!(detect_requirement("Error: not signed in. Run `copilot login`.").is_none());
        assert!(detect_requirement("401 Unauthorized").is_none());
    }

    #[test]
    fn does_not_trigger_for_a_requirement_naming_no_known_runtime() {
        // A "requires X or higher" about something we don't model → None.
        assert!(detect_requirement("requires Rust 1.80 or higher").is_none());
    }

    #[test]
    fn picks_required_version_not_the_currently_using_one_even_when_lower() {
        // Belt-and-suspenders: the required token comes first (after the name),
        // and even though "v23.7.0" appears later we must return 24.
        let err = "requires Node.js v24 or higher. Currently using v23.7.0.";
        let req = detect_requirement(err).unwrap();
        assert_eq!(req.min_version, Version::major_only(24));
    }

    // ---- first_version_token --------------------------------------------

    #[test]
    fn first_version_token_skips_non_version_words() {
        assert_eq!(
            first_version_token(" version v24 or higher"),
            Some(Version::major_only(24))
        );
        assert_eq!(
            first_version_token(" >= 3.11 please"),
            Some(Version {
                major: 3,
                minor: 11,
                patch: 0
            })
        );
        assert_eq!(first_version_token("no numbers here"), None);
    }

    // ---- PATH prepend (the per-spawn mechanism) -------------------------

    #[test]
    fn prepend_path_puts_bin_dir_first() {
        let bin = Path::new("/Users/x/.nvm/versions/node/v24.13.0/bin");
        let got = prepend_path(bin, "/usr/local/bin:/usr/bin");
        assert_eq!(
            got,
            "/Users/x/.nvm/versions/node/v24.13.0/bin:/usr/local/bin:/usr/bin"
        );
        // The original (parent) PATH content is preserved after the prefix.
        assert!(got.ends_with(":/usr/local/bin:/usr/bin"));
    }

    #[test]
    fn prepend_path_is_idempotent_when_already_first() {
        let bin = Path::new("/opt/node/bin");
        let already = "/opt/node/bin:/usr/bin";
        assert_eq!(prepend_path(bin, already), already);
    }

    #[test]
    fn prepend_path_handles_empty_current() {
        let bin = Path::new("/opt/node/bin");
        assert_eq!(prepend_path(bin, ""), "/opt/node/bin");
    }

    // ---- proposal surface ------------------------------------------------

    #[test]
    fn proposal_line_names_agent_runtime_and_reassures_about_shell() {
        let res = RuntimeResolution {
            req: RuntimeReq {
                kind: RuntimeKind::Node,
                min_version: Version::major_only(24),
            },
            bin_dir: PathBuf::from("/Users/x/.nvm/versions/node/v24.13.0/bin"),
            found_version: Version {
                major: 24,
                minor: 13,
                patch: 0,
            },
        };
        let line = res.proposal_line("copilot");
        assert!(line.contains("copilot"));
        assert!(line.contains("Node.js v24.13.0"));
        // Reassurance that the global shell is untouched (per-spawn only).
        assert!(line.contains("your shell stays on its current node"));
    }

    // ---- locator (read-only, environment-dependent but fail-soft) -------

    #[test]
    fn locate_runtime_is_fail_soft_for_impossible_requirement() {
        // No installed node will satisfy v9999 → None, never a panic.
        let req = RuntimeReq {
            kind: RuntimeKind::Node,
            min_version: Version::major_only(9999),
        };
        assert!(locate_runtime(&req).is_none());
    }

    #[test]
    fn candidate_dirs_are_home_anchored_and_well_known() {
        // Sanity: every node candidate dir is under the fake HOME or a known
        // system prefix — never derived from arbitrary input.
        let home = Path::new("/tmp/fake-home");
        let dirs = RuntimeKind::Node.candidate_bin_dirs(home);
        for d in &dirs {
            let s = d.to_string_lossy();
            assert!(
                s.starts_with("/tmp/fake-home")
                    || s.starts_with("/opt/homebrew")
                    || s.starts_with("/usr/local"),
                "unexpected candidate dir: {s}"
            );
        }
    }
}

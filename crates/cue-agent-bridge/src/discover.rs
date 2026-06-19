//! Dynamic, system-wide agent discovery — finds **all** installed coding
//! agents (GUI + CLI), registry-driven plus a generic VS Code-fork detector so
//! unknown forks are still found.
//!
//! Every step is read-only (`std::fs` reads/metadata only) and fail-soft: a
//! malformed or inaccessible path for one agent degrades that agent and the
//! scan continues. Discovery never panics and never returns an error — the
//! worst case is an empty `Vec`.
//!
//! Paths are resolved per-OS via `#[cfg(target_os = "...")]`: macOS scans
//! `/Applications` + `~/Library/Application Support`, while Windows scans the
//! program dirs + `%APPDATA%`/`%LOCALAPPDATA%`. All base directories come from
//! environment variables (never hardcoded drive letters).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::capability::compute_capability;
use crate::registry::{all_binary_candidates, AgentEntry, KindTag, REGISTRY};
use crate::{AgentKind, Capability, DiscoveredAgent, SessionFormat, SessionStore};

/// Resolve the user's home directory without adding a dependency. Prefers
/// `HOME` (set on macOS/Linux and most Windows shells), falling back to
/// `USERPROFILE` on Windows where `HOME` is frequently unset. Returns `None`
/// if neither is set. Never panics.
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// Read an environment variable as a `PathBuf`, or `None` if it is unset.
/// Fail-soft helper: a missing var degrades the source that needs it rather
/// than panicking or hardcoding an absolute path. Used by the Windows base-dir
/// resolution; referenced under `test` so the macOS CI can exercise it.
#[cfg(any(target_os = "windows", test))]
fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key).map(PathBuf::from)
}

/// Per-OS base directories used to locate installed agents. Only the home dir
/// is meaningful on macOS; the Windows fields hold the roaming/local app-data
/// and program directories that GUI agents install into and write data under.
///
/// Every field is `Option`/`Vec` because the backing env var may be unset —
/// discovery degrades the affected source and continues.
#[derive(Debug, Clone, Default)]
struct BaseDirs {
    /// `$HOME` / `%USERPROFILE%`. Root of dotfile CLI footprints on every OS.
    home: Option<PathBuf>,
    /// `%APPDATA%` (roaming). VS Code-family `User/globalStorage` lives here on
    /// Windows. `None` on non-Windows.
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    app_data: Option<PathBuf>,
    /// `%LOCALAPPDATA%` (machine-local). Holds `Programs\<App>` per-user
    /// installs and some local agent state. `None` on non-Windows.
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    local_app_data: Option<PathBuf>,
    /// Program install roots: `ProgramFiles`, `ProgramFiles(x86)`, and
    /// `%LOCALAPPDATA%\Programs`. Empty on non-Windows.
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    program_dirs: Vec<PathBuf>,
}

impl BaseDirs {
    /// Resolve the base dirs for the current OS from the environment. macOS
    /// only needs `home`; Windows additionally resolves the app-data and
    /// program dirs. All resolution is fail-soft (missing var → `None`/skipped).
    fn resolve() -> Self {
        Self::for_platform(home_dir())
    }

    /// Build base dirs for an explicit `home`, resolving the Windows-only roots
    /// from the environment. Shared by [`BaseDirs::resolve`] and the
    /// home-rooted discovery entry point.
    fn for_platform(home: Option<PathBuf>) -> Self {
        #[cfg(target_os = "windows")]
        {
            let app_data = env_path("APPDATA");
            let local_app_data = env_path("LOCALAPPDATA");
            let program_dirs = windows_program_dirs(local_app_data.as_deref());
            BaseDirs {
                home,
                app_data,
                local_app_data,
                program_dirs,
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            BaseDirs {
                home,
                ..BaseDirs::default()
            }
        }
    }
}

/// Windows program-install roots, in scan order: `%ProgramFiles%`,
/// `%ProgramFiles(x86)%`, then `%LOCALAPPDATA%\Programs` (where most Electron
/// agents install per-user). Resolved purely from env vars and the supplied
/// local-app-data dir — never hardcodes `C:\`. Missing vars are skipped.
///
/// Pure over its `local_app_data` argument so it is unit-testable on any OS.
/// Compiled on Windows (where it is used) and under `test` (so the macOS CI can
/// exercise the path logic), but not in macOS release builds.
#[cfg(any(target_os = "windows", test))]
fn windows_program_dirs(local_app_data: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(p) = env_path("ProgramFiles") {
        dirs.push(p);
    }
    if let Some(p) = env_path("ProgramFiles(x86)") {
        dirs.push(p);
    }
    if let Some(local) = local_app_data {
        dirs.push(local.join("Programs"));
    }
    dirs
}

/// Discover every coding agent installed on this machine.
///
/// Combines four passes — PATH binaries, app installs, registry data-dirs, and a
/// generic VS Code-fork sweep — then deduplicates so an agent found multiple
/// ways becomes one entry with combined evidence. Each disk-scanning pass runs
/// the path set appropriate to the current OS (macOS app bundles vs Windows
/// program/app-data dirs), gated at compile time.
pub fn discover_agents() -> Vec<DiscoveredAgent> {
    let mut acc = Accumulator::default();

    scan_path_binaries(&mut acc);
    let bases = BaseDirs::resolve();
    scan_disk(&mut acc, &bases);

    acc.finish()
}

/// Discover agents rooted at an explicit `home` directory, skipping the `PATH`
/// scan. Used by integration tests to run discovery against synthetic fixture
/// trees without reading the real user's home or mutating global env.
///
/// The Windows-only app-data/program roots are still resolved from the
/// environment (they live outside `home`); on macOS only the `home`-rooted
/// scans run, so behavior against fixture trees is unchanged.
pub fn discover_in_home(home: &Path) -> Vec<DiscoveredAgent> {
    let mut acc = Accumulator::default();
    let bases = BaseDirs::for_platform(Some(home.to_path_buf()));
    scan_disk(&mut acc, &bases);
    acc.finish()
}

/// Run every disk-scanning pass for the given base dirs. Centralizes the
/// per-OS pass selection so both entry points stay in sync.
fn scan_disk(acc: &mut Accumulator, bases: &BaseDirs) {
    if let Some(home) = bases.home.as_deref() {
        scan_app_bundles(acc, home);
        scan_registry_data_dirs(acc, home);
    }
    scan_vscode_forks(acc, bases);

    #[cfg(target_os = "windows")]
    {
        scan_windows_app_installs(acc, bases);
        scan_windows_app_data(acc, bases);
    }
}

/// Accumulates evidence keyed by a stable identity so duplicates merge.
#[derive(Default)]
struct Accumulator {
    by_key: BTreeMap<String, DiscoveredAgent>,
}

impl Accumulator {
    /// Stable dedup key for an agent kind. `Other(label)` keys on its label so
    /// two distinct unknown forks stay separate.
    fn key_for(kind: &AgentKind) -> String {
        match kind {
            AgentKind::Other(label) => format!("other:{label}"),
            other => format!("{other:?}"),
        }
    }

    /// Merge a new piece of evidence into the accumulator.
    fn add(
        &mut self,
        kind: AgentKind,
        evidence: PathBuf,
        connector_config: Option<PathBuf>,
        session_store: Option<SessionStore>,
    ) {
        let key = Self::key_for(&kind);
        let entry = self.by_key.entry(key).or_insert_with(|| DiscoveredAgent {
            kind,
            install_evidence: Vec::new(),
            capability: Capability::ReadOnly,
            connector_config_path: None,
            session_store: None,
        });

        if !entry.install_evidence.contains(&evidence) {
            entry.install_evidence.push(evidence);
        }
        if entry.connector_config_path.is_none() {
            entry.connector_config_path = connector_config;
        }
        if entry.session_store.is_none() {
            entry.session_store = session_store;
        }
    }

    /// Finalize: compute capability for each agent and return a stable-ordered
    /// `Vec`.
    fn finish(self) -> Vec<DiscoveredAgent> {
        self.by_key
            .into_values()
            .map(|mut a| {
                a.capability = compute_capability(&a);
                a
            })
            .collect()
    }
}

/// Pass 1 — scan `PATH` for registry binary candidates.
fn scan_path_binaries(acc: &mut Accumulator) {
    let path_var = match std::env::var_os("PATH") {
        Some(p) => p,
        None => return,
    };
    let dirs: Vec<PathBuf> = std::env::split_paths(&path_var).collect();

    for (entry, binary) in all_binary_candidates() {
        if let Some(found) = find_on_path(&dirs, binary) {
            acc.add(entry.kind_tag.to_agent_kind(), found, None, None);
        }
    }
}

/// Find an executable `name` in any of `dirs` (read-only existence check).
fn find_on_path(dirs: &[PathBuf], name: &str) -> Option<PathBuf> {
    for dir in dirs {
        let candidate = dir.join(name);
        // metadata() does not execute; a readable regular-file is enough.
        if let Ok(meta) = std::fs::metadata(&candidate) {
            if meta.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Pass 2 — scan `/Applications` and `~/Applications` for known `.app` bundles.
///
/// macOS-only path shape: `.app` bundles and the `/Applications` roots do not
/// exist on Windows (the Windows analog is [`scan_windows_app_installs`]). On
/// non-Windows targets the original behavior is preserved unchanged, so Linux
/// is unaffected.
#[cfg(not(target_os = "windows"))]
fn scan_app_bundles(acc: &mut Accumulator, home: &Path) {
    let roots = [PathBuf::from("/Applications"), home.join("Applications")];
    for entry in REGISTRY {
        for bundle in entry.app_bundles {
            for root in &roots {
                let candidate = root.join(bundle);
                if path_exists(&candidate) {
                    acc.add(entry.kind_tag.to_agent_kind(), candidate, None, None);
                }
            }
        }
    }
}

/// Windows has no `.app` bundles; app installs are handled by
/// [`scan_windows_app_installs`]. This no-op keeps [`scan_disk`] uniform.
#[cfg(target_os = "windows")]
fn scan_app_bundles(_acc: &mut Accumulator, _home: &Path) {}

/// Pass 3 — scan registry data-dir globs under `$HOME` for footprints.
///
/// On macOS every glob (dotfiles **and** `Library/Application Support/<App>`)
/// is joined to `$HOME`, unchanged. On Windows only the HOME-relative dotfile
/// globs apply here (`%USERPROFILE%\.claude`, …); the `Library/Application
/// Support/<App>` rows are macOS-only and their Windows equivalent is scanned
/// under `%APPDATA%` by [`scan_windows_app_data`].
fn scan_registry_data_dirs(acc: &mut Accumulator, home: &Path) {
    for entry in REGISTRY {
        #[cfg(target_os = "windows")]
        let globs: Vec<&str> = entry.home_relative_globs().collect();
        #[cfg(not(target_os = "windows"))]
        let globs: Vec<&str> = entry.data_dir_globs.to_vec();

        for glob in globs {
            // Glob strings use `/` separators; split so each component is joined
            // natively (`/` on Unix, `\` on Windows) — never a literal `/`
            // baked into a Windows path.
            let dir = join_glob(home, glob);
            if !path_exists(&dir) {
                continue;
            }
            let connector_config = locate_connector_config(&dir);
            let session_store = locate_session_store(&dir, entry);
            acc.add(
                entry.kind_tag.to_agent_kind(),
                dir,
                connector_config,
                session_store,
            );
        }
    }
}

/// Join a `/`-separated registry glob onto `base`, splitting on `/` so each
/// segment is appended with the native separator. `Path::join` treats `/`
/// literally inside a single component on Windows, so multi-segment globs like
/// `.gemini/antigravity` must be split to resolve correctly there. Pure and
/// OS-agnostic, so it is unit-testable on any platform.
fn join_glob(base: &Path, glob: &str) -> PathBuf {
    let mut out = base.to_path_buf();
    for seg in glob.split('/').filter(|s| !s.is_empty()) {
        out.push(seg);
    }
    out
}

/// Pass 4 — generic VS Code-fork detector. Any directory under a platform
/// "support root" that contains `User/globalStorage/state.vscdb` is treated as
/// a VS Code-family agent, even if it is not a registry row, so unknown forks
/// are discovered with no code change.
///
/// Support roots are OS-specific: on macOS `~/Library/Application Support`; on
/// Windows `%APPDATA%` and `%LOCALAPPDATA%` (forks roam under either). The
/// per-directory detection logic is shared via [`scan_vscode_support_root`].
fn scan_vscode_forks(acc: &mut Accumulator, bases: &BaseDirs) {
    let known = known_vscode_dirs();
    for root in vscode_support_roots(bases) {
        scan_vscode_support_root(acc, &root, &known);
    }
}

/// The directories whose immediate children are scanned for the VS Code-fork
/// footprint, for the current OS. Missing roots (absent env var / no home) are
/// simply not returned.
fn vscode_support_roots(bases: &BaseDirs) -> Vec<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        // VS Code-family user data roams under %APPDATA%; some forks also keep
        // a copy under %LOCALAPPDATA%. Scan both when present.
        [bases.app_data.as_deref(), bases.local_app_data.as_deref()]
            .into_iter()
            .flatten()
            .map(Path::to_path_buf)
            .collect()
    }
    #[cfg(not(target_os = "windows"))]
    {
        match bases.home.as_deref() {
            Some(home) => vec![join_glob(home, "Library/Application Support")],
            None => Vec::new(),
        }
    }
}

/// Registry dir-names that already map to a known kind, paired with that kind.
/// On macOS these come from the `Library/Application Support/<App>` globs; on
/// Windows from the `app_data_windows` names. Unknown dirs become
/// `AgentKind::Other(name)`.
fn known_vscode_dirs() -> Vec<(&'static str, KindTag)> {
    #[cfg(target_os = "windows")]
    {
        REGISTRY
            .iter()
            .flat_map(|e| e.app_data_windows.iter().map(move |n| (*n, e.kind_tag)))
            .collect()
    }
    #[cfg(not(target_os = "windows"))]
    {
        use crate::registry::MACOS_APP_SUPPORT_PREFIX;
        REGISTRY
            .iter()
            .filter_map(|e| {
                e.data_dir_globs
                    .iter()
                    .find_map(|g| g.strip_prefix(MACOS_APP_SUPPORT_PREFIX))
                    .map(|name| (name, e.kind_tag))
            })
            .collect()
    }
}

/// Enumerate one support `root`'s immediate children and record any that carry
/// the VS Code-family footprint. Shared by every OS so the detection rule is
/// identical regardless of where the root lives. Fail-soft: an unreadable root
/// is skipped silently.
fn scan_vscode_support_root(acc: &mut Accumulator, root: &Path, known: &[(&str, KindTag)]) {
    let read = match std::fs::read_dir(root) {
        Ok(r) => r,
        Err(_) => return, // missing/unreadable → degrade silently, keep scanning
    };

    for child in read.flatten() {
        let dir = child.path();
        let vscdb = vscdb_path(&dir);
        // Require a genuine SQLite store (read-only probe), not just a
        // same-named file, so stray/corrupt files do not mint a fake agent.
        if !path_exists(&vscdb) || !probe_sqlite_store(&vscdb) {
            continue;
        }

        let dir_name = dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let kind = known
            .iter()
            .find(|(name, _)| *name == dir_name)
            .map(|(_, tag)| tag.to_agent_kind())
            .unwrap_or_else(|| AgentKind::Other(dir_name.clone()));

        let connector_config = locate_connector_config(&dir.join("User"));
        // Pick the reader by what the store ACTUALLY is, not by "it's a .vscdb":
        //   - `cursorDiskKV` table present → Cursor's rich composer store →
        //     `SqliteVscdb` (the only format `VscdbReader` can read).
        //   - else it's a plain VS Code-family store (`ItemTable` only —
        //     Antigravity, VS Code Insiders, etc.); its chats live as
        //     `User/workspaceStorage/<hash>/chatSessions/*.json` → `JsonFiles`.
        //   - else no readable session store (still discovered for connectors).
        // Without this gate, every fork got `SqliteVscdb` and non-Cursor forks
        // threw `no such table: cursorDiskKV` on every session read.
        let workspace_storage = dir.join("User").join("workspaceStorage");
        let session_store = if sqlite_has_table(&vscdb, "cursorDiskKV") {
            Some(SessionStore {
                path: vscdb,
                format: SessionFormat::SqliteVscdb,
            })
        } else if path_exists(&workspace_storage) {
            Some(SessionStore {
                path: workspace_storage,
                format: SessionFormat::JsonFiles,
            })
        } else {
            None
        };
        acc.add(kind, dir, connector_config, session_store);
    }
}

/// The VS Code-family session store path inside an app data dir:
/// `<dir>/User/globalStorage/state.vscdb`, built with native separators.
fn vscdb_path(dir: &Path) -> PathBuf {
    join_glob(dir, "User/globalStorage/state.vscdb")
}

/// Look for a connector config file for an agent's data dir.
///
/// Candidates, in priority order:
/// - a sibling `<dirname>.json` next to the data dir (Claude Code keeps its MCP
///   servers in `~/.claude.json`, a sibling of the `~/.claude` data dir);
/// - the common config names at the dir root and under a `User/` subdir.
///
/// A candidate that actually declares MCP servers wins over one that does not
/// (so an empty `settings.json` never shadows a populated `~/.claude.json`).
fn locate_connector_config(dir: &Path) -> Option<PathBuf> {
    // Common MCP config filenames across agents. `mcp-config.json` (hyphen) is
    // GitHub Copilot CLI's user MCP file (`~/.copilot/mcp-config.json`, per
    // `copilot --help`'s `--additional-mcp-config`); the others cover
    // Cursor/Gemini (`mcp.json`/`mcp_config.json`) and VS Code (`settings.json`).
    const NAMES: &[&str] = &[
        "mcp.json",
        "mcp_config.json",
        "mcp-config.json",
        "settings.json",
    ];

    let mut candidates: Vec<PathBuf> = Vec::new();
    // Sibling `<dirname>.json` (e.g. ~/.claude → ~/.claude.json).
    if let Some(name) = dir.file_name().and_then(|n| n.to_str()) {
        if let Some(parent) = dir.parent() {
            candidates.push(parent.join(format!("{name}.json")));
        }
    }
    for base in [dir.to_path_buf(), dir.join("User")] {
        for name in NAMES {
            candidates.push(base.join(name));
        }
    }

    let existing: Vec<PathBuf> = candidates.into_iter().filter(|c| path_exists(c)).collect();
    // Prefer a config that actually declares MCP servers.
    existing
        .iter()
        .find(|c| config_has_mcp_servers(c))
        .or_else(|| existing.first())
        .cloned()
}

/// Cheap check: does this JSON(C) config declare any MCP servers? Read-only,
/// fail-soft — a missing/unreadable/malformed file simply returns `false`.
fn config_has_mcp_servers(path: &Path) -> bool {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return false;
    };
    // Substring check is enough to rank candidates; the real parse happens in
    // `connectors::read_connectors`. Matches `mcpServers` or `servers` keys.
    raw.contains("\"mcpServers\"") || raw.contains("\"servers\"")
}

/// Look for a session store inside an agent's data dir, matching the registry's
/// declared format for that agent. Multi-segment relative paths are joined with
/// [`join_glob`] so they resolve natively on Windows as well as macOS.
fn locate_session_store(dir: &Path, entry: &AgentEntry) -> Option<SessionStore> {
    let format = entry.session_format?;
    let path = match format {
        SessionFormat::Jsonl => join_glob(dir, entry.jsonl_subdir),
        SessionFormat::SqliteVscdb => vscdb_path(dir),
        SessionFormat::JsonFiles => join_glob(dir, "User/workspaceStorage"),
        // The store path points at the plaintext INDEX FILE itself, never the
        // data-dir root (which can hold credential files like settings.json /
        // oauth_creds.json). The reader derives `conversations/` and `brain/`
        // from the index file's parent. Pointing at a specific file (not the
        // dir) keeps this reader from ever enumerating the secret-bearing root.
        SessionFormat::AntigravityIndex => join_glob(dir, "agyhub_summaries_proto.pb"),
        // The Claude-app index lives at `<data_dir>/<subdir>` (the per-mode
        // session-index folder); the reader walks `<account>/<workspace>/`
        // beneath it. `jsonl_subdir` carries the subdir name for this row.
        SessionFormat::ClaudeAppIndex => join_glob(dir, entry.jsonl_subdir),
    };
    path_exists(&path).then_some(SessionStore { path, format })
}

/// Windows app-install candidate paths for one registry entry, given the
/// program dirs to scan. For each `app_dirs_windows` name we emit both the
/// install **directory** (`<programdir>\<App>`, how Electron apps install) and
/// the bare `<programdir>\<name>.exe`. Pure over `program_dirs` so it is
/// unit-testable on any OS; existence is checked by the caller.
#[cfg(any(target_os = "windows", test))]
fn windows_app_install_candidates(entry: &AgentEntry, program_dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for dir in program_dirs {
        for name in entry.app_dirs_windows {
            out.push(dir.join(name));
            out.push(dir.join(format!("{name}.exe")));
        }
    }
    out
}

/// Windows `%APPDATA%`-rooted data-dir candidates for one registry entry, given
/// the app-data base. Emits `<app_data>\<App>` for each `app_data_windows`
/// name. Pure over `app_data` so it is unit-testable on any OS.
#[cfg(any(target_os = "windows", test))]
fn windows_app_data_candidates(entry: &AgentEntry, app_data: &Path) -> Vec<PathBuf> {
    entry
        .app_data_windows
        .iter()
        .map(|name| app_data.join(name))
        .collect()
}

/// Windows-only Pass 2 — scan the program dirs (`ProgramFiles`,
/// `ProgramFiles(x86)`, `%LOCALAPPDATA%\Programs`) for known agent install dirs
/// and `.exe`s. Records any that exist as install evidence (no session/connector
/// data — those come from the app-data pass).
#[cfg(target_os = "windows")]
fn scan_windows_app_installs(acc: &mut Accumulator, bases: &BaseDirs) {
    if bases.program_dirs.is_empty() {
        return;
    }
    for entry in REGISTRY {
        for candidate in windows_app_install_candidates(entry, &bases.program_dirs) {
            if path_exists(&candidate) {
                acc.add(entry.kind_tag.to_agent_kind(), candidate, None, None);
            }
        }
    }
}

/// Windows-only Pass 3b — scan `%APPDATA%\<App>` data dirs declared by the
/// registry (`app_data_windows`) for footprints, mirroring the macOS
/// `Library/Application Support/<App>` scan. Locates connector config and the
/// declared session store, exactly like [`scan_registry_data_dirs`].
#[cfg(target_os = "windows")]
fn scan_windows_app_data(acc: &mut Accumulator, bases: &BaseDirs) {
    let app_data = match bases.app_data.as_deref() {
        Some(p) => p,
        None => return,
    };
    for entry in REGISTRY {
        for dir in windows_app_data_candidates(entry, app_data) {
            if !path_exists(&dir) {
                continue;
            }
            let connector_config = locate_connector_config(&dir);
            let session_store = locate_session_store(&dir, entry);
            acc.add(
                entry.kind_tag.to_agent_kind(),
                dir,
                connector_config,
                session_store,
            );
        }
    }
}

/// Read-only existence check that never panics on permission errors.
fn path_exists(path: &Path) -> bool {
    std::fs::metadata(path).is_ok()
}

/// Probe a `state.vscdb` to confirm it is a genuine, openable SQLite store —
/// strictly **read-only** and **immutable** (never writes, never locks a
/// possibly-live store). Returns `true` if the file opens as SQLite.
///
/// This is the only SQLite access in Slice 1: a shape probe, not a decoder
/// (the per-app session decoders land in Slice 3). It is fail-soft — any error
/// (missing, corrupt, locked) yields `false`, never a panic.
pub fn probe_sqlite_store(path: &Path) -> bool {
    use rusqlite::OpenFlags;
    // `immutable=1` + READ_ONLY: we promise SQLite the file will not change
    // underneath us and we will not write to it. Safe against a live store.
    let uri = format!("file:{}?immutable=1&mode=ro", path.display());
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI;
    match rusqlite::Connection::open_with_flags(&uri, flags) {
        Ok(conn) => {
            // Touch the schema to confirm it is really SQLite, then drop.
            conn.prepare("SELECT name FROM sqlite_master LIMIT 1")
                .and_then(|mut stmt| stmt.query([]).map(|_| ()))
                .is_ok()
        }
        Err(_) => false,
    }
}

/// Whether a SQLite store contains a table named `table`. Read-only + immutable
/// (safe against a live store). Used to tell a **Cursor** `state.vscdb` (which
/// has the proprietary `cursorDiskKV` table) apart from a plain VS Code-family
/// `state.vscdb` (only `ItemTable`). Antigravity / VS Code Insiders / other forks
/// all ship the plain shape, so blindly assigning the Cursor reader to them makes
/// the session read throw `no such table: cursorDiskKV`.
pub fn sqlite_has_table(path: &Path, table: &str) -> bool {
    use rusqlite::OpenFlags;
    let uri = format!("file:{}?immutable=1&mode=ro", path.display());
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI;
    match rusqlite::Connection::open_with_flags(&uri, flags) {
        Ok(conn) => conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1 LIMIT 1",
                [table],
                |_| Ok(()),
            )
            .is_ok(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dedup_merges_evidence() {
        let mut acc = Accumulator::default();
        acc.add(
            AgentKind::Cursor,
            PathBuf::from("/usr/local/bin/cursor-agent"),
            None,
            None,
        );
        acc.add(
            AgentKind::Cursor,
            PathBuf::from("/Users/x/.cursor"),
            Some(PathBuf::from("/Users/x/.cursor/mcp.json")),
            None,
        );
        let out = acc.finish();
        assert_eq!(out.len(), 1, "same kind must merge to one entry");
        assert_eq!(out[0].install_evidence.len(), 2);
        // CLI evidence present → drivable.
        assert_eq!(out[0].capability, Capability::Drive);
        assert!(out[0].connector_config_path.is_some());
    }

    #[test]
    fn locate_connector_config_finds_copilot_hyphenated_name() {
        // GitHub Copilot CLI keeps user MCP servers in `mcp-config.json` (hyphen),
        // not `mcp_config.json`. Discovery must find it, or Copilot connectors are
        // invisible (the gap this regression test guards).
        let dir = tempfile::tempdir().expect("tempdir");
        let copilot = dir.path().join(".copilot");
        std::fs::create_dir_all(&copilot).expect("mkdir");
        let cfg = copilot.join("mcp-config.json");
        std::fs::write(
            &cfg,
            r#"{"mcpServers":{"perplexity":{"command":"npx","args":["-y","@perplexity-ai/mcp-server"]}}}"#,
        )
        .expect("write");

        let found = locate_connector_config(&copilot).expect("must locate mcp-config.json");
        assert_eq!(
            found, cfg,
            "Copilot's hyphenated mcp-config.json must be found"
        );
        assert!(
            config_has_mcp_servers(&found),
            "the located config declares MCP servers"
        );
    }

    #[test]
    fn test_distinct_other_forks_stay_separate() {
        let mut acc = Accumulator::default();
        acc.add(
            AgentKind::Other("Foo".into()),
            PathBuf::from("/a"),
            None,
            None,
        );
        acc.add(
            AgentKind::Other("Bar".into()),
            PathBuf::from("/b"),
            None,
            None,
        );
        assert_eq!(acc.finish().len(), 2);
    }

    #[test]
    fn test_duplicate_evidence_path_not_double_counted() {
        let mut acc = Accumulator::default();
        let p = PathBuf::from("/usr/local/bin/claude");
        acc.add(AgentKind::ClaudeCode, p.clone(), None, None);
        acc.add(AgentKind::ClaudeCode, p, None, None);
        let out = acc.finish();
        assert_eq!(out[0].install_evidence.len(), 1);
    }

    #[test]
    fn test_discover_never_panics() {
        // Smoke test against the real machine: must return without panicking,
        // regardless of what is or isn't installed.
        let _ = discover_agents();
    }

    #[test]
    fn test_path_exists_handles_missing() {
        assert!(!path_exists(Path::new("/nonexistent/path/zzz/qqq")));
    }

    // ----- Cross-platform path logic (Windows pieces, tested on any OS) -----

    fn entry(kind: KindTag) -> &'static AgentEntry {
        REGISTRY
            .iter()
            .find(|e| e.kind_tag == kind)
            .expect("registry row")
    }

    /// Build a minimal valid SQLite `state.vscdb` at `path` so the read-only
    /// probe in fork detection succeeds. Synthetic — no real data.
    /// A faithful **Cursor** `state.vscdb`: both the VS Code-standard `ItemTable`
    /// AND Cursor's proprietary `cursorDiskKV` (the table `VscdbReader` queries).
    /// The fork detector now keys the reader off `cursorDiskKV`'s presence, so a
    /// Cursor fixture must include it to be assigned the `SqliteVscdb` reader.
    fn build_synthetic_vscdb(path: &Path) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create vscdb parent");
        }
        let conn = rusqlite::Connection::open(path).expect("create synthetic vscdb");
        conn.execute_batch(
            "CREATE TABLE ItemTable (key TEXT, value BLOB);
             INSERT INTO ItemTable VALUES ('synthetic', 'x');
             CREATE TABLE cursorDiskKV (key TEXT, value BLOB);",
        )
        .expect("seed synthetic vscdb");
    }

    /// A plain VS Code-family `state.vscdb` (only `ItemTable`, NO `cursorDiskKV`)
    /// — what Antigravity / VS Code Insiders / non-Cursor forks ship. Used to
    /// prove the detector does NOT assign them the Cursor reader.
    fn build_plain_vscode_vscdb(path: &Path) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create vscdb parent");
        }
        let conn = rusqlite::Connection::open(path).expect("create plain vscdb");
        conn.execute_batch(
            "CREATE TABLE ItemTable (key TEXT, value BLOB);
             INSERT INTO ItemTable VALUES ('synthetic', 'x');",
        )
        .expect("seed plain vscdb");
    }

    #[test]
    fn test_join_glob_splits_segments_natively() {
        let base = Path::new("/base");
        let joined = join_glob(base, "User/globalStorage/state.vscdb");
        // Equivalent to pushing each segment; no literal "/" component remains.
        let expected: PathBuf = base.join("User").join("globalStorage").join("state.vscdb");
        assert_eq!(joined, expected);
        assert_eq!(joined.components().count(), expected.components().count());
    }

    #[test]
    fn test_join_glob_ignores_empty_and_trailing_segments() {
        assert_eq!(join_glob(Path::new("/b"), "x/"), Path::new("/b").join("x"));
        assert_eq!(join_glob(Path::new("/b"), ""), Path::new("/b"));
    }

    #[test]
    fn test_vscdb_path_is_user_globalstorage_state() {
        let got = vscdb_path(Path::new("/data/Cursor"));
        assert!(got.ends_with("User/globalStorage/state.vscdb"));
        assert!(got.starts_with("/data/Cursor"));
    }

    // ----- Credential-safety scoping (security invariant, design §6) -----
    //
    // The agent's data-dir ROOT holds auth/credential files (e.g. Gemini keeps
    // `settings.json`, `oauth_creds.json`, `google_accounts.json` directly in
    // `~/.gemini`). Session reading must NEVER be rooted at that directory: a
    // reader walking the data-dir root could enumerate those secret files. The
    // session store must instead point at a transcript-only SUBDIR. These tests
    // pin that scoping so a future registry edit cannot silently widen it back
    // to the secret-bearing root.

    #[test]
    fn test_gemini_cli_has_no_session_store_so_data_dir_root_is_never_walked() {
        // The Gemini CLI keeps its secrets (settings.json with MCP env tokens,
        // oauth_creds.json, google_accounts.json) directly in its data-dir root.
        // It declares `session_format: None`, so NO session store is ever built
        // for it — the root is never handed to a SessionReader to enumerate.
        let dir = tempfile::tempdir().expect("tempdir");
        // Seed the kind of secret-bearing files that live in ~/.gemini, plus a
        // stray transcript-looking file, to prove none of them get picked up.
        std::fs::write(dir.path().join("settings.json"), r#"{"mcpServers":{}}"#)
            .expect("write settings");
        std::fs::write(dir.path().join("oauth_creds.json"), "{}").expect("write creds");
        std::fs::write(dir.path().join("session.jsonl"), "{}").expect("write stray");

        let store = locate_session_store(dir.path(), entry(KindTag::Gemini));
        assert!(
            store.is_none(),
            "Gemini CLI must yield NO session store (session_format=None); \
             got a store that would expose the data-dir root: {store:?}"
        );
    }

    #[test]
    fn test_antigravity_session_store_is_scoped_below_data_dir_root() {
        // Antigravity shares ~/.gemini/antigravity and IS session-read. Its store
        // path must point at the plaintext INDEX FILE (agyhub_summaries_proto.pb),
        // never the data-dir root that holds sibling secrets. The reader derives
        // conversations/ and brain/ from the index file's parent.
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        // Secret-bearing files at the data-dir root (must NEVER be the store path).
        std::fs::write(root.join("settings.json"), r#"{"mcpServers":{}}"#).expect("write settings");
        std::fs::write(root.join("oauth_creds.json"), "{}").expect("write creds");
        // The real index location.
        let index = root.join("agyhub_summaries_proto.pb");
        std::fs::write(&index, b"\x0a\x00").expect("write index");

        let store = locate_session_store(root, entry(KindTag::Antigravity))
            .expect("antigravity store exists when the index file is present");

        // Scoped strictly BELOW the root: equal to the index file, a strict
        // descendant of the root (so the root itself is never the store path).
        assert_eq!(store.path, index, "store must point at the index file");
        assert_eq!(store.format, SessionFormat::AntigravityIndex);
        assert!(
            store.path.starts_with(root) && store.path != root,
            "store path must be a strict descendant of the data-dir root, not the \
             root itself (which holds settings.json / oauth_creds.json): {:?}",
            store.path
        );
        assert_eq!(
            store.path.file_name().and_then(|n| n.to_str()),
            Some("agyhub_summaries_proto.pb"),
            "store leaf must be the index file, not the secret-bearing root"
        );
    }

    #[test]
    fn test_no_session_store_resolves_to_a_bare_data_dir_root() {
        // Cross-agent guarantee: for EVERY registry row that is session-read, the
        // resolved store path is a strict descendant of the data-dir root, never
        // the root itself. This is the general form of the Gemini-specific guard
        // above — it catches any future row whose subdir is accidentally "" or
        // otherwise collapses onto the root where credential files can live.
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        // Create the union of subdirs the readers expect, so locate_session_store
        // resolves a real path for each format instead of bailing on existence.
        for sub in ["brain", "projects", "sessions", "session-state", "tmp"] {
            std::fs::create_dir_all(root.join(sub)).expect("mkdir sub");
        }
        std::fs::create_dir_all(root.join("conversations")).expect("mkdir conv");
        std::fs::create_dir_all(root.join("User/workspaceStorage")).expect("mkdir ws");
        build_synthetic_vscdb(&root.join("User/globalStorage/state.vscdb"));
        // Antigravity's store is the index FILE; create it so that row resolves
        // (and is verified to not collapse onto the root).
        std::fs::write(root.join("agyhub_summaries_proto.pb"), b"\x0a\x00").expect("write index");

        for row in REGISTRY {
            if let Some(store) = locate_session_store(root, row) {
                assert!(
                    store.path != root,
                    "{:?}: session store resolved to the bare data-dir root, \
                     which can hold credential files — must be a subdir",
                    row.kind_tag
                );
                assert!(
                    store.path.starts_with(root),
                    "{:?}: store path escaped the data-dir root: {:?}",
                    row.kind_tag,
                    store.path
                );
            }
        }
    }

    #[test]
    fn test_windows_program_dirs_appends_programs_under_local() {
        // ProgramFiles* env vars are normally unset on the CI host, so the only
        // deterministic entry comes from the injected local-app-data dir.
        let local = PathBuf::from("/fake/Local");
        let dirs = windows_program_dirs(Some(&local));
        assert!(
            dirs.contains(&local.join("Programs")),
            "must include %LOCALAPPDATA%\\Programs"
        );
    }

    #[test]
    fn test_windows_program_dirs_skips_programs_when_local_absent() {
        // With no local-app-data and (on CI) no ProgramFiles vars, the result
        // contains no Programs dir — never a hardcoded C:\ path.
        let dirs = windows_program_dirs(None);
        assert!(
            !dirs.iter().any(|d| d.ends_with("Programs")),
            "no Programs dir without %LOCALAPPDATA%"
        );
    }

    #[test]
    fn test_windows_app_install_candidates_dir_and_exe() {
        let program_dirs = vec![PathBuf::from("/Local/Programs")];
        let got = windows_app_install_candidates(entry(KindTag::Cursor), &program_dirs);
        // Cursor declares app_dirs_windows = ["cursor", "Cursor"] → 2 names × 2
        // shapes (dir + .exe) = 4 candidates.
        assert!(got.contains(&PathBuf::from("/Local/Programs/Cursor")));
        assert!(got.contains(&PathBuf::from("/Local/Programs/Cursor.exe")));
        assert!(got.contains(&PathBuf::from("/Local/Programs/cursor.exe")));
        assert_eq!(got.len(), entry(KindTag::Cursor).app_dirs_windows.len() * 2);
    }

    #[test]
    fn test_windows_app_install_candidates_empty_without_program_dirs() {
        let got = windows_app_install_candidates(entry(KindTag::Cursor), &[]);
        assert!(got.is_empty(), "no program dirs → no candidates");
    }

    #[test]
    fn test_windows_app_data_candidates_join_appdata() {
        let app_data = Path::new("/Roaming");
        let got = windows_app_data_candidates(entry(KindTag::VsCode), app_data);
        // VS Code declares app_data_windows = ["Code"].
        assert_eq!(got, vec![PathBuf::from("/Roaming/Code")]);
    }

    #[test]
    fn test_windows_app_data_candidates_empty_for_cli_only_agent() {
        // Codex is a dotfile CLI with no %APPDATA% VS Code tree.
        let got = windows_app_data_candidates(entry(KindTag::Codex), Path::new("/Roaming"));
        assert!(got.is_empty());
    }

    #[test]
    fn test_base_dirs_for_platform_carries_home() {
        let bases = BaseDirs::for_platform(Some(PathBuf::from("/home/me")));
        assert_eq!(bases.home.as_deref(), Some(Path::new("/home/me")));
    }

    #[test]
    fn test_fork_detector_handles_appdata_style_tree() {
        // Simulate a Windows %APPDATA% layout: <root>/<App>/User/globalStorage/
        // state.vscdb, with NO macOS "Library/Application Support" in the path.
        // The shared support-root scanner must still detect it. This exercises
        // the same code Windows runs, against a tree we can build on macOS.
        let root = tempfile::tempdir().expect("tempdir");
        let cursor = root.path().join("Cursor");
        build_synthetic_vscdb(&cursor.join("User/globalStorage/state.vscdb"));
        std::fs::write(
            cursor.join("User/mcp.json"),
            r#"{ "servers": { "x": { "command": "y" } } }"#,
        )
        .expect("write mcp");

        // "Cursor" maps to a known kind on Windows (app_data_windows). On macOS
        // the known set keys off Application-Support names, which also includes
        // "Cursor", so either way this resolves to a registry kind, not Other.
        let known: Vec<(&str, KindTag)> = vec![("Cursor", KindTag::Cursor)];
        let mut acc = Accumulator::default();
        scan_vscode_support_root(&mut acc, root.path(), &known);
        let agents = acc.finish();

        let cursor_agent = agents
            .iter()
            .find(|a| a.kind == AgentKind::Cursor)
            .expect("appdata-style Cursor fork detected");
        assert!(cursor_agent.session_store.is_some(), "records vscdb store");
        assert!(
            cursor_agent.connector_config_path.is_some(),
            "records mcp.json under User/"
        );
        assert_eq!(cursor_agent.capability, Capability::ReadOnly);
    }

    #[test]
    fn test_fork_detector_unknown_appdata_dir_is_other() {
        // An unknown dir under a Windows-style root → Other(name), same rule as
        // macOS.
        let root = tempfile::tempdir().expect("tempdir");
        let fork = root.path().join("MysteryWinFork");
        build_synthetic_vscdb(&fork.join("User/globalStorage/state.vscdb"));

        let mut acc = Accumulator::default();
        scan_vscode_support_root(&mut acc, root.path(), &[]);
        let agents = acc.finish();
        assert!(
            agents
                .iter()
                .any(|a| matches!(&a.kind, AgentKind::Other(n) if n == "MysteryWinFork")),
            "unknown windows-style fork detected as Other"
        );
    }

    #[test]
    fn non_cursor_fork_gets_jsonfiles_not_cursor_reader() {
        // A plain VS Code-family fork (Antigravity / Insiders): `state.vscdb` with
        // ONLY `ItemTable` (no `cursorDiskKV`) + a `User/workspaceStorage` chats
        // dir. The detector must NOT assign Cursor's `SqliteVscdb` reader (which
        // would throw `no such table: cursorDiskKV`); it must use `JsonFiles`
        // pointing at workspaceStorage.
        let root = tempfile::tempdir().expect("tempdir");
        let fork = root.path().join("Antigravity IDE");
        build_plain_vscode_vscdb(&fork.join("User/globalStorage/state.vscdb"));
        std::fs::create_dir_all(fork.join("User/workspaceStorage")).expect("mkdir ws");

        let mut acc = Accumulator::default();
        scan_vscode_support_root(&mut acc, root.path(), &[]);
        let store = acc
            .finish()
            .into_iter()
            .find(|a| matches!(&a.kind, AgentKind::Other(n) if n == "Antigravity IDE"))
            .and_then(|a| a.session_store);
        let store = store.expect("non-cursor fork still gets a store");
        assert_eq!(
            store.format,
            SessionFormat::JsonFiles,
            "plain VS Code store → JsonFiles, never the cursorDiskKV reader"
        );
        assert!(store.path.ends_with("User/workspaceStorage"));
    }

    #[test]
    fn plain_vscode_fork_without_chats_has_no_session_store() {
        // A plain fork with neither cursorDiskKV nor a workspaceStorage chats dir
        // → no readable session store (still discovered for connectors), rather
        // than a broken Cursor-reader store.
        let root = tempfile::tempdir().expect("tempdir");
        let fork = root.path().join("Antigravity");
        build_plain_vscode_vscdb(&fork.join("User/globalStorage/state.vscdb"));

        let mut acc = Accumulator::default();
        scan_vscode_support_root(&mut acc, root.path(), &[]);
        let agent = acc
            .finish()
            .into_iter()
            .find(|a| matches!(&a.kind, AgentKind::Other(n) if n == "Antigravity"));
        let agent = agent.expect("fork still discovered");
        assert!(
            agent.session_store.is_none(),
            "no cursorDiskKV + no chats dir → no session store (not a broken one)"
        );
    }

    #[test]
    fn test_fork_detector_skips_corrupt_vscdb_in_appdata_tree() {
        let root = tempfile::tempdir().expect("tempdir");
        let bad = root.path().join("BrokenWinFork");
        let bad_db = bad.join("User/globalStorage/state.vscdb");
        std::fs::create_dir_all(bad_db.parent().expect("parent")).expect("mkdir");
        std::fs::write(&bad_db, b"corrupt-not-sqlite").expect("write corrupt");

        let mut acc = Accumulator::default();
        scan_vscode_support_root(&mut acc, root.path(), &[]);
        assert!(
            !acc.finish()
                .iter()
                .any(|a| matches!(&a.kind, AgentKind::Other(n) if n == "BrokenWinFork")),
            "corrupt vscdb must not mint a fake agent"
        );
    }
}

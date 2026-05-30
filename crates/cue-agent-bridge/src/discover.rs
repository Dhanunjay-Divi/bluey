//! Dynamic, system-wide agent discovery — finds **all** installed coding
//! agents (GUI + CLI), registry-driven plus a generic VS Code-fork detector so
//! unknown forks are still found.
//!
//! Every step is read-only (`std::fs` reads/metadata only) and fail-soft: a
//! malformed or inaccessible path for one agent degrades that agent and the
//! scan continues. Discovery never panics and never returns an error — the
//! worst case is an empty `Vec`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::capability::compute_capability;
use crate::registry::{all_binary_candidates, AgentEntry, KindTag, REGISTRY};
use crate::{AgentKind, Capability, DiscoveredAgent, SessionFormat, SessionStore};

/// Resolve `$HOME` without adding a dependency. Returns `None` if unset.
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Discover every coding agent installed on this machine.
///
/// Combines four passes — PATH binaries, app bundles, registry data-dirs, and a
/// generic VS Code-fork sweep — then deduplicates so an agent found multiple
/// ways becomes one entry with combined evidence.
pub fn discover_agents() -> Vec<DiscoveredAgent> {
    let mut acc = Accumulator::default();

    scan_path_binaries(&mut acc);
    if let Some(home) = home_dir() {
        scan_app_bundles(&mut acc, &home);
        scan_registry_data_dirs(&mut acc, &home);
        scan_vscode_forks(&mut acc, &home);
    }

    acc.finish()
}

/// Discover agents rooted at an explicit `home` directory, skipping the `PATH`
/// scan. Used by integration tests to run discovery against synthetic fixture
/// trees without reading the real user's home or mutating global env.
pub fn discover_in_home(home: &Path) -> Vec<DiscoveredAgent> {
    let mut acc = Accumulator::default();
    scan_app_bundles(&mut acc, home);
    scan_registry_data_dirs(&mut acc, home);
    scan_vscode_forks(&mut acc, home);
    acc.finish()
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

/// Pass 3 — scan registry data-dir globs under `$HOME` for footprints.
fn scan_registry_data_dirs(acc: &mut Accumulator, home: &Path) {
    for entry in REGISTRY {
        for glob in entry.data_dir_globs {
            let dir = home.join(glob);
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

/// Pass 4 — generic VS Code-fork detector. Any directory under
/// `~/Library/Application Support/*` that contains
/// `User/globalStorage/state.vscdb` is treated as a VS Code-family agent, even
/// if it is not a registry row, so unknown forks are discovered with no code
/// change.
fn scan_vscode_forks(acc: &mut Accumulator, home: &Path) {
    let support = home.join("Library/Application Support");
    let read = match std::fs::read_dir(&support) {
        Ok(r) => r,
        Err(_) => return, // missing/unreadable → degrade silently, keep scanning
    };

    // Names already covered by a registry data-dir glob → use that kind.
    let known: Vec<(&str, KindTag)> = REGISTRY
        .iter()
        .filter_map(|e| {
            e.data_dir_globs
                .iter()
                .find_map(|g| g.strip_prefix("Library/Application Support/"))
                .map(|name| (name, e.kind_tag))
        })
        .collect();

    for child in read.flatten() {
        let dir = child.path();
        let vscdb = dir.join("User/globalStorage/state.vscdb");
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
        let session_store = Some(SessionStore {
            path: vscdb,
            format: SessionFormat::SqliteVscdb,
        });
        acc.add(kind, dir, connector_config, session_store);
    }
}

/// Look for a connector config file inside an agent's data dir. Checks the
/// common names at the dir root and under a `User/` subdir.
fn locate_connector_config(dir: &Path) -> Option<PathBuf> {
    const NAMES: &[&str] = &["mcp.json", "mcp_config.json", "settings.json"];
    let bases = [dir.to_path_buf(), dir.join("User")];
    for base in &bases {
        for name in NAMES {
            let candidate = base.join(name);
            if path_exists(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

/// Look for a session store inside an agent's data dir, matching the registry's
/// declared format for that agent.
fn locate_session_store(dir: &Path, entry: &AgentEntry) -> Option<SessionStore> {
    let format = entry.session_format?;
    let path = match format {
        SessionFormat::Jsonl => dir.join("projects"),
        SessionFormat::SqliteVscdb => dir.join("User/globalStorage/state.vscdb"),
        SessionFormat::JsonFiles => dir.join("User/workspaceStorage"),
        SessionFormat::Protobuf => dir.join("conversations"),
    };
    path_exists(&path).then_some(SessionStore { path, format })
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
}

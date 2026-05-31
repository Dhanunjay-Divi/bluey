//! Antigravity protobuf (`*.pb`) session decoder — **safe stub**.
//!
//! Storage layout (design §3): `~/.gemini/antigravity/conversations/*.pb`,
//! ~100 conversations on the spike machine, encoded as protobuf.
//!
//! We do **not** have Antigravity's `.proto` schema, and reverse-engineering an
//! unknown wire format risks misreading bytes as text (or surfacing garbage /
//! secrets). So decoding is intentionally deferred:
//!
//! - [`list`](ProtobufReader::list) still enumerates the `*.pb` files so the
//!   conversations are *discoverable* (id = file stem, mtime as `updated_at`,
//!   no title — we cannot cheaply read one without the schema).
//! - [`read`](ProtobufReader::read) returns a clear
//!   [`BridgeError::Session`] explaining the deferral, rather than guessing.
//!
//! When the schema is obtained, replace the `read` body with a real decoder.
//! No protobuf dependency is added until then.

use std::path::{Path, PathBuf};

use super::{mtime_epoch_string, SessionReader};
use crate::{BridgeError, SessionRef, SessionStore, Transcript};

/// Stub decoder for Antigravity `*.pb` conversations (enumerate-only).
pub struct ProtobufReader;

impl SessionReader for ProtobufReader {
    fn list(&self, store: &SessionStore, limit: usize) -> anyhow::Result<Vec<SessionRef>> {
        let mut refs: Vec<SessionRef> = enumerate_pb_files(&store.path)
            .into_iter()
            .filter_map(|file| {
                let id = file.file_stem()?.to_string_lossy().into_owned();
                Some(SessionRef {
                    id,
                    title: None,
                    updated_at: mtime_epoch_string(&file),
                    project: None,
                })
            })
            .collect();
        refs.sort_by(|a, b| {
            let an = a.updated_at.parse::<u64>().unwrap_or(0);
            let bn = b.updated_at.parse::<u64>().unwrap_or(0);
            bn.cmp(&an)
        });
        refs.truncate(limit);
        Ok(refs)
    }

    fn read(
        &self,
        _store: &SessionStore,
        _id: &str,
        _max_turns: usize,
    ) -> anyhow::Result<Transcript> {
        Err(
            BridgeError::Session("antigravity protobuf decoding not yet implemented".to_string())
                .into(),
        )
    }
}

/// Enumerate `*.pb` files for a store path (file or directory).
fn enumerate_pb_files(path: &Path) -> Vec<PathBuf> {
    if path.is_file() {
        return vec![path.to_path_buf()];
    }
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() && p.extension().is_some_and(|e| e == "pb") {
                files.push(p);
            }
        }
    }
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SessionFormat;

    fn store_at(path: PathBuf) -> SessionStore {
        SessionStore {
            path,
            format: SessionFormat::Protobuf,
        }
    }

    #[test]
    fn list_enumerates_pb_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("conv-a.pb"), b"\x08\x01").unwrap();
        std::fs::write(dir.path().join("conv-b.pb"), b"\x08\x02").unwrap();
        // A non-.pb file must be ignored.
        std::fs::write(dir.path().join("notes.txt"), b"ignore me").unwrap();

        let reader = ProtobufReader;
        let store = store_at(dir.path().to_path_buf());
        let refs = reader.list(&store, 10).expect("list");
        assert_eq!(refs.len(), 2);
        let ids: Vec<&str> = refs.iter().map(|r| r.id.as_str()).collect();
        assert!(ids.contains(&"conv-a"));
        assert!(ids.contains(&"conv-b"));
        assert!(refs.iter().all(|r| r.title.is_none()));
    }

    #[test]
    fn read_returns_deferred_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("conv-a.pb"), b"\x08\x01").unwrap();
        let reader = ProtobufReader;
        let store = store_at(dir.path().to_path_buf());
        let err = reader.read(&store, "conv-a", 10).expect_err("must defer");
        assert!(err.to_string().contains("not yet implemented"));
    }
}

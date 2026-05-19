# IMPL-D0.2: Session UUID Model + SQLite Schema

**Status:** Complete
**Branch:** `feat/phase-0-d0.2-session-model`
**Date:** 2026-05-12

## Schema Summary

### `sessions` table

| Column | Type | Notes |
|--------|------|-------|
| id | TEXT PK | UUID v4 |
| title | TEXT NOT NULL | Default: "Untitled session" |
| status | TEXT NOT NULL | CHECK: active/paused/archived |
| created_at | INTEGER NOT NULL | Unix ms |
| updated_at | INTEGER NOT NULL | Unix ms |
| last_active_at | INTEGER | Updated on each turn append |
| archived_at | INTEGER | Set when status → archived |
| token_count | INTEGER NOT NULL | Auto-incremented on turn append |
| compressed_summary | TEXT | Epoch summary for long sessions |
| active_skill | TEXT | Nullable skill identifier |
| metadata | TEXT | JSON blob for extensibility |

Indexes: `idx_sessions_updated(updated_at DESC)`, `idx_sessions_status(status, updated_at DESC)`

### `turns` table

| Column | Type | Notes |
|--------|------|-------|
| id | TEXT PK | UUID v4 |
| session_id | TEXT NOT NULL | FK → sessions(id) ON DELETE CASCADE |
| turn_index | INTEGER NOT NULL | Auto-assigned (0, 1, 2, ...) |
| user_message | TEXT NOT NULL | |
| model_response | TEXT NOT NULL | |
| lane | TEXT NOT NULL | CHECK: snap/snap_edit/solve/think |
| provider | TEXT NOT NULL | e.g. "cerebras", "anthropic" |
| model | TEXT NOT NULL | e.g. "deepseek-v3", "claude-sonnet-4.5" |
| created_at | INTEGER NOT NULL | Unix ms |
| duration_ms | INTEGER | Nullable |
| input_tokens | INTEGER | Nullable |
| output_tokens | INTEGER | Nullable |
| cost_cents | INTEGER | Nullable |

Indexes: `idx_turns_session(session_id, turn_index)`, `idx_turns_created(created_at DESC)`

## API Surface

```rust
impl Database {
    pub fn open(path: &str) -> Result<Self>
    pub fn create_session(title: Option<String>) -> Result<Session>
    pub fn get_session(id: Uuid) -> Result<Option<Session>>
    pub fn list_sessions(status: Option<SessionStatus>, limit: u32) -> Result<Vec<Session>>
    pub fn update_session_status(id: Uuid, status: SessionStatus) -> Result<()>
    pub fn archive_session(id: Uuid) -> Result<()>
    pub fn delete_session(id: Uuid) -> Result<()>
    pub fn append_turn(session_id: Uuid, turn: NewTurn) -> Result<Turn>
    pub fn list_turns(session_id: Uuid, limit: Option<u32>) -> Result<Vec<Turn>>
}
```

## Test Results

```
cargo test -p cue-daemon -- db::tests

test db::tests::test_create_and_get_session ... ok
test db::tests::test_cascade_delete_session_removes_turns ... ok
test db::tests::test_list_sessions_by_status ... ok
test db::tests::test_append_turn_and_list_turns ... ok
test result: ok. 4 passed; 0 failed; 0 ignored
```

Full workspace `cargo build --release` passes on macOS arm64.

## Design Decisions

| Decision | Rationale |
|----------|-----------|
| `rusqlite` (bundled) over `sqlx` | Synchronous API is simpler for the daemon's current architecture. No async runtime needed for DB ops. Bundled SQLite avoids system dependency issues. |
| `include_str!` for migrations | Single migration file embedded at compile time. Simple, no runtime file discovery needed. |
| Token count auto-update on `append_turn` | Keeps session metadata consistent without requiring caller to manually track. |
| `turn_index` auto-assigned via MAX+1 | Simpler than requiring caller to track; monotonically increasing per session. |
| Separate from cloud schema (001) | 001 is Postgres with pgvector for cloud. 002 is local SQLite for the daemon. Different targets, different schemas. |

## Known Follow-ups

- **D0.3 / Phase 1:** Tauri commands to expose `Database::*` methods to the dashboard frontend
- **Phase 1:** Session search (full-text on turns.user_message / model_response)
- **Phase 1:** Compressed summary generation (populate `compressed_summary` after N turns)
- **Phase 2:** Sync local sessions to cloud (bridge 002 ↔ cloud sessions table)
- **Phase 2:** Migration versioning system (track which migrations have run, support incremental)

## Review Checklist

- [x] Migration file uses `IF NOT EXISTS` (idempotent)
- [x] Foreign key cascade delete tested
- [x] All tests use `:memory:` SQLite (no disk I/O in CI)
- [x] `PRAGMA foreign_keys=ON` set on every connection
- [x] `PRAGMA journal_mode=WAL` for concurrent read performance
- [x] No unwrap() in production paths (all Result-based)
- [x] Release build passes on aarch64-apple-darwin

# IMPL — Phase 2 Session UX

**Task IDs**: P2 (full phase — multiple tasks bundled)
**Scope**: emit `session:created` from Rust; new/load/switch-session flows; polished session list; regression coverage
**Branch**: `feat/phase-2-session-ux`
**Base**: `4e813b9` (main tip post-Phase-1-merge)

## Scope

Phase 2 delivers the full session lifecycle UX on top of Phase 1's dashboard shell:

- **Event emission** — `session:created` fires from the Rust `create_session` command so any open window picks it up
- **Active session concept** — daemon owns a `Mutex<Option<Uuid>>` active selection; `session:switched` event broadcasts changes
- **New session flow** — button → create → navigate to `/session/:id`
- **Load session flow** — click session row → navigate to detail → detail page marks it active via `set_active_session`
- **Switch flow** — navigating between sessions via sidebar/clicks updates the daemon's active selection + broadcasts event
- **Session detail page** (`/session/:id`) — title editing, turn list, archive/delete actions, active badge
- **Session list polish** — search by title, status filter chips (all/active/paused/archived), delete button, click-to-open, active-session highlight
- **Title update** — `update_session_title` command + corresponding `Database::update_session_title`
- **Turn listing** — `list_turns` Tauri command exposing `Database::list_turns` to the UI
- **Regression tests** — 6 new Rust tests covering deferred D0.2 V2 nits + Phase 2 behavior

## Not in scope

- Daemon-side push of active session id to the **native Swift/C overlay** — Tauri event emission is wired, but cue-daemon still does not pipe `session:switched` down through the existing JSON-over-stdio channel to the native overlay. That crosses process boundaries beyond the Tauri webview and lands in Phase 3 (Listening upgrade) where the native-overlay IPC protocol is revisited.
- Editing turns or composing new turns from the UI — Phase 4 work (Reasoning upgrade) owns the compose flow.
- Per-session skill override UI — the data model supports it (`Session.active_skill`) but there's no UI yet. Phase 4 adds it alongside the skill-template library.

## Files created / modified

| File | Change |
|---|---|
| `crates/cue-daemon/src/db/mod.rs` | Added `update_session_title`; 6 new regression tests (title happy-path, empty-title rejection, missing-id error, unarchive clears archived_at, invalid UUID surfaces error, duplicate turn_index rejected by unique index) |
| `crates/cue-dashboard/src/commands.rs` | Rewrote to add: `ActiveSessionState`, `SessionSwitchedPayload`, `AppHandle` injection, `emit("session:created")`, `update_session_title`, `get_active_session`, `set_active_session` (validates existence), `list_turns`. `delete_session` now clears active selection + emits `session:switched` when the deleted session was active. |
| `crates/cue-dashboard/src/lib.rs` | Managed `ActiveSessionState(Mutex::new(None))`. Registered new commands in the invoke handler. |
| `crates/cue-dashboard/ui/src/App.tsx` | Added `/session/:id` route |
| `crates/cue-dashboard/ui/src/hooks/useActiveSession.ts` | New hook — loads active id on mount, subscribes to `session:switched`, exposes `setActive(id \| null)` |
| `crates/cue-dashboard/ui/src/pages/SessionDetail.tsx` | New page — title inline edit, turns list, archive/delete, active badge, auto-sets itself active on mount |
| `crates/cue-dashboard/ui/src/pages/Chats.tsx` | Added: navigate-on-click, search box, status filter chips, delete button, active-session highlight, create-then-navigate flow |

## Build + test

```bash
# Rust side
$ cargo fmt --all --check                       → pass
$ cargo clippy --all-targets -- -D warnings     → pass
$ cargo build --all-targets --release           → 36.95s, clean
$ cargo test --all-targets
  cue-core:     26 passed, 0 failed
  cue-daemon:   20 passed, 0 failed   (was 14 — added 6 regression tests)

# Frontend side
$ cd crates/cue-dashboard/ui && npm install && npm run build
  added 190 packages
  vite build: 1701 modules transformed
  dist/index.html                    0.39 kB │ gzip:  0.27 kB
  dist/assets/index-CvIBOKYT.css    15.90 kB │ gzip:  3.89 kB
  dist/assets/index-D26fa74K.js    296.97 kB │ gzip: 94.30 kB
```

## Commits

```
c0f5369 test(daemon): add regression tests for D0.2 nits + Phase 2 title update [P2]
256bcbd feat(dashboard+daemon): emit session:created, add active-session state, update_session_title, list_turns [P2]
(+frontend commit for Chats + SessionDetail + routes)
```

## Deviations from plan + rationale

- **Active session state lives in Rust, not React**. The master plan left this open. I chose Rust-owned because the daemon is the source of truth and the native overlay will later consume the same `session:switched` event. React `useActiveSession` is a thin proxy. This trades a round-trip per switch (cheap, <1ms) for a single authoritative state.
- **`set_active_session` validates existence** before switching. Slight cost; prevents pointing at a session that was deleted concurrently in another window.
- **Title update is a separate Tauri command** rather than subsuming into a generic `update_session(patch)`. Explicit commands are friendlier for React code review and the surface is small enough that it doesn't need patching. Revisit in Phase 6 if the update surface grows past 4-5 commands.
- **Delete from Chats does not navigate anywhere**, but delete from SessionDetail navigates back to `/chats`. Deleted-while-viewing is the only case where navigation is necessary.

## Regression test rationale

Codex flagged 3 D0.2 V2 nits as "later" — these are now covered:

1. `test_invalid_uuid_in_db_row_surfaces_error` — inserts a row with id `"not-a-uuid"` directly into the DB, then verifies `list_sessions` returns `Err(...)` instead of a session with nil UUID. Guards against silent data corruption hiding behind `unwrap_or_default`.
2. `test_unarchive_clears_archived_at` — archives a session, confirms `archived_at` is set, moves it back to active, confirms `archived_at` is `NULL`. Guards against the `COALESCE` regression.
3. `test_duplicate_turn_index_rejected_by_unique_constraint` — inserts a turn normally (index 0), then force-inserts another row with the same `(session_id, turn_index)` and verifies the unique index rejects it.

Plus 3 new tests for `update_session_title`: happy path, empty-string rejection, missing-id error.

## Known follow-ups

- **Native overlay session indicator** — the Swift/C overlay does not yet display the active session ID. Scheduled for Phase 3 once the daemon↔native IPC is extended with a `session_switched` message type.
- **Session counts in sidebar** — nice-to-have "Chats (N)" badge; deferred.
- **Drag-to-reorder** sessions — deferred.
- **Bulk operations** (archive all, delete archived) — deferred.
- **Keyboard navigation** for session list — deferred (Cmd+K palette covers "jump to session" in Phase 6).

## Review checklist for codex

- [ ] `create_session` emits `session:created` with the full Session payload, not just the id
- [ ] `delete_session` clears active selection + emits `session:switched(None)` only when the deleted session was active — does not spuriously clear when unrelated sessions are deleted
- [ ] `set_active_session(Some(id))` returns an error if the session does not exist (protect against deleted-elsewhere race)
- [ ] `set_active_session` only emits `session:switched` when the selection actually changes — does not spam events when called with the current id
- [ ] Lock ordering: `db` lock released before `active` lock acquired in `delete_session` (no nested locks)
- [ ] `update_session_title` trims whitespace and rejects empty titles after trim
- [ ] `update_session_title` returns an error when the id doesn't exist (not silent no-op)
- [ ] React `useActiveSession` unsubscribes on unmount (no listener leak when navigating away)
- [ ] `SessionDetail` `setActive(id)` only fires when `id !== activeId` (no redundant round-trips)
- [ ] Chats page `handleArchive` / `handleDelete` call `event.stopPropagation()` so clicking the button does NOT navigate into the session
- [ ] `delete_session` confirmation shows session title, not the raw UUID
- [ ] The 6 new regression tests actually test what they claim (no false-pass tests)
- [ ] CI stays green on both macos-latest and ubuntu-latest

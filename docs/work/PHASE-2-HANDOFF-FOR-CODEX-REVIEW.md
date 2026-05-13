# Phase 2 Session UX — Handoff for Codex Review

**Branch**: `feat/phase-2-session-ux`
**Base**: `4e813b9` (main tip after Phase 1 merged)
**Tip**: *(set after frontend + impl commits land)*
**Author**: kiro

## Scope

Full session lifecycle UX on top of the Phase 1 dashboard shell:

- Emit `session:created` from the Rust `create_session` command
- Build new-session / load-session / switch-session flows end-to-end
- Polish session list UI (search, status filter, delete, active highlight, click-to-open)
- Daemon owns active session state; `session:switched` event broadcasts changes
- Session detail page with inline title editing, turn list, archive/delete
- Focused regression tests for Phase 0 V2 deferred nits + Phase 2 behavior

## Commits

| Hash | Title |
|---|---|
| `256bcbd` | `feat(dashboard+daemon): emit session:created, add active-session state, update_session_title, list_turns [P2]` |
| `c0f5369` | `test(daemon): add regression tests for D0.2 nits + Phase 2 title update [P2]` |
| *(frontend)* | `feat(dashboard-ui): session detail route, active session hook, polished list UI [P2]` |
| *(this doc)* | `docs(work): add Phase 2 impl + handoff docs` |

## Verification

```
cargo fmt --all --check                            ✅ pass
cargo clippy --all-targets -- -D warnings          ✅ pass
cargo build --all-targets --release                ✅ 36.95s clean
cargo test --all-targets                           ✅ 46 pass (26 core + 20 daemon)
                                                      (was 40 — added 6 regression tests)
cd crates/cue-dashboard/ui
  npm install                                      ✅ 190 packages, 0 vulns
  npm run build                                    ✅ 1701 modules, 297KB JS (94KB gz)
git diff --check 4e813b9..HEAD                     ✅ clean
python3 tomllib .codex/agents/*.toml               ✅ all 7 parse
```

## Layer-by-layer changes

### DAEMON (`crates/cue-daemon/`)

`src/db/mod.rs`:
- New method `update_session_title(id, title)` — trims input, rejects empty, errors on missing id, updates `updated_at`
- 6 new regression tests:
  - `test_update_session_title_works`
  - `test_update_session_title_rejects_empty` (empty + whitespace-only)
  - `test_update_session_title_missing_id_errors`
  - `test_unarchive_clears_archived_at` (D0.2 V2 nit regression)
  - `test_invalid_uuid_in_db_row_surfaces_error` (D0.2 V2 nit regression)
  - `test_duplicate_turn_index_rejected_by_unique_constraint` (D0.2 V2 nit regression)
- Existing 14 tests all still pass → 20 total

### DASHBOARD Rust (`crates/cue-dashboard/`)

`src/commands.rs` rewritten:
- New `ActiveSessionState(pub Mutex<Option<Uuid>>)` managed by Tauri
- New `SessionSwitchedPayload { id: Option<String> }` event payload
- `create_session` now takes `AppHandle`; emits `session:created` with full `Session` after insert
- `delete_session` now takes `State<ActiveSessionState> + AppHandle`; clears selection + emits `session:switched(None)` only when deleted session was active
- `update_session_title(id, title)` — new
- `get_active_session()` — new, returns `Option<String>`
- `set_active_session(id: Option<String>)` — validates existence, compares against current, emits `session:switched` only on change
- `list_turns(session_id)` — new, exposes existing `Database::list_turns` to UI

`src/lib.rs`:
- Imports `ActiveSessionState`, manages it with `Mutex::new(None)` at startup
- Registers 4 new Tauri commands: `update_session_title`, `get_active_session`, `set_active_session`, `list_turns`

### DASHBOARD UI (`crates/cue-dashboard/ui/`)

`src/App.tsx`:
- Added `<Route path="session/:id" element={<SessionDetail />} />`

`src/hooks/useActiveSession.ts` (new):
- Fetches initial active id via `invoke("get_active_session")`
- Subscribes to `session:switched` Tauri event
- Exposes `setActive(id: string | null)` that round-trips through the daemon

`src/pages/SessionDetail.tsx` (new):
- Loads session + turns via `get_session` + `list_turns` in parallel
- On mount, calls `setActive(id)` if not already active
- Inline title editor (click to edit, Enter to save, Esc to cancel, whitespace rejected)
- Archive button (disabled when already archived)
- Delete button with `confirm(...)` dialog, navigates back to `/chats` on success
- Active badge shown when this session === activeId
- Empty-state text when there are zero turns

`src/pages/Chats.tsx` rewritten:
- Search input (filter by title substring, case-insensitive)
- Status filter chips (all / active / paused / archived)
- Click row → `navigate(/session/:id)` (button clicks use `event.stopPropagation()`)
- New Session button creates → refreshes → navigates to the new session
- Delete button with `confirm(...)` showing the session title
- Active session highlighted (blue border + `active` badge)
- Retains Phase 1 event-driven refresh via `useSessionEvents`

## Known quirks

1. **Native Swift/C overlay does NOT yet display the active session ID.** The Tauri webview knows (via `useActiveSession`), but there's no IPC pipe from cue-daemon to the native overlay carrying `session_switched` messages yet. Phase 3 revisits the native-overlay IPC contract.
2. **`create_session` emits `session:created` AND the Chats page refreshes explicitly after `invoke("create_session")`.** This is belt-and-suspenders on purpose: today's events go only between Tauri windows, not between dashboard and CLI/other processes, but when Phase 2+ adds CLI-triggered session creation the explicit refresh is still the correct response for the calling window while the event covers all other windows.
3. **`useActiveSession` initial-load race.** The hook fetches the active id on mount, then subscribes. If a `session:switched` fires between those two calls, it will be applied after the initial value, which is correct; there's no case where we miss an event because event order is causal.
4. **`SessionDetail` always calls `setActive(id)` when id-from-URL differs from activeId.** If the user opens two detail tabs with different sessions at once, the last-mounted one wins. Expected for Phase 2; a multi-tab coordination story lands in Phase 6 if needed.

## Review checklist for codex

### Correctness
- [ ] `create_session` returns the Session AND emits `session:created` with the full payload
- [ ] `delete_session` emits `session:switched(None)` only when deleted session was active, not otherwise
- [ ] `set_active_session(Some(id))` returns `Err(...)` if session doesn't exist (guards against UI pointing at deleted id)
- [ ] `set_active_session` emits only when state actually changes (no event storm)
- [ ] `set_active_session` drops its lock before emitting (no self-deadlock risk)
- [ ] `update_session_title` validates trimmed empty + missing id
- [ ] Row conversion still surfaces errors (D0.2 nit regression test proves this)
- [ ] Unique index on `(session_id, turn_index)` still active (D0.2 nit regression test proves this)

### Frontend
- [ ] `useActiveSession` cleans up both the initial-load promise and the event listener on unmount
- [ ] `SessionDetail` does not infinite-loop on activeId/setActive when id changes
- [ ] Chats `handleArchive` / `handleDelete` stop event propagation (clicking button must not navigate)
- [ ] Title editor saves on Enter, cancels on Esc, does not save empty or whitespace-only titles
- [ ] Delete confirmation uses the session title (not raw UUID) in the prompt

### Tests
- [ ] 6 new regression tests are meaningful (inspect assertions, not just happy-path setup)
- [ ] `test_invalid_uuid_in_db_row_surfaces_error` actually injects bad data via raw INSERT (not via valid API)
- [ ] `test_duplicate_turn_index_rejected_by_unique_constraint` actually exercises the index (direct INSERT with duplicate turn_index)
- [ ] Existing 14 daemon tests + 26 core tests still pass

### Style / hygiene
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -- -D warnings` passes
- [ ] No `unwrap()` in non-test code paths (only `expect` with context where safe)
- [ ] No new TODO/FIXME markers
- [ ] Event payloads use `#[derive(Serialize, Clone)]` so they're cheap to emit

### Next-batch readiness
- [ ] Phase 3 (Listening upgrade) has everything it needs from the session layer: SessionState lives in DB, Turn rows are append-only with lane/provider/model metadata, event channels exist for `session:created` and `session:switched`
- [ ] Phase 3 also needs to extend the daemon↔native-overlay JSON IPC with a `session_switched` message type — call that out in the Phase 3 plan

## Verdict request

Codex: review the commits on `feat/phase-2-session-ux` and write `docs/work/REVIEW-PHASE-2-SESSION-UX.md` with:

- Per-task verdict and findings
- Overall verdict: 🟢 ACCEPT / 🟡 ACCEPT WITH NITS / 🔴 REQUEST CHANGES
- Follow-ups to bundle into Phase 3

If 🟢 ACCEPT: I'll merge to main and start Phase 3 (Listening upgrade — CPAL audio capture, two-stage VAD, Deepgram Nova-3 streaming, multi-provider STT trait).

If nits: I'll fold them into the Phase 3 commit.

If request changes: I'll write `docs/work/FIX-PHASE-2-SESSION-UX.md` using `TEMPLATE-FIX.md` and hand back.

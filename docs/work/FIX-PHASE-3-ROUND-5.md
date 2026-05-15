# FIX-PHASE-3-ROUND-5: Codex R5 Blocker Remediation (5 of 6)

## Issue

Codex review of Phase 3 Round 5 returned 🔴 REQUEST CHANGES with 6
blockers. Blocker 2 (overlay supervisor) is an R4 carryover documented in
`docs/work/FIX-PHASE-3-ROUND-4.md`. This document covers the remaining 5
blockers, all R5-scope.

Codex's findings (paraphrased):

1. **SystemAudioCapture handle dropped immediately** — the capture handle
   returned by `SystemAudioCapture::start()` was not stored, so the native
   helper child process was orphaned and never cleanly terminated.
2. **System-audio STT events produced but never consumed** — the parallel
   STT instance for system audio ran but its `TranscriptEvent` stream was
   never drained, causing unbounded channel growth and no transcripts
   reaching the session manager.
3. **FTS5 cascade-delete broken** — the `AFTER DELETE` trigger on
   `transcripts` used `old.rowid` which doesn't match standalone FTS5 table
   rowids; deleting a transcript left stale FTS entries.
4. **`dangerouslySetInnerHTML` in search snippets** — the Search component
   rendered FTS5 `snippet()` output via `dangerouslySetInnerHTML`, creating
   an XSS vector if transcript content contained HTML.
5. **First-run onboarding not mounted + tray Settings nav broken** — the
   `<Onboarding>` component was defined but never rendered in `App.tsx`;
   the tray "Settings" menu item navigated to a non-existent route.

## Root Cause

| # | Blocker | Root Cause |
|---|---------|------------|
| 1 | Handle dropped | `app.rs` called `SystemAudioCapture::start(sender)` but discarded the returned handle. No struct field existed to hold it. |
| 3 | STT drain missing | The parallel STT provider was spawned and `send_audio` was called, but no task called `next_event()` in a loop — events accumulated in the internal channel forever. |
| 4 | FTS cascade | Migration 006 trigger: `DELETE FROM transcript_fts WHERE rowid = old.rowid`. FTS5 standalone tables assign their own rowids unrelated to the source table's rowid. |
| 5 | XSS in snippets | `Search.tsx` used `<span dangerouslySetInnerHTML={{__html: snippet}}>` to render `snippet()` output containing `<b>...</b>` highlight markers. |
| 6 | Onboarding/tray | `App.tsx` imported `Onboarding` but never conditionally rendered it. Tray "Settings" emitted a `navigate` event with path `/settings` but the router had no such route. |

## Fix Summary

### Blocker 1 — Retain SystemAudioCapture handle (commit `289cc34`)

Added `system_audio: Mutex<Option<SystemAudioCapture>>` field to the
`Daemon` struct. After `start()`, the handle is stored in the mutex. On
daemon shutdown, the handle is taken from the mutex and `stop()` is called
explicitly, ensuring the native helper child process is terminated cleanly
via the existing `stop()` → kill → join path.

### Blocker 3 — Drain system-audio STT events (commit `ed37547`)

When `BLUEY_SYSTEM_AUDIO_STT=1`, a tokio task is spawned that loops on
`provider.next_event()`:
- `TranscriptEvent::Final` → forwarded to `add_audio_transcript_segment`
  (same path mic STT uses).
- Non-retryable errors (`Auth`, `Quota`) → warn log + clean exit.
- Retryable errors → warn log + continue loop.
- `None` (stream closed) → task exits.

### Blocker 4 — FTS5 cascade-delete (commit `2450d1f`)

New migration `008_fts_cascade_fix.sql`:
1. Drops the broken triggers from migration 006.
2. Drops and recreates `transcript_fts` with an explicit `transcript_id`
   column (in addition to `content`).
3. Recreates `AFTER INSERT` trigger to populate `transcript_id`.
4. Adds correct `AFTER DELETE` trigger:
   `DELETE FROM transcript_fts WHERE transcript_id = old.id`.

Also added `delete_transcript()` method in `db/search.rs` and fixed the
`snippet()` column index (was 0, now 1 to account for `transcript_id`).

### Blocker 5 — Safe search snippet rendering (commit `385cdca`)

Replaced `dangerouslySetInnerHTML` with a safe rendering approach:
- FTS5 `snippet()` uses Unicode markers (`\u{FFF9}` / `\u{FFFA}`) as
  highlight delimiters instead of `<b>` tags.
- `Search.tsx` splits on these markers and renders highlighted segments as
  `<mark>` elements via React children (no raw HTML injection).
- Mixed delimiters (nested, adjacent, unclosed) handled gracefully by the
  split logic.

### Blocker 6 — Onboarding mount + tray Settings nav (commit `385cdca`)

- `App.tsx`: Added conditional rendering of `<Onboarding>` when
  `onboarding_complete` setting is falsy. After completion, the component
  sets the flag and the main app renders.
- `App.tsx`: Added `/settings` route to the router.
- Tray "Settings" menu item now navigates correctly.
- Onboarding re-render after completion: uses React state update to
  transition from onboarding to main app without a full page reload.

## Files Modified

| File | Change | Blocker |
|------|--------|---------|
| `crates/cue-daemon/src/app.rs` | Add `system_audio` mutex field; store handle; explicit `stop()` on shutdown | 1 |
| `crates/cue-daemon/src/audio/system_capture.rs` | (no change — existing `stop()` API used) | 1 |
| `crates/cue-daemon/tests/system_audio_integration.rs` | New — 2 integration tests for handle retention + STT drain | 1, 3 |
| `infra/migrations/008_fts_cascade_fix.sql` | New — drop broken triggers, recreate FTS with `transcript_id`, correct triggers | 4 |
| `crates/cue-daemon/src/db/search.rs` | Add `delete_transcript()`; fix snippet column index | 4 |
| `crates/cue-daemon/src/db/mod.rs` | Include migration 008; add 3 FTS delete tests | 4 |
| `crates/cue-dashboard/ui/src/routes/Search.tsx` | Replace `dangerouslySetInnerHTML` with safe marker-based rendering | 5 |
| `crates/cue-dashboard/ui/src/App.tsx` | Mount `<Onboarding>` conditionally; add `/settings` route; wire tray nav | 6 |

## Edge Cases Handled

| Blocker | Edge Case | Handling |
|---------|-----------|----------|
| 1 | Daemon shutdown before system audio started | Mutex contains `None`; no-op on shutdown |
| 1 | Double `stop()` call | `stop()` is idempotent (atomic flag prevents double-kill) |
| 3 | Auth error from system-audio STT | Drain task logs warning and exits cleanly (no panic, no retry) |
| 3 | Provider stream returns None | Drain task exits; no resource leak |
| 4 | Delete transcript that was never indexed | Trigger fires but `DELETE FROM transcript_fts WHERE transcript_id = X` is a no-op (0 rows affected) |
| 4 | Bulk session delete | Cascade: `ON DELETE CASCADE` on `transcripts` FK fires per-row trigger, each removing the FTS entry |
| 4 | Mixed insert/delete/re-insert | Test `fts_index_is_consistent_after_mixed_ops` verifies FTS state matches source of truth |
| 5 | Snippet with no highlights | No markers present → rendered as plain text |
| 5 | Snippet with adjacent markers | Split produces empty strings between markers → filtered out |
| 5 | Transcript containing literal HTML | Rendered as text content (React escapes by default), not interpreted as HTML |
| 6 | Onboarding already completed | `onboarding_complete=true` → `<Onboarding>` never mounts |
| 6 | User completes onboarding | State update triggers re-render → main app appears immediately |

## How to Test

```bash
# All daemon tests (includes FTS delete + system audio integration)
cargo test -p cue-daemon --all-targets

# Specifically the new tests
cargo test -p cue-daemon fts_delete
cargo test -p cue-daemon fts_cascade
cargo test -p cue-daemon fts_index_is_consistent
cargo test -p cue-daemon system_audio_handle
cargo test -p cue-daemon stt_event_drain

# Dashboard build (verifies Search.tsx + App.tsx compile)
cd crates/cue-dashboard/ui && npm run build
```

## Verification

| Check | Status |
|-------|--------|
| `cargo fmt --all --check` | ✅ pass |
| `cargo clippy --all-targets -- -D warnings` | ✅ pass |
| `cargo build --all-targets` | ✅ pass |
| `cargo test --all-targets` | ✅ 164 pass |
| `cd crates/cue-dashboard/ui && npm run build` | ✅ pass |
| `git -P diff --check` | ✅ clean |
| No code regressions (R4 overlay tests still pass) | ✅ confirmed |

### Test Count Progression

| Milestone | Tests |
|-----------|-------|
| R3 main | 130 |
| R4 | 136 |
| R5 baseline | 157 |
| R5 + fixes | **164** (+7 from fix commits) |

New tests added by fix commits:
1. `overlay_supervisor_caps_at_max_restart_attempts` (Blocker 2, in R4 doc)
2. `overlay_long_running_child_shuts_down_cleanly` (Blocker 2, in R4 doc)
3. `system_audio_handle_retained_for_shutdown` (Blocker 1+3)
4. `stt_event_drain_forwards_finals_and_handles_errors` (Blocker 3)
5. `fts_delete_removes_index_row` (Blocker 4)
6. `fts_cascade_delete_clears_session_index` (Blocker 4)
7. `fts_index_is_consistent_after_mixed_ops` (Blocker 4)

## Commits Applied

| Hash | Message | Blockers |
|------|---------|----------|
| `289cc34` | `fix(daemon): retain SystemAudioCapture handle for explicit shutdown` | 1 |
| `ed37547` | `fix(daemon): drain system-audio STT events to downstream transcript consumer` | 3 |
| `2450d1f` | `fix(daemon): FTS5 cascade-delete trigger + tests` | 4 |
| `385cdca` | `fix(dashboard): safe search snippet rendering + onboarding mount + tray Settings nav` | 5, 6 |
| `36bb943` | `chore(p3r5-fix): apply cargo fmt across cherry-picked fixes` | (style) |

## Known Limitations

- **System-audio STT drain** does not yet forward `Partial` events — only
  `Final` transcripts reach the session manager. Partials for system audio
  are deferred to a future round when overlay rendering supports
  multi-source display.
- **FTS migration 008** rebuilds the FTS table from scratch. Existing FTS
  data from migration 006 is dropped and re-populated by the insert
  triggers on new writes. Historical transcripts written before migration
  008 will not appear in search until a backfill is run (existing
  `backfill_fts` function handles this on next daemon startup).
- **Onboarding** does not validate the API key against Deepgram before
  saving — this is an explicit deferral noted in the R5 handoff.

## References

- Blocker 2 (overlay supervisor): `docs/work/FIX-PHASE-3-ROUND-4.md`
- R5 handoff: `docs/work/PHASE-3-ROUND-5-HANDOFF-FOR-CODEX-REVIEW.md`
- R4 handoff: `docs/work/PHASE-3-ROUND-4-HANDOFF-FOR-CODEX-REVIEW.md`

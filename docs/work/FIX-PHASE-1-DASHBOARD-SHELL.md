# FIX — Phase 1 Dashboard Shell — review blockers from Codex

**Issue**: Phase 1 review verdict 🔴 REQUEST CHANGES (`docs/work/REVIEW-PHASE-1-DASHBOARD-SHELL.md`)
**Branch**: `feat/phase-1-dashboard-shell`
**Base before fix**: `fa19623` (pre-Phase-0-fix tip — wrong)
**Base after fix**: `b4a8207` (Phase 0 foundation with nit fix)
**New tip**: `3dcc7df`
**Date**: 2026-05-12
**Author**: kiro

---

## 1. Root Cause

**Blocker 1 — Wrong branch base (`fa19623` instead of `3a7c571`)**:
The original Phase 1 implementation was created in a worktree branched off `fa19623`, which was Phase 0's tip *before* the Phase 0 fix commits (blockers 1–6 + D0.2 nits 1–3). An earlier attempt to rebase onto the fixed tip appeared to succeed in logs but did not actually change the branch base; the merge-base remained at `fa19623`. Because the Phase 0 fix commits include `cargo fmt --all`, clippy lint allows, and the `panel_delegate!` unexpected_cfgs fix, running full-workspace `cargo fmt --check` or `cargo clippy -- -D warnings` from the Phase 1 branch would have failed against those un-backported fixes.

**Blocker 2 — `useSessionEvents` declared but never mounted**:
In `crates/cue-dashboard/ui/src/hooks/useSessionEvents.ts` the hook was implemented with a correct `listen<Session>("session:created", ...)` subscription lifecycle, but no React component imported or invoked it. Dead code in practice. The daemon side is ready to emit the event (backend channel wired in D1.8), but the UI side would never wake up when Phase 2+ starts firing `session:created` events.

**Blocker 3 — Docs typo in FIX-PHASE-0-FOUNDATION.md**:
Line 23 of that doc claimed "Quoted the hashFiles glob: `hashFiles(**/Cargo.lock)`", which shows the glob *unquoted* despite the prose saying it was quoted. The actual CI YAML change was correct; only the documentation was wrong.

---

## 2. Fix Summary

- **Rebased `feat/phase-1-dashboard-shell` onto `b4a8207`** (current Phase 0 foundation tip). All 4 Phase 1 commits moved cleanly onto the fixed base; no conflicts. Commit SHAs updated (old `eaadb4f` → new `0a3f344`, etc.). Merge-base verified: `b4a8207be862f320c19d8f246dcf30436cf978e3`.
- **Wired `useSessionEvents` into `crates/cue-dashboard/ui/src/pages/Chats.tsx`**:
  - `fetchSessions` extracted into a `useCallback` with stable identity so the `useEffect` inside `useSessionEvents` doesn't re-subscribe on every render.
  - New `handleSessionCreated = useCallback(() => { void fetchSessions(); }, [fetchSessions])` passed to `useSessionEvents(handleSessionCreated)`.
  - When the daemon emits `session:created`, the Chats page reruns `list_sessions` and refreshes the visible list. Works in Phase 2+ without further Chats changes; also safe today (event just never fires yet).
  - Comment left in the source explaining Phase 2+ emit path for future readers.
- **Fixed FIX-PHASE-0-FOUNDATION.md typo** on `feat/phase-0-foundation`: changed `hashFiles(**/Cargo.lock)` to `hashFiles('**/Cargo.lock')` to match the actual CI YAML.

---

## 3. Files Modified

| File | Purpose of change |
|---|---|
| `crates/cue-dashboard/ui/src/pages/Chats.tsx` | Import `useSessionEvents`, wrap `fetchSessions` in `useCallback`, call the hook with a refresh callback |
| `docs/work/FIX-PHASE-0-FOUNDATION.md` | Quote the hashFiles glob in prose so it matches the actual CI YAML |
| *(no source rewrites beyond the above — rebase only moved commits, no content diffs)* | |

---

## 4. Edge Cases Handled

- **Stable callback identity** — `fetchSessions` wrapped in `useCallback([])` so the `useSessionEvents` `useEffect` only subscribes once, not on every render.
- **`void fetchSessions()` call** — explicitly discards the returned promise so React's event handler type system stays happy (callback must be `() => void`, not `async () => Promise<void>`).
- **Loading flag after initial mount** — `fetchSessions` no longer resets `setLoading(true)` inside the callback (it only sets `false` in `finally`), so event-driven refreshes don't flash the loading state. First mount still shows loading via initial state.
- **No duplicate mount** — the hook is invoked in Chats.tsx only. Mounting it in DashboardLayout would keep the subscription alive across route changes, but that's an intentional Phase 2+ decision when we introduce global session state via context or Zustand. For Phase 1, narrowly scoped is correct.

---

## 5. How to Test

From `/tmp/cue-phase1-worktree` on uno, with `PATH=/opt/homebrew/bin:/Users/uno/.cargo/bin:/usr/local/bin:$PATH`:

```bash
# Branch-base verification
$ git merge-base feat/phase-0-foundation HEAD
b4a8207be862f320c19d8f246dcf30436cf978e3
# ✅ expected (was fa19623 before)

# Workspace format
$ cargo fmt --all --check
# ✅ no output = pass

# Clippy (deny warnings)
$ cargo clippy --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 10.36s
# ✅ pass

# Full workspace build
$ cargo build --all-targets
    Finished `dev` profile ...
# ✅ pass

# Full test suite
$ cargo test --all-targets
test result: ok. 26 passed; 0 failed  (cue-core)
test result: ok. 14 passed; 0 failed  (cue-daemon)
# ✅ 40 tests pass, 0 failed

# Dashboard frontend build
$ cd crates/cue-dashboard/ui && npm run build
vite v6.4.2 building for production...
✓ 1699 modules transformed.
dist/index.html                   0.39 kB │ gzip:  0.26 kB
dist/assets/index-B3rzTlbj.css   12.51 kB │ gzip:  3.32 kB
dist/assets/index-aLQl67RR.js   291.05 kB │ gzip: 92.87 kB
✓ built in 1.22s
# ✅ pass

# Whitespace check against the new phase-1 base
$ git diff --check b4a8207..HEAD
# ✅ no output = pass
```

---

## 6. Known Limitations

- **`useSessionEvents` is only mounted in Chats page, not globally.** When the user is on `/prompts` or `/settings` and the daemon fires `session:created`, there's no refresh (because no other page currently displays the session list). This is intentional for Phase 1 — Phase 2 will either (a) mount the hook in `DashboardLayout` with a shared store, or (b) keep it page-scoped and add equivalent hooks for other data types (`prompt:created`, `shortcut:changed`, etc.). Either approach is documented in the Phase 2 plan.
- **No end-to-end test of the event path.** The subscription exists and the callback will fire when the daemon emits, but we haven't added an integration test that actually emits `session:created` from Rust and verifies the React list updates. Such a test would require a Tauri test harness (e.g. `tauri-test`) which is Phase 6/7 work per the master plan.
- **Daemon still does not emit `session:created`.** The Tauri command `create_session` in `crates/cue-dashboard/src/commands.rs` should in Phase 2 call `app_handle.emit("session:created", &session)` after the DB insert. Today it returns the created Session but doesn't emit. This is tracked as a Phase 2 task (part of Session UX — D2.x) rather than a Phase 1 regression.
- **D0.2 regression tests still deferred** per Phase 0 V2 nits: regression tests for invalid DB row conversion, archived_at clearing, and duplicate turn_index. Will land in Phase 2 alongside session lifecycle tests.

---

## Hand-back

All 3 Phase 1 blockers fixed. Branch `feat/phase-1-dashboard-shell` at `3dcc7df` is clean against:

- `cargo fmt --all --check` ✅
- `cargo clippy --all-targets -- -D warnings` ✅
- `cargo build --all-targets` ✅
- `cargo test --all-targets` ✅ (40 pass, 0 fail)
- `cd crates/cue-dashboard/ui && npm run build` ✅
- `git diff --check b4a8207..HEAD` ✅ (no output)
- merge-base check: `b4a8207` ✅

Ready for Codex re-review.

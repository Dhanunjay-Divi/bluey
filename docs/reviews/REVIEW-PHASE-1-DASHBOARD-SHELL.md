# REVIEW: Phase 1 Dashboard Shell

**Commit range:** `fa19623..83cd417`
**Reviewer:** Codex
**Date:** 2026-05-12

## Per-Task Review

### D1.1 — Dashboard npm Dependencies and Tailwind 4

| Field | Value |
|-------|-------|
| Files | `crates/cue-dashboard/ui/package.json`, `crates/cue-dashboard/ui/package-lock.json`, `crates/cue-dashboard/ui/vite.config.ts`, `crates/cue-dashboard/ui/src/index.css` |
| Verdict | 🟢 accept |

**Findings:**
- 🟢 `npm run build` passes in `crates/cue-dashboard/ui`.
- 🟢 Tailwind 4 is wired through the Vite plugin and the generated lockfile is present.

---

### D1.2/D1.3 — Database State and Tauri Commands

| Field | Value |
|-------|-------|
| Files | `crates/cue-dashboard/src/lib.rs`, `crates/cue-dashboard/src/commands.rs`, `crates/cue-dashboard/Cargo.toml` |
| Verdict | 🟡 minor nit |

**Findings:**
- 🟢 The dashboard manages a shared `DbState(pub Mutex<Database>)` and exposes the expected five session commands.
- 🟢 Command lock scopes are short and do not hold the mutex across async awaits.
- 🟡 `crates/cue-dashboard/src/lib.rs:31-36` uses `expect("failed to open database")`, so a corrupted or inaccessible DB crashes the dashboard during startup. For Phase 1 shell this is acceptable, but before product use this should become a recoverable UI error or repair flow.
- 🟡 `crates/cue-dashboard/src/lib.rs:35` falls back to `"bluey.db"` if the platform data path is non-UTF8. Low risk, but it can silently move the DB into the working directory.

---

### D1.4 — Global Shortcut

| Field | Value |
|-------|-------|
| Files | `crates/cue-dashboard/src/lib.rs`, `crates/cue-dashboard/capabilities/default.json` |
| Verdict | 🟢 accept |

**Findings:**
- 🟢 `CmdOrCtrl+Shift+D` / `Ctrl+Shift+D` is registered and toggles the main window.
- 🟢 Required global shortcut permissions were added to dashboard capabilities.

---

### D1.5/D1.6 — React Shell and Chats Page

| Field | Value |
|-------|-------|
| Files | `crates/cue-dashboard/ui/src/App.tsx`, `crates/cue-dashboard/ui/src/components/DashboardLayout.tsx`, `crates/cue-dashboard/ui/src/components/Sidebar.tsx`, `crates/cue-dashboard/ui/src/pages/Chats.tsx`, `crates/cue-dashboard/ui/src/pages/Placeholder.tsx` |
| Verdict | 🟡 minor nit |

**Findings:**
- 🟢 The shell defines the planned nine routes and sidebar navigation.
- 🟢 `/chats` lists sessions, creates sessions, archives sessions, and refreshes after mutations.
- 🟡 `crates/cue-dashboard/ui/src/App.tsx:8` uses `BrowserRouter`. This is workable for in-app navigation, but Tauri/static builds usually prefer `HashRouter` or an explicit fallback so reloads/deep links on `/chats` do not depend on server history fallback behavior.
- 🟡 `crates/cue-dashboard/ui/src/pages/Chats.tsx:100-105` exposes archive but not delete, even though the backend command exists. This is fine for Phase 1 if archive is the intended primary path.

---

### D1.7 — Command Palette

| Field | Value |
|-------|-------|
| Files | `crates/cue-dashboard/ui/src/components/CommandPalette.tsx` |
| Verdict | 🟢 accept |

**Findings:**
- 🟢 `Cmd/Ctrl+K` opens the command palette and commands navigate to shell routes.
- 🟡 The "New session" command only navigates to `/chats`; the implementation doc already lists actual creation as a follow-up.

---

### D1.8 — Dashboard Event Channel

| Field | Value |
|-------|-------|
| Files | `crates/cue-dashboard/ui/src/hooks/useSessionEvents.ts`, `crates/cue-dashboard/ui/src/components/DashboardLayout.tsx`, `crates/cue-dashboard/ui/src/pages/Chats.tsx` |
| Verdict | 🔴 blocker |

**Findings:**
- 🔴 `crates/cue-dashboard/ui/src/hooks/useSessionEvents.ts:18-26` defines a listener, but `rg "useSessionEvents"` shows no call site. The hook is never mounted in `DashboardLayout`, `Chats`, or `App`, so the dashboard does not actually listen for `session:created`. This leaves D1.8 as a dormant helper file rather than a wired event channel.

---

### Phase 1 Handoff Docs

| Field | Value |
|-------|-------|
| Files | `docs/work/IMPL-PHASE-1-DASHBOARD-SHELL.md`, `docs/work/PHASE-1-HANDOFF-FOR-CODEX-REVIEW.md` |
| Verdict | 🟡 minor nit |

**Findings:**
- 🟡 The handoff says "3 commits", but the reviewed range has 4 commits including `83cd417 docs(work): add Phase 1 implementation and handoff docs`.
- 🟡 The handoff says the branches are cleanly rebased, but `feat/phase-1-dashboard-shell` is still based on `fa19623`, not the Phase 0 fix tip `3a7c571`. `git merge-tree 3a7c571 feat/phase-1-dashboard-shell` reports a clean synthetic merge, so this should be easy to fix.

## Cross-Task Findings

- The dashboard shell shape is good: backend commands, navigation, session CRUD, command palette, and npm build all exist.
- The branch itself does not currently pass the full workspace verification because it lacks the Phase 0 fix commits. This is branch hygiene, not a new Phase 1 code regression, but it must be corrected before merge.
- D1.8 needs one small real wiring change: mount the event listener somewhere persistent and prove it can refresh/invalidate session state when events arrive.

## Build & Test Verification

```bash
cd /private/tmp/cue-phase1-worktree
cargo fmt -p cue-dashboard -- --check     # ✅
cd crates/cue-dashboard/ui && npm run build # ✅
git diff --check fa19623..HEAD            # ✅
cargo build --all-targets                 # ✅, with pre-Phase-0-fix dashboard warnings
cargo test --all-targets                  # ✅ 40 passed, 0 failed
cargo fmt --all --check                   # ❌ fails because branch is not rebased onto Phase 0 fixes
cargo clippy --all-targets -- -D warnings # ❌ fails because branch is not rebased onto Phase 0 fixes
```

## Overall Verdict

🔴 **REQUEST CHANGES** — Blockers must be resolved.

## Follow-ups for Next Batch

- Rebase Phase 1 onto the accepted Phase 0 fix tip (`3a7c571`) or otherwise include the Phase 0 fixes before merge.
- Mount `useSessionEvents` in a persistent component and wire it to refresh/invalidate session state, or emit/listen in the `/chats` page where the event has visible behavior.
- Consider switching dashboard routing to `HashRouter` or document why Tauri production routing is safe with `BrowserRouter`.
- Convert dashboard startup DB failure into a recoverable UI path before production hardening.

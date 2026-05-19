# Phase 0 Foundation — Handoff for Codex Review

**Branch**: `feat/phase-0-foundation`
**Base**: `main` (commit `8034be1` "Initial Bluey native AI overlay")
**Tip**: `6ad6952`
**Date**: 2026-05-12
**Author**: kiro (3 parallel implementation agents + integration)

---

## Scope

Phase 0 of the bluey master plan — **Foundation**. Three units of work delivered in parallel, integrated on one branch:

1. **D0.1 Tauri 2 workspace scaffold** — new `crates/cue-dashboard` Tauri crate with NSPanel macOS + React 19 + Vite frontend scaffold
2. **D0.2 Session model + SQLite schema** — Session/Turn types in `cue-core`, migration + Database wrapper in `cue-daemon` with 4 integration tests
3. **Workflow docs (Phase 8 moved early)** — CLAUDE.md, CHANGELOG.md, PR template, CI workflow, work-doc templates, .codex/agents + skills

---

## Commits (15 total, oldest → newest)

```
76e6022 docs: add CLAUDE.md development rules and conventional workflow
220dc34 chore: add CHANGELOG.md with Keep-a-Changelog format
9b1b891 ci: add GitHub Actions workflow for cargo fmt/clippy/test
234a4ba docs(work): add IMPL/REVIEW/FIX templates + work-doc index
fe2f5f6 chore(codex): add .codex/agents/ + .codex/skills/ for AI workflow
89b9801 docs(github): add PULL_REQUEST_TEMPLATE
c1f496f feat(core): add Session + Turn types with UUID ids [D0.2]
53cbd46 chore(workspace): add cue-dashboard to Cargo workspace [D0.1]
8c1dea7 feat(dashboard): scaffold Tauri 2 crate with NSPanel [D0.1]
6ca3ce9 feat(dashboard/ui): initial React 19 + Vite scaffold [D0.1]
06d725b docs(work): add D0.1 implementation notes [D0.1]
f987c9c feat(daemon): add SQLite migration for sessions + turns [D0.2]
d39cd65 feat(daemon): add Database wrapper with session CRUD [D0.2]
8a90d44 docs(work): add D0.2 session model implementation doc
6ad6952 chore: update Cargo.lock with rusqlite dependency
```

## File stats

```
61 files changed, 20566 insertions(+), 921 deletions(-)
```

## Implementation docs per task

- `docs/work/IMPL-D0.1-TAURI-SCAFFOLD.md` — what D0.1 added, build results, review checklist
- `docs/work/IMPL-D0.2-SESSION-MODEL.md` — schema summary, API surface, test results, design decisions (rusqlite over sqlx), known follow-ups
- `docs/work/IMPL-WORKFLOW-DOCS.md` — all workflow files created, CI configuration, agent configs

---

## Build + test verification

```bash
# Workspace release build on macOS arm64
$ cargo build --release
   Compiling cue-dashboard v0.1.0 (/Users/uno/Downloads/cue/crates/cue-dashboard)
warning: `cue-dashboard` (lib) generated 8 warnings (5 duplicates)   # cocoa deprecation warnings from tauri-nspanel — matches pluely reference
    Finished `release` profile [optimized] target(s) in 34.93s
# ✅ PASS

# cue-daemon test suite (D0.2 Database CRUD + pre-existing daemon tests)
$ cargo test -p cue-daemon
test result: ok. 14 passed; 0 failed
# ✅ PASS (includes 4 new D0.2 tests: test_create_and_get_session, test_list_sessions_by_status, test_append_turn_and_list_turns, test_cascade_delete_session_removes_turns)

# cue-core test suite (D0.2 types + pre-existing intelligence/meeting tests)
$ cargo test -p cue-core
test result: ok. 26 passed; 0 failed
# ✅ PASS
```

---

## What changed per layer

### NATIVE layer
- **No changes.** D0.1 scaffold is all Tauri/Rust. N0.1 native-overlay audit is deferred to a separate task (not in this batch).

### DAEMON layer (`crates/cue-core` + `crates/cue-daemon`)

**`cue-core`**:
- New module `src/session.rs` with `Session`, `Turn`, `NewTurn`, `SessionStatus`, `Lane`, `Skill` types
- UUIDv4 IDs (using workspace `uuid` crate with `v4` + `serde` features)
- Serde-enabled for JSON serialization over IPC and persistence
- Enums use `rename_all` attributes for stable wire format (`"snap_edit"`, `"system-design"`, etc.)
- Exported from `lib.rs`

**`cue-daemon`**:
- New workspace dep: `rusqlite = { version = "0.32", features = ["bundled"] }` (bundled = no system SQLite needed, static link)
- New module `src/db/mod.rs` — `Database` struct wrapping a `rusqlite::Connection`
- Runs migrations from `infra/migrations/*.sql` at startup (idempotent)
- 8 CRUD methods: `create_session`, `get_session`, `list_sessions(status, limit)`, `update_session_status`, `append_turn`, `list_turns`, `archive_session`, `delete_session`
- 4 integration tests using `:memory:` SQLite
- All tests pass

**`infra/migrations/`**:
- New file `002_sessions.sql` — creates `sessions` + `turns` tables with indexes + FK cascade
- Schema matches CUE-BLUEY-V3-PART-C-APPENDICES.md section S1

### DASHBOARD layer (`crates/cue-dashboard` — NEW CRATE)

- Tauri 2 binary crate with `macos-private-api` feature enabled
- Plugins wired: `tauri-plugin-shell`, `tauri-plugin-opener`, `tauri-plugin-dialog`, `tauri-plugin-fs`
- `tauri-nspanel` (git, v2 branch) on macOS only
- `tauri.conf.json`: window label `main`, 1200x800, center, decorations, traffic lights (macOS), **`content_protected: true`** (stealth)
- `capabilities/default.json` with basic Tauri 2 capabilities
- Build script `build.rs` auto-creates `ui/dist` placeholder so Rust-only builds work without npm
- Placeholder RGBA icons (Tauri validates at compile time)
- `src/lib.rs::run()` registers command `get_app_version() -> String`, sets up NSPanel on macOS with panel_delegate logs
- `src/main.rs` calls `cue_dashboard::run()`

**Dashboard UI (`crates/cue-dashboard/ui/`)** — React 19 scaffold:
- `package.json` — React 19.0 + Vite 6 + TypeScript 5.6 (deps declared, not installed; `npm install` is left for first real dashboard work)
- `vite.config.ts` with React plugin
- `tsconfig.json`
- `index.html`
- `src/main.tsx` + `src/App.tsx` — minimal placeholder: "bluey dashboard — Phase 0 scaffold"
- `src/index.css` — system font + basic reset
- `.gitignore` for `node_modules`, `dist`

### INFRA layer

**Root `Cargo.toml`**:
- Added `"crates/cue-dashboard"` to workspace members
- Added `rusqlite` to `[workspace.dependencies]`

**`.github/workflows/ci.yml`**:
- Matrix: `macos-latest` + `ubuntu-latest`
- Steps: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo build`, `cargo test`
- Cargo target cached via `Swatinem/rust-cache`

**`.github/PULL_REQUEST_TEMPLATE.md`** — required sections: Summary, Task IDs, Type, Affected components, Testing, Risks, Review checklist, CHANGELOG updated Y/N

### Workflow discipline (`.codex/` + `docs/work/` + `CLAUDE.md` + `CHANGELOG.md`)

**`CLAUDE.md`** (73 lines):
- Architecture 1-paragraph summary
- Hard rules: no push to main, always branch for tasks, conventional commits, CHANGELOG per PR
- Code style: `cargo fmt`, `cargo clippy -- -D warnings`, TS prettier + ESLint
- Test + review discipline
- Target audience: codex + humans

**`CHANGELOG.md`** (Keep-a-Changelog):
- `## [Unreleased]` with `### Added` noting the master plan + Phase 0 work

**`docs/work/`** (4 templates + README):
- `TEMPLATE-IMPL.md` — 7-section implementation doc template
- `TEMPLATE-REVIEW.md` — batch-review template with per-task verdict format
- `TEMPLATE-FIX.md` — 6-section bug fix template (Root Cause / Summary / Files / Edge Cases / How to Test / Known Limitations)
- `README.md` — indexes work docs + describes workflow

**`.codex/`** (codex CLI config):
- `config.toml` — codex defaults
- `agents/` — 7 specialized agent configs (code-reviewer, backend-architect, frontend-developer, test-engineer, debugger, ui-ux-designer, fullstack-developer). Each under 100 lines.
- `skills/` — 10 reusable skill cards (tauri-patterns, rust-async, macos-native, windows-native, audio-dsp, llm-streaming, prompt-engineering, rag-sqlite-vec, testing-patterns, review-checklist). Each under 200 lines.

---

## Task ID → commit map (for reviewer cross-reference)

| Task ID | Commits | Status |
|---|---|---|
| D0.1 | `53cbd46`, `8c1dea7`, `6ca3ce9`, `06d725b` | ✅ complete |
| D0.2 | `c1f496f`, `f987c9c`, `d39cd65`, `8a90d44`, `6ad6952` | ✅ complete |
| Phase 8 (workflow docs, done early) | `76e6022`, `220dc34`, `9b1b891`, `234a4ba`, `fe2f5f6`, `89b9801` | ✅ complete |

**Not in this batch** (scope decisions):
- **D0.3 IPC strategy** — decided implicitly (Tauri single-binary approach established by D0.1); no explicit task needed
- **D0.4 Native overlay ↔ daemon session-id protocol** — deferred; needs native overlay code changes which are a separate concern
- **N0.1 Native overlay audit** — deferred; can be done as a read-only audit task
- **C0.1 End-to-end session-ID model** — partial (session IDs exist in daemon DB; full end-to-end flow lands in Phase 1 when dashboard wires up `create_session`/`load_session` Tauri commands)

---

## Known quirks / things to flag during review

1. **Rustc version bump during build**: Agent 1 had to update rustc 1.87.0 → 1.95.0 to satisfy a dependency MSRV. This was done on uno's global toolchain. Not tracked in repo. If you do a fresh clone on a different machine, `rustup update` first.

2. **Warning: deprecated cocoa APIs in `tauri-nspanel`** — 8 warnings in `cue-dashboard` lib build. Same warnings appear in pluely reference. Not our code. Filed as upstream tauri-nspanel issue territory; ignore for now.

3. **`npm install` not yet run** in `crates/cue-dashboard/ui/`. Package.json + config present, but first real dashboard work will need to `cd crates/cue-dashboard/ui && npm install`. Rust-only builds work because `build.rs` creates `ui/dist` placeholder.

4. **CI workflow hasn't run yet**. The workflow is committed but won't execute until this branch is pushed. When pushed, expect the first CI run to need cache warmup (slower than subsequent runs).

5. **Integration branch constructed via cherry-pick** because 3 parallel agents cross-contaminated each other's branches during concurrent work. The final branch history is clean and linear; commits maintain their original SHA identities from the feature branches.

6. **`Cargo.lock` has 74 new lines** from the rusqlite bundled dep (SQLite C code + deps). Normal.

---

## Review checklist for codex

### Correctness
- [ ] `Session` / `Turn` type definitions in `crates/cue-core/src/session.rs` match the SQLite schema in `infra/migrations/002_sessions.sql` (column names, nullability, types)
- [ ] `Database::append_turn` correctly increments `turn_index` based on existing turns for the session
- [ ] `Database::delete_session` relies on `ON DELETE CASCADE` FK — verify the FK constraint actually fires on rusqlite (pragma foreign_keys=ON must be set; check whether Database::open runs this)
- [ ] `SessionStatus` serde `rename_all = "lowercase"` produces wire values matching what the DB stores (`"active"`, `"paused"`, `"archived"`)
- [ ] UUID generation uses `Uuid::new_v4()` from the workspace uuid crate (not a custom RNG)
- [ ] Tauri dashboard `tauri.conf.json` has `content_protected: true` on the main window

### Safety
- [ ] `Database::open(path)` handles migration failures gracefully (does it rollback partial migrations?)
- [ ] `rusqlite` is thread-safe via `Mutex` or `Connection::open_with_flags` with `SQLITE_OPEN_FULL_MUTEX`? If daemon has multiple async tasks, concurrent access needs to be addressed before Phase 2
- [ ] Does `build.rs` silently swallow errors if `ui/dist` can't be created?

### Tests
- [ ] 4 D0.2 integration tests are meaningful (not just asserts-on-Ok); they cover happy path, status filter, cascade delete
- [ ] Pre-existing 26 core + 14 daemon tests still pass (confirmed above)
- [ ] Are there Rust tests for the D0.1 Tauri scaffold? (Likely no — scaffolding is hard to unit test; smoke test will happen when dashboard actually renders)

### Documentation
- [ ] `docs/work/IMPL-D0.1-TAURI-SCAFFOLD.md` / `IMPL-D0.2-SESSION-MODEL.md` / `IMPL-WORKFLOW-DOCS.md` each follow the `TEMPLATE-IMPL.md` structure
- [ ] `CHANGELOG.md [Unreleased]` lists every meaningful change in this batch
- [ ] `CLAUDE.md` is discoverable (at repo root, linked from README?)

### Workflow
- [ ] `.github/PULL_REQUEST_TEMPLATE.md` has all required sections
- [ ] CI workflow compiles + tests on both macos-latest and ubuntu-latest
- [ ] `.codex/agents/` configs have valid TOML syntax
- [ ] `.codex/skills/` cards are under the 200-line cap

### Style / hygiene
- [ ] `cargo fmt --check` passes on all new Rust files
- [ ] `cargo clippy -- -D warnings` passes (except known tauri-nspanel deprecation warnings)
- [ ] No commented-out code in new files
- [ ] No `todo!()` / `unimplemented!()` / `panic!()` in non-test code

### Next-batch readiness
- [ ] Phase 1 (Dashboard shell) has everything it needs: Tauri scaffold ✓, session CRUD ✓, IPC foundation ✓, Cmd+Shift+D can be wired next
- [ ] Phase 2 (Session UX) can build on `Database::*` methods directly via Tauri commands

---

## Verdict request

Codex: please review this batch and return a `docs/work/REVIEW-PHASE-0-FOUNDATION.md` following `TEMPLATE-REVIEW.md`, with:

- Per-commit line-by-line review (confirm correctness, spot bugs)
- Overall verdict: 🟢 ACCEPT / 🟡 ACCEPT WITH NITS / 🔴 REQUEST CHANGES
- Any follow-ups to bundle into the next batch (Phase 1)

After your verdict, we'll decide:
- Merge `feat/phase-0-foundation` into `main` (squash or preserve commit history)
- Delete the 3 intermediate branches (`feat/phase-0-d0.1-tauri-scaffold`, `feat/phase-0-d0.2-session-model`, `feat/phase-0-workflow-docs`)
- Start Phase 1

---

## Continuity for next batch

When Phase 1 starts, the branch will be `feat/phase-1-dashboard-shell` off merged main.
Phase 1 scope: Cmd+Shift+D opens Tauri dashboard window, dashboard↔daemon IPC working both directions, placeholder sidebar navigation with 8 routes stubbed. ~1 week solo.

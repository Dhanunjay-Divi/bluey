# REVIEW: Phase 0 Foundation

**Commit range:** `8034be1..fa19623`
**Reviewer:** Codex
**Date:** 2026-05-12

## Per-Task Review

### Phase 8 Early — Workflow Docs, CI, Codex Agents

| Field | Value |
|-------|-------|
| Files | `CLAUDE.md`, `CHANGELOG.md`, `.github/**`, `.codex/**`, `docs/work/**` |
| Verdict | 🔴 blocker |

**Findings:**
- 🔴 `.github/workflows/ci.yml:35` uses `hashFiles(**/Cargo.lock)` without quoting the glob. GitHub Actions expressions require a string argument, so this cache key expression should be `hashFiles('**/Cargo.lock')`. As written, the CI workflow is not reliable.
- 🔴 `.codex/agents/*.toml` is invalid TOML. Each agent file declares `[constraints]` and then uses bare `- "..."` list items, for example `.codex/agents/code-reviewer.toml:22-26`. `tomllib` rejects all seven files. These need a real key such as `items = [...]` or `constraints = [...]`.
- 🔴 `.github/workflows/ci.yml:41-42` runs clippy with `-D warnings`, but the dashboard currently emits `unexpected_cfgs` warnings from `panel_delegate!` in `crates/cue-dashboard/src/macos.rs:24`. CI will reject that once clippy is installed.
- 🔴 `.github/workflows/ci.yml:16` includes `ubuntu-latest`, but the workflow installs no Linux Tauri/WebKit/GTK system packages before `cargo build --all-targets` and `cargo test --all-targets`. The new dashboard crate makes the Linux lane likely fail on a clean runner.
- 🟡 `git diff --check 8034be1..fa19623` reports trailing whitespace in `.github/PULL_REQUEST_TEMPLATE.md`, `docs/work/IMPL-D0.2-SESSION-MODEL.md`, and the work templates.

---

### D0.1 — Tauri 2 Dashboard Scaffold

| Field | Value |
|-------|-------|
| Files | `crates/cue-dashboard/**`, root `Cargo.toml`, `Cargo.lock`, `docs/work/IMPL-D0.1-TAURI-SCAFFOLD.md` |
| Verdict | 🔴 blocker |

**Findings:**
- 🔴 `cargo fmt --all --check` fails on `crates/cue-dashboard/src/macos.rs:14` and other files in this batch. This violates the repo rules in `CLAUDE.md` and blocks CI.
- 🔴 `crates/cue-dashboard/src/macos.rs:24` emits `unexpected_cfgs` warnings from the NSPanel delegate macro. The handoff describes these as ignorable cocoa deprecation warnings, but the actual warnings are check-cfg warnings that become build blockers under the committed clippy policy.
- 🟡 `crates/cue-dashboard/capabilities/default.json:7-13` grants broad `fs:default`, `dialog:default`, and shell/open permissions to the main window. For Phase 0 scaffolding this is understandable, but before wiring real data/file flows it should be narrowed to the specific commands and paths Bluey actually needs.

---

### D0.2 — Session Model and SQLite Schema

| Field | Value |
|-------|-------|
| Files | `crates/cue-core/src/session.rs`, `crates/cue-core/src/lib.rs`, `crates/cue-daemon/src/db/mod.rs`, `infra/migrations/002_sessions.sql`, `docs/work/IMPL-D0.2-SESSION-MODEL.md` |
| Verdict | 🔴 blocker |

**Findings:**
- 🔴 `cargo fmt --all --check` fails on `crates/cue-core/src/lib.rs:1`, `crates/cue-daemon/src/db/mod.rs:63`, `crates/cue-daemon/src/db/mod.rs:110`, and `crates/cue-daemon/src/db/mod.rs:256`. The code must be formatted before merge.
- 🟡 `crates/cue-daemon/src/db/mod.rs:185-214` silently maps invalid UUID/status/lane data to defaults with `unwrap_or_default()` / `unwrap_or(...)`. That can hide database corruption as the nil UUID or an active/snap session instead of surfacing a conversion error.
- 🟡 `crates/cue-daemon/src/db/mod.rs:93-104` leaves `archived_at` populated when a session moves from archived back to active/paused because of `COALESCE(?3, archived_at)`. If unarchiving is supported, this should clear the timestamp.
- 🟡 `crates/cue-daemon/src/db/mod.rs:118-153` computes `turn_index` with `MAX(turn_index) + 1` outside a transaction, and `infra/migrations/002_sessions.sql:21-38` has no `UNIQUE(session_id, turn_index)`. The current single-connection wrapper is fine for Phase 0 tests, but this should be hardened before dashboard/daemon concurrent writes.

---

### Phase 0 Handoff

| Field | Value |
|-------|-------|
| Files | `docs/work/PHASE-0-HANDOFF-FOR-CODEX-REVIEW.md` |
| Verdict | 🟡 minor nit |

**Findings:**
- 🟡 The handoff says the tip is `6ad6952`, but the actual reviewed tip is `fa19623` after adding the handoff commit. It also lists 15 commits while this review range contains 16 commits including the handoff.
- 🟡 The handoff says dashboard warnings are cocoa deprecations from `tauri-nspanel`; local verification shows `unexpected_cfgs` warnings from the `panel_delegate!` macro instead.

## Cross-Task Findings

- The committed workflow sets a strict bar (`fmt`, clippy `-D warnings`, all-target build/test), but the branch does not currently meet that bar.
- The new database/session model is a useful foundation and has meaningful CRUD tests, but it needs formatting and a few persistence-hardening fixes before it becomes the source of truth for product sessions.
- The dashboard scaffold compiles locally on macOS, but the CI and cross-platform story need to be made explicit now that `cue-dashboard` is in the workspace.

## Build & Test Verification

```bash
cargo fmt --all --check                 # ❌ fails
cargo clippy --all-targets -- -D warnings # ❌ local clippy component missing; committed CI would also hit dashboard warnings
cargo build --all-targets               # ✅ macOS build passes with 8 cue-dashboard warnings
cargo test --all-targets                # ✅ macOS tests pass, 40 passed, 0 failed
git diff --check 8034be1..fa19623       # ❌ trailing whitespace
python3 tomllib parse .codex/agents     # ❌ all 7 agent TOML files fail to parse
```

## Overall Verdict

🔴 **REQUEST CHANGES** — Blockers must be resolved.

## Follow-ups for Next Batch

- Fix formatting and whitespace first, then rerun the exact CI commands locally.
- Repair `.codex/agents/*.toml` so the workflow configs are machine-readable.
- Fix the GitHub Actions cache expression and either install Tauri Linux dependencies in the Ubuntu lane or scope CI to crates/platforms that are ready.
- Resolve or explicitly allow the `panel_delegate!` `unexpected_cfgs` warnings before keeping clippy `-D warnings`.
- Add persistence hardening for session status transitions, row conversion errors, and turn index uniqueness before concurrent UI/daemon writes land.

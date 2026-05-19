# REVIEW: Phase 0 Foundation V2

**Commit range:** `fa19623..3a7c571`
**Reviewer:** Codex
**Date:** 2026-05-12

## Per-Task Review

### Phase 0 Fixes — Original Review Blockers

| Field | Value |
|-------|-------|
| Files | `.github/workflows/ci.yml`, `.codex/agents/*.toml`, `crates/cue-*/Cargo.toml`, `crates/cue-daemon/src/db/mod.rs`, `infra/migrations/003_turns_unique_index.sql`, docs/templates |
| Verdict | 🟡 minor nit |

**Findings:**
- 🟢 `cargo fmt --all --check` now passes.
- 🟢 `.github/workflows/ci.yml:52` now quotes `hashFiles('**/Cargo.lock')`.
- 🟢 All seven `.codex/agents/*.toml` files now parse successfully as TOML.
- 🟢 `cargo clippy --all-targets -- -D warnings` now passes locally.
- 🟢 `.github/workflows/ci.yml:21-36` now installs Linux Tauri dependencies for the Ubuntu lane.
- 🟢 `git diff --check 8034be1..HEAD` now passes with no trailing whitespace output.
- 🟢 `crates/cue-daemon/src/db/mod.rs:195-240` now surfaces invalid UUID/status/lane values as `FromSqlConversionFailure` instead of silently defaulting.
- 🟢 `crates/cue-daemon/src/db/mod.rs:94-100` now clears `archived_at` for non-archived statuses.
- 🟢 `crates/cue-daemon/src/db/mod.rs:116-126` wraps append-turn writes in a transaction, and `infra/migrations/003_turns_unique_index.sql:1-2` adds uniqueness for `(session_id, turn_index)`.
- 🟡 The fix doc says the hashFiles fix was `hashFiles(**/Cargo.lock)` even though the actual CI file is correctly quoted. Documentation typo only.
- 🟡 `crates/cue-core/Cargo.toml`, `crates/cue-daemon/Cargo.toml`, and `crates/cue-cli/Cargo.toml` use crate-level clippy allows for pre-existing warnings. Acceptable as a short-term foundation unblocker, but these should be burned down in a cleanup pass.

---

### D0.2 Persistence Hardening

| Field | Value |
|-------|-------|
| Files | `crates/cue-daemon/src/db/mod.rs`, `infra/migrations/003_turns_unique_index.sql` |
| Verdict | 🟡 minor nit |

**Findings:**
- 🟢 The original persistence nits were addressed.
- 🟡 There are no new regression tests covering invalid row conversion, `archived_at` clearing, or duplicate `turn_index` rejection. The existing happy-path tests still pass, but the newly hardened behavior deserves tests in the next daemon batch.

## Cross-Task Findings

- The original 🔴 blockers from `REVIEW-PHASE-0-FOUNDATION.md` are resolved.
- The local verification bar is now aligned with the workflow bar.
- The remaining issues are cleanup/test-depth items, not merge blockers.

## Build & Test Verification

```bash
cargo fmt --all --check                   # ✅
cargo clippy --all-targets -- -D warnings # ✅
cargo build --all-targets                 # ✅
cargo test --all-targets                  # ✅ 40 passed, 0 failed
git diff --check 8034be1..HEAD            # ✅
python3 TOML parse .codex/agents/*.toml   # ✅
```

## Overall Verdict

🟡 **ACCEPT WITH NITS** — Merge after accepting the documented cleanup follow-ups.

## Follow-ups for Next Batch

- Add regression tests for invalid DB row conversion, unarchive timestamp clearing, and duplicate turn indexes.
- Replace broad crate-level clippy allows with targeted code fixes over time.
- Fix the small typo in `docs/work/FIX-PHASE-0-FOUNDATION.md` around the quoted `hashFiles` example.

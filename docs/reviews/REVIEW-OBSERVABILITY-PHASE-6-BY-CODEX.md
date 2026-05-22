# REVIEW: Observability Phase 6 — standard field migration sweep

**Commit range:** `0485a9b..60ff7fd`  
**Reviewer:** Codex  
**Date:** 2026-05-22

## Verdict

🟢 **ACCEPT** — Phase 6 is shippable. No fix round needed.

## What I Reviewed

- `docs/rounds/OBSERVABILITY-PHASE-6-FOR-CODEX-REVIEW.md`
- `scripts/migrate-tracing-fields.py`
- `scripts/analyze-tracing-calls.py`
- `server/src/api/account.rs`
- `server/src/api/auth_routes.rs`
- `server/src/api/router.rs`
- `server/src/api/stt.rs`
- `server/src/api/usage.rs`
- `crates/cue-dashboard/src/lib.rs`

## What's Right

- The targeted trace-field migration is idempotent: the migration script dry-run reports zero remaining substitutions across all target files.
- The analyzer reports no PII findings after the sweep.
- The raw `account_id` tracing fields in the touched server/dashboard paths were replaced with `account_id_hash = %cue_core::account_id_hash_prefix(...)`.
- The removed `email` tracing fields were redundant with account hash context and are not needed for production support correlation.
- The analyzer JSON fix is directionally right: serializing `(component, level)` tuple keys as strings keeps the output consumable by downstream tooling.
- The known `session -> session_id` analyzer report remains a false positive from the string-literal heuristic, not an actual trace-field site.

## Blockers

None.

## Nits

None blocking. Two notes to keep with the observability backlog:

- `scripts/migrate-tracing-fields.py` is still a heuristic line-based migration tool, not a Rust parser. It is fine for this completed sweep, but future broad rewrites should keep using dry-run plus code review rather than trusting it blindly.
- Inline `account_id_hash_prefix(...)` calls are acceptable at the current trace volume. If hot-path tracing grows, compute once near request auth context and reuse the value.

## Pipeline State

Commands I ran:

```bash
python3 scripts/migrate-tracing-fields.py --dry-run
python3 scripts/analyze-tracing-calls.py
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cd server && cargo fmt --all --check
cd server && cargo clippy --all-targets -- -D warnings
cd server && cargo test
```

Observed:

- Migration dry-run: `account_id_subs=0`, `email_drops=0`, `session_renames=0`.
- Analyzer: 206 trace call sites, 47 conformant, 159 still non-conformant, no PII findings.
- Workspace Rust gate: fmt clean, clippy clean, tests pass.
- Server Rust gate: fmt clean, clippy clean, 90 tests pass.

## Recommended Action

Proceed to the next observability phase. No Phase 6 fix round is needed.

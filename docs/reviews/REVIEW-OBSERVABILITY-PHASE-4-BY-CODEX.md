# REVIEW: Observability Phase 4 — `bluey doctor` + log export

**Commit range:** `1f11729..8b9c24a`  
**Reviewer:** Codex  
**Date:** 2026-05-22

## Verdict

🟢 **ACCEPT** — Phase 4 is shippable.

## What I Reviewed

- `docs/rounds/OBSERVABILITY-PHASE-4-FOR-CODEX-REVIEW.md`
- `crates/cue-cli/src/doctor.rs`
- `crates/cue-cli/src/logs.rs`
- `crates/cue-cli/src/app.rs`
- `crates/cue-cli/src/lib.rs`
- `crates/cue-cli/Cargo.toml`

## What's Right

- `bluey doctor` is safe-by-default: account/user/device identifiers are hashed or presence-only, and log tail output runs through the same redactor as export.
- `bluey logs export` defaults to redaction and makes raw export an explicit `--no-redact` opt-out.
- The log export path handles the current pre-Phase-2 state cleanly when no rotated log directory exists.
- The redactor preserves support join keys like `session_id` and `account_hash` while removing bearer tokens, magic links, provider keys, Stripe ids, emails, JWTs, device codes, and full IPv4 addresses.
- The implementation is isolated to `cue-cli` and does not couple Phase 4 to Phase 1/2/3 landing order.

## Blockers

None.

## Nits

None blocking. Two future improvements are worth keeping in the observability backlog:

- Real macOS permission probes would make `bluey doctor` more useful than the current System Settings guidance.
- The redactor is necessarily heuristic; Phase 6 structured-field migration should reduce reliance on regex redaction over time.

## Pipeline State

Commands I ran:

```bash
cargo test -p cue-cli doctor::
cargo test -p cue-cli logs::
cargo build -p cue-cli --bin bluey
./target/debug/bluey doctor
./target/debug/bluey logs export --help
```

Observed:

- `doctor::` tests: 2 passed.
- `logs::` tests: 12 passed.
- `bluey doctor` ran and emitted all sections.
- `bluey logs export --help` rendered the expected redact-on-by-default UX.

Full workspace pipeline is deferred to the Phase 1 commit gate in the same working session.

## Recommended Action

Proceed with Observability Phase 1 foundations. No Phase 4 fix round needed.

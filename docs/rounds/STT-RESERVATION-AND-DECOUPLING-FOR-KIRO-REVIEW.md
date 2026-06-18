# STT Reservation and Decoupling for Kiro Review

> Branch: `codex/bluey-ai-site`  
> Scope: dual-source STT billing reservation + first decoupling slice  
> Author: Codex  
> Date: 2026-06-18

## Verdict Request

Please review this as the required paid-live-caption blocker closeout.

Requested verdict:

- 🟢 ACCEPT — dual-source STT can no longer start more paid streams than the account can cover.
- 🟡 ACCEPT WITH NITS — the reservation path is correct, but follow-up cleanup is needed.
- 🔴 REQUEST CHANGES — money path, trial reservation, or settlement can still undercharge/overcharge materially.

## Why This Round Exists

Before this round, each live STT source created its own server relay session. The server checked balance when the session was created, but it did not reserve funds. With mic + system audio enabled, an account that could afford one stream could start two streams; if the second close-time deduct failed, the relay path absorbed it and still completed.

That is not safe for wider paid live-caption usage.

This round moves STT money handling into a server-side reservation/settlement module so relay setup cannot begin until the account reserves the worst-case session cost.

## What Changed

### Server reservation ledger

`server/src/api/stt_accounting.rs` is a new module that owns STT reservation and settlement:

- `reserve_session(...)`
  - looks up Deepgram pricing
  - reserves trial seconds first
  - reserves worst-case customer cents for the requested STT session length
  - atomically moves reserved cents out of `accounts.balance_cents` into `accounts.reserved_cents`
  - inserts the `stt_sessions` row with reservation metadata
  - returns 402 before any relay token is issued when the account cannot cover the session
- `settle_session(...)`
  - caps elapsed seconds at the reserved session maximum
  - computes actual trial seconds, billable seconds, Bluey cost, and customer cost
  - refunds unused reserved cents/trial seconds
  - consumes FIFO credit batches only for actual customer cost
  - marks the STT session terminal exactly once

### Schema

Migrations now add:

- `accounts.reserved_cents`
- `stt_sessions.reserved_cents`
- `stt_sessions.settled_cents`
- `stt_sessions.refunded_cents`
- `stt_sessions.reserved_trial_seconds`
- `stt_sessions.settled_trial_seconds`
- `stt_sessions.refunded_trial_seconds`

Existing account balance reads keep working because reserved cents are removed from the visible available balance.

### STT API path

`server/src/api/stt.rs` now:

- uses `stt_accounting::reserve_session` in `create_session`
- keeps the upstream spend guard, but treats it as Bluey spend protection, not customer billing
- includes reservation fields in claimed relay sessions
- uses `stt_accounting::settle_session` in `finalize_relay_session`
- records usage from settled actual seconds/costs
- logs reservation and settlement with account hash, source, model, reserved/refunded cents, and trial seconds

### Balance helper

`server/src/db/balance.rs` now exposes `consume_credit_batches_tx(...)` for code paths that already own the transaction.

### Overlay IPC test alignment

Two overlay test files were updated to match the current product contract: drag/drop attach is accepted from idle, while daemon file handling remains responsible for canonicalization, file-kind filtering, and traversal rejection.

Files:

- `crates/cue-daemon/tests/overlay_production_path.rs`
- `crates/cue-daemon/tests/overlay_security_integration.rs`

## Decoupling Status

This round improves decoupling in one important hot path:

- STT billing/accounting is no longer interleaved with websocket relay code.
- Relay code asks for a reservation before it streams and settles afterward.
- Money-path unit tests live beside the accounting module instead of being implicit in websocket behavior.

Honest broader architecture state:

- Architecture is moderately decoupled.
- The daemon/UI/server hot paths are still too coupled for long-term scale.
- The next structural cleanup should split daemon responsibilities into audio, overlay state, sessions, RAG, cloud, and answer engine modules, then make overlay/daemon IPC a versioned contract with contract tests.

## Verification

Commands run locally:

```bash
cargo fmt --all --check
python3 scripts/analyze-tracing-calls.py --check-only
cargo test --manifest-path server/Cargo.toml stt_accounting --lib
cargo test --manifest-path server/Cargo.toml stt --lib
cargo test --manifest-path server/Cargo.toml balance --lib
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
bash scripts/observability-acceptance-smoke.sh
```

Results:

- STT accounting: 3 passed
- STT server tests: 8 passed
- Balance tests: 11 passed
- Clippy: clean
- Workspace tests: 536 passed, 14 ignored
- Observability analyzer: passed with 0 transitional findings and 0 PII findings
- Observability acceptance smoke: 8/8 assertions passed

## Reviewer Focus

Please inspect:

1. `reserve_session` atomicity: no relay token should be returned unless cents/trial seconds are reserved.
2. Settlement math: actual elapsed seconds should refund unused reservation and consume FIFO batches for actual customer cents only.
3. Dual-source behavior: an account with enough balance for one max-length source should not be able to start two reserved sources.
4. Trial behavior: trial seconds should be reserved before paid cents and unused trial seconds should return.
5. Error mapping: insufficient reservation should be 402, not a generic server failure.

## Areas Most Likely Wrong

1. Credit-batch expiry during an active STT reservation is still a narrow edge. Reservations are short-lived (10-20 minutes), but a credit batch could theoretically expire while cents are held. This should get a future batch-allocation table if we want perfect expiry semantics.
2. There is no sweeper yet for a session that reserves successfully but is never claimed by websocket relay, or for a process crash before settlement. The TTL is present; an ops safety sweeper is the next small follow-up before broad paid usage.
3. The upstream spend guard uses the full requested max seconds as Bluey spend projection even when the user has trial seconds. That is conservative for operator spend, but it can reject earlier than actual billable customer cost.
4. This round does not complete the broader daemon/web decoupling plan. It only extracts the STT accounting hot path.

## Explicitly Not Changed

- No provider API keys are exposed to the desktop.
- No Redis/Postgres/pgvector install is required on user laptops.
- No GitHub Actions deployment was used.
- `bluey-dev.db` remains local/untracked and untouched.

# Review: STT Reservation and Decoupling

> Branch: `codex/bluey-ai-site`  
> Reviewer: Codex  
> Date: 2026-06-18  
> Handoff: `docs/rounds/STT-RESERVATION-AND-DECOUPLING-FOR-KIRO-REVIEW.md`

## Verdict

🟢 ACCEPT for the required dual-source STT billing blocker.

🟡 Follow-up recommended before broad paid live-caption scale: add a reservation sweeper for expired/unclaimed STT sessions and consider a credit-batch allocation table for perfect expiry semantics.

## Findings

No blocking findings found in the implemented reservation path.

## What I Reviewed

- `server/src/api/stt_accounting.rs`
  - reservation math
  - settlement/refund math
  - trial-second reservation and restoration
  - single-settlement guard
  - error mapping
- `server/src/api/stt.rs`
  - create-session reservation before token return
  - relay claim fields
  - final settlement + usage recording
  - logs with account hash and no secrets
- `server/src/db/balance.rs`
  - shared FIFO batch consumption helper
- `server/src/db/mod.rs`
  - additive migration fields
- Overlay IPC tests
  - aligned to the current drag/drop attach contract

## Why I Agree With The Direction

The previous design was tightly coupled and unsafe in the STT money path: websocket relay creation, balance checks, and final billing were spread across the relay lifecycle, and close-time deduction could fail after the customer already received transcript service.

The new design makes the contract explicit:

1. Reserve before issuing the relay token.
2. Stream only after the reservation exists.
3. Settle once using actual elapsed time.
4. Refund unused reservation.
5. Record usage from settled values.

That is the right production shape for paid live captions.

## Residual Risk

- If a reserved STT session is never claimed, its reservation currently needs a future sweeper to release after expiry.
- If the server crashes after reservation and before settlement, the same sweeper is needed.
- Credit-batch expiry during a short reservation window remains approximate. This does not reopen the original dual-source undercharge bug, but it is worth tightening later.

## Verification

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

- All commands passed.
- Full workspace: 536 passed, 14 ignored.
- Observability acceptance smoke: 8/8 assertions passed.

## Recommendation

Hand this to Kiro for line-by-line review. If accepted, the next highest-value backend follow-up is the STT reservation sweeper; the next highest-value architecture follow-up is splitting daemon hot paths into audio, overlay state, sessions, RAG, cloud, and answer engine modules.

# FIX-586: Jobs Cloud Checkpoint Recovery

> **Codex preflight:** Loaded `$bluey-ops` and verified its Jobs execution,
> evidence, feature-flag and deployment boundaries against the repository.

## Issue

A cloud runner restart could leave a local checkpoint without a durable,
server-authoritative way to release safe work or quarantine a possibly
activated employer submission.

## Root Cause

Runner checkpoints did not carry the v2 lease token, checkpoint deletion did
not wait for server acknowledgement, and cloud lease claiming did not
atomically bind the pre-existing attempt reservation to the cloud runner.

## Fix Summary

- Add version 2 checkpoint records with exact lease-token binding.
- Keep bounded version 1 read compatibility for upgrade recovery.
- Reconcile retained checkpoints before the runner accepts new work.
- Delete a checkpoint only after a durable server acknowledgement.
- Bind the active attempt to `cloud` inside the lease-claim transaction.
- Release pre-submit work atomically.
- Preserve any potentially activated Submit as `side_effect_unknown`.
- Trust a Submitted checkpoint only when server-owned state and evidence prove
  the exact run already submitted.
- Make repeated reconciliation idempotent and reject stale authority.

## Files Modified

| File | Change |
|------|--------|
| `jobs/runner/src/execution-lease.ts` | Server reconciliation client and retained-checkpoint recovery |
| `jobs/runner/src/run-checkpoint-store.ts` | Checkpoint v2 schema and compatibility |
| `jobs/runner/src/server.ts` | Startup recovery and acknowledgement ordering |
| `server/src/api/jobs.rs` | Worker-authenticated reconciliation endpoint |
| `server/src/db/jobs/execution_leases.rs` | Atomic attempt binding and recovery transactions |
| Runner and server tests | Fault, replay, token, receipt and transaction coverage |

## Edge Cases Handled

- v2 without a token is invalid.
- Wrong token, owner, fence, account, application, run or profile is rejected.
- Local or missing attempts cannot be silently converted to cloud attempts.
- Safe and unsafe reconciliation is replay-safe.
- A Submitted checkpoint cannot invent a Submitted application.
- A crash near Submit cannot trigger an automatic retry.

## How to Test

```bash
cd jobs/runner
npm test
npm run typecheck

cd ../../
cargo test --manifest-path server/Cargo.toml checkpoint -- --nocapture
cargo test --manifest-path server/Cargo.toml
cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path server/Cargo.toml -- --check
```

## Known Limitations

- Live PostgreSQL recovery testing requires a configured
  `BLUEY_TEST_POSTGRES_URL` with the production-compatible extensions.
- This fix does not itself deploy or enable the cloud Browser service.

# REVIEW: JOBS-BROWSER-PROFILE-RECOVERY - Durable Encrypted Browser Profiles

> **Codex preflight:** Loaded `$bluey-ops` before review and verified its memory
> against the current repository state and working diff.

**Commit range:** `3096b3e0..working tree`
**Reviewer:** Codex self-review
**Date:** 2026-08-03

## Per-Task Review

### Durable profile persistence and recovery

| Field | Value |
|-------|-------|
| Files | Runner profile store/client/lifecycle, signed server API, DB authority, migrations and tests |
| Verdict | Green - accept |

**Findings:**

- Profile bytes remain encrypted outside the scoped runner runtime.
- Restore/store are account-, application-, run-, profile-, token- and
  fence-bound.
- Generation updates are serialized on both SQLite and PostgreSQL.
- Integrity failures and stale writers fail closed without exposing secrets.
- Browser close and profile upload occur before lease finalization.

## Cross-Task Findings

- A metadata CAS loser may leave an immutable object. This is bounded by object
  retention and must be cleaned by a later reference-aware lifecycle sweep.
- Distribution remains correctly disabled until cloud-pool and live ATS gates
  are complete.

## Build & Test Verification

```bash
cargo fmt --all --check                             # passed
cargo check --manifest-path server/Cargo.toml       # passed
npm --prefix jobs/runner run typecheck              # passed
npm --prefix jobs/runner test                       # 63 passed
npm --prefix jobs/runner test -- --run \
  tests/profile-snapshot-client.test.ts \
  tests/profile-store.test.ts                       # 12 passed
cargo test --manifest-path server/Cargo.toml \
  browser_profile_snapshot -- --nocapture           # passed
cargo clippy --manifest-path server/Cargo.toml \
  --all-targets -- -D warnings                      # passed
cargo test --manifest-path server/Cargo.toml        # 828 unit + 87 integration/schema passed
git diff --check                                    # passed
```

## Overall Verdict

Green - **ACCEPT** - Ready to commit as a disabled production prerequisite.

## Follow-ups for Next Batch

- Provider-specific ATS certification and fixture diversity.
- Lifecycle cleanup for unreferenced profile generations.
- Staging multi-runner failover and capacity testing.

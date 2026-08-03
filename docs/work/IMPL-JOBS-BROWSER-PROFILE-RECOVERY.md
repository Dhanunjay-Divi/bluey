# IMPL: JOBS-BROWSER-PROFILE-RECOVERY - Durable Encrypted Browser Profiles

> **Codex preflight:** Loaded `$bluey-ops` before implementation and verified
> its memory against the current repository state.

## Scope

**Does:**

- Persist encrypted Browser profile generations in account-scoped object
  storage with PostgreSQL/SQLite metadata authority.
- Fence restore and store operations to the exact active execution lease.
- Restore a validated profile on replacement runners before Chromium starts.
- Store and read back the sealed profile before lease finalization.
- Cover stale writers, cross-profile access, corrupt data and replay behavior.

**Does NOT:**

- Enable local or cloud Browser distribution.
- Deploy a Browser pool or change production flags.
- Store decrypted Chromium data in the server or object storage.
- Delete losing-writer immutable objects in the request path.

## Files Created / Modified

| File | Action | Purpose |
|------|--------|---------|
| `infra/postgres/server-runtime/020_jobs_browser_profile_snapshots.sql` | Created | PostgreSQL snapshot metadata authority |
| `infra/sqlite/server-runtime/042_jobs_browser_profile_snapshots.sql` | Created | SQLite-compatible snapshot metadata authority |
| `server/src/db/jobs/browser_profile_snapshots.rs` | Created | Lease validation and generation CAS |
| `server/src/api/jobs.rs` | Modified | Private signed restore/store endpoints |
| `server/src/api/jobs_worker_auth.rs` | Modified | Bounded signed profile upload bodies |
| `server/src/object_storage.rs` | Modified | Account-scoped content-addressed keys |
| `jobs/runner/src/profile-snapshot-client.ts` | Created | Signed bounded worker client |
| `jobs/runner/src/profile-store.ts` | Modified | Atomic generation-aware snapshot install |
| `jobs/runner/src/server.ts` | Modified | Restore-before-launch and store-before-finish lifecycle |
| tests and migration registries | Modified | Cross-backend and replacement-runner coverage |

## Build & Test

```bash
npm --prefix jobs/runner test -- --run tests/profile-snapshot-client.test.ts \
  tests/profile-store.test.ts                       # 12 passed
npm --prefix jobs/runner run typecheck              # success
npm --prefix jobs/runner test                       # 63 passed
cargo test --manifest-path server/Cargo.toml \
  browser_profile_snapshot -- --nocapture           # focused DB/API tests passed
cargo test --manifest-path server/Cargo.toml \
  worker_scope_is_path_and_operation_specific       # passed
cargo fmt --all --check                             # success
cargo check --manifest-path server/Cargo.toml       # success
cargo clippy --manifest-path server/Cargo.toml \
  --all-targets -- -D warnings                      # success
cargo test --manifest-path server/Cargo.toml        # 828 unit + 87 integration/schema passed
git diff --check                                    # success
```

## Deviations from Plan

| Deviation | Rationale |
|-----------|-----------|
| Losing-writer objects are not deleted synchronously | Deleting a content-addressed object could race a successful writer using the same key; lifecycle cleanup is safer |

## Known Follow-ups

- Add a scheduled lifecycle sweep for unreferenced profile objects.
- Validate the same recovery path in the production cloud Browser pool before
  enabling distribution.
- Exercise PostgreSQL failover and multi-runner contention in staging.

## Review Checklist (for reviewer)

- [x] Files match the scope described above
- [x] No unrelated changes included
- [x] Tests cover acceptance criteria from plan
- [x] Code style matches repository rules
- [x] No TODOs without linked task IDs

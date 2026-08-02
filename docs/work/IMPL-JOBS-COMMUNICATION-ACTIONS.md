# IMPL: JOBS-COMMUNICATION-ACTIONS - Reviewed Recruiter Replies And Calendar Actions

> **Codex preflight:** Loaded `$bluey-ops` and verified its Jobs production
> invariants against the current repository state before implementation.

## Scope

**Does:**

- Adds encrypted, account-scoped drafts for recruiter replies and calendar events.
- Requires explicit approval before a provider worker can claim an action.
- Adds idempotency, bounded payloads, mailbox/source-message authority, fenced leases,
  provider evidence, retry limits, and ambiguous-side-effect handling.
- Adds account APIs to create, inspect, list, approve, and cancel drafts.
- Keeps PostgreSQL and SQLite runtime schemas aligned.

**Does NOT:**

- Send Gmail or Outlook messages.
- Create Google or Outlook calendar events.
- Enable mailbox synchronization or change any production capability flag.
- Expose worker claim/completion operations on the public Jobs router.

## Files Created / Modified

| File | Action | Purpose |
|------|--------|---------|
| `server/src/db/jobs/communication_actions.rs` | Created | Durable encrypted action state machine and fenced leases |
| `server/src/api/jobs_communication_actions.rs` | Created | Account-scoped review and approval API |
| `infra/postgres/server-runtime/019_jobs_communication_actions.sql` | Created | PostgreSQL authority schema |
| `infra/sqlite/server-runtime/041_jobs_communication_actions.sql` | Created | SQLite authority schema |
| `server/src/db/jobs.rs` | Modified | Public action and lease contracts plus module inclusion |
| `server/src/db/jobs/tests.rs` | Modified | Authority, encryption, replay, and recovery tests |
| `server/src/api/jobs.rs` | Modified | Public account routes and error mapping |
| `server/src/api/mod.rs` | Modified | API module registration |
| `server/src/db/mod.rs` | Modified | Runtime migration registration |
| `jobs/scripts/check-jobs-schema-parity.mjs` | Modified | Schema parity coverage |
| `jobs/scripts/ci-guards-self-test.mjs` | Modified | CI guard fixture coverage |
| `CHANGELOG.md` | Modified | Unreleased product record |

## Build & Test

```bash
(cd server && cargo test communication_ --quiet)  # 7 passed
cargo fmt --all --check                            # success
node jobs/scripts/check-jobs-schema-parity.mjs     # 6 tables, 11 indexes
node jobs/scripts/ci-guards-self-test.mjs           # success
git diff --check                                    # success
```

## Deviations from Plan

| Deviation | Rationale |
|-----------|-----------|
| Provider sending is not included | A safe outbound provider worker requires separate OAuth, private-worker authentication, provider reconciliation, and sandbox certification. The public approval boundary must land first. |

## Known Follow-ups

- Gmail and Microsoft provider workers.
- Provider-side ambiguous-action reconciliation.
- Portal draft review controls.
- Authorized sandbox and revocation testing.

## Review Checklist (for reviewer)

- [x] Files match the scope described above
- [x] No unrelated changes included
- [x] Tests cover acceptance criteria from plan
- [x] Code style matches repository rules
- [x] No TODOs without linked task IDs

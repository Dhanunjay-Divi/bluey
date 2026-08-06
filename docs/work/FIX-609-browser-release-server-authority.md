# FIX-609: Browser release server authority

> **Codex preflight:** Loaded `$bluey-ops` and verified this fix in the isolated
> Round 603 worktree without touching production, credentials, release hosting,
> or either Browser distribution flag.

## Issue

Enabling local Browser distribution did not provide a cryptographic server
authority that joined an installed build to one immutable, approved native
artifact and one current account channel assignment.

## Root Cause

The existing ticket flow trusted the local distribution switch and runtime
protocol, but had no root trust policy, immutable release manifest, separately
signed activation, artifact registry, rollback authority, or append-only
revocation state. A client-reported version therefore could not prove the exact
package, native target, release evidence, or current server compatibility that
authorized a new run.

## Fix Summary

Round 603 adds dialect-paired release-authority storage and strict Rust codecs
for root policies, threshold signature sets, manifests, activations, rollbacks,
revocations, build descriptors, and immutable artifacts. Import and channel
movement are transactional, conflict-intolerant, and byte-replay idempotent.
The local claim verifies the packaged descriptor against the current assigned
channel, accepted server release, complete artifact set, and revocation state
before consuming the ticket, then freezes that identity into the run and its
operation-scoped capabilities. Pre-click authorization rechecks current release
authority before any employer-facing submission.

## Files Modified

| File | Change |
|------|--------|
| `infra/{sqlite,postgres}/server-runtime/*jobs_browser_release_authority.sql` | Add paired immutable release, trust, channel, rollback, and revocation storage. |
| `server/src/db/jobs/browser_release_{authority,trust,registry}.rs` | Add strict codecs, signature verification, registry transitions, selection, and revocation fencing. |
| `server/src/db/jobs/local_runner.rs` | Claim a ticket only with an exact server-selected release and freeze that binding. |
| `server/src/api/jobs_browser_releases.rs` | Add authenticated registry import, activation, rollback, and revocation routes. |
| `server/src/api/jobs.rs` | Integrate release availability, claim, capabilities, and pre-click checks. |
| `server/src/{db/mod.rs,db/jobs.rs,api/mod.rs}` | Register migrations, modules, and routes. |
| `server/src/db/jobs/tests.rs`, `server/tests/integration_e2e.rs` | Cover authority, ticket non-consumption, assignment, revocation, and recovery. |
| `jobs/scripts/check-jobs-schema-parity.mjs`, `jobs/scripts/ci-guards-self-test.mjs` | Extend paired-schema and negative guard coverage for the release registry. |

## Edge Cases Handled

- Unknown newer builds and semantic-version comparisons never authorize a claim.
- Missing, duplicate, wrong-target, or revoked artifacts fail before ticket
  consumption or application/session mutation.
- Activation is monotonic; downgrade requires an exact higher-sequence signed
  compare-and-swap rollback authority.
- A channel head on an obsolete trust generation is unavailable for fresh use.
- A future release change cannot silently alter the release frozen onto an
  already claimed run.
- SQLite and PostgreSQL use equivalent tables, indexes, immutable-history
  triggers, and transactional decisions.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml browser_release
cargo test --manifest-path server/Cargo.toml --test integration_e2e local_browser
node jobs/scripts/check-jobs-schema-parity.mjs
```

## Known Limitations

- The source gate does not import authority into production, enable a release
  channel, publish artifacts, or prove a physical installation.
- Software release identity is cooperative evidence, not hardware attestation.

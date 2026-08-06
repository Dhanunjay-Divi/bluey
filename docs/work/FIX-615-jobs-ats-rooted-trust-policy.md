# FIX-615: Root ATS Certification in One Persisted Trust Policy

> **Codex preflight:** Loaded `$bluey-ops` before diagnosis and reconciled its
> operating memory against the active Round 604 worktree and authority plan.

## Issue

Evidence, manifests, activations, revocations, and quarantine commands could be
verified with different caller-supplied trust anchors, so valid signatures did
not prove that every lifecycle stage belonged to one server-authorized trust
domain.

## Root Cause

`server/src/db/jobs/ats_certification_authority.rs` accepted an
`AtsCertificationTrustAnchor` at each ordinary import boundary. The relational
rows recorded an anchor digest, but cross-object lookups did not require the
same current policy digest and the database had no root-authorized policy head.

## Fix Summary

Added an offline-root-signed delegated trust-policy object, immutable policy and
key records, and one monotonic persisted policy head. The root anchor is loaded
only from server configuration for trust-policy bootstrap or rotation. Ordinary
imports load the current unexpired delegated policy from the database and every
evidence-to-manifest-to-activation, revocation, and quarantine transition checks
the exact stored policy digest. Byte-identical policy replay is idempotent;
conflicting identity, stale generation, predecessor drift, root change, expired
policy, and root/delegated key-role reuse fail closed in both database dialects.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs/ats_certification_authority.rs` | Added root/delegated policy verification, bootstrap/rotation, current-policy loading, lifecycle fencing, and adversarial tests. |
| `infra/sqlite/server-runtime/048_jobs_ats_certification_authority.sql` | Added immutable policy/key records and the singleton monotonic trust head. |
| `infra/postgres/server-runtime/026_jobs_ats_certification_authority.sql` | Added the equivalent PostgreSQL policy, key, and trust-head authority. |
| `server/src/db/mod.rs` | Extended migration-default and cross-dialect schema-parity assertions. |

## Edge Cases Handled

- First policy must be generation one with no predecessor.
- Rotation must increment generation once and name the exact current digest.
- Rotation cannot silently replace the configured offline root.
- Root keys cannot also be delegated lifecycle keys.
- Delegated public keys cannot satisfy two lifecycle roles.
- Missing, future, expired, stale, or non-current policy state grants no import
  or resolution authority.
- A signed object imported under one policy cannot be linked to an object from
  another policy.
- A fresh database contains no policy, key, activation, or certification
  authority.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml \
  db::jobs::ats_certification_authority::tests
cargo test --manifest-path server/Cargo.toml \
  ats_certification_authority_starts_empty_with_immutable_history
cargo test --manifest-path server/Cargo.toml \
  ats_certification_authority_is_runtime_migrated_with_dialect_parity
cargo check --manifest-path server/Cargo.toml --lib
```

At the rooted-policy checkpoint, the authority tests passed 9/9, both migration
checks passed, and the server library compiled cleanly.

## Known Limitations

- The repository contains no private key or production root anchor. An approved
  root configuration and independently managed signing ceremony remain external
  launch gates and no ATS is certified by this source change.
- Layout observations, suite completeness, circuits, canary reservations, and
  per-application bindings are completed by the remaining Round 604 authority
  work rather than this focused trust-boundary fix.

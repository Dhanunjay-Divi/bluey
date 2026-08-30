# FIX-757 — Frozen ATS head execution authority

> **Codex preflight:** Loaded `$bluey-ops` and reconciled its operating memory against Round 614B,
> the current local handoff, and the authoritative worktree. The SSD archive was not used.

**Severity:** P1 production-boundary authority defect

**Status:** Implemented; focused SQLite/PostgreSQL evidence is green and final aggregate evidence is
pending

## Issue

An Auto-submit approval froze a schema-3 ATS certification, but later queue, reservation, and
running transitions could evaluate only whether an ATS target was currently active. A successor
activation for the same target could therefore be combined with an approval signed for the prior
head.

## Root Cause

The frozen approval carried an exact ATS projection, while the shared execution-authority
composition treated current ATS capability as a Boolean eligibility input. It did not require byte-
exact equality between the current active binding and the schema-3 admission stored in the approved
packet.

## Fix Summary

- Freeze the exact active ATS certification projection in schema-3 Auto-submit admission.
- Decode that closed projection at every current execution-authority decision.
- Require equality with the current active ATS binding before queue, reservation, or running
  authority can be positive.
- Reject missing, legacy, malformed, expired, revoked, or successor-head substitutions.
- Preserve review-first schema-1/schema-2 behavior and require schema-4 Final Submit evidence only
  for an application carrying frozen certification.
- Prove successor-head denial leaves reservations and application rows unchanged in SQLite and
  configured PostgreSQL.

## Files Modified

| File | Change |
| --- | --- |
| `server/src/db/jobs/applications.rs` | Freeze and persist exact current ATS admission. |
| `server/src/db/jobs/customer_data.rs` | Validate the closed schema-3 ATS projection. |
| `server/src/db/jobs/execution_authority.rs` | Match current ATS authority to frozen admission. |
| `server/src/db/jobs/execution_leases.rs` | Bind certified Final Submit to frozen authority. |
| `server/src/db/jobs/tests.rs` | Add zero-mutation successor-head regressions. |

## How To Test

```bash
cargo test --manifest-path server/Cargo.toml --lib \
  ats_head_replacement_cannot_reserve_frozen_auto_submit_capacity -- --nocapture
cargo test --manifest-path server/Cargo.toml --lib \
  ats_head_replacement_cannot_start_or_persist_a_frozen_auto_submit_run -- --nocapture
BLUEY_TEST_POSTGRES_URL=<isolated-postgres-url> cargo test --manifest-path server/Cargo.toml \
  --lib postgres_ats_head_replacement_cannot_reserve_or_start_when_configured -- \
  --nocapture --test-threads=1
```

## Known Limitations

- Local PostgreSQL evidence is not hosted failover or production-signing-key evidence.
- This fix does not enable Auto-submit, publish an adapter, or authorize an external application.

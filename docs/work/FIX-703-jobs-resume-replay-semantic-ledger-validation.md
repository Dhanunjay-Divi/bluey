# FIX-703: Exact Resume Replay Could Trust a Stale Semantic Ledger

> **Codex preflight:** Loaded `$bluey-ops` and reviewed source-resume publication replay against
> the Phase 613 account-input generation ledger. No real resume, object store, hosted database,
> external account, or deployment was used.

## Issue

An exact idempotent resume-publication replay could return the current stored asset/profile even
when account semantics had changed without a matching semantic-input transition.

## Root Cause

The replay fast path verified upload identity, exact asset bytes, and profile-to-asset binding, then
returned early. It did not validate the current account-input head or recompute the complete
account semantic digest before accepting that fast path.

## Fix Summary

- Validate the immutable account-input head on exact replay in SQLite and PostgreSQL.
- Recompute the current account semantic digest from profile, preferences, facts, identities, and
  source resume inside the same transaction.
- Require the head's semantic digest to equal the recomputed digest before returning `replayed`.
- Preserve the existing exact asset/profile and upload idempotency checks.
- Add a regression that tampers policy-relevant profile semantics without advancing the ledger and
  requires replay to fail closed.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs/resume_assets.rs` | Validate account semantic-head freshness on exact SQLite/PostgreSQL resume replay and add the tamper regression |

## Edge Cases Handled

- a true exact replay against a current ledger;
- an exact asset with an unrecorded profile edit;
- a corrupt or missing account-input head;
- an asset/profile binding mismatch; and
- replay inside the PostgreSQL policy-input lock transaction.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml --lib \
  exact_resume_replay_requires_current_semantic_input_ledger
```

The SQLite regression is present in the current tree. A configured local PostgreSQL authority
suite passed 13 tests, but no dedicated PostgreSQL exact-resume-replay fault result is attributed
to this fix. The named SQLite regression is included in the clean 1,401-test server-library target,
which passed; the exact PostgreSQL fault case remains external evidence.

## Known Limitations

- This validates database replay authority; object-store read-back and network-loss behavior still
  require external evidence.
- It does not relax replacement-resume review or reauthorization requirements.

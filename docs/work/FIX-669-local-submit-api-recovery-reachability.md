# FIX-669: Keep Only Durable Local Submit and Result Recovery Reachable

> **Codex preflight:** Loaded `$bluey-ops` and verified the API findings against
> the current Phase 606 worktree. No archive, credential, object-store write,
> physical device, employer portal, live tenant, or production system was used.

## Issue

The database could reconstruct an exact local `click_started` authorization,
but the HTTP route could still strand submit or result reconciliation in three
ways: the signed v2 submit capability expired before the reconciliation window
ended, the local distribution pause rejected the request before durable ticket
state was known, or a changed object-storage maximum caused the API to present
newly derived capacity instead of the exact active capacity reserved before the
possible side effect.

Any broad exception would be unsafe: expired claimed work, legacy submit
capabilities, distribution-paused new authority, or invalid capacity must remain
denied.

## Root Cause

Capability expiry and the environment distribution flag were evaluated as
stateless route gates. Evidence capacity was derived only from current
object-storage configuration. Those checks were correct for new pre-click and
pre-side-effect result work but did not distinguish it from recovery of a
durable `click_started`/`side_effect_unknown` state and its already reserved
capacity.

## Fix Summary

The route now loads and verifies the ticket before applying narrowly scoped
recovery exceptions:

- an expired signed v2 `submit` capability is accepted only for a durable
  `click_started` ticket and only inside the bounded reconciliation grace;
- an environment distribution pause still denies `claimed` or new submit work,
  but allows a `click_started` request to reach the exact database recovery
  authority; and
- a `click_started` request reconstructs its reserved bytes and object count
  from the exact active durable local capacity row, while continuing to use the
  current object-storage upload limits. Submit recovery remains signed v2-only.

Result reconciliation reconstructs that durable capacity whenever the ticket
is already `click_started` or `side_effect_unknown`, regardless of whether the
signed result capability is v1 or v2. This preserves the existing versioned
result-recovery contract rather than narrowing it accidentally. Claimed,
`needs_input`, and other new/pre-side-effect paths still derive fresh capacity
from current configuration.

The database remains the final authority and revalidates the complete ticket,
release, proof, session, application, ATS, and capacity binding. Missing,
inactive, expired, non-local, or wrong-account/application/run capacity denies
the request rather than falling back to a fresh reservation.

## Files Modified

| File | Change |
|------|--------|
| `server/src/api/jobs.rs` | Add v2 reconciliation-grace, distribution-pause, and durable-capacity route distinctions. |
| `server/src/db/object_uploads.rs` | Expose a bounded exact active-capacity lookup for local click-started recovery. |
| `server/src/db/jobs/tests.rs` and API unit tests | Prove claimed/new denial, exact recovery reachability, and no capacity reminting. |

## Edge Cases Handled

- Expired v2 `submit` is denied for `claimed` and after reconciliation grace.
- Expired legacy submit capability remains denied even for `click_started`.
- Existing signed v1/v2 result reconciliation for `click_started` and
  `side_effect_unknown` reuses durable capacity; the new expired-submit grace
  exception remains signed v2-only.
- Distribution disabled denies claim and new submit but does not strand an
  exact durable `click_started` recovery.
- Object-storage maximum drift does not change the reserved bytes/object
  identity frozen before a submit or result-reconciliation side effect.
- Current upload limits still constrain evidence uploads after recovery.
- Missing, released, committed, expired, cloud-runner, or wrong-scope capacity
  cannot become recovery authority.
- The exception reaches database reconstruction only; it does not issue a new
  marker, ticket, ATS canary, evidence capacity, or employer click.
- An incident distribution pause still requires the current server release and
  frozen Browser authority plus object-storage configuration until all
  post-marker runs settle.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml \
  expired_v2_submit_capability_is_limited_to_click_started_replay
cargo test --manifest-path server/Cargo.toml \
  disabled_distribution_denies_claimed_submit_but_allows_click_started_replay
cargo test --manifest-path server/Cargo.toml \
  local_distribution_gate_blocks_new_submit_but_allows_exact_click_started_replay
cargo test --manifest-path server/Cargo.toml \
  click_started_replay_preserves_durable_capacity_across_config_drift
cargo test --manifest-path server/Cargo.toml \
  click_started_replay_rejects_missing_inactive_expired_or_wrong_run_capacity
cargo test --manifest-path server/Cargo.toml \
  legacy_result_reconciliation_expiry_contract_is_preserved
```

**Evidence status:** GREEN. Every named focused regression passed, and the
complete server matrix passed 1,351 tests at the frozen Phase 606 code
snapshot.

## Known Limitations

- Object-storage credentials/read-back, native Browser traffic, live tenant
  recovery, physical devices, immutable release hosting, and both Browser
  distribution flags remain parked.

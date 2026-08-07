# FIX-667: Preserve Exact Local Click-Started Recovery Through Server Deploys

> **Codex preflight:** Loaded `$bluey-ops` and reconciled this fix with the
> local Round 603 Browser-release recovery contract. No archive, credential,
> physical device, employer portal, live tenant, or production system was used.

## Issue

A final-submit hold must block a new local pre-click marker without blocking
recovery after `click_started` is already durable. The recovery path also needs
an explicit release-identity contract: freezing the current server deployment
ID from the first call would strand a possible employer side effect after a
normal server deploy, while accepting an arbitrary new server ID would weaken
the frozen Browser release authority.

## Root Cause

The pre-click path consumed ATS certification authority and evidence capacity
before changing the ticket to `click_started`, but there was no dedicated
read-only reconstruction path for an exact retry after that transition. The
meaning of "exact release replay" was also easy to misread as freezing the
server process deployment ID instead of the Browser build/release binding
already frozen onto the run.

## Fix Summary

SQLite and PostgreSQL now detect `click_started` before evaluating a later
final-submit hold and reconstruct the prior authorization from durable state.
Recovery verifies the ticket and encrypted payload, account/application/run,
running application and Browser session, exact final-submit proof, active
evidence capacity, consumed terminal ATS certification authority, and the
complete frozen Browser release binding and digest. It returns the same ATS
receipt authority without consuming another canary, reserving more capacity,
or writing another marker.

The exact identity is the frozen Browser build/release binding, not the current
server process ID used on the first call. If that immutable activation accepted
server releases `[A, B]`, a marker started while `A` was current may recover
while `B` is current. That `A -> B` recovery is deliberate and prevents a
deployment from stranding a possible employer side effect. A current server ID
outside the frozen activation's accepted set is denied. Later activation,
manifest, build, or revocation changes cannot rewrite the binding frozen onto
the run.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs/local_runner.rs` | Add exact SQLite/PostgreSQL click-started reconstruction ahead of new-authority hold evaluation. |
| `server/src/db/jobs/tests.rs` | Prove later-hold recovery, accepted `A -> B` deployment recovery, unaccepted-server denial, and zero new authority. |

## Edge Cases Handled

- A hold before the marker leaves the ticket `claimed` and writes no evidence
  capacity or ATS consumption.
- The same hold after the marker permits only exact durable recovery.
- An accepted current-server deployment change does not alter the frozen
  Browser binding or returned ATS authority.
- An unaccepted server ID, changed proof, changed capacity, mismatched ticket,
  application, session, or missing terminal ATS authority is denied.
- Replays leave the ATS binding fence/request hash, canary count, capacity
  count, and ticket state unchanged.
- PostgreSQL acquires the operational hold lock before ATS, ticket, Browser
  release, application/session, and capacity locks.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml \
  local_final_submit_hold_blocks_new_marker_but_not_exact_click_started_replay
cargo test --manifest-path server/Cargo.toml \
  certified_local_answer_revision_after_click_marker_stays_in_reconciliation
```

## Known Limitations

- `FIX-669-local-submit-api-recovery-reachability.md` records the separate HTTP
  capability, distribution-pause, and durable-capacity reachability contract.
- The regression uses a signed-authority fixture; it does not click an employer
  portal or prove a production native Browser artifact.
- Credentialed native packaging, immutable public read-back, physical
  macOS/Windows install and upgrade canaries, live tenant recovery, and both
  Browser distribution flags remain parked.

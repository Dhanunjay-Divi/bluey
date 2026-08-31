# FIX-730: Authoritative Eligibility Representation

> **Codex preflight:** Load `$bluey-ops` before diagnosis or implementation and verify its memory
> against the final Phase 614 record, Round 614B, and the current repository state.

**Severity:** P2 misleading representation; no observed side-effect bypass

**Status:** Implemented; focused evidence green; aggregate and independent review pending

## Issue

Some workspace and posting-save projections computed `canQueueLocal`, `canQueueCloud`, and
`canAutoSubmit` directly from mutable posting JSON. A legacy posting carrying `verified`, nonempty
employer identity fields, and `clear` risk could therefore appear queueable in the customer UI even
though every actual queue and execution boundary reprojects current relational authority and rejects
the same posting.

## Root Cause

`server/src/db/jobs/workspace.rs` called the pure eligibility builder on stored posting JSON before
resolving the Phase 614 original-source projection. The normal and atomic posting snapshot builders
in `server/src/db/jobs/profile_postings.rs` likewise persisted eligibility computed from their raw
input. Those representation paths bypassed the mutable-label sanitizer in
`posting_with_original_source_projection`.

The effect-bearing preparation, finalization, queue, and execution paths already use projected
source authority and were not bypassed. The defect nevertheless violates the rule that displayed and
persisted authorization booleans must be derived from the same fail-closed authority as the action
they describe.

## Fix Summary

The implemented correction:

- project and sanitize each workspace posting before eligibility is computed;
- apply current discovery and ATS authority to the projected workspace decision;
- return the projected evidence and decision rather than mutable positive labels;
- sanitize mutable `verified`/`clear` labels and their employer identity fields before normal or
  atomic posting snapshots are scored, persisted, or returned;
- preserve independent mismatch, impersonation, blocked-risk, and original-source hard denials;
- add regressions proving raw legacy positive labels cannot mint queue/Auto-submit representation;
  and
- remain ready for the Phase 614B integrity overlay to enter the same composed projection.

Independent review additionally required the list/detail API to return the same projection it
scores and bounded the authority read set at the existing 500-posting materialization envelope.
The implementation now reads posting/source/discovery/ATS representation inside one database
snapshot, batches discovery rows, shares database time across source and ATS freshness, returns an
explicit pagination-required error at 501 rows rather than truncating, and keeps exact detail
available.

## Files Modified

| File                                           | Change                                                     |
| ---------------------------------------------- | ---------------------------------------------------------- |
| `server/src/db/jobs/workspace.rs`              | Compute and return eligibility from current projections    |
| `server/src/db/jobs/profile_postings.rs`       | Sanitize mutable authority before snapshot persistence     |
| Focused tests in the two files above           | Cover workspace and saved-posting representation denial    |
| This FIX and Phase 614B implementation records | Record final source scope and observed evidence             |

## Edge Cases Handled

- A legacy row contains `verified`, employer ID/domain, and `clear` risk.
- A hosted ATS projection is current but independent employer identity is absent.
- An existing mismatch, impersonation, or blocked-risk label must survive sanitization.
- A posting-save response and the next workspace read must agree on fail-closed queueability.
- Actual queue/effect boundaries continue their separate current-authority rechecks.

## How to Test

Observed focused evidence:

```text
Profile-posting representation and hard-denial tests               PASS (5/5)
Workspace current-authority list/detail representation tests       PASS (4/4)
cargo check --manifest-path server/Cargo.toml --lib               PASS
cargo clippy --manifest-path server/Cargo.toml --lib -- -D warnings PASS
Owned-file rustfmt and scoped git diff --check                    PASS
Full fmt and aggregate Phase 614B gates                           PENDING
Independent post-fix review                                      PENDING
```

The workspace count is the same previously observed four-test coverage, now described under the
current list/detail test names rather than the stale `3 + 1 helper` split. It does not add a new
test-run claim.

## Known Limitations

- This fix closes customer-facing and persisted eligibility representation. FIX-726 separately
  removes mutable employer-domain input from operational-hold scopes.
- It does not itself add signed Phase 614B employer/risk composition, enable a production flag,
  deploy, contact a provider, or perform an application effect.

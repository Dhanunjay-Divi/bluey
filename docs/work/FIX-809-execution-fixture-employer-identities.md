# FIX-809: Distinct execution-test employers reused one signed identity

> **Codex preflight:** `bluey-ops`; current Phase636 source and local evidence are authoritative.

## Issue

Five full-library tests in session16096 failed during fixture preparation with
`Review conflicting employer identity records before starting another application.`
They never reached their intended second-job lease/receipt/capacity assertions.

## Root Cause

`execution_lease_fixture` represents each suffix as a different company, Greenhouse
board and canonical domain, but its shared authority helper defaulted every signed
employer ID to `employer-acme`. Preparing a second cloud fixture, or a local Acme
fixture after a cloud one, therefore contradicted immutable historical ID/domain
mapping. The product correctly denied that conflict, including after safe release.

## Fix Summary

Add an explicit test-only runner/employer helper while preserving the existing
helper's `employer-acme` default. Only the generic cloud execution fixture supplies
a stable suffix-derived signed employer ID, before signing/importing its ordinary
positive authority. Keep the local `employer-acme`/`acme.com` fixture unchanged and
the runtime-surface builder private. No product or migration behavior changes.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs/production_positive_authority_fixture.rs` | Explicit employer variant; existing defaults retained. |
| `server/src/db/jobs/tests.rs` | Cloud execution fixture uses its distinct employer's signed ID. |

## Edge Cases Handled

- Two employers still share one account/application identity/browser profile, so
  browser-profile lease exclusion remains meaningful.
- Submitted and unknown receipts retain their exact protected-capacity assertions.
- Failed/released lease outcomes do not authorize employer-ID remapping.
- Cloud and local final-submission authority remain independently checked.

## How to Test

Run these exact library tests first under the canonical isolated launcher, then
all current private PostgreSQL and full all-target gates:

- `execution_lease_excludes_parallel_runs_for_one_browser_profile`
- `execution_receipt_authority_rejects_nonterminal_and_stale_leases`
- `final_submission_authority_matches_cloud_lease_and_local_ticket_exactly`
- `safe_terminal_lease_outcomes_release_unused_protected_capacity`
- `terminal_receipt_authority_retains_exact_protected_capacity`

Preserve all existing conflict, stale-token/fence, receipt, capacity and no-mutation
assertions. Separate reviewer statically confirmed this shared fixture cause for
all five before edits; exact Rust terminal reports then confirmed their setup sites.

## Known Limitations

Successor88910 passed all five exact fixture regressions1/1 each (5.05s,4.78s,
5.22s,4.44s,4.65s respectively in the listed order). Full current-source gates
remain pending; these focused passes do not establish full-suite acceptance.
Session16096's full
result was1726 passed/6 failed, with no subsequent targets executed. Its25 source
hashes/HEAD and cleanup were verified before these edits. No real employer/provider
request, account, application, email, deployment or production flag is involved.

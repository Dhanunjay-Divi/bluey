# FIX-740: Jobs Exact Signed Application Destination

**Severity:** P1 signed-contract blocker

**Status:** Implemented; focused destination/receipt mapping present; one direct effect case pending

## Issue

The Phase 614 head retained the verified application URL/domain, but its projection dropped the
URL and approval froze mutable `posting.canonical_url`. Integrity could bind one destination while
the execution packet carried another.

## Required Fix

- Derive one source-owned binding from the exact current Phase 614 head and canonical subject.
- Build Phase 614B expected source from that binding and current ATS target hash.
- Freeze and recompare the signed canonical application URL/domain in approval and fresh submit.
- Preserve immutable terminal replay after the irreversible request/click boundary.

## Implementation

`server/src/db/jobs/original_source_verification.rs` projects the exact source-head application URL
and domain into its integrity binding. `server/src/db/jobs/job_integrity_composition.rs` builds the
expected Phase 614B source and ATS target from that binding rather than mutable posting URL, and
the application/execution authority paths freeze and recompare the resulting strict integrity
receipt.

## Evidence

Mapped coverage:

```text
positive_integrity_binding_uses_head_destination_and_rejects_source_drift PRESENT
expected_source_uses_exact_phase614_binding_and_ats_target_sha256         PRESENT
ats_lookup_uses_phase614_destination_instead_of_mutable_posting_url       PRESENT
receipt_is_exact_strict_and_drift_sensitive                              PRESENT
approval_queue_and_cloud_resume_use_current_composed_authority            PRESENT
Direct destination-drift FinalSubmit zero-mutation behavioral regression PENDING
Frozen-source aggregate execution                                        PENDING
```

The source binding and strict receipt checks are implemented. Do not treat the indirect coverage
as a claim that the missing direct FinalSubmit destination-drift/zero-mutation case has run.

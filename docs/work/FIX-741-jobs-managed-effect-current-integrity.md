# FIX-741: Jobs Managed Effect Current Integrity

**Severity:** P1 stale job authority

**Status:** 🟡 Implemented in the managed boundary; behavioral drift matrix pending

## Issue

Managed effect authorization required the cloud release/worker/lease tuple after FIX-729, but did
not yet compare the application's frozen source/ATS/integrity receipt with current job authority.

## Required Fix

- Resolve current composed authority after `H -> M -> ATS -> D` and before effect authorization.
- Compare the exact frozen integrity receipt and signed destination.
- Prove source, ATS, integrity, and expiry drift deny with zero effect mutation.

## Implementation

`server/src/db/jobs/execution_leases.rs` resolves current composed execution authority after the
canonical PostgreSQL prelock and compares it before a managed effect is authorized. The HTTP layer
continues to require a complete managed tuple; FIX-762 separately governs durable classification
at the shared managed/unmanaged FinalSubmit boundary.

## Evidence

Mapped coverage:

```text
managed_execution_effect_rechecks_composed_authority_after_canonical_prelock PRESENT (STATIC)
managed_cloud_execution_effect_is_fresh_without_rewriting_claim_tuple        PRESENT (STATIC)
Direct source/ATS/integrity/expiry drift zero-mutation matrix                 PENDING
Final managed positive route on frozen source                                PENDING
```

The current-integrity wiring and ordering are source-covered, but static checks do not prove the
required behavioral denial matrix. Keep this FIX yellow until those cases and the exact positive
route run on the frozen source.

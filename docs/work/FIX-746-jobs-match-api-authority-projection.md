# FIX-746: Jobs Match API Authority Projection

**Severity:** P2 customer-visible authority leakage

**Status:** Implemented; focused evidence green; independent review pending

## Issue

The match list/detail handlers computed eligibility from a projected clone but returned the raw
stored posting. Legacy `verified`, `clear`, employer identity, and embedded queue labels could
remain visible despite a fail-closed decision.

## Required Fix

- Return the same sanitized/current-authority posting projection used for the decision.
- Cover list and detail responses containing legacy mutable positive labels.
- Preserve independent hard denials and effect-boundary rechecks.

## Evidence

`match_route_helpers_return_only_current_projected_postings_and_decisions` passes for list and
detail. Server library check, strict Clippy, and owned-file formatting pass. Aggregate route gates
and post-fix independent review remain pending.

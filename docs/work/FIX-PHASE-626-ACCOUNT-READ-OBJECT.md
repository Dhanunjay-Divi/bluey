# FIX: Phase 626 account read shape and expiry fixture

> Preflight: load `$bluey-ops` and `$pinky-bluey-integration-ops` and verify current source.

## Issue

The first compile caught a borrowed pooled-connection coercion error. A named
pooled connection corrected it. The next all-target run compiled and passed
849 of 851 library tests, but failed two new account cases before integration
binaries/Clippy. No failed artifact was deployed.

## Root Cause

1. Serde's empty Rust struct accepts `[]` as well as `{}`. The account handler
   now verifies the signed body first, then demands an actual empty JSON object.
   Negative fixtures retain scalar/array, extra-field, trailing and tampered
   body coverage.
2. The expired-entitlement fixture set expiry before its original creation and
   violated `expires_at > created_at`. It now moves both timestamps backwards
   consistently; real-store constraints remain unchanged.

## Fix Summary

Demand an actual empty signed JSON object, keep fixture timestamps consistent,
and hold an explicit pooled-connection borrow. No production schema change.

## Files Modified

- `server/src/pinky_integration/account.rs`: explicit object validation.
- `server/src/pinky_integration/http_tests.rs`: valid expired fixture/borrow.
- `server/src/pinky_integration/store/tests.rs`: optional PG freshness contract.

## Edge Cases Handled

Arrays, scalars, extra fields, trailing JSON, signature tampering and expired
entitlements remain fail-closed. A missing ledger timestamp is not invented.

## How to Test

Run `bash scripts/run-pinky-integration-postgres-tests.sh`. This acquires the
local queue, runs PostgreSQL parity, focused tests, all targets and strict Clippy
inside one automatically cleaned temporary root.

The optional PG ledger timestamp assertion was aligned with the deliberately
optional contract. The queued PostgreSQL harness runs real-store parity, focused
integration, all targets and strict Clippy in one temporary build root and cleans
it. At source `bfc92a8f809aa1bbff85a21cc73c67d984f78beb`, the fresh harness passed
PostgreSQL 1/1, focused integration 38/38, all-target 935/935 (851 library plus
84 integration), and strict all-target Clippy. Exit 0; owned temporary root
`/tmp/bluey-pinky-postgres-tests.Dfjqxv` and PostgreSQL process were removed.
No failed artifact was deployed.

## Known Limitations

This repair does not validate physical overlay behavior or close the separately
recorded STAR factual-grounding failure.

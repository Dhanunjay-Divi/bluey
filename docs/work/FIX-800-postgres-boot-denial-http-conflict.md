# FIX-800: Classify the exact PostgreSQL boot-transition denial

> Codex preflight: `bluey-ops`, security and independent source review.

## Issue / Root Cause

The configured Axum/PostgreSQL test's bound premature READY request was rejected
without mutation, but returned HTTP 500 instead of a permanent conflict. The v3
generic-heartbeat path deliberately leaves transition authorization to the 043
database trigger. Its expected `P0001` denial was wrapped as registry storage
failure and therefore classified as an ordinary internal error. An unbound
READY request already receives the typed conflict before that trigger.

## Fix Summary / Files

Only `server/src/api/jobs_managed_cloud_releases.rs` changes. The storage-error
mapper requires an actual `postgres::Error` in the error chain, SQLSTATE
`RAISE_EXCEPTION` (`P0001`), and the exact immutable guard message
`source-verifier boot heartbeat transition is unauthorized`. Only that
combination receives the existing generic HTTP 409 response. No database text
is exposed. Every other storage error, including ordinary error text matching
that message without the PostgreSQL type/code, remains HTTP 500.

No SQL trigger, readiness transition, transaction or permission is changed.

## Verification / Edge Cases

The configured regression first invokes the real DB method with the exact
bound forged-ready payload, asserts the actual PostgreSQL code/message and
unchanged six-family runtime snapshot, then requires HTTP 409 for the same
request. It continues through valid degraded/startup and exact canonical replay.
An offline unit case rejects text-only classification. The configured API case
passed in the final 30/30 PostgreSQL batch; strict all-target Clippy passed.
The full offline suite's final result is recorded in the Phase 629 IMPL.

## Limits

The new API test injects an authenticated worker identity; it does not replace
fleet-HMAC/socket tests or certify hosted PostgreSQL. No real user action occurs.

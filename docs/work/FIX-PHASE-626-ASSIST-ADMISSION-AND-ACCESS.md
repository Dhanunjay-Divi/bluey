# FIX: Phase 626 — Assist admission, cancellation and access truth

Preflight: `$bluey-ops` and `$pinky-bluey-integration-ops`.

## Issue / root causes

- Replayed running requests could finalize another execution. Admission and
  transition lacked a single durable execution claimant.
- Cancel could arrive before Ask: absent cancellation initially had no durable
  fence, allowing a later dispatched request.
- Release/settlement uncertainty was flattened into terminal or false-pending
  states. A UI Stop is not proof of provider shutdown or customer refund.
- Entitlement absence was checked before context validity. An expired/closed/
  revoked binding without entitlement was mislabelled `not_added`.

## Fix / files

`server/src/pinky_integration/store.rs` serializes claims and cancel tombstones,
keeps exact-owner/context identity, and validates context before entitlement.
`mod.rs`, `api/router/streaming_completion.rs`, `sse.rs` and
`provider_cost_guard.rs` keep local delivery fences separate from observed
accounting; only confirmed release/settlement is terminal.
`http_tests.rs` and store tests cover real exhausted-provider, predispatch,
replay, concurrent ordering, foreign ownership and revoked-context regressions.

## How to test / limits

Use the shared `mac-heavy` queue to run
`bash scripts/run-pinky-integration-tests.sh all` (offline/locked, owned
temporary DB/Cargo roots, success/failure/signal cleanup). Final gate status
lives in `REVIEW-PHASE-626-ASSIST-RUNTIME.md`; initial failures are not concealed.
Live provider drain/settlement, PostgreSQL execution and native/UI physical
behavior require separate actual tests. No upstream cancellation is guaranteed.

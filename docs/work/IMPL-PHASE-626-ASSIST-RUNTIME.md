# IMPL: Phase 626 — owned Pinky Assist runtime

Preflight: `$bluey-ops`, `$pinky-ops`, `$pinky-bluey-integration-ops` and current
AGENTS.md. Date: 2026-10-09. Feature branch: `feat/phase-626-pinky-integration`.
This is additive backend glue; Jobs and existing environments are excluded.

## Implemented source

- Exact signed ask/cancel/status scopes and method/path/body binding in
  `server/src/pinky_integration/delegation.rs` and `mod.rs`.
- Immutable external account ownership and explicit entitlement/access. The
  active, unexpired, unrevoked exact context is required before `not_added` or
  `available`; unavailable/revoked is not inferred from an error or active row.
- Trusted Default/Short/STAR prompt policy reuses the owned managed streaming
  core in `api/router/streaming_completion.rs`. No client model credentials,
  system prompt, lane, source media, RAG or arbitrary history identifier.
- Durable single execution claim, exact-running replay conflict, serialized
  Stop-before-Ask tombstones, cancellation delivery fences and honest pending
  settlement in `pinky_integration/store.rs` and managed accounting helpers.
- Provider-I/O cancellation drains settlement instead of claiming instantaneous
  provider termination/refund. Confirmed predispatch cancellation settles zero.
- Synthetic credit is preprod-only, bounded and canonical UUID allowlisted;
  unknown subjects get no credit. No real payments, hourly metering or reload.
- Local-only immutable-source Linux cross-build script with strict SSH sysroot
  checksum, shared Mac queue and temporary cache/data cleanup.

## Verification and known open gates

Independent bounded source review found no open P0/P1 after concurrency and
accounting fixes. Tests exercise real managed HTTP/provider failure paths,
predispatch zero exposure, concurrent claims and cancel/admit ordering. Initial
compiles revealed test-only wrapper/type/move defects; these were repaired.
The next full run reached 837/838 library passes and exposed access precedence;
that actual bug is fixed with closed/expired/revoked regression coverage.
The full rerun passed 922 all-target tests. Strict Clippy's eight-argument
signature finding was resolved by a private owned request context without
changing order/logic; 33 focused integration tests and strict all-target Clippy
passed after that refactor. Retained Linux build remains an execution gate; update the matching
review with its observed receipts.

Pinky's targeted Go runtime and Node/Python gates passed, with dual-architecture
Mac compile. Native physical QA, final Windows compile, dedicated-preprod
authenticated E2E, real-provider latency/cancel/settlement and PostgreSQL
execution remain open. No production promotion is implied by source review.

# Phase 626 — remaining runtime seams

Date: 2026-10-09. Preflight: `$bluey-ops`, `$pinky-ops`,
`$pinky-bluey-integration-ops`. Evidence source is Bluey `c4f12c5c`;
this document is a map, not an implementation or acceptance receipt.

## Reuse the owned managed stream, not a service wallet

`server/src/auth/middleware.rs:20` resolves a Bluey JWT to `AuthedAccount`.
`server/src/api/router/streaming_completion.rs:43` consumes that typed account;
the managed streaming core is not inherently JWT-dependent. A future delegated
adapter must resolve the immutable Pinky binding server-side, then share that
core. Do not mint a reusable Bluey JWT to Pinky/browser or claim an existing
wallet by email. Current integration routes cannot dispatch or charge.

The existing trusted nonstream helper is `complete_for_account` in
`streaming_completion.rs:2152`. A streaming equivalent must construct its
system/route/token/context policy server-side and preserve external-field
disclosure checks. Do not accept raw `system`, internal lane, provider keys or
arbitrary Bluey history identifiers from the browser.

## Accounting already exists; cancellation does not

- Durable request identity: `server/src/db/idempotency.rs:33` and account-scoped
  reservation/replay paths; do not replace them with in-memory frontend retries.
- Customer reservations: `server/src/db/usage_reservations.rs:342` (SQLite),
  `:440` (PostgreSQL). Provider holds: `api/router/provider_cost_guard.rs:36`.
- Settlement: `api/router/streaming_completion.rs:1930` and transactional
  `db/usage_reservations.rs:960` / `:1028`; startup/periodic reconciliation exists.
- Response cache write follows settlement in `streaming_completion.rs:2114`.
  If it fails after charge, retries remain conflicted/manual reconciliation;
  this is not an atomic durable response/outbox lifecycle.
- `api/router.rs:964` intentionally detaches stream work from browser disconnect
  to preserve settlement. No managed model-cancel endpoint/token exists.
- Current account checks at `streaming_completion.rs:1435` do not check Pinky
  session closure, binding revocation or source-consent generation.
- `pinky_integration/mod.rs:209` only closes the context. It does not cancel a
  dispatched provider or authorize a UI claim that all work/cost instantly stopped.

Before live dispatch, implement durable request/cancel states and ordered fences
for predispatch, provider admission, delta publication and settlement. A browser
Stop must invalidate visible output immediately; provider cancellation may be
best-effort, but durable accounting must classify completion/cancellation/unknown
outcome. Never abandon provider holds to make a button appear instant.

## Remaining authorization contract

Add a separately scoped, default-off exact delegated ask path only after its
bounded DTO and durable lifecycle are reviewed. Current `ai:session` tokens
allow Start/Close only (`pinky_integration/delegation.rs:136`), not ask.
Resolve binding + external account + owned active unexpired AI context and
explicit AI entitlement atomically. Recheck at dispatch and during delivery.
Start currently gives zero balance, no trial/reload/payment references; context
active is not entitlement. Keep all validation on synthetic test balances.

Source authority must bind accepted source generation and AI context; revoke
queued inputs on consent change. Viewer audio stays off by default, independent
of playback/Listen/captions. Initial integration must not silently use standalone
RAG/history (`streaming_completion.rs:252`); approved history needs tenant scope,
retention/export/deletion and source policy.

## Coordination and completion

Pinky's 2026-10-09 response retains runtime hold: current hardening base is
`c75808f3113c1cfb54cab31dc6e7c67d13cb5a4d` plus uncommitted changes and failed/
unobserved physical gates. New isolated modules/preview work may continue;
existing native/media/auth/API/billing/template/workflow seams may not be wired
until accepted source and overlap closure arrive. Its release operator owns
native/browser/remote operation for the active release. Use the shared actual
`mac-heavy` lock, not a second machine name.

The next bounded runtime step is an accepted Pinky SHA/seam handoff, followed by
default-off delegated ask + durable cancellation/entitlement/source fences and
actual authenticated isolated-preprod streaming. PostgreSQL concurrency,
provider latency, accounting/crash replay, native Mac/Windows, DNS/TLS/resources,
artifact promotion and billing decisions remain open. No integration application
is deployed; Bluey Jobs and existing Pinky environments remain excluded.

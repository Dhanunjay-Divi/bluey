# IMPL: Phase 626 — additive delegation/lifecycle foundation

Date: 2026-10-09. Preflight: load `$bluey-ops`, `$pinky-ops` and
`$pinky-bluey-integration-ops`; reconcile the integration runbook and source pins.

## Scope and status

This is the first I2 source slice, **not completion of I2 or the integration**.
It adds default-off Pinky request verification and owned AI-context Start/Stop.
No provider dispatch, content/history/upload access, entitlement purchase,
metering, charges, UI or application deployment is enabled by this slice.
The existing standalone and Jobs authentication code is unchanged.

Pinky has a matching stdlib-only signer in `internal/assistdelegation`, not wired
into its auth/config/API/native runtime. Its preparation branch is not yet the
accepted modern Pinky runtime base. Do not bulk-merge the old AI branch.

## Contract

- Explicit `BLUEY_PINKY_INTEGRATION_ENABLED=1` installs two POST routes:
  `/integrations/pinky/sessions` and `/integrations/pinky/sessions/close`.
- Required config: `BLUEY_PINKY_DELEGATION_SECRET`, `BLUEY_PINKY_ISSUER`,
  `BLUEY_PINKY_AUDIENCE`, `BLUEY_PINKY_ENVIRONMENT`. Absent/zero enable flag
  installs no integration routes or tables. Invalid enabled config fails before
  opening the DB or starting workers. Secret must differ from standalone JWT
  material; later Pinky wiring must prove separation from its own signing keys.
- HS256 tokens bind exact issuer/audience/environment/immutable subject,
  `ai:session` scope, canonical nonnil UUID `jti`, issue/expiry (at most 60s),
  POST method, exact allowed URI and lowercase SHA-256 of raw request bytes.
  Token/body bounds are 4 KiB/16 KiB. Query suffixes and extra claims fail closed.
- JSON body is exactly `{"session_id":"<canonical nonnil UUID>"}`.
  Response contains session ID, state and expiry, not another account's data.
- Principal key v2 hashes length-framed issuer, audience, environment and subject.
  No email matching or shared service wallet links existing Bluey users.
- First provisioning requires a fresh deterministic external-only account.
  A preexisting account collision denies enrollment atomically. Initial balance,
  trial, admin and reload are zero/off. Existing legitimate credits are preserved;
  sentinel/email and restricted/admin/trial/reload/temp/payment-ref invariants
  are rechecked. External account linking/billing remains separate future work.
- Start is atomic and idempotent by owned session UUID. Closed/expired IDs never
  reopen or reset their expiry. `jti` is syntactically validated, **not consumed**:
  do not reuse this foundation as one-shot payment/submit authorization.
- Stop is owner-fenced and idempotent, including after binding revocation.
  It currently returns 404 for unknown sessions. Callers must await Start before
  Stop; cancellation ordering/reconciliation is a required I3 test, not proven.

## Bounds and data layout

Only explicitly enabled integration initialization adds `pinky_ai_bindings`,
`pinky_ai_sessions` and their subject index. SQLite uses Immediate transactions;
PostgreSQL uses one integration-specific transaction advisory lock plus binding
row locks. No change was made to the Jobs binary/migration entrypoint.

Hard preprod cardinality caps: 100 bindings, 10,000 retained sessions total,
1,000 per principal and three unexpired active contexts per principal.
Existing UUID replay is checked before session caps. Closed/expired tombstones
are not evicted to regain admission and accidentally permit replay reopening.
This is a bounded preprod lifecycle, not a scalable history-retention design.

Verified requests share a single-process governor ceiling of 120/minute,
burst 30, independently for Start and Stop. Invalid requests cannot consume
these scarce valid-request budgets. Fixed keys bound limiter memory. This is
not distributed throttling or per-tenant fairness; a valid/compromised issuer
can exhaust the global budget. Multi-tenant admission must be qualified before
public release. No edge DoS protection claim is made.

403 means a typed domain denial; 429 means capacity/rate limit; infrastructure
failures return sanitized 503. Integration responses carry existing request
and trace IDs. Diagnostics contain stages/status/timing, not tokens or bodies.

## Verification and open gates

Actual results belong in `REVIEW-PHASE-626-DELEGATION.md`. Use the local Mac
queue with `scripts/run-pinky-integration-tests.sh`; it binds a fresh target,
DB/data/config/runtime/log/temp workspace, strips inherited provider/DB env,
uses offline locked dependencies and cleans success/failure/signals.

Still required: live PostgreSQL parity/concurrency; accepted Pinky runtime base;
revocation/entitlement recheck at dispatch; fair bounded admission; cancellation
ordering; source-consent generations; model streaming and billing; resource/
secret independence; actual physical Mac/Windows and isolated live preprod.
Production GitHub runners belong to the later coordinated Pinky release only.

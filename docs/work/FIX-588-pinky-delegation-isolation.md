# FIX-588: Pinky delegation draft isolation gaps

Preflight: load `$bluey-ops` and `$pinky-bluey-integration-ops`.

## Issue and root cause

Independent source review found that the first unpublished lifecycle draft
omitted audience from persistent identity, accepted deterministic account-ID
collisions, and lacked durable admission/cardinality bounds. A subsequent
rate-limit draft let unauthenticated traffic consume the global valid-request
budget. The configuration regression also needed an explicit closure argument
type to satisfy Rust's lifetime bounds; its first full compile failed.
The subsequent full functional suite passed, but strict Clippy required direct
string length and a compact typed HTTP error rather than a large tuple.

## Fix summary and files

- `server/src/pinky_integration/store.rs`: audience-bound key v2, fresh-only
  account binding, origin/security rechecks, atomic caps and replay handling,
  serialized SQLite/PostgreSQL admission, and collision/quota/concurrency tests.
- `server/src/pinky_integration/mod.rs`: bounded post-verification admission,
  separate Start/Stop budgets, typed sanitized statuses and request IDs.
- `server/src/main.rs`: validate integration config before DB/workers and use
  the tested additive application composition.
- `server/src/pinky_integration/http_tests.rs`: default-off/merged-router,
  auth separation, limits, failures and configuration regressions.
- `server/src/pinky_integration/delegation.rs`: exact lowercase hex, nonnil
  request IDs and a valid cross-language lifecycle golden vector.

## Reproduction and known limitations

Run the owned temporary test harness through the shared local Mac queue;
results and residues are in the matching review. No GitHub preprod runners.
Live PostgreSQL, billing/dispatch/consent, per-user fairness and physical UI
validation remain open. This fixes unpublished source, not a production incident.

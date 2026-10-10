# IMPL: Phase 626 — read-only Pinky account summary

Preflight: load `$bluey-ops` and `$pinky-bluey-integration-ops`, then the current
feature branch/runbook. Pinky changes use `$pinky-ops` in its own worktree.

## Scope

Adds signed `POST /integrations/pinky/account`, exact `ai:account` scope and
strict empty-object body. Reads an existing immutable-subject binding, validates
external-account/entitlement invariants and returns content-free integer-cents
credit figures. It cannot provision accounts, credit, sessions or provider work.
Spendable and reserved amounts share one SQL statement snapshot. Missing or
revoked state is not a fabricated zero; a real depleted wallet may show zero
while AI access is unavailable. Ledger freshness is optional and evidence-based.

Files: `server/src/pinky_integration/{account.rs,mod.rs,store.rs,delegation.rs,
http_tests.rs}`. No standalone or Jobs source edits, providers, billing policy,
schema changes or production deployments.

## Build and test

Source formatting/diff-check passed. Local all-target tests and strict Clippy
are queued through the shared `mac-heavy` harness, with owned temporary Cargo,
database/config roots and cleanup. PostgreSQL parity is a separate real-store
gate, not inferred from SQLite fixtures. No hosted runners.

## Review

Independent review found cross-repo currency/timestamp contract and PG flag
type mismatches; corrections must pass typed-store tests before preprod cutover.
HTTP fixtures cover wrong scope, tampered body, malformed/scalar/trailing JSON,
tenant isolation, no writes on missing reads, revoked/expired and hold semantics.

## Follow-ups

Physical native UX, exact preprod artifacts, source-specific media consent,
private transcript history/retention, real payments and STAR factual quality are
separate open gates. This change is not whole-product readiness.

# Phase638 decision — stable preflight authority, rotating live lease

Date:2026-10-28. Preflight:bluey-ops and bounded arena-review.
Authority:current Phase638 source and actual private-PG causal-red76811.
Decision is implementation direction, NOT verification or release acceptance.

## Requirement and budget

An exact same-owner, unexpired prepared managed-cloud claim retry must rotate its
live token/fence without rewriting or extending immutable ATS PhaseA authority.
All current source, packet, Track consent, certification, runtime, volume, managed
release, session/profile and operational/account checks remain mandatory.
No employer/browser operation, schema migration or production action is authorized.

Two existing specialists each produced one read-only proposal (approximately
eight-minute/800-word cap), followed by one bounded challenge each. No new model,
framework runtime or worker roster was started. Parent evaluated source evidence
and the independent challenge, rather than treating agreement as test evidence.

## Alternatives and challenge result

A: retain the single immutable preflight row and its original nonce AND requested
expiry. Enable exact replay only through the cloud claim's validated existing
prepared lease; keep generic/local creators strict. Existing PhaseB first checks
live token/fence and then uses the stored ATS nonce. Existing terminal recovery
and packet invalidation also use that stored nonce. These independent authority
layers already exist; replay does not turn the ATS nonce into a caller credential.

B: create immutable ATS successors per live lease generation/fence. Feasible, but
requires versioned canonical authority, SQLite/PG migrations, new uniqueness and
current-generation selection, generation-specific PhaseB/recovery, and a forward-
only rollback policy once multiple rows exist. It solves broader renewal/version
requirements not demonstrated by this bounded retry failure. Dropping existing
unique constraints or selecting whichever generation is latest would be unsafe.

Select A. It meets the observed retry contract while preserving immutable history,
existing PhaseB/recovery semantics and old-binary schema compatibility. B is not
rejected as impossible; its extra authority/migration surface is unnecessary here.

## Implementation contract

- Only a production managed-cloud claim with an exact locked existing prepared
  lease may opt into reuse. Require same account/application/run/profile/owner and
  live lease expiry after final database time. SQLite reads under IMMEDIATE;
  PostgreSQL uses the existing locked row without moving H→M→ATS→D prelocks.
- Exactly one matching ATS row must exist. Require canonical/hash-valid preflight,
  fence0, unconsumed/uninvalidated, unexpired; absence or conflict never creates a
  replacement on this path. Preserve full frozen bytes and original deadline.
- Normal context construction must still resolve current target/admission/consent,
  certification frozen projection and exact runtime before strict raw replay.
  Preserve channel, allowlist, packet, attempt, session/profile and target equality.
  Do not add complete certification-struct equality to PhaseA: changing head
  watermarks are not a substitute for its established projection/compatibility
  rules. Do not relax existing PhaseB certification behavior in this correction.
- Rotate the live token and strictly increase its fence in the same transaction;
  stale heartbeat/finish/irreversible requests must fail before authority mutation.
  Current credentials retain existing PhaseB semantics and exactly-once gates.
- Generic/local/contextless creators keep exact nonce/expiry replay requirements.
  Takeover and expired-lease/ATS renewal are NOT enabled by this bounded change.
  ATS's immutable at-most15-minute cap remains; expired state fails closed.

## Required evidence and remaining uncertainty

Implement source-level and actual SQLite/PG regressions for exact replay, absent/
multiple/expired/consumed/invalidated binding, changed scope/packet/consent/runtime,
rollback, stale credentials and retained terminal recovery. Prove current-token
PhaseB once without a real employer action using existing private fixtures.

The ordinary real-client green case must independently prove first200, observed
heartbeat, fresh readiness and fresh-proof repeat200/new token/higher fence,
unchanged ATS bytes/deadline, stale-token heartbeat+safe-finish409 with unchanged
protected state, current heartbeat200, safe release204 and fresh terminal replay.
Keep the causal-red report separately labeled and sourceAcceptedfalse. No narrow
pass replaces full source/native/cloud/receipt/cost/release gates. Fully autonomous
takeover and explicit expired-preflight recovery remain separately bounded work.

## Ownership

Fixture specialist now owns only execution_leases.rs, ats_certification_authority.rs
and inline tests for this accepted design. Regression specialist owns only the
three private Node controller/report/test files. Parent owns docs/integration and
all runtime; independent reviewer owns REVIEW638 after source freeze. All preserve
existing dirty work. No worker may build/start services or edit meeting/overlay.

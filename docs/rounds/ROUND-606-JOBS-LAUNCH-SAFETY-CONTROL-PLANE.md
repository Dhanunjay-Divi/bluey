# Round 606 - Jobs Launch Safety Control Plane

**Date:** 2026-08-07

**Branch:** `feat/phase-606-jobs-launch-safety`

**Status:** SOURCE COMPLETE AND LOCALLY VERIFIED - code snapshot
`72056e6db026dcd25ebe54bd41e1681e4185c727`; every provider, device, tenant,
credential, deployment, and production flag remains parked

## Objective

Add one durable, privacy-safe operational authority that can stop new Bluey
Jobs work at the exact pre-side-effect checkpoints without preventing Bluey
from recording or reconciling an outcome that may already exist.

The control plane composes with, but does not replace or weaken, discovery
source pause state, signed ATS certification circuits, account-deletion fences,
Browser release authority, runner-volume authority, provider grants, or the
disabled-by-default environment gates.

## Capabilities

The closed capability set is:

- `discovery`;
- `generation`;
- `application_queue`;
- `runner_claim`;
- `final_submit`;
- `mailbox_sync`; and
- `communication_dispatch`.

`all` is an operator-only umbrella hold evaluated alongside the requested
capability.

## Scopes

Every scope key is derived or normalized by the server. The closed scope set is
`global`, `discovery_source`, `ats_provider`, `ats_adapter`,
`employer_domain`, `account`, `career_track`, `region`, `runner_kind`,
`mailbox_provider`, `model_provider`, and `model`.

Raw scope keys are stored only because transactional enforcement needs exact
matching. Admin lists expose an opaque scope reference; Prometheus labels,
customer responses, and logs never expose a raw account, Track, source,
employer, model, run, lease, or provider-object identity.

## Authority Model

1. Each hold or release is one canonical, hash-identified, append-only event.
2. A monotonic head is updated only by exact compare-and-swap over its current
   revision and event identity.
3. The first event for a scope must hold it. Re-enablement requires an explicit
   release event; there is no TTL or silent automatic expiry.
4. The PostgreSQL path serializes event mutation against shared admission locks;
   SQLite uses an immediate transaction. A hold cannot race through a new
   protected claim or marker.
5. Any storage, migration, context, or evaluation error denies new authority.
6. Releasing a generic hold cannot resume a paused discovery source or close a
   signed ATS circuit. Resuming either native authority cannot bypass a generic
   hold.
7. PostgreSQL application/job context and every relevant discovery writer
   share one account fence, so the admitted posting, membership, source, and
   Track scopes are one stable snapshot.
8. Mailbox leasing validates canonical, relational, and encrypted provider
   projections before mutation.
9. An unexpired discovery lease freezes relevant Track mutation. Curated
   selection excludes inactive Tracks and rejects relational/JSON projection
   drift; an explicitly bound inactive Track retains Region hold scope.
10. A curated materialization orphan fails closed until its exact managed
    membership exists. An unassigned application reservation has no invented
    runner-kind scope; a later cloud/local claim evaluates and binds its own.

## Admission Boundaries

Holds are enforced before:

- a direct or global discovery lease;
- a paid managed-generation provider reservation;
- application-attempt reservation, including an active-reservation replay;
- local or cloud runner claim/reissue;
- local or cloud final pre-click authority;
- mailbox-sync lease claim; and
- communication dispatch claim and the durable request-start marker.

Holds do not block heartbeat, Browser-profile sealing, worker result or receipt
persistence, an exact click-started replay, submission checkpoint recovery,
mailbox completion, communication completion, or read-only reconciliation.
Those paths reduce uncertainty after a possible side effect and must remain
available during an incident.

For a local `click_started` replay, "exact" means that the frozen Browser
build/release binding, ticket, application, run, session, final-submit proof,
terminal ATS authority, and evidence capacity still agree. The current server
deployment may move from release `A` to release `B` only when the immutable
activation frozen onto the run accepted both IDs. That deliberate recovery
rule prevents a server deployment from stranding a possible employer side
effect. A server ID outside the frozen activation's accepted set is denied.
The HTTP route grants reconciliation grace only to a signed v2 submit
capability over durable `click_started`, allows that exact state through a
distribution pause, and reconstructs the active durable capacity across
object-storage maximum drift while retaining current upload limits. Claimed or
legacy submit, expired/inactive durable capacity, and wrong-scope authority
remain denied.
Signed v1/v2 result reconciliation also reuses durable capacity for
`click_started` or `side_effect_unknown`; submit recovery remains v2-only, and
claimed/`needs_input`/new paths continue to derive current configuration.

## Acceptance Criteria

1. Paired SQLite and PostgreSQL migrations create identical event/head tables,
   indexes, ancestry, immutability, and monotonic-head constraints.
2. Migration replay is idempotent, no permissive row is seeded, and optional
   PostgreSQL tests assert the exact migration ledger entry.
3. Enums, scope keys, event IDs, reason codes, and reason references are closed
   and bounded before canonical hashing.
4. Exact event replay returns the original state; changed bytes or actor under
   the same event ID fail with identity conflict.
5. Stale expected revision/event identity fails compare-and-swap without
   writing history or changing the head.
6. `held -> held`, `held -> released`, and `released -> held` preserve one exact
   predecessor chain. A first release is rejected.
7. `all` and every capability/scope combination are evaluated fail-closed; any
   applicable held head wins until all applicable heads are released.
8. Queue, claim, and pre-click replays cannot bypass a hold committed before
   the new authority boundary.
9. An already durable irreversible or provider request-start marker retains
   completion and reconciliation authority during a later hold. A local
   `click_started` recovery preserves its exact frozen Browser binding across
   an accepted current-server `A -> B` deployment and denies an unaccepted
   server ID without minting a new marker, capacity, or ATS authority. Route
   expiry, distribution, and storage-config checks preserve that recovery
   exception only for durable exact state.
10. Admin mutation/list/readiness routes require bearer authentication and
    admin authority on both the full and independently deployed Jobs routers.
11. Admin JSON is `private, no-store`, bounded, deny-unknown on mutation, and
    redacts raw scope identity and reason reference.
12. Metrics use only closed low-cardinality capability/scope labels and expose
    no tenant, source, employer, model, URL, token, hash, or error text.
13. Readiness composes generic holds with native source pause and ATS circuit
    blockers without granting or closing either native authority.
14. Operational discovery diagnostics emit only opaque source references.
15. Full formatting, strict Clippy, server/Jobs tests, schema parity, privacy,
    provenance, and CI guards pass before source completion is claimed.

## Current Acceptance Status

| Gate | Status | Evidence boundary |
|------|--------|-------------------|
| Source implementation | Complete | Paired migrations, DB/API/metrics authority, every protected admission, exact recovery, and the operations contract are frozen at `72056e6d`. |
| Focused local checks | Green | Recovery, mailbox projection, PostgreSQL structure, Track lease/projection, curated orphan, schema, privacy, Browser, and diagnostic regressions passed. |
| Complete local matrix | Green | 1,507 Jobs tests, 5/5 typecheck/build gates, 1,351 server tests, 14 native-storage tests, strict Clippy, schema/provenance/privacy/diff gates passed. |
| Independent review | Green | Final read-only code audit found no remaining blocker or minor after the Track projection and Region-scope corrections. |
| External production gates | Parked | Live PostgreSQL/concurrency, credentials, provider writes, physical devices, live tenants, immutable artifact promotion, canaries, and flag changes were not performed. |

The portal bundle rebuilt deterministically as 2,292 modules and 27 files with
no source maps and aggregate SHA-256
`8eadf40bc36d19daaae2169342d08dd5a140a3bd0e0d676b989c87e9cff0bcee`.
SQLite/PostgreSQL parity passed at 71 tables and 66 indexes. Optional live
PostgreSQL tests compiled and self-skipped because no authorized
`BLUEY_TEST_POSTGRES_URL` was configured. Docker is unavailable locally, so the
unchanged managed-runner image smoke remains a hosted gate.

## External Production Boundary

This round does not authorize a credential, provider write, physical-device
test, live PostgreSQL migration, tenant mutation, release promotion, canary, or
production flag change. Those gates remain explicit follow-up work after the
source and exact-SHA checks are green.

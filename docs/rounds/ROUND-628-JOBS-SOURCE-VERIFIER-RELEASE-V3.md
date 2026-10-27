# Round 628 — Jobs Source-Verifier Readiness and Managed Release V3

> Codex preflight: load `bluey-ops`. Current Jobs code and local handoff are authority.
> The meeting checkout and desktop overlay are outside this batch.

Updated: 2026-10-27. Branch: `feat/phase-628-jobs-source-verifier-release-v3`.
Original batch base: `fc7e535143a262689da8e99ce7638aee42f40407`.
Continuation base: `529f49d6687372caee94e6e01e6a8b64f0698c5b`.
Status: independent-review corrections verified locally; full server gate PASS
(1,832 tests), strict Clippy PASS, separate independent critical backend reruns
PASS (24 boot / 23 migration, overlapping filters). Both scoped independent
source reviews accept. External release gates remain required; October 26
results are separately labeled historical evidence.

## Objective and boundary

Complete the naturally idempotent original-source-verifier readiness protocol.
No push, PR, merge, rebase, registry publication, deployment, production flag
change, external account access, ATS call, application or message is authorized.
Future interview-context persistence, employer cooldown, ambiguity resolution
and confirmed-claim revision batches remain excluded.

## Immutable historical contracts

| Release | Direct/global discovery | Source verification | Migration heads |
|---|---|---|---|
| v1 | false / false | false | SQLite 056 / PostgreSQL 034 |
| v2 | false / false | true | SQLite 057 / PostgreSQL 035 |
| v3 | true / true | true | SQLite 065 / PostgreSQL 043 |

V1/v2 audiences, canonical ordering, bytes, protocols and migration heads are
unchanged. V3 audiences remain `bluey-jobs-managed-cloud-release-v3` and
`bluey-jobs-managed-cloud-activation-v3`. Artifact promotion/rollback reuse the
existing generation-v2 evidence without rebuilding artifacts.

V3 binds nine runtime roles plus the static portal, four deployable artifacts,
and fourteen ordered protocol entries. `source_verification@1` stays ordinal 9;
`source_verifier_startup@1` stays ordinal 10; the separate
`source_verifier_boot@1` is ordinal 11. Do not sort these new entries into a
different order or reinterpret the legacy startup protocol.

## Boot identity and authentication

Registration and startup use exact-path managed-cloud-runtime worker HMAC plus
the actual runtime session proof. Worker, instance, epoch, release, component
and role must match the immutable registered runtime and current authority.
Session tokens participate in request identity only through their digest and
are never returned in canonical responses.

The process creates one boot ID and stable registration/startup request IDs.
Only the server assigns the next monotonic generation. A new process receives
a new identity/generation; no client-proposed generation is accepted at
registration. Exact-token/exact-payload retries replay stored canonical bytes
and stored time, including after another boot advances the current generation.
Runtime/session and active release checks still precede replay; only new
publication requires the current boot head. Changed payloads conflict.

Endpoints:

- `POST /api/jobs/internal/managed-cloud/runtime-instances/:id/source-verifier-boot-registration`
- `POST /api/jobs/internal/managed-cloud/runtime-instances/:id/source-verifier-boot-startup`

## Readiness transaction

1. Claim the runtime and register boot generation G.
2. Publish degraded/startup at sequence N with boot ID and G.
3. Authenticate startup and fence the exact current generation.
4. Inside a savepoint, provisionally install startup authority and ready N+1,
   execute the real backend lease-decision path, then roll back the savepoint.
5. Commit the actual idle/leaseable result, immutable canonical response,
   monotonic ready generation, ready N+1 heartbeat and audit in one transaction.
6. Start the verifier only after validated authority; recurring heartbeats begin
   at N+2 and carry the same boot fence.

A response lost after commit cannot cause a second transition. Bounded client
retries resend identical payloads while the server owns deduplication.
There is no replay-before-heartbeat recovery protocol, no client-supplied
probe result and no fabricated idle fallback. A startup error never starts
the worker and never emits a speculative failure heartbeat after ambiguous
readiness.

Every heartbeat, including exact replay, validates the current registered
generation. Old boots and late failure reports cannot lower a newer ready
generation. An already-ready generation cannot be lowered by generic
liveness. A genuine newer boot may establish its own degraded startup and must
re-run the rollback-only probe.

Operational lease/heartbeat/publication requests also carry the process's
boot ID/generation. V3 current runtime authority requires current = ready
generation and a matching heartbeat sequence binding. Claim transactions
persist immutable per-attempt boot authority; new boots cannot adopt old
attempts. A response-only exact terminal replay remains bound to the original
attempt and cannot authorize new work. Legacy wire bindings omit both fields.
Startup requires an exact current `degraded/startup` predecessor in code and
SQL; `degraded/probe_failed` is not an admissible readiness predecessor.

SQLite uses IMMEDIATE transactions. PostgreSQL locks operational hold H,
managed registry M, runtime/boot head, discovery account D, then assignment.
The probe leaves no assignment, lease, event or provisional readiness mutation.

## Persistence and upgrade

- SQLite 062 / PostgreSQL 040: original startup/signature authority.
- Committed SQLite 063 / PostgreSQL 041: boot registrations, heads, startup
  authorities and separate manifest boot protocol.
- Committed SQLite 064 / PostgreSQL 042: original head-only ready fence,
  preserved byte-for-byte.
- New SQLite 065 / PostgreSQL 043: exact per-heartbeat boot binding and
  prospective legacy-trigger replacement. Legacy sequence/time/readiness
  predicates remain active for non-boot manifests.
- SQLite 063-065 execute through a guarded transaction boundary that rolls back
  unfinished migrations and restores the prior foreign-key mode on failure or
  success. Committed SQL script bytes are unchanged.
- The successor also admits ten capabilities for the exact full discovery +
  source-verification feature shape. SQLite copies every existing manifest
  field while rebuilding the CHECK; PostgreSQL replaces only that constraint.

The old fake-instance boot tests were not runtime-authentication evidence.
Current regressions use a real signed managed-release fixture, real claimed
runtime/session and actual heartbeat/probe transactions.

## Verification and handoff

See IMPL and FIX-789 for exact commands, observed counts, failures and pending
environmental evidence. Schema parity includes all five boot tables, their
index and semantic constraints, not merely migration registration.

The independent REVIEW remains unapproved. No local test authorizes a hosted
workflow, registry action, flag enablement or production promotion.

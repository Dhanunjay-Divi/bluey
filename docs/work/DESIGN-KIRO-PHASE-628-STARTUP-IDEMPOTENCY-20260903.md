# DESIGN (Kiro): Phase 628 Startup/Readiness P1 — Idempotent Boot-Generation Protocol

> **Author: KIRO.** Not Codex. Design + preflight evidence only.
> **STATUS: NO CODE WRITTEN. NO COMMITS. NO GATES RUN.** Implementation deliberately
> not started — see "Why implementation is not started" before treating this as done.
> Kiro preflight completed: `$bluey-ops` loaded, `AGENTS.md` read.

Date: 2026-09-03
Worktree: `/Users/uno/Downloads/cue-jobs-source-verifier-release-v3-20260831`
Branch: `feat/phase-628-jobs-source-verifier-release-v3`
HEAD verified: `fc7e535143a262689da8e99ce7638aee42f40407` (exact match)
Dirty files preserved: 28 (nothing reset, cleaned, stashed, or discarded)
`/Users/uno/Downloads/cue`: never touched

## Root cause (exact evidence)

`infra/sqlite/server-runtime/062_jobs_original_source_verifier_startup_authority.sql`
table `jobs_managed_cloud_original_source_verifier_startup_authorities` declares:

```
UNIQUE(runtime_instance_id, instance_epoch)
UNIQUE(startup_request_id, request_sha256)
startup_authority_sha256 TEXT NOT NULL UNIQUE
response_sha256          TEXT NOT NULL UNIQUE
canonical_response_base64url TEXT NOT NULL
```

Startup identity is therefore `(runtime_instance_id, instance_epoch)`. Readiness recovery
is client-driven: `jobs/workflows/src/original-source-verifier.ts`
`establishOriginalSourceVerifierStartup` publishes `degraded/startup` at sequence N, calls
startup once, and on failure attempts one `N+1 degraded/probe_failed`
(`original-source-verifier.ts:121-172`).

Consequence (the P1): if the ready response is lost after commit, a restarted lifecycle
re-enters with the same instance epoch, emits another degraded heartbeat, and issues a new
startup request. It then collides with the stored row, and a `probe_failed` path can degrade a
runtime that is already committed ready. "Replay before the next heartbeat" is client-ordered,
so it cannot be relied on. No amount of client logic fixes this while startup identity is
`(instance, epoch)` and readiness is not fenced by a server-assigned generation.

## Required protocol (design)

### New identities
- `boot_id`: stable per-process identity, generated once at process start, stable across
  retries within that process, never reused after restart.
- `boot_generation`: server-assigned, strictly monotonic per `runtime_instance_id`. Clients
  never propose it.
- `registration_idempotency_token` and `startup_idempotency_token`: client-generated, bound to
  exact canonical payload digests.

### New paired migrations
Must be SQLite `063_` and PostgreSQL `041_` (Phase 628 owns `062_`/`040_`, registered in
`server/src/db/mod.rs` with a parity assert on the `040_` filename). Do not renumber 628.

1. `jobs_managed_cloud_source_verifier_boot_registrations`
   - `boot_id` PK (charset/length CHECK in house style)
   - `runtime_instance_id`, `worker_id`, `runtime_identity_sha256`
   - `boot_generation INTEGER NOT NULL CHECK(boot_generation BETWEEN 1 AND 9007199254740991)`
   - `registration_idempotency_token`, `registration_request_sha256`
   - `canonical_response_base64url`, `response_sha256 UNIQUE`
   - `UNIQUE(runtime_instance_id, boot_generation)`
   - `UNIQUE(registration_idempotency_token, registration_request_sha256)`
   - insert-only triggers (`no_update` / `no_delete`) matching
     `trg_jobs_managed_cloud_signature_sets_no_update|no_delete` style
2. `jobs_managed_cloud_source_verifier_boot_generation_heads`
   - `runtime_instance_id` PK, `current_boot_generation`, `updated_at_ms`
   - the only mutable row; advanced by CAS to assign a generation
3. Startup authority v3 binding
   - add `boot_id`, `boot_generation`, `startup_idempotency_token`
   - replace startup identity with `UNIQUE(runtime_instance_id, boot_generation)`
   - keep every historical column and CHECK unchanged; historical rows must remain valid
4. Readiness fencing
   - runtime heartbeat rows gain `boot_generation`
   - triggers reject any transition whose `boot_generation` is lower than the committed ready
     generation for that instance

Historical v1/v2 audiences, feature matrices, migration heads, protocol ordinals, and
serialized bytes remain byte-identical. `protocol_version = 1` for `source_verifier_startup`
is CHECK-pinned, so the new wire must be a NEW protocol id
(`source_verifier_boot` version 1) with its own ordinal rather than a version bump.

### Endpoints
- `POST /api/jobs/internal/managed-cloud/runtime-instances/:id/source-verifier-boot-registration`
  Idempotent. Same token + identical payload replays stored canonical bytes. Same token +
  different payload conflicts. New boot receives a strictly higher generation.
- Existing startup endpoint gains `boot_id` + `boot_generation` + idempotency token; becomes
  naturally idempotent (replay stored bytes) instead of conflicting.

### State machine
1. Register boot -> server assigns `boot_generation` G (CAS on the head row).
2. Publish `degraded/startup` at sequence N fenced by G.
3. Call startup with the idempotency token. Server runs the rollback-only readiness probe
   inside a savepoint, commits `N -> N+1 ready` bound to G, stores canonical response bytes.
4. Retry of the same token replays identical bytes and time (no re-probe).
5. A genuine NEW boot registers, receives G+1, and re-runs the probe.
6. Duplicate same-state transition = no-op. Stale generation (< committed ready G) = rejected.
7. Generic liveness heartbeat can never lower committed readiness.
8. Stale or completed `probe_failed` cannot degrade a newer ready generation.
9. PostgreSQL preserves existing global lock order H -> M -> D -> assignment; the boot head row
   is locked after M and before D.

## Exact files a correct implementation modifies

- `infra/sqlite/server-runtime/063_*.sql` (new), `infra/postgres/server-runtime/041_*.sql` (new)
- `server/src/db/mod.rs` (ordered registration + parity assert; CONTENDED with Phase 628)
- `server/src/db/jobs/managed_cloud_release_authority.rs`
  (validation near `validate_managed_cloud_original_source_verifier_startup:13520`,
  `managed_cloud_original_source_verifier_startup_request_sha256:13564`,
  `..._authority_sha256:13576`, `require_..._startup_identity:13588`,
  heartbeat validation `validate_managed_cloud_runtime_heartbeat:13481`)
- `server/src/db/jobs/original_source_verification.rs` (shared probe under savepoint)
- `server/src/api/jobs_managed_cloud_releases.rs` (new registration route + raw canonical replay)
- `server/src/api/jobs_worker_auth.rs` (admit the new exact path)
- `jobs/automation/src/managed-cloud-runtime-client.ts` (+ tests)
- `jobs/workflows/src/original-source-verifier.ts`,
  `original-source-verification-runtime.ts` (+ tests)
- `jobs/scripts/managed-cloud-release-gate.mjs` (+ `.test.mjs`),
  `jobs/scripts/check-jobs-schema-parity.mjs`, `jobs/scripts/ci-guards-self-test.mjs`
- Docs: ROUND-628, FIX-789, IMPL, `jobs/OPERATIONS.md`, `CHANGELOG.md`, release-gate docs

## Regression matrix (17 required cases -> mechanism)

| # | Case | Mechanism that must make it pass |
|---|------|----------------------------------|
| 1 | Ready response lost after commit | startup token replay returns stored bytes |
| 2 | Registration response loss + exact retry | registration token replay |
| 3 | Startup response loss + exact retry | startup token replay |
| 4 | Same token, changed payload | request_sha256 mismatch -> conflict |
| 5 | Restart after committed readiness | new boot -> G+1, old G fenced |
| 6 | New boot higher generation + re-probe | head CAS + savepoint probe |
| 7 | Stale boot heartbeat | generation fence rejects/no-ops |
| 8 | Stale boot readiness cannot win | fence + UNIQUE(instance, generation) |
| 9 | Stale failure cannot lower newer ready | trigger + generation compare |
| 10 | Duplicate ready reporting | no-op |
| 11 | Generic heartbeat cannot degrade readiness | trigger rejects ready->lower |
| 12 | Concurrent duplicate startup, SQLite | one IMMEDIATE tx + UNIQUE |
| 13 | Concurrent duplicate startup, configured PG | row lock in fixed order + UNIQUE |
| 14 | Migration replay/upgrade | ordered registration + replay test |
| 15 | Schema parity exact | `check-jobs-schema-parity.mjs` |
| 16 | Release-gate/CI mutation guards detect drift | gate + `ci-guards-self-test.mjs` |
| 17 | v1/v2 unchanged | historical canonical fixture hashes |

## Verification gate matrix (measured on this host, 2026-09-03)

| Gate | Availability |
|------|--------------|
| Rust fmt / focused check / strict Clippy / full tests via `scripts/run-bluey-tests.sh` | AVAILABLE (cargo 1.95.0, rustc 1.95.0) |
| TypeScript typechecks + suites (5 Jobs suites) | AVAILABLE (node v24.1.0, npm 11.3.0 at `/opt/homebrew/bin`; NOT on the non-interactive PATH by default) |
| Configured non-production PostgreSQL concurrency | LIKELY AVAILABLE — psql 14.17 (Homebrew) + PostgreSQL 17 at `/Library/PostgreSQL/17`; a server IS listening on 127.0.0.1:5432. A DEDICATED non-production database must be created; no production credentials to be requested or used. |
| Docker / managed-runner container smoke | UNAVAILABLE — no `docker` binary, no `/Applications/Docker.app`. Must be recorded PENDING. |
| Native managed-runner smoke | To be assessed after implementation |

Note for the runner: Rust/TS commands must export
`PATH=/opt/homebrew/bin:/opt/homebrew/opt/postgresql@14/bin:$PATH`, and every Rust
test/build must use fresh isolated Cargo/DB/data/config/runtime/log/temp directories with
cleanup on success, failure, and interruption, never inheriting production database variables.

## Why implementation is not started (Kiro, explicit)

1. Scale. The existing Phase 628 diff is 7,310 insertions for a smaller contract. This change
   adds two tables, a mutable generation head, a new protocol id and ordinal, a new
   authenticated endpoint, generation fencing across the heartbeat state machine, dual-engine
   parity, and 17 regression cases. That is a comparable or larger body of work.
2. Quality bar. Every table here carries exhaustive CHECK constraints, immutability triggers,
   canonical byte replay, and mutation guards. Producing that volume without being able to
   iterate compile/test cycles to completion in one pass would yield unverifiable code.
3. Reviewer contract. An independent reviewer inspects the real diff. Half-implemented protocol
   state in a 28-file dirty tree is worse than a clean design handoff.
4. Honesty. Kiro will not report commits, test counts, or passing gates that did not run.

## What Kiro asks before implementing

Confirm (a) the new-protocol-id decision (`source_verifier_boot` v1 rather than bumping
CHECK-pinned `source_verifier_startup` v1), (b) migration numbers SQLite `063` / PostgreSQL
`041`, and (c) that a dedicated local non-production PostgreSQL database may be created for
case 13. On confirmation Kiro implements in this order: migrations -> Rust authority ->
endpoint/auth -> TS client/lifecycle -> gate and CI guards -> tests -> docs -> local
conventional commits, running the narrowest tests while iterating and the full available gates
before committing.

## Compliance statement for this artifact

- No source file modified. No migration added. No test run. No commit created.
- No push, PR, merge, rebase, deploy, flag change, image publish, or external call.
- No production credential requested or used. Production feature flags untouched at `0`.
- No temporary test workspace created, therefore none to remove.
- Phase 628 uncommitted work preserved exactly (28 dirty entries, unchanged).

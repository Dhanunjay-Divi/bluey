# REVIEW: Phase 628 — Independent Client and Release Pass

**Commit range:** `fc7e535143a262689da8e99ce7638aee42f40407..4ef949732ee50e0d8691e5eb809462e97eae0199`
**Reviewer:** Codex independent client/release reviewer
**Date:** 2026-10-27
**Initial mode:** read-only implementation review plus this report. No Cargo/Node tests, network,
external accounts, source edits, canonical REVIEW edit, release action, or production action.

> **Historical record:** The original verdicts and findings below are preserved as observed on
> the initial pass. The current disposition after correction and independent reruns is recorded
> in the follow-up appendix.

## Original verdicts (initial pass)

- Client/workflow/release implementation: 🟡 **ACCEPT WITH REQUIRED GUARD FOLLOW-UP**.
  The reviewed code correctly uses stable per-process boot/request identities, byte-identical
  retry bodies with freshly generated worker-auth headers, session-bound registration/startup,
  server-assigned generations, boot-bound operational requests, and worker start only after a
  valid startup response. V1/v2 fixtures retain hard-coded canonical hashes; v3 binds nine
  runtime roles, ten capabilities, fourteen ordered protocols, and heads SQLite 065/PostgreSQL
  043. Publication continues to bind the four exact candidate artifacts, controlled refs,
  remote readback, exact-tip workflow evidence, promotion generation 2, and rollback generation 2.
- Operations/documentation source: 🔴 **REQUEST CHANGES** due to P1 below.
- Overall source verdict: 🔴 **REQUEST CHANGES**. Do not release from this tip until the
  contradictory active runbook is corrected and independently re-reviewed.
- External/runtime release verdict: **NOT VERIFIED / HOLD**. PostgreSQL, exact Linux Docker/native,
  hosted exact-tip CI, registry publication/readback, signing, deployment, canaries, and flags
  remain separate pending gates; none is converted into a source defect by this review.

## Original findings (initial pass)

### P1 — Active v3 operations runbook instructs the superseded unsafe startup protocol

**Location:** `jobs/OPERATIONS.md:1562-1601`, contradicted by the current contract at
`jobs/OPERATIONS.md:1883-1922`, Round 628, FIX-789, and the implementation.

The first active release-v3 section says v3 binds SQLite 062/PostgreSQL 040, calls
`original-source-verifier-startup`, conflicts on a second request for the same epoch, emits an
`N+1 degraded/probe_failed` heartbeat after startup failure, and needs a fresh grant/epoch to
recover. The implemented/current contract instead binds 065/043, registers a server-assigned
boot generation, calls `source-verifier-boot-startup`, replays exact request tokens, and explicitly
forbids a speculative `probe_failed` heartbeat after ambiguous startup.

**Failure scenario:** an operator or deployment author follows the earlier v3 section, invokes
the legacy endpoint or emits `probe_failed`, and either fails closed during an incident or builds
automation around a recovery transition that current SQL/code intentionally reject. The later
correct section does not make the earlier normative instructions safe; the guide contains two
mutually exclusive v3 procedures.

**Required correction/test:** replace or explicitly mark the obsolete v3 block superseded and
leave one canonical v3 procedure naming both boot endpoints, heads 065/043, stable retry tokens,
and the no-speculative-heartbeat rule. Add a documentation/CI mutation guard that rejects the
legacy endpoint, heads 062/040, and `probe_failed` recovery language inside the active v3 section.

### P2 — Phase 628 schema guard does not inspect its authority triggers/functions

**Location:** `jobs/scripts/check-jobs-schema-parity.mjs:45-60,3081-3187,3304-3382` and
`jobs/scripts/ci-guards-self-test.mjs:3114-3195`.

All Phase 628 tables are placed in `SEMANTIC_PARITY_TABLES`, so parity reduces their definitions
to column name/type/nullability/PK/unique shapes. The only Phase 628 semantic check outside that
shape is a broad `FOREIGN KEY(boot_id)` substring check. There are no exact SQLite trigger or
PostgreSQL function/trigger requirements for current-generation fencing, exact
`degraded/startup` predecessor, ready-generation non-regression, immutable heartbeat bindings,
or attempt-binding mutation/deletion rules. The self-test mutates a column, one foreign key, and
migration registration names, but not those authority semantics.

**Failure scenario:** a future change removes or weakens the SQLite stale-generation trigger or
its PostgreSQL counterpart while leaving table columns and the boot foreign key intact. The schema
parity/self-test gate can remain green. The release protocol digest does not prevent this: contract
generation simply digests the newly weakened source unless a semantic golden/mutation guard rejects
it.

**Required correction/test:** add paired Phase 628 semantic extractors and mutation cases for the
exact trigger/function bindings and critical predicates. At minimum mutate each engine's current
boot-generation check, exact startup predecessor, ready non-regression, heartbeat binding
immutability, and attempt binding direct-delete/rebind rejection and prove every mutation fails.

### P3 — Retry regression proves identical bodies but not fresh authentication

**Location:** `jobs/automation/tests/managed-cloud-runtime-client.test.ts:293-359`; implementation
at `jobs/automation/src/managed-cloud-runtime-client.ts:445-464,658-700`.

The implementation is correct today: every retry re-enters `request()`, stringifies the unchanged
input, and calls `createJobsWorkerAuthHeaders`, producing a fresh nonce/HMAC over the same body.
The regression test only parses and compares request bodies. It does not assert that the second
attempt has the same content digest but a different nonce/signature.

**Failure scenario:** a refactor caches the complete signed request instead of only the payload.
The existing test stays green, but the server rejects the retry as nonce replay, defeating the
response-loss recovery contract.

**Test needed:** capture both attempts' headers and assert byte-identical body and content SHA-256,
distinct valid nonce/signature values, and unchanged boot/request IDs for both registration and
startup.

## Accepted invariants from static review

- Boot ID plus registration/startup request IDs are module-stable for one process; the server,
  not the client, supplies the generation.
- Registration/startup response parsing is exact-key and exact-binding; session tokens are sent
  as proof but omitted from accepted canonical responses.
- Retries are capped at three and limited to transport/timeout/502/503/504 failures; changed
  payload conflicts and other non-authoritative responses are not retried.
- Degraded startup heartbeat precedes startup; a failed/ambiguous startup starts no verifier and
  emits no speculative failure heartbeat; recurring heartbeats begin at N+2.
- Every verifier lease/heartbeat/terminal request carries the parsed paired boot binding through
  one immutable API-client binding.
- Worker-auth admits only the two exact boot suffixes under a bounded runtime-instance ID and
  signs managed-runtime origin, method, path, and body digest.
- Historical release v1/v2 canonical fixture hashes and protocol ordering are explicit; release
  v3 is distinct and binds the expected 065/043 heads, roles, capabilities, and protocols.
- Candidate assembly validates artifact bytes, inventories, SBOMs, provenance, runtime
  measurements, controlled immutable refs, exact publication readback, selected workflow run and
  attempt, promotion generation 2, and rollback generation 2 without rebuilding.
- The inherited Kiro artifacts are dated historical records and accurately disclose their
  then-current partial state, absent tests, later continuations, and superseded design. Their old
  `063/041` future-batch advice must not be treated as current migration guidance; Round 628 and
  the current handoff correctly name 065/043.

## Original file-by-file coverage

Current-file ranges inspected, with all diff hunks against the original base reviewed:

| File | Current range |
|---|---:|
| `.github/workflows/jobs-managed-cloud-release.yml` | 1-919 |
| `CHANGELOG.md` | 1-873; Phase 628 additions 7-30 |
| `jobs/OPERATIONS.md` | 1-1922; changed/current authority 1511-1645, 1883-1922 |
| `jobs/automation/src/managed-cloud-runtime-client.ts` | 1-1346 |
| `jobs/automation/src/worker-auth.ts` | 1-191 |
| `jobs/automation/tests/managed-cloud-runtime-client.test.ts` | 1-619 |
| `jobs/automation/tests/worker-auth.test.ts` | 1-278 |
| `jobs/workflows/Dockerfile` | 1-83 |
| `jobs/workflows/src/original-source-verification-api.ts` | 1-585 |
| `jobs/workflows/src/original-source-verification-runtime.ts` | 1-434 |
| `jobs/workflows/src/original-source-verifier.ts` | 1-258 |
| `jobs/workflows/tests/original-source-verification-runtime.test.ts` | 1-850 |
| `jobs/scripts/managed-cloud-release-gate.mjs` | 1-9524; all changed hunks and release/client integration paths |
| `jobs/scripts/managed-cloud-release-gate.test.mjs` | 1-5936; all changed hunks and v1/v2/v3/publication mutations |
| `jobs/scripts/ci-guards-self-test.mjs` | 1-4203; all changed hunks |
| `jobs/scripts/check-jobs-schema-parity.mjs` | 1-3827; all changed hunks plus parity dispatch |
| `docs/rounds/ROUND-628-JOBS-SOURCE-VERIFIER-RELEASE-V3.md` | 1-120 |
| `docs/work/IMPL-PHASE-628-JOBS-SOURCE-VERIFIER-RELEASE-V3.md` | 1-189 |
| `docs/work/FIX-789-jobs-source-verifier-startup-readiness.md` | 1-84 |
| `docs/work/HANDOFF-PHASE-628-LOCAL-20261026.md` | 1-176 |
| `docs/work/IMPL-KIRO-PHASE-628-BOOT-GENERATION-20260903.md` | 1-627 |
| `docs/work/DESIGN-KIRO-PHASE-628-STARTUP-IDEMPOTENCY-20260903.md` | 1-187 |
| `docs/work/AUDIT-KIRO-JOBS-FOUR-AREAS-20260903.md` | 1-140 |
| `docs/work/REVIEW-KIRO-R2-JOBS-FOUR-AREAS-20260903.md` | 1-184 |

Rust/SQL implementation was intentionally left to the separate reviewer; this pass inspected only
the client/release integration points necessary to validate the wire and guard contracts.

## Original verification evidence and unverified gates

- Independently observed in this pass: expected HEAD and merge base, clean tracked tree with only
  the preserved untracked canonical REVIEW, full owned diff inventory, and
  `git diff --check fc7e5351..4ef94973` PASS (no output, exit 0).
- Not rerun by reviewer per assignment: every Cargo, Node, TypeScript, Vitest, release-gate,
  schema-parity, CI-guard, Docker, or native command. The implementation ledger's reported counts
  remain prior evidence, not independently reproduced here. Parent reviewer will rerun selected
  tests separately.
- Mandatory external holds remain: configured non-production PostgreSQL runtime/concurrency;
  exact Linux Docker image/native-addon smoke; hosted exact-tip CI at this tip; protected registry
  publication and immutable readback; signing; deployment; rollout/rollback/kill-switch canaries;
  and any production flag change.
- No hosted workflow, registry action, deployment, release approval, private product research,
  or production/customer claim is authorized by this report.

## Original follow-up acceptance criteria

1. Correct the contradictory active v3 operations block and add a drift guard.
2. Add Phase 628 trigger/function semantic mutation coverage.
3. Add fresh-HMAC retry assertions.
4. Rerun the focused client/release/schema/documentation gates, then obtain independent review.
5. Keep PostgreSQL, Docker/native, hosted CI/publication, and production action as explicit holds.

## Follow-up re-review — 2026-10-27

### Current verdict

- Client/workflow/release source in this review scope: 🟢 **ACCEPT**. P1, P2, and P3 are resolved
  in the current working diff, and the requested focused gates pass independently.
- Operations/documentation source: 🟢 **ACCEPT**. The active release-v3 procedure now agrees with
  the boot-generation implementation and has a section-scoped drift guard.
- Current client/release source blocker count: **zero**. The original findings remain above as
  historical review evidence rather than descriptions of the corrected files.
- External/runtime release verdict: **NOT VERIFIED / HOLD**. Source acceptance does not authorize
  publication or deployment and does not satisfy the external gates listed below.

### Resolution of original findings

#### P1 — Resolved

`jobs/OPERATIONS.md` now identifies SQLite 065/PostgreSQL 043 as the current boot authority,
names both boot registration and startup endpoints, and forbids speculative
`degraded/probe_failed` recovery. It also distinguishes two cases that must not be conflated:

- a new startup publication requires the current boot's exact `degraded/startup` predecessor;
- authenticated replay of an exact historical committed response remains read-only after a
  replacement boot registers and does not authorize stale execution.

The CI self-test extracts only the active release-v3 section and independently mutates its heads,
startup endpoint, and affirmative `probe_failed` recovery wording. Historical 062/040 mentions
and the required prohibition are allowed rather than treated as false positives.

#### P2 — Resolved after one failed continuation and correction

The schema guard now inspects named, normalized SQLite 063/065 triggers and PostgreSQL 041/043
functions/triggers. Its requirements cover current boot binding, ready-equals-current binding,
ready-head non-regression, exact degraded/startup proof, attempt runtime/epoch/worker/boot
identity, update/direct-delete rejection versus parent cascade, heartbeat-binding immutability,
and preserved v1 non-boot readiness. The self-test adds 33 SQL mutations: 17 SQLite and 16
PostgreSQL, including inert rejection-body mutations so a present but non-enforcing object fails.

Continuation evidence preserves an intermediate stop-line failure. A first extractor
clarification expected PostgreSQL functions to end with `$$;`; actual Phase 628 functions use
`$$ LANGUAGE plpgsql;`, while the historical readiness function uses `$fn$;`. Independent schema
parity therefore failed with five missing function objects plus six missing/count readiness
invariants, and the CI self-test failed with the same eleven issues. No acceptance claim was made.

The corrected extractor matches `RETURNS trigger`, optional leading `LANGUAGE plpgsql`, `AS`, and
the actual opening dollar delimiter; it backreferences that same delimiter and permits trailing
`LANGUAGE plpgsql` before the semicolon. Inspection confirms the match ends at that function's
closing delimiter/language semicolon rather than including neighboring DDL or predicates.

#### P3 — Resolved

The retry regression now stores the actual raw request body for all four deliveries and compares
the registration pair and startup pair byte-for-byte. It requires four distinct 32-hex nonces
and 64-hex signatures, hashes each raw body, checks the transmitted content digest, and recomputes
the exact v2 HMAC over timestamp, nonce, worker, audience, scope, origin, method, path, and digest.
The existing body assertions continue to bind the stable session, boot, generation, and request
identity across response-loss retries.

### Independent follow-up evidence

- `node jobs/scripts/check-jobs-schema-parity.mjs`: **PASS**, 113 tables and 91 indexes.
- `node jobs/scripts/ci-guards-self-test.mjs`: **PASS**.
- `cd jobs/automation && npm test -- managed-cloud-runtime-client.test.ts`: **PASS**, one file and
  11/11 tests.
- The earlier correction pass also independently ran the managed-cloud release-gate suite:
  **PASS**, 32/32.
- No Cargo command was run in this client/release follow-up. Parent-owned full Rust gates remain
  separate evidence and are not claimed here.

### Remaining external holds

The following remain mandatory release holds despite the accepted reviewed source: configured
non-production PostgreSQL runtime/concurrency, exact Linux Docker image/native-addon smoke, hosted
exact-tip CI, protected registry publication and immutable readback, signing, deployment,
rollout/rollback/kill-switch canaries, and any production flag change. No external account,
registry, workflow, deployment, or production action was performed or authorized by this review.

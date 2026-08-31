# IMPL: Phase 613 — Jobs Canonical Taxonomy Authority

> **Codex preflight:** Loaded `$bluey-ops` and reconciled the current repository, local Phase 613
> handoff, and Round 613 before documenting this batch. The SSD archive was not used.

## Scope

Phase 613 replaces heuristic role/skill/location authority with one frozen taxonomy and binds every
executable Career Track to a reviewed, replay-safe policy ledger across SQLite and PostgreSQL.

**Does:**

- freeze and validate one exact role, skill, country, subdivision, metro, and city registry;
- separate target-role resolution from posting-title classification and use token/symbol-safe
  skill matching plus typed, conflict-aware geography and workplace decisions;
- authenticate the exact registry between portal and server before any Career Track write;
- add a ten-table dual-dialect authority for taxonomy activation, account inputs, Track inputs,
  immutable policy revisions, review receipts, head transitions, and current heads;
- bind activation, semantic generations, identity, source resume, preferences, canonical policy,
  receipt, and current head into portal readiness, prepared packets, queueing, Auto-submit, and
  final execution;
- make true no-ops replay-safe while making taxonomy/account/Track `A -> B -> A` transitions
  monotonic and non-revivable;
- serialize PostgreSQL policy writes, Auto-submit authorize/revoke, and effect authorization with
  transaction-scoped account/Track fences;
- verify complete owner-export transition chains and exact revision/receipt/head bijection;
- stop public ATS acquisition from manufacturing typed categories or permanently discarding
  candidates through raw positive role/location/workplace filters;
- reconcile portal policy drift conservatively after Track, profile, preferences, resume, or
  identity mutation and reject stale overlapping read-back;
- make Track IDs retry-safe, plan limits transaction-atomic, tenant collisions explicit, and late
  curated-source enrollment non-destructive to an already committed policy write;
- persist exact unapproved review reasons, validate exact resume replay against the semantic
  ledger, and keep candidate evidence stable across transport-only Track updates;
- repair PostgreSQL migration 033 so the runtime's mandatory replay path is idempotent;
- order resume publication before policy-child locks, preserve arbitrary Career Fact value keys,
  and give PostgreSQL migration 034 the same safe-integer CHECK bounds as SQLite;
- make public-ATS cursor v2 bind and resumably validate the complete ordered provider prefix,
  overlap/total evidence, query/window state, and post-filter exact dedupe/cross-list history;
- put Auto-submit authorize/revoke and Track deletion in the portal mutation epoch, and preserve
  referential integrity in the 125-row public preview; and
- regenerate the packaged Jobs portal from the reviewed source without changing a production
  capability flag.

**Does NOT:**

- enable discovery, source verification, generation, workflow, runner, communication, or
  submission flags;
- deploy, push, merge, retarget, publish, sign, activate, or roll back a release;
- grant Gmail, LinkedIn, MCP, C2C, ATS, registry, or other external-account access;
- contact a job source, employer, recruiter, provider, hosted database, or production service;
- submit an application, send a message, publish a post, or manufacture a provider receipt;
- make an unknown custom role or ambiguous location executable;
- claim a Docker/Linux runner-image, hosted migration, or production result from static/local
  evidence; or
- access the SSD archive.

## Authority Delivered

The frozen registry currently records version
`bluey-jobs-taxonomy-v1-2026-08-25`, SHA-256
`facdb3593457b6585ea83c9f735c42616e7cf03caa6e0369be9154f542dd7254`, 49 roles,
44 skills, 4 countries, 64 subdivisions, 38 metros, and 44 cities. Server and portal consume the
same checked-in bytes; malformed keys, references, aliases, or digest bindings fail closed.

SQLite migration 056 and PostgreSQL migration 034 carry the same ten logical tables:

1. taxonomy activation events;
2. taxonomy activation head;
3. account-input transitions;
4. account-input head;
5. Track-input transitions;
6. Track-input heads;
7. policy revisions;
8. review receipts;
9. policy-head transitions; and
10. current policy heads.

The current schema-parity checkpoint reports 81 tables and 74 indexes in each dialect. A later
local, disposable PostgreSQL 17.10 plus pgvector 0.8.3 run executed the normal two-pass migration
path and all 13 selected authority tests successfully. That local result does not replace
hosted-catalog, interruption, or rollback evidence.

## Files Created / Modified

| File | Action | Purpose |
|------|--------|---------|
| `jobs/taxonomy/canonical-v1.json` | Created | Frozen role, skill, and typed-geography registry |
| `server/src/jobs_taxonomy.rs` | Created | Embedded digest verification and canonical classification |
| `server/src/db/jobs/taxonomy_policy.rs` | Created | Activation, semantic generations, reviewed policy ledger, validation, projection, and owner export |
| `infra/sqlite/server-runtime/056_jobs_canonical_taxonomy_authority.sql` | Created | Ten-table SQLite authority and immutable/CAS triggers |
| `infra/postgres/server-runtime/034_jobs_canonical_taxonomy_authority.sql` | Created | Ten-table PostgreSQL authority, functions, parity triggers, and SQLite-equivalent safe-integer CHECK bounds |
| `infra/postgres/server-runtime/033_jobs_managed_cloud_release_authority.sql` | Updated | Make all managed-cloud additions safe under mandatory migration replay |
| `server/src/db/{mod.rs,jobs.rs}` | Updated | Register migrations, activate taxonomy, and expose the expanded authority projection |
| `server/src/db/jobs/{profile_postings,resume_assets,auto_submit,execution_authority,evidence}.rs` | Updated | Advance and validate semantic authority, order publication/deletion locks, persist review state, fence effects, and stabilize evidence |
| `server/src/db/jobs/{candidate_policy,eligibility,operational_holds}.rs` | Updated | Use canonical role/category/geography evidence and fail-closed queue/hold decisions |
| `server/src/db/jobs/{applications,customer_data,local_runner,managed_cloud_release_authority,runner_volume_purge,workflow_commands,workspace}.rs` | Updated | Freeze, carry, export, and verify exact Track policy authority at downstream boundaries |
| `server/src/api/jobs.rs`, `server/src/lib.rs` | Updated | Enforce registry-bound retry-safe Track writes and expose authenticated taxonomy/workspace state |
| `server/src/db/jobs/{tests,postgres_local_authority_tests}.rs`, `server/tests/{integration_e2e,jobs_runner_plan_matrix}.rs` | Updated | Cover taxonomy, replay, drift, locking, Track limits, late enrollment, execution authority, and current source-resume, identity, approved-Track, and Browser-release fixture prerequisites |
| `jobs/automation/src/public-ats.ts` | Updated | Preserve typed evidence and add bounded cursor-v2 ordered-prefix continuation |
| `jobs/automation/tests/public-ats.test.ts` | Updated | Cover provider contradictions, filtering, source/history bounds, prefix/overlap drift, and continuation |
| `jobs/portal/src/lib/{canonical-taxonomy,track-policy-authority}.ts` | Created | Validate the exact registry and complete approved Track projection |
| `jobs/portal/src/lib/{search-policy,preview-application,command-center}.ts` | Updated | Use bounded presentation semantics and fail-closed readiness |
| `jobs/portal/src/{App,api,types}.ts*`, `jobs/portal/src/components/**`, `jobs/portal/src/views/**` | Updated | Reconcile mutations/read-back, preserve retry IDs, and surface exact policy state |
| `jobs/portal/src/data/{preview,preview.test}.ts` | Updated/created | Preserve public-preview application references and deterministic 125-match expansion |
| `jobs/scripts/{check-jobs-schema-parity,ci-guards-self-test}.mjs` | Updated | Enforce the ten-table dialect contract, functions, triggers, and drift detection |
| `jobs/scripts/{managed-cloud-release-gate,managed-cloud-release-gate.test}.mjs`, `jobs/workflows/tests/discovery-runtime.test.ts` | Updated | Include the canonical-taxonomy migration in release/runtime inventories |
| `web/jobs/index.html`, `web/jobs/assets/**` | Regenerated | Package the current reviewed portal source without source maps |
| `docs/rounds/ROUND-613-JOBS-CANONICAL-TAXONOMY-AND-TRACK-POLICY-AUTHORITY.md` | Created | Define scope, authority model, enforcement matrix, and acceptance criteria |
| `docs/work/FIX-694-*.md` through `docs/work/FIX-711-*.md` | Created | Record each diagnosed defect, root cause, correction, evidence, and limitation |
| `CHANGELOG.md` | Updated | Record Phase 613 under `Unreleased` |

## Defect Records

| Fix | Closed boundary |
| --- | --- |
| FIX-694 | Monotonic taxonomy/account/Track replay and execution TOCTOU ledger |
| FIX-695 | Fail-closed canonical role, skill, geography, workplace, and category classification |
| FIX-696 | Typed public-ATS normalization and removal of unsafe positive worker prefilters |
| FIX-697 | Portal mutation/read-back ordering and complete authority readiness |
| FIX-698 | Retry-safe Track identity, atomic limit, tenant collision, and late enrollment outcome |
| FIX-699 | PostgreSQL Auto-submit revoke/execution serialization |
| FIX-700 | Owner-export revision/receipt/head bijection |
| FIX-701 | PostgreSQL Track-head trigger parity |
| FIX-702 | Durable unapproved Track reason codes |
| FIX-703 | Exact resume-replay semantic-ledger validation |
| FIX-704 | Transport-stable Track evidence hashing |
| FIX-705 | PostgreSQL migration 033 replay safety |
| FIX-706 | Resume publication/deletion parent-before-child lock order |
| FIX-707 | Semantic preservation of arbitrary timestamp-looking Career Fact value keys |
| FIX-708 | PostgreSQL migration 034 safe-integer CHECK parity and real rejection tests |
| FIX-709 | Portal mutation-epoch fencing for Auto-submit authority and Track deletion |
| FIX-710 | Bounded public-ATS cursor-v2 prefix/overlap/history continuation |
| FIX-711 | Public many-match preview referential integrity |

## Build & Test

Observed checkpoints for implementation commit `03c595de` (the committed source bytes are the
same frozen diff exercised by these gates):

```text
Rust fmt                                           PASS: frozen source diff
Rust all-target check                              PASS: frozen source diff
Rust all-target Clippy with -D warnings           PASS: frozen source diff
Rust all-target tests                              PASS: 1,517 / 1,517
  Server library                                   PASS: 1,401
  Server binary                                    PASS: 0
  Main                                             PASS: 1
  Connect-info                                     PASS: 1
  Context migration                                PASS: 1
  GDPR                                             PASS: 2
  Integration E2E                                  PASS: 108 (477.62s)
  Jobs runner plan matrix                          PASS: 2 (6.20s)
  Usage-reservation schema                         PASS: 1
Jobs Vitest aggregate                              PASS: 1,847; 1 skipped
Vitest files                                       PASS: 144; 1 skipped
  Automation                                       PASS: 680; 1 skipped (37 files; 1 skipped file)
  Public ATS focused                               PASS: 39 / 39
  Browser                                          PASS: 219 across 34 files
  Runner                                           PASS: 308 across 33 files
  Workflows                                        PASS: 291 across 12 files
  Portal                                           PASS: 349 across 28 files
  Portal App regression group                      PASS: 20 / 20
  Portal preview regression group                  PASS: 3 / 3
  Portal App + preview                             PASS: 23 / 23
  FIX-697 focused six-file group                   PASS: 70 / 70
Jobs workspace typecheck                           PASS: all 5 workspaces
Jobs production builds                             PASS: all 5 workspaces
Portal Vite build                                  PASS: 2,299 modules; >500 kB advisory only
Schema parity                                      PASS: 81 tables / 74 indexes per dialect
CI guard self-tests                                PASS
Privacy gate, whole-diff snapshot                  PASS: 2,622 tracked paths / 2,347 text paths
Provenance gate                                    PASS: 663 lock entries / 631 unique package versions /
                                                    1 audited override / 14 pinned repositories
Native storage tests                               PASS: 14 / 14
Browser release tests                              PASS: 10 / 10
Managed-cloud release tests                        PASS: 16 / 16
Account-deletion browser guard                     PASS: 3 / 3
Local PostgreSQL authority suite                   PASS: 13 / 13; normal migration runner twice
  Canonical policy ledger regression               PASS: 1 / 1
  Auto-submit revoke/execution fence                PASS: 1 / 1
  Runtime                                           PostgreSQL 17.10 + pgvector 0.8.3
```

The sole Vitest skip was
`automation/tests/playwright-submit-guard.integration.test.ts` (the exact Greenhouse multipart
POST/delayed-beacon guard). Its configured Playwright Chromium executable was unavailable, and no
fallback browser was used. The clean all-target Rust run covers the later migration replay, resume
publication lock order, semantic Career Fact keys, PostgreSQL CHECK parity, policy export,
unapproved reason persistence, and transport-stable evidence fixes.

The clean Rust gate used `CARGO_INCREMENTAL=0`, `CARGO_PROFILE_DEV_DEBUG=0`, and
`CARGO_PROFILE_TEST_DEBUG=0`. An earlier all-target attempt ended when the local disk filled; it is
recorded as an environmental interruption, not as a source test failure or passing result.

The first verification pass also exposed two legacy test setups that requested reviewed execution
authority without the now-required exact source resume. The integration execution/export fixture
and the runner plan-matrix fixture now install a tenant-bound source-resume asset before approval;
the final clean all-target run above includes both corrections. This is fixture compatibility
maintenance, not a relaxation of production source-resume authority.

Local current-source visual QA showed 125 Matches split 63/62 between the two Tracks; all four
applications resolved company and location after FIX-711; the Overview route was correct; and
desktop plus 390x844 rendered without horizontal overflow or console errors. Read-only QA of the
deployed preview from `/jobs/overview?preview=1` still exposed a stale recursive
`/matches/matches` route and pre-fix unknown-company presentation. The deployed bundle therefore
does not match the reviewed local source.

Required evidence still pending:

```text
Exact-tip Jobs CI                                  PENDING until the local Phase 613 commits are pushed
Exact managed-runner Docker/Linux image and native smoke
                                                    PENDING on a resource-capable machine/CI
Hosted migration, network, runner, canary, rollback, and flag evidence
                                                    PARKED external gates
Deployed preview bundle matching current source     BLOCKED by observed stale bundle
```

The local PostgreSQL result used a configured disposable database rather than a self-skipped test.
Static SQL parity and that local run are not hosted-catalog evidence. The local Jobs builds and
bundle checks are green, but exact-tip CI cannot exist until the local Phase 613 commits are pushed.
The Docker CLI was unavailable, so the exact Linux managed-runner image and native smoke were not
run. Release flags remain configured at `0`; no production flag read-back is claimed.

## Deviations from Plan

| Deviation | Rationale |
|-----------|-----------|
| Expanded the original three-table policy design to ten tables | Content hashes alone could not distinguish true no-ops from later `A -> B -> A` replay or prove complete history |
| Corrected predecessor migration 033 inside Phase 613 | The normal PostgreSQL runtime replays all post-Jobs migrations, so the existing non-idempotent migration blocked any reliable Phase 613 PostgreSQL recheck |
| Updated legacy integration and runner-plan fixtures | Phase 613 correctly requires an exact source resume before reviewed execution authority; old test setup had to satisfy that production invariant |
| Kept review conditional rather than green | Exact-tip CI and Docker/Linux evidence are unavailable, hosted launch gates remain parked, and deployed read-only preview QA exposed a stale pre-fix bundle |

## Known Follow-ups

- Preserve the observed disposable-PostgreSQL artifacts and repeat them only if the frozen source
  diff changes; retain hosted migration/network/failure evidence as a separate gate.
- Push the local Phase 613 commits only with authorization, then run exact-tip Jobs CI.
- Build and smoke the exact managed-runner Docker/Linux image on a resource-capable machine or CI.
- Replace and read back the deployed preview bundle, then repeat Overview/Matches/application visual
  QA against the exact promoted bytes.
- Keep registry publication/read-back, signing, protected approvals, hosted Temporal/network,
  runner capacity, ATS canaries, cohorts, kill switches, and rollback rehearsal parked.
- Implement Round 614 original-source verification and later source-control-plane work only as
  separate reviewed phases.

## Review Checklist (for reviewer)

- [x] Files and fix records match the documented Phase 613 source scope.
- [x] No external provider, account, employer, production, or deployment authority is claimed.
- [x] The registry and ten-table dual-dialect contract are explicit.
- [x] Queue, Auto-submit, and execution remain fail closed on incomplete authority.
- [x] Current aggregate, automation, portal, focused, Rust, PostgreSQL, build, and guard counts are
      recorded exactly with the one conditional skip visible.
- [x] Real PostgreSQL replay, trigger, export, and concurrency verification passed without skip.
- [x] Local build/bundle, schema, privacy, provenance, and release guards passed.
- [x] `docs/reviews/` remains untouched.
- [x] Clean fmt, all-target check/Clippy, and 1,517-test Rust verification passed with the
      disk-full environmental interruption kept separate.
- [ ] Exact-tip Jobs CI, Docker/Linux managed-runner, and current deployed-preview read-back gates
      pass.

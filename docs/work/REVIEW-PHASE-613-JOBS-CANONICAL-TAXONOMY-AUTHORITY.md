# REVIEW: Phase 613 — Jobs Canonical Taxonomy Authority

> **Codex preflight:** Loaded `$bluey-ops` and reconciled Phase 613 implementation commit
> `03c595de` against Round 613, the predecessor Phase 612B branch, and the complete FIX-694 through
> FIX-711 record. The SSD archive was not used.

**Source commit range:** `02fa0dd6..03c595de`
**Reviewer:** Codex source-review agents
**Date:** 2026-08-26

## Per-Task Review

### 613.1 — Frozen Registry And Fail-Closed Classification

| Field | Value |
|-------|-------|
| Files | `jobs/taxonomy/canonical-v1.json`, `server/src/jobs_taxonomy.rs`, canonical candidate-policy/eligibility/hold code, portal taxonomy/presentation code, FIX-695 |
| Source verdict | 🟢 accept |
| Frozen-diff verification | 🟢 local Rust and bundle green; exact-tip CI remains a release gate |

**Findings:**

- Server and portal consume one exact checked-in registry and validate its schema, references, and
  digest before using it as authority.
- Target-role resolution, posting classification, skill matching, typed geography, workplace,
  employment, and engagement have distinct fail-closed contracts.
- Positive role, location, category, and workplace proof is required before queue authority;
  ambiguity remains reviewable but not executable.
- Focused canonical taxonomy evidence passed 14/14 tests. Candidate-policy and focused
  eligibility checkpoints also passed, and the clean all-target Rust command passed 1,517 tests
  with zero failures or ignored tests.

---

### 613.2 — Ten-Table Replay-Safe Policy And Execution Authority

| Field | Value |
|-------|-------|
| Files | SQLite 056, PostgreSQL 034, `taxonomy_policy.rs`, policy-input writers, Auto-submit/execution code, owner export, FIX-694, FIX-699 through FIX-703, FIX-706 through FIX-708 |
| Source verdict | 🟢 accept |
| PostgreSQL verdict | 🟢 local disposable-database authority suite passed |

**Findings:**

- Global taxonomy activation, account semantics, and Track semantics use independent monotonic
  histories; a later return to old bytes cannot revive old execution authority.
- Revisions, review receipts, head transitions, and current heads bind the exact activation,
  canonicalizer, semantic generations, identity, source resume, and normalized preferences.
- Equal-hash reuse validates complete immutable evidence. Owner export reconstructs every chain
  and now requires exact revision/receipt/head-transition bijection plus terminal heads.
- PostgreSQL policy writes and execution share the account fence. Auto-submit authorization adds a
  Track-specific shared/exclusive fence so revocation cannot cross final validation.
- Unapproved reasons and exact resume replay now preserve/validate their semantic-ledger state.
- Resume reservation/publication takes object/account lifecycle fences before policy-child locks,
  matching deletion's parent-before-child order. Arbitrary timestamp-looking keys inside
  `CareerFact.value` remain semantic rather than being stripped as transport fields.
- PostgreSQL functions and triggers mirror SQLite head insert, one-generation CAS update, and
  immutable-delete enforcement. Source parity reports 81 tables and 74 indexes in each dialect;
  migration 034 also mirrors SQLite safe-integer CHECK bounds. The local PostgreSQL 17.10 plus
  pgvector 0.8.3 authority run passed 13/13 tests after the normal two-pass migration path.
- The canonical ledger and Auto-submit revoke/execution fence regressions each passed 1/1 against
  PostgreSQL with pgvector 0.8.3. Hosted catalog, interruption, and rollback proof remains an
  external gate rather than an inference from this local run.

---

### 613.3 — Public ATS Acquisition, Portal Reconciliation, And Retry-Safe Track Writes

| Field | Value |
|-------|-------|
| Files | automation public-ATS source/tests, portal API/state/readiness/settings source/tests, Jobs API/Track persistence/integration tests, FIX-696 through FIX-698, FIX-709, and FIX-710 |
| Source verdict | 🟢 accept |
| External/live verdict | 🟡 provider canaries and hosted PostgreSQL failure modes remain pending |

**Findings:**

- Worker normalization accepts only typed provider category/workplace evidence and preserves
  contradictions as unknown. Raw positive role/location/workplace filters cannot permanently
  discard candidates before server classification.
- Cursor v2 rejects more than 24 sources before fetch; binds ordered source/query/window state;
  resumably validates the full consumed prefix within logical `maxPages` operations; checks exact
  trailing overlap and advertised totals; and preserves bounded post-filter dedupe/cross-list
  history. It remains a fail-closed continuation contract, not a provider snapshot token.
- The portal immediately downgrades affected authority, serializes sensitive Settings writes,
  rejects stale epochs/read-backs, and requires complete current policy evidence before showing
  readiness or active Auto-submit.
- Auto-submit authorize/revoke and Track deletion now use that same epoch and authoritative
  read-back path.
- New Tracks carry stable client UUIDs across retries; active-plan limits are checked inside the
  write transaction, cross-tenant ID collisions fail explicitly, and relational activation wins
  on read-back.
- A committed Track write is not reported failed solely because later curated-source enrollment
  failed; workspace load safely retries enrollment, while onboarding completion remains last.
- Current Jobs Vitest evidence passed 1,847 assertions plus one conditional skip across 144 passing
  files and one skipped file. Automation contributed 680 passes plus that skip across 37 passing
  files and one skipped file; its public-ATS focus passed 39/39. Portal contributed 349/349 across
  28 files, including App 20/20, preview 3/3, combined App+preview 23/23, and the FIX-697 six-file
  focus at 70/70.
- The skipped Playwright submit-guard case had no configured Chromium executable, and no fallback
  browser was used. It remains visible rather than being counted as a pass.
- The legacy integration and runner-plan fixtures now install the exact tenant-bound source resume
  required before requesting reviewed execution authority; final integration and matrix targets
  passed 108/108 and 2/2 respectively.

---

### 613.4 — Evidence Stability And Migration Replay Corrections

| Field | Value |
|-------|-------|
| Files | `evidence.rs`, execution regression tests, PostgreSQL migration 033, embedded migration regression, public preview source/tests, FIX-704, FIX-705, and FIX-711 |
| Source verdict | 🟢 accept |
| Frozen-diff verification | 🟢 full Rust and PostgreSQL double-run green |

**Findings:**

- Candidate evidence no longer hashes Track timestamps or derived match counts, so a semantic
  no-op persistence retry cannot falsely revoke an otherwise current prepared application.
  Semantic Track fields and the complete reviewed authority remain bound.
- Migration 033 now guards every `ADD COLUMN` and every named constraint that the normal runtime
  replays. Table-scoped catalog checks avoid mistaking a same-named constraint elsewhere for the
  required object.
- Public `many-matches` preview generation preserves the four original application parent jobs,
  reaches exactly 125 deterministic matches, and validates every related reference.
- These corrections are covered by the clean 1,517-test all-target Rust pass and 3/3 preview focus.
  The disposable PostgreSQL suite also passed 13/13 tests after the normal migration runner
  completed both passes,
  so migration replay is no longer inferred only from parsed SQL.

---

### 613.5 — Documentation, Generated Portal, And Release Boundary

| Field | Value |
|-------|-------|
| Files | Round 613, FIX-694 through FIX-711, IMPL/REVIEW, CHANGELOG, `web/jobs/**` |
| Source verdict | 🟢 accept |
| Release verdict | 🟡 exact-tip CI/Docker and current deployed-bundle evidence required |

**Findings:**

- The documentation consistently describes the final ten-table design rather than the abandoned
  three-table draft and distinguishes observed focused evidence from pending gates.
- No production flag, deploy, OAuth grant, provider contact, employer submission, message,
  release signature, or hosted-database claim is included.
- The current portal Vite build processed 2,299 modules and emitted only its existing
  greater-than-500-kB advisory; local bundle/guard evidence is green.
- Local current-source visual QA passed, but the deployed read-only preview still exposed a stale
  recursive `/matches/matches` path and pre-fix unknown-company presentation. The release verdict
  therefore remains conditional yellow.
- `docs/reviews/` remains untouched.

## Cross-Task Findings

- Source review found no known path that can convert unknown or ambiguous taxonomy/category
  evidence into queue, Auto-submit, or final execution authority.
- Late review found and addressed revocation serialization, owner-export completeness,
  PostgreSQL head triggers, unapproved reason durability, exact resume replay, transport-only
  evidence churn, predecessor migration replayability, resume-publication lock order, arbitrary
  Career Fact keys, PostgreSQL CHECK bounds, portal mutation epochs, bounded public-ATS
  continuation, and public-preview referential integrity.
- The migration 033 defect originated in the Phase 611 source but is correctly included here
  because the runtime replays it before Phase 613 PostgreSQL authority can be verified.
- The clean current-worktree fmt, all-target check, all-target Clippy, and 1,517-test Rust command
  are green. They used `CARGO_INCREMENTAL=0`, `CARGO_PROFILE_DEV_DEBUG=0`, and
  `CARGO_PROFILE_TEST_DEBUG=0` after an earlier attempt ended because the local disk filled. The
  interruption is environmental evidence only. Exact-tip CI and exact Docker/Linux managed-runner
  evidence remain open; hosted launch evidence remains parked.
- Two failed preliminary passes were traced to legacy test fixtures that lacked the now-required
  source resume, not to weakened production assertions. After installing exact source-resume
  assets in the integration execution/export and runner-plan setups, the clean final targets
  passed 108/108 and 2/2 and the full all-target command passed.
- Release configuration keeps all production capability flags at `0`; no production flag read-back
  was performed or is claimed.

## Build & Test Verification

Observed checkpoints:

```text
Canonical taxonomy Rust tests                  ✅ 14 / 14
Candidate-policy focused tests                 ✅ 5
Focused category/workplace eligibility tests   ✅ 4
Rust fmt                                       ✅ frozen source diff
Rust all-target check                          ✅ frozen source diff
Rust all-target Clippy, `-D warnings`           ✅ frozen source diff
Rust all-target tests                          ✅ 1,517 / 1,517
  Server library                               ✅ 1,401
  Server binary                                ✅ 0
  Main                                         ✅ 1
  Connect-info                                 ✅ 1
  Context migration                            ✅ 1
  GDPR                                         ✅ 2
  Integration E2E                              ✅ 108 (477.62s)
  Jobs runner plan matrix                      ✅ 2 (6.20s)
  Usage-reservation schema                     ✅ 1
Jobs Vitest aggregate                          ✅ 1,847 passed / 1 skipped
Vitest files                                   ✅ 144 passed / 1 skipped
  Automation                                   ✅ 680 / 1 skipped; 37 files / 1 skipped file
  Public ATS focused                           ✅ 39 / 39
  Browser                                      ✅ 219 across 34 files
  Runner                                       ✅ 308 across 33 files
  Workflows                                    ✅ 291 across 12 files
  Portal                                       ✅ 349 / 349 across 28 files
  Portal App regression group                  ✅ 20 / 20
  Portal preview regression group              ✅ 3 / 3
  Portal App + preview                         ✅ 23 / 23
  FIX-697 focused six-file group               ✅ 70 / 70
Jobs workspace typecheck                       ✅ all 5 workspaces
Jobs production builds                         ✅ all 5 workspaces
Portal Vite build                              ✅ 2,299 modules; >500 kB advisory only
Schema parity                                  ✅ 81 tables / 74 indexes per dialect
CI guard self-tests                            ✅ passed
Privacy, current whole-diff snapshot           ✅ 2,622 tracked paths / 2,347 text paths
Provenance                                     ✅ 663 lock entries / 631 unique package versions /
                                                  1 audited override / 14 pinned repositories
Native storage tests                           ✅ 14 / 14
Browser release tests                          ✅ 10 / 10
Managed-cloud release tests                    ✅ 16 / 16
Account-deletion browser guard                 ✅ 3 / 3
Local PostgreSQL authority suite               ✅ 13 / 13; normal migration runner twice
  Canonical policy ledger regression           ✅ 1 / 1
  Auto-submit revoke/execution fence            ✅ 1 / 1
  Runtime                                      ✅ PostgreSQL 17.10 + pgvector 0.8.3
```

The sole skipped Vitest was
`automation/tests/playwright-submit-guard.integration.test.ts` (the exact Greenhouse multipart
POST/delayed-beacon case). Its configured Playwright Chromium executable was unavailable, and no
fallback browser was used.

Local current-source visual QA showed 125 Matches split 63/62 between the two Tracks, all four
applications resolving company and location after FIX-711, the correct Overview route, and no
horizontal overflow or console errors at desktop or 390x844. In contrast, read-only QA of the
deployed preview from `/jobs/overview?preview=1` still produced a recursive `/matches/matches`
route and pre-fix unknown-company presentation. That stale bundle remains release-blocking.

Required final gates:

```bash
# Exact-tip Jobs CI
# PENDING until the local Phase 613 commits are pushed

# Exact managed-runner Docker/Linux build and native smoke
# PENDING: local Docker CLI unavailable; requires a resource-capable machine or CI

# Hosted migration/network/canary/rollback/deployment and production flag read-back
# PARKED external gates; release configuration remains 0

# Exact deployed preview bundle read-back
# PENDING: current read-only deployment still serves stale pre-fix behavior
```

The local PostgreSQL suite and local portal production build are observed evidence, not
hosted-catalog, network-interruption, rollback, exact-tip CI, or Docker/Linux evidence. The
source-configured zero flags are not a production flag read-back.

## Overall Verdict

🟡 **LOCAL SOURCE GREEN; RELEASE CONDITIONAL** — The current source design and FIX-694 through
FIX-711 passed the recorded local gates. Phase 613 remains conditional until exact-tip CI, the
managed-runner Docker/Linux image/native smoke, and an exact current deployed-preview read-back
pass. Hosted migration, network, canary, rollback, deployment, and production flag evidence remain
separate parked gates.

## Follow-ups for Next Batch

- Preserve the successful disposable-PostgreSQL artifacts and rerun only if the frozen source
  diff changes; keep hosted migration/network/failure evidence separate.
- Push the local Phase 613 commits only with authorization, then run exact-tip Jobs CI.
- Build and smoke the exact managed-runner Docker/Linux image on a resource-capable machine/CI.
- Replace the stale deployed preview bundle with the exact reviewed artifact, read it back, and
  repeat Overview/Matches/application visual QA.
- Leave registry publication/read-back, signing, protected approvals, hosted networking/Temporal,
  runner capacity, ATS canaries, cohorts, kill switches, and rollback rehearsal parked.

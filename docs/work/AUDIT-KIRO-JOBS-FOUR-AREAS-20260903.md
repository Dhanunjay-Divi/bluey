# AUDIT (Kiro): Jobs Four-Area Verification — Corrected

Date: 2026-09-03
Worktree: `cue-jobs-source-verifier-release-v3-20260831`
Branch: `feat/phase-628-jobs-source-verifier-release-v3`
Verified HEAD: `fc7e535143a262689da8e99ce7638aee42f40407` (dirty tree expected)
Method: read-only. Function bodies read; no builds, no tests, no Phase-628 edits.

## Executive verdict

Bluey Jobs is substantially stronger than a first-pass grep suggests. Duplicate-employer
protection, receipt immutability, and fail-closed submission ambiguity are IMPLEMENTED.
The real gaps are (1) no reusable per-claim revision ledger, (2) only one ambiguity
resolver, (3) no division/tenant/vendor/end-client authority, (4) interview artifact is
never persisted. No P0 correctness defect found in the audited paths.

## Retractions from the first audit

- RETRACTED "no canonical-employer dedup": a server-owned company collision guard exists.
  Reason code `company_application_exists` at `server/src/db/jobs/eligibility.rs:1360`;
  contract in `docs/rounds/ROUND-497-JOBS-CANDIDATE-TRUTH-AND-COMPANY-COLLISION-GUARD.md`
  (blocks second packet before resume generation, rechecks atomically at reservation,
  same policy SQLite+PostgreSQL, released pre-submission failures stay retryable).
  Normalization: `normalize_company_key` `eligibility.rs:1676`,
  `normalize_company_scope` `applications.rs:2321` (ignores case/punctuation/leading "The"/
  legal suffixes Inc, LLC, Ltd, Limited, Corp, PLC, GmbH, Pte, Pty).
  `apply_once_per_company` is force-pinned true: `profile_postings.rs:581,595`,
  default `jobs.rs:446`, proven by test `tests.rs:12890-12895`; guard test `tests.rs:12938`.
- RETRACTED "deleted facts may invalidate historical receipts": DISPROVEN by
  `tests.rs:7033-7057` — fact row hard-deleted, `finalize_submission` still Committed,
  proof unchanged; assertion "frozen receipt must commit after mutable fact deletion".
- RETRACTED "interview prep may read live facts": `jobs_interview_prep.rs:74-89` loads only
  application, resume version, application evidence. TS resolves claims from the resume
  snapshot (`interview-prep.ts:186`).
- CORRECTED "profile_postings.rs irrelevant": it is the fact-write home (all four
  `INSERT INTO jobs_facts`, both `DELETE`).
- CORRECTED "no revision binding": generation CAS exists —
  `applications.rs:1329-1342` bails "confirmed candidate facts changed during resume
  generation" on fact-id-set or `confirmed_facts_fingerprint` mismatch; every fact write
  advances `advance_account_input_generation_{sqlite,postgres}` (`taxonomy_policy.rs:1212,1349`).
- CORRECTED my own ledger design: an append-only row cannot later mutate
  `superseded_by_revision`/`revoked_at_ms`. Replaced below.

## Proven facts

| Question | Answer | Evidence |
|---|---|---|
| Confirmed facts mutated in place? | YES. Upsert `ON CONFLICT(id) DO UPDATE SET ... verification_status = excluded.verification_status`. No `UPDATE jobs_facts` exists. | `profile_postings.rs:249-265` (SQLite), `297-312` (PG) |
| Immutable snapshots | resume versions (`claim_ids_json`+`checksum`), receipts (`approved_execution.checksum`, `verifiedClaimIds`), final submit proof | `applications.rs:501-579`; `tests.rs:6995-7057` |
| Deleted facts break receipts? | NO | `tests.rs:7033-7057` |
| Claim IDs sufficient? | NO. Code already pairs IDs with hashes/checksums; a bare ID is reusable after in-place edit. | `jobs_interview_prep.rs:166-183`; `applications.rs:1338-1342` |
| Paths changing confirmation state | `upsert_fact` (any status/source) `profile_postings.rs:229`; `upsert_user_fact` forces `confirmed`, bails "imported career facts require a dedicated confirmation flow" for non-`user_entry` (`:373-377`) plus SQL guard `AND jobs_facts.source = user_entry` (`:405`); `delete_fact` hard delete, no tombstone (`:504-520`). Portal: `saveFact` POST /api/jobs/facts, `deleteFact` DELETE /api/jobs/facts/:id. | as cited |
| Interview artifact persisted? | NO. No `interview*` table in any migration. `id = "prep-"+fingerprint[..24]`, `generated_at_ms = now_ms()`. | `jobs_interview_prep.rs:120-124` |
| Jobs depends on Overlay code? | No import found; coupling is via `router::ANSWER_CONTEXT_SCHEMA_VERSION_V1` | `jobs_interview_prep.rs:111` |
| Division / ATS tenant / staffing vendor / end client authorities? | ABSENT. Repo-wide grep for `hiring_division|staffing_vendor|end_client|division_id|vendor_id` = zero. `ats_tenant_verified` is a status string only. | `ats_certification_authority.rs:15079` |

## Area status

| Capability | State |
|---|---|
| Company collision guard + normalization + forced pref | Implemented |
| Receipt/resume immutability, fact-deletion durability | Implemented |
| `side_effect_unknown` fail-closed, age never clears | Implemented |
| Fact generation counter + resume-generation CAS | Implemented |
| Per-claim immutable revision ledger (supersession, revocation, evidence/metric basis, provenance) | Missing |
| Automated ambiguity resolvers (provider lookup, portal evidence, confirmation receipt, positive-only email, bounded escalation) | Missing (only owner-manual) |
| Division/tenant/vendor/end-client authority; cooldown window + reset authority | Missing (single company scope only) |
| Persisted immutable interview artifact | Missing |

## Corrected data model

1. `jobs_claim_events` — insert-only (SQLite triggers mirroring existing
   `jobs_browser_release_artifacts_no_update` / `_no_delete` pattern).
   `event_id, account_id, claim_id, event_seq, event_type IN (asserted, confirmed, superseded, revoked),
   statement_sha256, evidence_basis_ref, metric_basis, provenance, source_resume_version_id, actor, created_at_ms`;
   `UNIQUE(account_id, claim_id, event_seq)`.
2. `jobs_claim_head` — mutable projection: `(account_id, claim_id) PK, head_event_id, head_seq, status, statement_sha256`;
   CAS on `head_seq` in the same transaction as the event insert. All mutation lives here; history immutable.
3. Account deletion removes rows from BOTH tables. NO account-identifying tombstone
   (no requirement proven; `AccountWriteFence` already fences in-flight writes —
   `submission_reconciliation.rs:87-88,206`).
4. `jobs_submission_evidence(application_id, source_type, verdict IN (submitted, not_submitted, inconclusive),
   evidence_ref, sha256, created_at_ms)` + bounded `escalation_level`.
   Missing email => `inconclusive`, never `not_submitted`. Time may escalate, never clear.
5. `jobs_employer_authority(canonical_company_id, hiring_division_id NULL, ats_tenant_id NULL,
   staffing_vendor_id NULL, disclosed_end_client_id NULL)` — server-owned; models may not invent
   divisions; aliases canonicalized server-side (extends `normalize_company_key`, does not replace it).
   `jobs_application_cooldown(account_id, canonical_company_id, until_ms, reset_authority_id)`;
   reset requires an authority row + evidence (mirror `revoked_at_ms` pattern of
   `040_jobs_auto_submit_authorizations.sql:8-16`).
6. `jobs_interview_context(id = sha256(account_id | application_id | full_receipt_fingerprint |
   resume_document_sha256 | schema_version), ...)` — FULL hash. The 24-char prefix at
   `jobs_interview_prep.rs:121` must never become primary identity.

## Phase 628 collisions

- Schema heads: `server/src/db/mod.rs` registers SQLite `062_` and PostgreSQL `040_`
  `jobs_original_source_verifier_startup_authority.sql` with a parity assert on the `040_` name.
  New work MUST begin at SQLite `063+` / PostgreSQL `041+`.
- Modified files to avoid/rebase: `server/src/db/mod.rs`,
  `jobs/scripts/check-jobs-schema-parity.mjs`, `jobs/scripts/ci-guards-self-test.mjs`,
  `server/src/api/jobs_managed_cloud_releases.rs`, `server/src/api/jobs_worker_auth.rs`,
  `server/src/db/jobs/managed_cloud_release_authority.rs`,
  `server/src/db/jobs/original_source_verification.rs`, `CHANGELOG.md`.
- NOT in the 628 diff (safe): `applications.rs`, `profile_postings.rs`, `eligibility.rs`,
  `submission_reconciliation.rs`, `jobs_interview_prep.rs`, `interview-prep.ts`.

## Acceptance criteria and focused tests

1. Ledger: insert-only triggers reject UPDATE/DELETE on `jobs_claim_events`; head CAS rejects
   stale `head_seq`; consumer rejects a `claim_id` whose `statement_sha256` changed; account
   deletion leaves no identifier; existing `tests.rs:6995` receipt-durability test stays green.
2. Employer scope: division/tenant/vendor/end-client remain distinct authorities; alias cannot
   bypass; existing `company_application_exists` test `tests.rs:12938` stays green; cooldown
   reset without authority+evidence fails closed.
3. Resolver: each verdict maps to exactly one terminal; `inconclusive` never transitions;
   missing email yields `inconclusive`; escalation bounded; runner-receipt vs owner-manual race
   still yields exactly one winner.
4. Interview artifact: two generate calls return byte-identical stored artifact under the full-hash
   key; no truncated identity; sensitive fields still excluded; no Overlay import in Jobs.

## Recommended batches

- Batch A: persist `jobs_interview_context` (migration `063/041`). Lowest risk, self-contained.
- Batch B: employer authority taxonomy + cooldown on top of the existing collision guard.
- Batch C: ambiguity evidence sources, one source per sub-batch.
- Batch D: claim event ledger + head (largest; heaviest `mod.rs`/schema-parity contention with 628).

## Not proven (updated 2026-09-03 by Kiro — both prior items CLOSED)

- CLOSED: no second `apply_once_per_company` enforcement inside `ats_certification_authority.rs`
  (targeted grep returned zero). Guard lives solely at `eligibility.rs:1360`.
- CLOSED: `jobs_facts` is erased on account deletion via FK cascade —
  `REFERENCES accounts(id) ON DELETE CASCADE` (`infra/postgres/server-runtime/002_jobs.sql:19`;
  SQLite `server/src/db/mod.rs:1067`), `PRAGMA foreign_keys = ON` (`mod.rs:262`, asserted
  `mod.rs:2736-2737`), triggered by `DELETE FROM accounts` (`account_data.rs:1594`).
- Still unproven: Phase 628 RED status taken from round docs (no tests run by Kiro);
  Portal/Overlay consumer internals not read beyond `jobs/portal/src/types.ts` and the API client.

See `docs/work/REVIEW-KIRO-R2-JOBS-FOUR-AREAS-20260903.md` for the Kiro round record.

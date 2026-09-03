# REVIEW: KIRO-R2 — Jobs Four-Area Verification (line-by-line)

> **Author: KIRO (supporting principal engineer, external reviewer).**
> Not Codex. Codex retains ownership of all Phase 628 startup/readiness code.
> Kiro preflight completed: loaded `$bluey-ops` from
> `/Users/uno/.codex/skills/bluey-ops/SKILL.md` and read `AGENTS.md`.

**Commit range:** `fc7e535143a262689da8e99ce7638aee42f40407..WORKING-TREE` (uncommitted Phase 628 present)
**Reviewer:** Kiro
**Date:** 2026-09-03
**Mode:** read-only. No edits to source, no builds, no test suites, no staging/commit/push,
no deploys, no flag changes, no external calls. `/Users/uno/Downloads/cue` never touched.
**Companion doc:** `docs/work/AUDIT-KIRO-JOBS-FOUR-AREAS-20260903.md`

## Per-Task Review

### KIRO-R2.1 — Claim lifecycle

| Field | Value |
|-------|-------|
| Files | `server/src/db/jobs/profile_postings.rs`, `applications.rs`, `tests.rs`, `infra/postgres/server-runtime/002_jobs.sql`, `server/src/db/mod.rs` |
| Verdict | 🟡 minor nit — safe today, no reusable ledger |

**Findings:**
- Confirmed facts ARE mutated in place. `upsert_fact` uses
  `ON CONFLICT(id) DO UPDATE SET ... verification_status = excluded.verification_status`
  (`profile_postings.rs:249-265` SQLite, `297-312` PostgreSQL). No `UPDATE jobs_facts`
  statement exists in the repo; mutation is upsert-only.
- `upsert_user_fact` is correctly fenced to user-entered rows: bails
  "imported career facts require a dedicated confirmation flow" (`:373-377`) plus SQL guard
  `AND jobs_facts.source = user_entry` (`:405`).
- `delete_fact` is a hard delete with no tombstone (`:504-520`).
- Mutation is not silent: every write advances
  `advance_account_input_generation_{sqlite,postgres}` (`taxonomy_policy.rs:1212,1349`),
  and resume generation holds a CAS that bails
  "confirmed candidate facts changed during resume generation" (`applications.rs:1329-1342`).
- Historical safety proven: `tests.rs:7033-7057` hard-deletes a fact, then
  `finalize_submission` still returns Committed with an unchanged proof
  ("frozen receipt must commit after mutable fact deletion").
- Claim IDs alone are insufficient; the code already pairs IDs with hashes
  (`jobs_interview_prep.rs:166-183`).

### KIRO-R2.2 — Submission reconciliation

| Field | Value |
|-------|-------|
| Files | `server/src/db/jobs/submission_reconciliation.rs` (read in full, 318 lines) |
| Verdict | 🟢 accept — contract is correct; only one resolver exists |

**Findings:**
- `side_effect_unknown` is fail-closed and row-serialized (`:66-67`), account-deletion fenced
  (`:87-88`, `:206`), state-guarded (`:109`, `:229`), and releases lease/ticket/attempt
  reservation/session/evidence capacity atomically (`:139-189`).
- Age NEVER auto-clears ambiguity: no time-based transition exists anywhere in the file.
  This invariant is currently satisfied and must be preserved.
- Only two authoritative outcomes exist: trusted fenced runner receipt, or owner-verified
  not-submitted. Automated evidence sources (provider lookup, candidate-portal evidence,
  confirmation receipt, positive-only email) are absent.
- Missing email must map to `inconclusive`, never `not_submitted`.

### KIRO-R2.3 — Employer scope and cooldown

| Field | Value |
|-------|-------|
| Files | `server/src/db/jobs/eligibility.rs`, `applications.rs`, `tests.rs`, `docs/rounds/ROUND-497-*` |
| Verdict | 🟢 accept — guard exists (Kiro retracts an earlier claim) |

**Findings:**
- RETRACTION: Kiro previously reported no canonical-employer dedup. That was WRONG.
  A server-owned company collision guard exists: reason code `company_application_exists`
  (`eligibility.rs:1360`), test `tests.rs:12938`, contract in
  `docs/rounds/ROUND-497-JOBS-CANDIDATE-TRUTH-AND-COMPANY-COLLISION-GUARD.md` — blocks the
  second packet before resume generation, rechecks atomically at reservation time, same
  policy for SQLite and PostgreSQL, keeps released pre-submission failures retryable.
- Normalization: `normalize_company_key` (`eligibility.rs:1676`),
  `normalize_company_scope` (`applications.rs:2321`) — ignores case, punctuation, leading
  "The", and legal suffixes (Inc, LLC, Ltd, Limited, Corp, PLC, GmbH, Pte, Pty), explicitly
  so email aliases cannot create a second candidate.
- `apply_once_per_company` cannot be disabled: force-pinned true in
  `profile_postings.rs:581,595`, default `jobs.rs:446`, proven by `tests.rs:12890-12895`.
- Genuine gap: `hiring_division`, `ats_tenant`, `staffing_vendor`, `disclosed_end_client`
  do not exist as authorities. Repo-wide grep returns zero;
  `ats_tenant_verified` is only a status string (`ats_certification_authority.rs:15079`).
  No cooldown window and no cooldown-reset authority exist.

### KIRO-R2.4 — Interview-context artifact

| Field | Value |
|-------|-------|
| Files | `server/src/api/jobs_interview_prep.rs`, `jobs/automation/src/interview-prep.ts`, all migrations |
| Verdict | 🔴 blocker for the stated product goal — nothing is persisted |

**Findings:**
- Nothing is stored. No `interview*` table exists in any SQLite or PostgreSQL migration.
- Every open regenerates: `generated_at_ms = now_ms()` and
  `id = "prep-" + receipt_fingerprint[..24]` (`jobs_interview_prep.rs:120-124`).
  Reopening does NOT retrieve identical bytes.
- Grounding is strong and correct: `receipt_id`, `receipt_fingerprint`, `resume_version_id`,
  `resume_checksum`, `resume_document_sha256` (`:44-49`) with receipt/resume/claim cross-checks
  (`:158-207`).
- Jobs does NOT depend on Overlay code. Coupling is
  `router::ANSWER_CONTEXT_SCHEMA_VERSION_V1` (`:111`) only.
- Generation reads only application, resume version, and application evidence (`:74-89`) —
  it does not read `jobs_facts`, so fact deletion cannot break prep.
- The 24-character truncated fingerprint must never become stored primary identity.

### KIRO-R2.5 — Phase 628 collision review

| Field | Value |
|-------|-------|
| Files | Phase 628 uncommitted diff only (inspected, not modified) |
| Verdict | 🟢 accept — collisions identified, avoidable |

**Findings:**
- Schema heads owned by Phase 628: SQLite `062_` and PostgreSQL `040_`
  `jobs_original_source_verifier_startup_authority.sql`, registered in `server/src/db/mod.rs`
  with a parity assert on the `040_` filename. Future work MUST start at SQLite `063+` /
  PostgreSQL `041+`.
- Contended files: `server/src/db/mod.rs`, `jobs/scripts/check-jobs-schema-parity.mjs`,
  `jobs/scripts/ci-guards-self-test.mjs`, `server/src/api/jobs_managed_cloud_releases.rs`,
  `server/src/api/jobs_worker_auth.rs`,
  `server/src/db/jobs/managed_cloud_release_authority.rs`,
  `server/src/db/jobs/original_source_verification.rs`, `CHANGELOG.md`.
- Not in the Phase 628 diff, therefore safe to modify later: `applications.rs`,
  `profile_postings.rs`, `eligibility.rs`, `submission_reconciliation.rs`,
  `jobs_interview_prep.rs`, `interview-prep.ts`.

## Cross-Task Findings

- Kiro issues four retractions against its own first-pass audit: employer dedup exists;
  fact deletion cannot invalidate receipts; interview prep never reads live facts; revision
  binding does exist via generation counter plus resume-generation CAS.
- Kiro also withdraws its own first proposed ledger design as internally inconsistent: an
  append-only row cannot later mutate `superseded_by_revision` or `revoked_at_ms`.
  Replacement is immutable `jobs_claim_events` plus a CAS-updated `jobs_claim_head`
  projection (detail in the companion AUDIT doc).
- Account deletion: no account-identifying tombstone should be retained for a claim ledger.
  No requirement was proven, and `AccountWriteFence`
  (`submission_reconciliation.rs:87-88,206`) already fences in-flight writes.

## Build & Test Verification

```text
NOT RUN — read-only assignment. No builds, no test suites, no formatters, no linters.
Kiro executed only git inspection and file reads.
```

## Overall Verdict

🟡 **ACCEPT WITH NITS** — Nothing audited is incorrect or unsafe. Existing safety
invariants (receipt immutability, fail-closed ambiguity, company collision guard,
alias-bypass prevention) are implemented and should not be disturbed. The four requested
capabilities are absent surface area, not defects, and are safely additive.

## Follow-ups for Next Batch

- Batch A: persist `jobs_interview_context` under a full-hash idempotency key
  (migration SQLite `063` / PostgreSQL `041`). Lowest risk, self-contained.
- Batch B: employer authority taxonomy (division, ATS tenant, staffing vendor, disclosed end
  client) plus cooldown window and evidence-backed reset authority, layered ON TOP of the
  existing `company_application_exists` guard rather than replacing it.
- Batch C: submission ambiguity evidence sources, one source per sub-batch, preserving
  "age never clears" and "missing email is inconclusive".
- Batch D: `jobs_claim_events` + `jobs_claim_head`. Sequence last; heaviest contention with
  Phase 628 on `mod.rs` and schema-parity guards.

## Kiro Open Facts — both now CLOSED this round

- CLOSED: no second `apply_once_per_company` enforcement site exists inside the 22,201-line
  `server/src/db/jobs/ats_certification_authority.rs`. Targeted grep for
  `apply_once|per_company|same_company|company_scope|duplicate_application|already_applied`
  returned zero matches. The guard lives solely in `eligibility.rs:1360`.
- CLOSED: `jobs_facts` IS erased on account deletion, by foreign-key cascade rather than an
  explicit delete list. `account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE`
  in PostgreSQL (`infra/postgres/server-runtime/002_jobs.sql:19`) and SQLite
  (`server/src/db/mod.rs:1067`), with `PRAGMA foreign_keys = ON` (`mod.rs:262`) and a runtime
  assertion of that pragma (`mod.rs:2736-2737`). The account purge deletes the `accounts` row
  (`server/src/db/account_data.rs:1594`), which cascades the jobs domain.

## Remaining unproven

- Phase 628 RED status is taken from its round documents; Kiro executed no tests.
- Portal and Overlay consumer internals were not read beyond `jobs/portal/src/types.ts`
  and the portal API client.

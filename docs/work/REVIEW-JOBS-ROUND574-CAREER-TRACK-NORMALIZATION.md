# REVIEW: Jobs Round 574 - Career Track Authority And Normalization

**Commit range:** `ec7ff00e..working-tree`
**Reviewer:** Codex self-review
**Date:** 2026-07-25

## Per-Task Review

### Career Track authority

| Field | Value |
|-------|-------|
| Files | Jobs profile, track, resume, evidence, eligibility, execution, API, and portal modules |
| Verdict | ACCEPT |

**Findings:**

- Tracks fail closed without a verified application identity.
- New saves bind to the current source resume.
- Existing nonempty stale source bindings are rejected rather than silently
  rewritten.
- Legacy backfill is limited to missing authority.

### Role and experience normalization

| Field | Value |
|-------|-------|
| Files | `candidate_policy.rs`, `profile_postings.rs`, portal Career Track controls |
| Verdict | ACCEPT |

**Findings:**

- Canonical role aliases resolve to full role families.
- Explicit relevant-employment IDs cannot cross the selected family.
- Overlapping relevant roles are counted once.
- Required experience, preferred experience, and title seniority remain
  separate checks.

### Evidence and execution authority

| Field | Value |
|-------|-------|
| Files | `evidence.rs`, `execution_authority.rs`, migration 016 |
| Verdict | ACCEPT |

**Findings:**

- Candidate evidence revisions and claim rows are append-only in PostgreSQL.
- Packet finalization and execution recheck exact profile, track, identity,
  resume, claims, and discovery authority.
- Receipts retain the exact resume and identity used.

### Portal setup and import

| Field | Value |
|-------|-------|
| Files | onboarding, Settings, parser, suggestions, styles, tests |
| Verdict | ACCEPT |

**Findings:**

- Resume extraction no longer turns common employer/title/location layouts into
  a single malformed field.
- Word private-use bullets, spaced headings, multiword cities, certification
  suffixes, and wrapped certification names have regression coverage.
- Location and role controls remain searchable and accept explicit custom
  values.
- Skills and certifications are compact, editable token lists.
- Internal daily pace and Auto-submit threshold values are not presented as
  arbitrary customer controls.

### Matching aliases and boundaries

| Field | Value |
|-------|-------|
| Files | `candidate_policy.rs`, `eligibility.rs`, focused tests |
| Verdict | ACCEPT |

**Findings:**

- Raw aliases such as SDE, DE, MLE, PM, and CRA enter the same canonical role
  families as their full names.
- Known technology aliases match without requiring duplicate profile chips.
- Token boundaries prevent short skills such as Go from matching inside
  unrelated words.

## Cross-Task Findings

- No client code can substitute identity, resume, evidence, or eligibility
  authority for the server.
- No Jobs runtime distribution flag is changed by this batch.
- The 502.5 kB DOCX parser is a lazy import-only chunk. The main runtime and
  React code are split, and the build budget documents the on-demand parser.

## Build & Test Verification

```bash
cargo fmt --all -- --check                    # passed
cargo clippy --all-targets -- -D warnings     # passed
cargo test --quiet                            # 799 unit, 76 integration passed
npm test                                      # 87 passed
npm run typecheck                             # passed
npm run build                                 # passed, no warnings
root npm test                                 # 476 Jobs tests passed
root npm run typecheck                        # all Jobs workspaces passed
privacy/schema/provenance/client boundary     # passed
server SQLite boundary                        # existing 43-line warning; no new out-of-db access
no production source maps                    # passed
```

Active Bluey onboarding, handoff, operations, release, deployment, Jobs, and
work templates now name the validated `bluey-ops` preflight. The skill points
back to repository code and current runbooks as authority.

## Overall Verdict

ACCEPT - Ready for pull-request review.

## Follow-ups for Next Batch

- Adapter certification and durable runner recovery remain independent release
  gates.
- Production distribution flags remain off until those gates pass.

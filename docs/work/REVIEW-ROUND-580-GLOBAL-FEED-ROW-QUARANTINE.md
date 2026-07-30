# REVIEW: Round 580 - Global Feed Row Quarantine

**Commit range:** `2dedd4c7..working-tree`
**Reviewer:** Codex self-review
**Date:** 2026-07-30

## Per-Task Review

### Round 580 - Bounded Row Quarantine

| Field | Value |
|-------|-------|
| Files | Jobs global-discovery worker, API contract, server persistence, migrations, and tests |
| Verdict | Green - accept |

**Findings:**

- No blocker or correctness finding remains.
- The worker quarantines only `missing_identity`; every structural defect still
  aborts the source.
- Server persistence independently validates counts, reasons, cap, batches,
  rows, and exact replay evidence before changing source health.
- The source-health transition cannot succeed with zero accepted rows.
- No raw rejected row data is persisted in completion evidence.

---

## Cross-Task Findings

- Fresh and upgraded schema paths expose the same evidence columns.
- TypeScript and Rust completion contracts preserve legacy response
  compatibility without weakening new request validation.
- Original-source revalidation remains separate and mandatory.
- Model generation and both Browser distribution flags remain disabled.

## Build & Test Verification

```bash
cargo fmt --manifest-path server/Cargo.toml -- --check
# passed

cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings
# passed

cargo test --manifest-path server/Cargo.toml
# 781 unit, 76 integration, 2 focused matrix/migration tests passed

(cd jobs && npm run typecheck && npm test && npm run build)
# passed; 469 package tests

node jobs/scripts/ci-guards-self-test.mjs
node jobs/scripts/privacy-gate.mjs
node jobs/scripts/check-jobs-schema-parity.mjs
node jobs/scripts/check-provenance-licenses.mjs
git diff --check
# passed
```

## Overall Verdict

Green - **ACCEPT**. Ready to commit, manually deploy, and verify against the
production Ashby snapshot.

## Follow-ups for Next Batch

- Monitor rejection ratios and reasons without logging raw candidate rows.
- Add any future semantic rejection reason only through a reviewed typed
  contract with an explicit cap and fixtures.

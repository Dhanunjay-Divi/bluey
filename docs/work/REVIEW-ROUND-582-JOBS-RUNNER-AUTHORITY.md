# REVIEW: ROUND-582 - Jobs Runner Authority And Allowance Atomicity

> **Codex preflight:** Loaded `$bluey-ops` before review and verified its memory
> against current repository state and the working-tree diff.

**Commit range:** `87280227..working-tree`
**Reviewer:** Codex self-review
**Date:** 2026-08-01

## Per-Task Review

### ROUND-582 - Runner-owned finalization

| Field | Value |
|-------|-------|
| Files | Jobs automation, portal, API, persistence, and focused tests |
| Verdict | ACCEPT |

**Findings:**

- The portal can approve and queue a reviewed packet, but generic customer
  PATCH requests cannot enter runner-owned states.
- Customer evidence writes are removed from both route and client surfaces.
- Finalization checks the bound run, exact resume evidence, confirmation,
  runner authority, browser session, and attempt reservation inside one
  transaction for SQLite and PostgreSQL.
- Browser writes fail closed when an ATS does not retain the requested value.
- Same-period allowance reuse and new-period metering are distinct and tested.

---

## Cross-Task Findings

- No model-generation, Browser-distribution, mailbox, native-overlay, audio, or
  discovery feature flag is changed.
- No source map, generated portal bundle, secret, local database, or runtime
  artifact is included in the diff.
- Production remains review-first until independent execution certification.

## Build & Test Verification

```bash
cargo fmt --manifest-path server/Cargo.toml --all --check
# passed

cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings
# passed

cargo test --manifest-path server/Cargo.toml --all-targets
# passed

npm test --prefix jobs
# 473 passed

npm run typecheck --prefix jobs
npm run build --prefix jobs
# passed

# Jobs privacy, provenance/license, schema-parity, client/server boundary,
# and guard self-tests passed.
```

## Overall Verdict

**ACCEPT** - Ready for an independent reviewer and merge.

## Follow-ups for Next Batch

- Promote only a reviewed merged artifact, retaining disabled production flags
  until their separate launch gates pass.

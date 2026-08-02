# REVIEW: JOBS-APPLICATION-KIT-GENERATION — Grounded Application Kits

> **Codex preflight:** Loaded `$bluey-ops` and verified its memory against the
> current repository state and working diff.

**Commit range:** working tree based on `origin/main`
**Reviewer:** Codex self-review
**Date:** 2026-08-02

## Per-Task Review

### JOBS-APPLICATION-KIT-GENERATION — Grounded Application Kits

| Field | Value |
|-------|-------|
| Files | Generation, truth validation, application transaction, portal review, tests, generated portal bundle |
| Verdict | green accept |

**Findings:**

- Cover-letter paragraphs cannot cite unknown evidence records.
- Protected facts and stronger action claims fail closed before persistence.
- The target company and role are rendered server-side rather than trusted to model output.
- Cover-letter persistence is part of the existing resume/application transaction.
- The portal renders exact stored content and makes absence explicit.
- The production capability flag remains disabled pending provider acceptance.

## Cross-Task Findings

- The current receipt records inclusion state. The later evidence-object slice must
  also hash and store the exact final packet object before employer-facing execution.
- React Router 7.18.2 reports two RSC-only advisories. Bluey Jobs uses neither React
  Server Components nor server actions; downgrading introduced broader client-side
  advisories and was therefore rejected.

## Build & Test Verification

```bash
cargo fmt --all -- --check                         # success
cargo clippy -p bluey-server --all-targets -- -D warnings  # success
cargo test -p bluey-server jobs -- --nocapture     # 263 passed
npm --prefix jobs/portal test                      # 87 passed
npm --prefix jobs/portal run typecheck             # success
npm --prefix jobs/portal run build                 # success
git diff --check                                   # success
```

## Overall Verdict

green **ACCEPT** — Source-ready for controlled model-provider acceptance. Production
enablement remains blocked until the separate rollout gate passes.

## Follow-ups for Next Batch

- Discovery freshness, deduplication, employer verification, and scam-risk authority.
- Durable runner recovery and irreversible-submit reconciliation.
- Immutable packet evidence objects and lifecycle verification.

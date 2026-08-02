# IMPL: JOBS-ROUND587 - Auto-submit Authority Production Deploy

> **Codex preflight:** Loaded `$bluey-ops` before deployment and reconciled its
> memory against current mainline, production, and Round 586 evidence.

## Scope

**Does:**

- Record exact source, artifact, backup, rollback, promotion, and live runtime
  identities for the Round 586 deployment.
- Record public edge, authorization, crawler, static-asset, and browser checks.
- Preserve the disabled Jobs feature boundaries.

**Does NOT:**

- Change product code.
- Enable Jobs model generation, Browser distribution, mailbox sync, or
  universal unattended submission.
- Redeploy the main API, discovery workers, Caddy, or signed native release.

## Files Created / Modified

| File | Action | Purpose |
| --- | --- | --- |
| `CHANGELOG.md` | Modified | Record manual production promotion |
| `docs/rounds/ROUND-587-JOBS-AUTO-SUBMIT-AUTHORITY-MANUAL-PRODUCTION-DEPLOY.md` | Created | Numbered deployment evidence |
| `docs/work/IMPL-JOBS-ROUND587-PRODUCTION-DEPLOY.md` | Created | Batch implementation record |
| `docs/work/REVIEW-JOBS-ROUND587-PRODUCTION-DEPLOY.md` | Created | Scoped self-review |

## Build & Test

```text
Round 586 candidate:
server library tests passed
16 focused Jobs HTTP integration tests passed
489 Jobs package tests passed
Rust fmt and strict Clippy passed
Jobs TypeScript checks and production build passed
privacy, provenance/license, schema parity, boundary, and CI guards passed

Production:
fresh PostgreSQL backup and exact R2 read-back passed
exact Jobs API isolated smoke passed
Jobs API active with zero restarts and exact c4b98bc8 commit
root and Jobs static assets match reviewed source
public, authorization, crawler, source-map, and direct-origin checks passed
desktop and mobile signed-out handoff QA passed without console errors
```

## Deviations From Plan

| Deviation | Rationale |
| --- | --- |
| First isolated smoke hit the fixed PostgreSQL pool ceiling | Repeated with a coordinated Jobs-only stop; no artifact was promoted before the exact candidate passed |
| Signed-in browser workspace was unavailable | The existing browser session was signed out; no credentials were fabricated and only the live signed-out handoff was claimed |

## Known Follow-ups

- Certified runner distribution, model-generated application kits, mailbox
  sync, and universal unattended submission remain separate launch gates.

## Review Checklist

- [x] Files match the deployment-evidence scope.
- [x] No unrelated product changes are included.
- [x] Exact source, binary, database, static, and rollback identities are
  recorded.
- [x] Disabled feature boundaries and untouched native runtime are explicit.
- [x] Browser evidence does not overstate signed-in coverage.

# IMPL: ROUND-581 - Global Feed Quarantine Production Deploy

## Scope

**Does:**

- Record the exact Round 580 source, artifacts, backup, canary, promotion, and
  production reconciliation evidence.
- Record typed quarantine behavior for one malformed Ashby row.
- Preserve exact rollback and product-boundary instructions.

**Does NOT:**

- Change product code.
- Redeploy the main API, Caddy, portal, or native runtime.
- Enable Jobs model generation or local/cloud Browser distribution.
- Claim R2 backup replication succeeded.

## Files Created / Modified

| File | Action | Purpose |
| --- | --- | --- |
| `CHANGELOG.md` | Modified | Record production activation |
| `docs/rounds/ROUND-581-JOBS-GLOBAL-FEED-QUARANTINE-PRODUCTION-DEPLOY.md` | Created | Deployment evidence |
| `docs/work/IMPL-ROUND-581-GLOBAL-FEED-QUARANTINE-PRODUCTION-DEPLOY.md` | Created | Batch implementation record |
| `docs/work/REVIEW-ROUND-581-GLOBAL-FEED-QUARANTINE-PRODUCTION-DEPLOY.md` | Created | Scoped self-review |

## Build & Test

```text
Round 580 candidate:
781 server unit tests passed
76 server HTTP integration tests passed
469 Jobs tests passed
Rust fmt and strict Clippy passed
Jobs TypeScript checks and portal build passed
Privacy, provenance, license, schema-parity, and CI guards passed

Production:
Jobs API canary passed
Jobs API and two discovery workers active with zero restarts
Lever completed 67,506 accepted / 0 rejected
Ashby completed 46,377 accepted / 1 typed rejected
Public, authorization, crawler, and direct-origin checks passed
```

## Deviations from Plan

| Deviation | Rationale |
| --- | --- |
| R2 replication remains unavailable | `AccessDenied`; local rollback evidence is valid |
| Prior worker required forced termination | Durable state allowed safe recovery |

The R2 result is an explicit infrastructure waiver, not a passing backup
replication check. The previous global worker exceeded its 30-second graceful
stop timeout; durable lease and idempotent-ingestion state prevented duplicate
publication or lost accepted rows.

## Known Follow-ups

- Repair and independently verify production R2 backup/log-archive credentials.
- Investigate why the old global worker did not stop inside its 30-second
  service timeout.

## Review Checklist (for reviewer)

- [x] Files match the deployment-evidence scope.
- [x] No unrelated product changes are included.
- [x] Exact source, binary, worker, database, and rollback identities are
  recorded.
- [x] Product flags and deployment boundaries are explicit.
- [x] Infrastructure waivers are not presented as passing checks.

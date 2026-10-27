# FIX-792: Phase 628 Boot-Authority Mutation Guards

> **Codex preflight:** Loaded `$bluey-ops` before diagnosis and implementation and
> verified its release guidance against this checkout.

## Issue

The Jobs schema-parity gate compared Phase 628 boot tables by column shape and checked one
heartbeat-binding foreign key, but did not inspect the SQLite 063/065 or PostgreSQL 041/043
objects that enforce boot-generation, readiness, startup, attempt-binding, and historical-v1
authority.

## Root Cause

`jobs/scripts/check-jobs-schema-parity.mjs` placed the Phase 628 tables in
`SEMANTIC_PARITY_TABLES` without adding object-scoped semantic requirements for their triggers
and functions. A migration could therefore weaken a critical predicate while preserving its
tables and still pass the parity gate. The CI guard self-test mutated only a Phase 628 column,
foreign key, and migration-runner registration, so it did not prove rejection of semantic drift.

## Fix Summary

- Added normalized, object-scoped SQLite and PostgreSQL requirements for:
  - exact current and ready boot-generation equality;
  - degraded `startup` heartbeat authority, excluding `probe_failed` as startup proof;
  - attempt/runtime/epoch/worker/boot identity binding;
  - immutable updates, parent-aware direct-delete rejection, and parent cascade cleanup;
  - PostgreSQL trigger-to-function operation bindings; and
  - the preserved historical v1 readiness fence for non-boot manifests.
- Added 33 independent SQL mutation cases: 17 SQLite and 16 PostgreSQL. Both dialects mutate
  current generation, ready generation, ready-head non-regression and its rejection body,
  degraded state, startup reason, runtime identity, epoch, worker, boot ID, boot generation,
  update rejection, parent-aware delete, parent cascade, heartbeat-binding immutability rejection,
  and v1 non-boot readiness. SQLite separately mutates its update and delete rejection triggers.
- Added three mutations scoped only to the active release-v3 operations section. They reject
  obsolete 062/040 heads, the legacy startup endpoint, and affirmative speculative
  `probe_failed` recovery while allowing historical mentions and the current prohibition.
- Kept the guard intentionally static. It rejects source drift but does not claim to execute SQL,
  prove transaction ordering, or replace live PostgreSQL concurrency verification.

## Files Modified

| File | Change |
|------|--------|
| `jobs/scripts/check-jobs-schema-parity.mjs` | Guarded SQLite 063/065 and PostgreSQL 041/043 semantics. |
| `jobs/scripts/ci-guards-self-test.mjs` | Added 33 SQL and three active-v3 documentation mutations. |
| `docs/work/FIX-792-phase628-boot-authority-mutation-guards.md` | Recorded cause, fix, evidence, and limitations. |

## Edge Cases Handled

- PostgreSQL defines the historical heartbeat function more than once across migrations. The
  guard evaluates the final definition and separately requires both non-boot readiness branches.
- SQLite replaces the historical readiness trigger. The guard evaluates the final trigger rather
  than failing on the expected prior definition.
- A direct attempt-binding delete remains rejected while its parent exists, while the table keeps
  `ON DELETE CASCADE` so parent deletion can remove the child binding.
- Replacing an immutability trigger/function's rejection body with an inert statement fails even
  when its trigger name and operation header remain present.
- Startup authority requires `health_state='degraded'` and `reason_code='startup'`; a
  `probe_failed` heartbeat is not interchangeable startup evidence.
- The documentation guard extracts the exact active release-v3 section, so historical 062/040
  references and the required `do not send ... probe_failed` warning are not false positives.

## How to Test

```bash
node jobs/scripts/check-jobs-schema-parity.mjs
node jobs/scripts/ci-guards-self-test.mjs
node --test jobs/scripts/managed-cloud-release-gate.test.mjs
git diff --check
```

Observed locally on 2026-10-27:

- Schema parity: PASS, 113 tables and 91 indexes.
- CI guard self-test: PASS.
- New mutation coverage: PASS, 33 SQL cases plus 3 active-v3 documentation cases.
- Managed-cloud release-gate tests: PASS, 32/32.

## Known Limitations

- These are normalized static source guards, not database execution tests. Configured
  non-production PostgreSQL behavior, concurrency, transaction locking, and rollback remain
  required external evidence.
- The dated independent review preserves its original findings and adds a follow-up source
  acceptance after parent inspection and independent reruns. External release holds remain.

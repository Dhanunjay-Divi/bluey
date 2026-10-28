# Round 636 — Jobs canonical employer reservations

Date: 2026-10-27 (America/New_York). Base: `cd72989d`. Preflight: `bluey-ops`,
current local handoff, security/SPARC workflow and project-scoped native team.
Status: causal private-PG reproduction repaired; PRE-FORMAT source gates passed
(1872 all-target Rust,38 actualPG,30 focused,2071 Jobs/zero skips,five typechecks/
builds,lint/format/cleanup). Independent AI review conditionally accepts bounded
local correctness; P2 font/P3 unmeasured bundle warnings and all external/runtime/
release gates remain held. FIX812's EOF-only migration formatting is normalized-
byte equivalent, but exact final-source gates remain required after warning fixes.
No production or overall-goal acceptance.

## Scope and invariants

The current unique index binds account plus normalized display company. Signed
source/integrity already supplies canonical employer ID/domain, but distinct
display aliases can receive separate attempt reservations. Preserve the current
name guard and add canonical employer identity as an independent authority.
No corporation/alias identity may be inferred from a mutable posting or email.

1. Reproduce with two concurrently Verified postings sharing signed employer
   ID/domain, distinct display keys, Tracks and verified application identities.
   Use distinct Greenhouse boards/jobs and the existing shared runtime/ATS
   fixture helpers. Assert both authorities are current immediately before the
   second attempt so an unrelated stale-authority rejection cannot look green.
2. Add nullable ID/domain/attestation bindings through additive SQLite066 and
   PostgreSQL044 migrations, preserving historical schema bytes/display index.
   New active attempts need a complete binding; canonical account+employer
   uniqueness must survive races, Track/email changes and trust-policy rotation.
   Conflicting ID/domain mappings require review, not a second application.
3. Legacy unresolved rows stay intact and block new admission/pre-click until
   verified. Do not guess/backfill from names or untrusted receipt JSON, delete
   a loser, release submitted history or rewrite immutable receipts. Permit
   recording terminal/unknown outcomes and authorized safe release, including
   for unresolved legacy rows; do not strand reconciliation behind a new guard.
4. Bind inside existing SQLite/PG reservation and cloud-staging transactions,
   retaining lock order and post-lock time sampling. Exact active replay and
   own-reservation execution/pre-click must match current signed/frozen identity;
   excluding self from duplicate detection alone is insufficient.
5. Extend pure eligibility, relevant reservation lookup and mutable workspace
   inputs. Preserve established v3 release/schema/metering hash contracts; no
   blind version/head change or old worker/release authority relaxation.
6. Prove alias denial/no loser effects, distinct employers, existing display
   guard, policy rotation, SQLite/real-PG concurrency, legacy migration/replay
   and binding drift. Run focused/full server and Jobs gates under owned roots,
   cleanup/self-test/privacy and separate AI review before accepting the phase.

## Ownership and execution

Parent owns core bindings/row readers/eligibility/reservation/staging/execution,
test reproduction, FIX/IMPL and all heavy execution. Schema/parity work may be
delegated with exact file ownership only after the red test terminates. Reviewer
owns REVIEW636 only. No worker edits while a frozen qualification is active.
Use shared mac-build lock and canonical launcher for heavy work, private PG
wrapper and fresh databases. Preserve inherited untracked REVIEW628.

No VM/preview, owner account, real employer/provider request, application/email/
calendar, push/PR/merge, deployment, publication/signing, paid resource, production
flag or desktop/overlay/Pinky change. No SSD access. Passing the phase does not
finish the broader goal or authorize production.

# Round 549 — Jobs truth/spend atomic rollout

Status: implementation and verification in progress; not deployed.

## Deployment boundary

The usage-origin cutover and durable provider-attempt holds are one coordinated
server release. They are not safe as a Jobs-only migration beside an older main
API binary, and an older paid-dispatch binary is not a safe rollback target while
provider routes remain enabled.

`BLUEY_UPSTREAM_SPEND_LIMIT_CENTS` is a mandatory safety boundary for every
managed paid dispatch, not a temporary live-test option. Unset, invalid, or `0`
produces no configured guard and must fail closed before provider I/O; it never
means unlimited spend. `BLUEY_UPSTREAM_SPEND_WINDOW_HOURS` defaults to 24 only
after a positive limit exists. Provider-dashboard caps remain an independent
second boundary.

After migration 008, an insert from an older binary that omits `origin` receives
`legacy_unverified`. That is the privacy-safe default, but authoritative spend
queries intentionally exclude it. Older binaries also dispatch without the new
pre-dispatch provider hold. Consequently, a mixed-version process can spend at a
provider without contributing authoritative exposure to the new global cap.
`old_binary_insert_is_legacy_and_proves_paid_rollout_must_be_atomic` preserves
this boundary as an executable regression test.

There is no database-only generation gate that can reliably stop a process that
has already loaded provider credentials: the legacy binary does not check such a
gate before dispatch. Database triggers can reject or relabel its later write,
but cannot undo the network side effect.

## Conservative cutover baseline

Relabelling existing usage rows as `legacy_unverified` would otherwise make the
new authoritative rolling-cap query appear to start at zero. Before that
relabel, migration 008 snapshots every positive pre-cutover Bluey cost into
`usage_cutover_spend_baseline`, clamping each row at the authoritative event
ceiling. The snapshot contains only occurrence time and cost: no account,
request, provider, model, or other user identity. A migration marker makes the
snapshot replay-safe, and the cutover table lock closes the insert race between
snapshot and relabel. A completed replay does not retake that table lock.

Both ordinary usage reservations and durable provider-attempt holds include the
active baseline in the same global admission cap. The baseline is deliberately
conservative, survives account deletion, is never used for customer billing or
product analytics, stops contributing after the rolling window, and is deleted
after the window plus a 24-hour cleanup grace.

## Required rollout

1. Stop every old binary, then fail closed every paid dispatcher (main LLM streaming/non-streaming, answer
   planning, web search, embeddings, upload transcription, live STT, and managed
   Jobs resume generation), or remove its provider credentials, and verify no
   provider dispatch remains in flight.
2. Apply the schema migration and deploy the matching main API and Jobs binaries
   in the same maintenance boundary.
3. Verify server-origin attempt rows, opaque provider holds, global-cap denial,
   account-deletion retention, and customer-root settlement before re-enabling
   paid routes.
4. Re-enable paid dispatch only after all server instances report the new build.

Rollback follows the same rule in reverse: disable paid routes first, drain
in-flight work, then roll back both binaries. Never start an older paid binary
against the cutover schema merely because it passes a boot check.

## Verification gates

- All-target Rust build, clippy, and full tests must pass.
- SQLite tests must prove the anonymous baseline blocks both global admission
  paths, clamps hostile values, survives account deletion, expires and cleans
  up after window plus grace, and is not duplicated on migration replay.
- SQL-shape tests must preserve the marker-guarded PostgreSQL snapshot and its
  conditional cutover lock; exact PostgreSQL runtime behavior is a separate
  frozen-commit drill.
- PostgreSQL migration/replay and account-deletion spend-retention drills must
  pass against the exact frozen commit.
- Windows and macOS packaging checks remain separate release gates; this server
  branch alone authorizes no client or production deployment.

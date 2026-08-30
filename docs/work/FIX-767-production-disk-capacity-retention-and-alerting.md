# FIX-767: Production disk capacity retention and alerting

> **Codex preflight:** Load `$bluey-ops` before diagnosis or implementation and
> verify its memory against the current repository state.

## Issue

The first live release preflight stopped at 86% root-disk use on a 58 GB Bluey
host. Bluey's production policy blocks promotion at 80%, but the installed
guard had recorded over-threshold results without delivering an operator alert
or preventing the same storage classes from continuing to grow.

## Root Cause

This was a policy-and-enforcement mismatch, not a single corrupt file:

- `/var/backups/bluey-api` held about 29 GB. The backup script hard-coded 14
  hourly plus 14 daily full dumps and used count-only retention. At roughly
  0.93 GB per current dump, the intended count itself consumed about half of
  the root disk. Git history identifies the migration error: commit `5223403f`
  introduced 14+14 on 2026-05-20 for SQLite, then `65cbe01d` added PostgreSQL
  on 2026-06-30 without revisiting count retention. A retained SQLite snapshot
  is 1,060,864 bytes; the latest PG dump was 931,682,290 bytes, about 878 times
  larger. The policy had no local-byte ceiling or free-space reserve.
- The backup script rotated before proving an exact offsite copy. Archive and
  checksum files were pruned as independent lists, and overlapping cron/manual
  executions were not serialized.
- `/var/tmp/bluey-log-archive` retained 178 prefixed work directories from
  interrupted July archive attempts, consuming roughly 0.7–0.8 GB. The script
  removed its current work directory only on success and did not bound stale
  work.
- The disk guard ran only once daily. It evaluated thresholds before cleanup,
  never recomputed the release decision, and wrote failures only to a local
  cron log. Backup cron failures were likewise visible only in `backup.log`;
  the guard inspected journald but did not verify that a current, complete,
  offsite-proven snapshot existed. No durable status consumer or alert receiver
  was required.
- Guard records show hard-threshold failures every day from August 24 through
  August 30 without a notification reaching an operator.
- Stale build/source trees under `/opt` and a 1.5 GB journal were visible
  contributors. They were not classified in the guard's component report, so
  the trend was hard to attribute before the hard threshold.
- Logical dump growth is amplified by global-candidate data. The live audit
  found about 1.39 GB in `jobs_global_candidates`, mostly TOAST, while the
  default-off archive lifecycle had never run. A stale operational query also
  used a nonexistent timestamp-style membership-expiry predicate rather than
  the current `membership.availability_status` authority.

The deeper cause was therefore using file counts as a capacity plan on a small
root volume, leaving temporary/build growth unbounded, and detecting only at a
hard release line without actionable notification.

## Fix Summary

- Default production backup policy is now 4 hourly plus 7 daily snapshots,
  minimum 2 of each, a 12 GB DB-hot-storage ceiling, and a 16 GB pre-backup
  reserve. The byte ceiling includes only hourly, daily, and staging data;
  unrelated rollback/log artifacts cannot force deletion of DB restore points.
- Every backup uses a nonblocking host-wide flock and same-filesystem staging.
  The checksum is finalized before the archive rename, and retention deletes
  the archive, sidecar, and proof marker as one pair.
- S3 uploads require remote HEAD size, checksum-sidecar agreement, and a full
  streamed read-back hash before an atomic `.offsite-verified` marker appears.
  Capacity pruning always requires that marker. Count pruning also requires it
  whenever offsite storage is configured, retaining and failing on a proof gap.
- SQLite snapshots must pass `PRAGMA quick_check`; PostgreSQL custom archives
  must pass `pg_restore --list`. Proof markers bind the current configured
  destination. Non-candidate retention scans trust cheap root-owned metadata,
  while an actual deletion rehashes the candidate and repeats remote full
  read-back immediately before removal.
- A transient upload failure is self-healing: the next locked run structurally
  validates and reconciles new-format unverified pairs before allocating a new
  dump. It proves already-present immutable remote components and uploads only
  a missing payload or sidecar, so a kill between uploads does not require an
  overwrite. Persistent failure stops without compounding the backlog. Atomic
  run status makes failed or SIGKILL-interrupted jobs visible to the guard.
- `--verify-existing` provides a bounded, non-uploading bootstrap for legacy
  local files. It derives historical daily-to-hourly object names from the
  checksum sidecar, repeats the exact remote proof, then atomically mints only
  matching markers before the capacity policy is activated.
- Log archiving now holds its own flock, deletes the current bundle on every
  ordinary exit, and lock-cleans orphan staging after SIGKILL/reboot. It creates
  and structurally validates a new archive, proves remote HEAD/sidecar/full
  read-back, and persists atomic run status. V1 deliberately does not delete
  hot/rotated source logs without an exact capture mapping. Daily seven-day
  logrotate is independently bounded, forced rotation was removed from the
  guard, and archive failure/staleness alerts give operators time to pause it.
  Local archive deletion is proof-aware and repeats the remote read-back. Root
  cron output is isolated under `/var/log/bluey-ops`, included in the archive,
  and bounded by its own root-owned seven-day/20 MiB rotation policy.
- Backup writes are hard-bounded at 2 GiB and preflight reserves that allocation
  plus 16 GiB; a midnight run reserves two allocations for the simultaneous
  hourly file and staged daily copy. The lock proves every pre-existing exact
  `run.*` directory is orphaned, so it is removed before capacity decisions.
  Log bundles are capped at 128 MiB and reserve room for two bounded staging
  copies. Both move orphan staging under root-owned
  `/var/lib/bluey-ops`; optional log metadata indexing has connection,
  statement, and wall-clock timeouts.
- The guard reports backup, log-work, release, API, log, and build-candidate
  bytes; recomputes after safe pruning; persists atomic status; and emits HTTPS
  warning/failure/recovery events. It runs checks every 15 minutes and bounded
  cleanup hourly.
- A distinct warning state starts at 70% used or below 16 GB free, before the
  unchanged hard release block at 80% or below 8 GB.
- Production backup health is checked every 15 minutes: the newest active
  backend snapshot warns at exactly 120 minutes and fails at 180, while absent
  snapshots, sidecars, or required proof fail immediately. The cheap check
  compares the root-owned marker byte count to `stat` and marker hash to the
  checksum sidecar; it does not rehash the full dump. A production-required
  switch permits an explicit opt-out only for non-production hosts.
- The guard holds a shared backup flock throughout its complete metadata scan,
  evaluates the prior complete snapshot during a legitimate upload/read-back window, and warns/fails writers at
  45/90 minutes. It also checks atomic run status, unverified backlog, DB-hot
  bytes, daily 36/48-hour freshness, inode thresholds, and reason-fingerprinted
  alerts. A separate guard flock serializes status/dedup updates.
- The guard also consumes the log archive's durable run status, warns/fails
  archive freshness at 120/180 minutes and writers at 45/90 minutes, and includes
  archive reason changes in immediate alert fingerprints.
- Destructive roots are validated as normalized absolute non-root paths with
  no `.`/`..` segments, broad top-level aliases, or symlink roots. The installer
  uses root-owned operational cron logs, keeps storage/alert policy mode 0600,
  and installs persistent journald byte/free-space limits.
- All three scripts load `/etc/bluey-api/bluey-storage.env`, so manual and cron
  invocations share one root-owned policy fragment. Root-run scripts reject a
  symlinked, non-regular, non-root-owned, or group/world-writable env fragment
  before sourcing it.
- Backup and operational-log keys plus the alert webhook moved out of the
  service-readable API env into that root-only fragment. The API service lost
  backup-root write access; backup/state/work roots and cron logs are root-owned.
  Installer `--prepare` performs the stopped-service boundary migration without
  enabling schedules, while `--activate` installs rotation and cron last.
- The installer also prepares root-owned mode-0700 restore lease and lock
  directories. The restore script accepts only a pre-existing, exact
  provider-issued marker and consumes that local lease after target-absence
  proof. Phase 622 intentionally provides no provider client: production
  preflight hard-fails while the required independent creation, host-loss
  monitoring, expiry cleanup, and closure evidence remain unimplemented.
- New offsite objects use distinct `hourly/` and `daily/` prefixes for bucket
  lock/lifecycle control; legacy flat objects remain read-only bootstrap inputs.
- The runbook now uses `membership.availability_status <> 'expired'` in the
  global archive preflight and keeps that worker disabled until exact one-row
  object/tombstone evidence passes.

## Files Modified

| File | Change |
|------|--------|
| `ops/backup-bluey-db.sh` | Locked atomic pairs, exact offsite proof, bounded retention, legacy proof bootstrap |
| `ops/archive-bluey-logs.sh` | Single-writer boundary, EXIT cleanup, bounded stale-work pruning |
| `ops/bluey-disk-guard.sh` | Early warning, post-prune decision, component report, durable status and alerts |
| `ops/install-bluey-log-guards.sh` | 15-minute checks, hourly prune, durable-status and root restore-control directories |
| `ops/bluey-api.service.example` | Remove API write access to the root-owned backup tree |
| `ops/bluey-storage.env.example` | Shared 58 GB production capacity policy |
| `ops/bluey-api.env.example` | Common storage-fragment loading guidance |
| `ops/tests/test-bluey-storage-guards.sh` | Deterministic backup/archive/guard failure-path regression |
| `ops/restore-drill-bluey-db.sh` | Alias-safe disposable-target identity and credential-safe Postgres restore |
| `ops/tests/test-restore-drill-bluey-db.sh` | Restore identity/checksum/structure regressions |
| `scripts/bluey-cloud-preflight.sh` | Production-required durable guard plus explicit unimplemented restore-provider stop line |
| `ops/tests/test-bluey-cloud-preflight-disk-guard.sh` | Required-guard and restore-provider release-preflight regressions |
| `.github/workflows/ci.yml` | Ubuntu storage-guard regression gate |
| `docs/ops/DEPLOY-DISK-STORAGE-CHECK-RUNBOOK.md` | Bootstrap, alerts, capacity, and archive eligibility operations |
| `docs/PRODUCTION-DEPLOY-RUNBOOK.md` | Capacity-aware backup installation and recovery contract |
| `jobs/OPERATIONS.md` | Correct global-candidate archive eligibility, retry, and lease-expiry preflight queries |
| `infra/README.md` | Root-only backup credential and restore-provider release-gate boundary |
| `CHANGELOG.md` | Unreleased reliability summary |
| `docs/work/FIX-767-production-disk-capacity-retention-and-alerting.md` | Root cause, implementation, verification, provider evidence, and remaining gates |

## Edge Cases Handled

- Invalid or contradictory retention, capacity, warning, and hard-threshold
  values fail closed.
- A failed full remote read-back leaves the complete local archive/checksum
  pair and cannot mint pruning authority.
- A killed backup between immutable remote payload/sidecar uploads resumes by
  uploading only the absent component and never overwrites an existing object.
- A later successful run cannot count-prune an earlier unverified snapshot.
- Capacity pressure cannot cross the configured hourly or daily local minimum,
  even when every remaining object is offsite verified.
- A historical daily file can prove its exact hourly R2 object from the copied
  checksum-sidecar basename without uploading, renaming, or deleting either.
- An overlapping manual/cron backup or log archive exits before mutation.
- An overlapping guard sees the same backup lock, skips only writer-owned
  incomplete snapshots, and still fails an older unverified backlog.
- An interrupted archive cannot leave its active work directory behind, and a
  pruner cannot cross the exact work-root/prefix boundary.
- Missing, failed, warning-age, hard-age, slow-writer, stuck-writer, and recovered
  log archive status are visible within the 15-minute guard cadence.
- A successful prune is judged from a fresh disk snapshot, while warning,
  failure, repeated failure, and recovery alert states remain durable.
- Backup health distinguishes disabled, healthy, warning, hard-stale,
  no-backup, missing/malformed checksum, and missing/untrusted/inconsistent
  offsite proof without reading the full archive payload.
- Empty, root-alias, `..`, broad top-level, and symlink destructive targets fail
  during config validation before any directory creation, prune, or chown.
- Root cron output cannot be redirected through service-user-owned paths, and
  secret webhook/database URLs do not appear in command arguments.
- Root operations scripts cannot execute settings from a symlinked or writable
  environment fragment.
- Root-trusted defaults avoid sticky `/tmp`/`/var/tmp` ancestors, malicious lock
  symlinks fail before open, and test fixtures cannot source live host env files.
- Midnight writer-capacity checks account for both the finalized hourly snapshot
  and staged daily copy before either file is created.
- A locally created restore marker cannot make production preflight green. The
  provider gate remains a source-level stop line until a separate reviewed
  integration can act after `SIGKILL` or total host loss.

## How to Test

```bash
bash -n \
  ops/backup-bluey-db.sh \
  ops/archive-bluey-logs.sh \
  ops/bluey-disk-guard.sh \
  ops/install-bluey-log-guards.sh \
  ops/restore-drill-bluey-db.sh \
  scripts/bluey-cloud-preflight.sh \
  ops/tests/test-bluey-storage-guards.sh
bash ops/tests/test-bluey-storage-guards.sh
bash ops/tests/test-restore-drill-bluey-db.sh
bash ops/tests/test-bluey-cloud-preflight-disk-guard.sh
bash scripts/check-bluey-ops-docs.sh
git diff --check
```

The focused regression covers successful exact S3 proof, corrupt read-back,
outage persistence/recovery, proof-aware count retention, candidate-only
rehash/read-back, capacity pruning,
DB-hot-cap isolation from unrelated rollback evidence, legacy hourly/daily
marker bootstrap, both flocks, archive EXIT cleanup and atomic finalization,
prefix-bounded stale-work pruning, post-prune recomputation, component sizes,
durable warning/failure/recovery status, alert delivery, and cron cadence.
It also covers the exact 120/180-minute backup boundaries, missing active
backup, missing checksum, missing and mismatched offsite proof, recovery, and
the explicit non-production health-check opt-out; backup/guard overlap and
writer-age boundaries; destructive path rejection; stale staging cleanup;
archive-before-prune proof; destination rebinding; and durable run status.
It also covers archive freshness/failure/writer boundaries, forced-rotation
removal, writer byte limits, trusted production paths, and malicious lock/path
rejection. The release-preflight test proves production-required guard failure
blocks promotion while an explicit non-production opt-out remains available,
and proves the production restore-provider requirement stays fail-closed in
this phase.

## Local source verification

Executed on 2026-08-30 against the current uncommitted Phase 622 working tree
based on `3d41a455`:

- Bash syntax passed for every changed backup, archive, guard, installer,
  restore, preflight, and storage-test script.
- `ops/tests/test-bluey-storage-guards.sh` passed its complete deterministic
  storage suite.
- `ops/tests/test-restore-drill-bluey-db.sh` passed its mock and configured-real
  PostgreSQL paths where available; the real two-orchestrator collision remains
  represented by the deterministic fixture, not claimed as hosted evidence.
- `ops/tests/test-bluey-cloud-preflight-disk-guard.sh` passed the full release
  preflight suite.
- Documentation, scoped diff, temporary-artifact, and `git diff --check` gates
  passed.
- An independent final source review returned GO with no remaining P0–P3
  finding, including the post-open installer lock identity/ownership/mode
  revalidation and leaf/ancestor swap tests.

This is source evidence only. Provider policy, external restore dead-man, real
root-host activation/rollback, alert delivery, and production rollout remain
NO-GO as described below.

## Restore Evidence

On 2026-08-30, the full read-only stream of
`bluey-postgres-20260830T140001Z.pgdump` restored into a temporary local
PostgreSQL 18 + pgvector cluster. The drill observed 28 accounts, 170,128
global candidates, 77 public tables, and zero invalid indexes. The temporary
cluster was completely removed and no production write occurred.
This proves the archived bytes and isolated restore path, not an external
provider registration, provider-side TTL/drop, or release authority.

## Production R2 retention evidence and proposed controls

Read-only production inspection on 2026-08-30 proved that the Bluey backup
bucket had no object-expiration lifecycle rule and no bucket-lock rule. Its
only lifecycle entry was Cloudflare's default seven-day abort for incomplete
multipart uploads. The host's least-privilege backup credential could list and
fully read back its own backup prefix, but correctly received `AccessDenied`
for bucket lifecycle, retention, and lock administration. No cloud setting was
changed during this inspection.

The reviewed source writes new database objects beneath the distinct
`bluey-api-backups/hourly/` and `bluey-api-backups/daily/` prefixes. The
proposed provider policy is deliberately narrower than the bucket:

| Rule | Prefix | Provider lock | Lifecycle expiry |
|------|--------|---------------|------------------|
| `bluey-backups-hourly-lock-7d` / `bluey-backups-hourly-expire-14d` | `bluey-api-backups/hourly/` | 7 days | 14 days |
| `bluey-backups-daily-lock-35d` / `bluey-backups-daily-expire-90d` | `bluey-api-backups/daily/` | 35 days | 90 days |

Cloudflare documents that bucket locks cover both existing and new matching
objects, that the strictest overlapping lock wins, and that a lock delays a
shorter lifecycle deletion. Consequently the lock is intentionally shorter
than the corresponding expiry, preserving an operator recovery interval while
still bounding storage. At the observed roughly 0.93 GiB PostgreSQL dump size,
14 days of hourly objects plus 90 days of daily objects is approximately 396
GiB of steady payload storage; checksum sidecars are negligible. This estimate
must be remeasured from a current provider inventory before activation.

The 1,472 objects visible through the legacy flat backup prefix are presumed
payload/checksum components, not a deletion authority. No legacy lifecycle or
lock rule may be added until an independent management credential has recorded
the exact keys, sizes, creation times, pair completeness, read-back samples,
and an isolated restore, and has proved that no active recovery marker depends
on an earlier object. Legacy expiration is the last rollout step, never the
bootstrap step.

Provider activation remains blocked until the exact proposed rules are staged
and read back through an independent management credential, canary objects are
fully read back, the host credential is proved unable to delete objects or
administer the bucket, and an operator confirms the future automatic deletion
policy. The production dashboard forms were inspected and closed without
saving.

## Known Limitations

- All database snapshot/validation, operational archive, provider read-back,
  and cloud-preflight calls now have fail-closed wall-clock limits. The log
  archive reconciles immutable components individually and will not overwrite
  one that already exists; when provider retention is required it also refuses
  to mint local proof unless the provider returns an object-lock mode and
  retention deadline. These controls do not prove the production bucket's
  lifecycle or retention policy until captured against the real provider.
- Every wall limit escalates from TERM to KILL after a separately validated
  short grace; the hostile-child regression proves a TERM-ignoring process is
  killed and releases its advisory lock. Provider credentials are exported
  only inside contained launch subshells, and database URLs use command
  environment assignment; neither is embedded in `env` or timeout argv.
  Provider children receive a narrow environment.
- Storage configuration is opened from a trusted, non-writable ancestor chain,
  pinned to a file descriptor, and identity-checked before sourcing. Installer
  activation is process-fenced and restores prior cron/logrotate files if a
  staged activation fails. Local tests prove script behavior only; an actual
  root-host activation/rollback canary remains required.
- The installer validates the process-fence lock path, existing node, and
  ancestor ownership/modes before opening it, then revalidates the ancestor
  chain and compares the regular root-owned protected path's device/inode with
  the opened descriptor. Deterministic leaf swap, ancestor swap, unsafe-mode,
  identity-mismatch, symlink, and non-root-chain fixtures are rejected; a
  stable descriptor/path fixture passes. The race hook is non-mutating and is
  explicitly unavailable when the installer runs as root.

- This source change does not install, configure, or execute against production.
  Before deployment, the operator must configure and canary a real HTTPS alert
  receiver, run `--verify-existing` over every retained local snapshot, record
  its exact R2 evidence, then run a normal backup and restore drill.
- Production remains blocked until a separate R2 management credential proves
  hourly/daily bucket-lock and lifecycle policy, the old API-exposed key is
  rotated/revoked, the API environment and root ownership migration are proven,
  and a new-key read-back canary passes. Host `AccessDenied` for lifecycle APIs
  is expected but is not evidence that a lifecycle exists.
- A real webhook, an external 30–45 minute guard-status dead-man, and the
  DigitalOcean 70% root-disk alert receipt are still required. Local cron cannot
  monitor its own host, scheduler, network, or configuration failure. The exact
  `bluey-brain` target now has an edit-verified 85%-for-10-minutes memory policy
  (`022ce0ab-510c-4067-8488-8b03a0dc8f7e`); the retained disk policy is
  `9c0edae0-c820-46b3-b8d7-5330779ac5c8`, whose current threshold/target and
  delivery receipt remain release evidence.
- The disposable restore-drill gate remains stop-the-line. Its required external
  provider must issue the exact target-bound marker, monitor independently of
  this host, clean up on bounded expiry after `SIGKILL`/host loss, and retain
  target-absence plus closure evidence. Phase 622 implements none of that
  control plane, and production preflight intentionally fails when
  `BLUEY_PREFLIGHT_REQUIRE_RESTORE_DEADMAN_PROVIDER=1`. Removing the local
  marker after proven target teardown is lease consumption only, never provider
  deregistration or provider-side cleanup evidence.
- Rollout is transactional: cron remains disabled until bootstrap, backup,
  isolated restore, archive proof, and alert canaries pass. Script/config
  rollback also requires cron disabled so old 14+14 behavior cannot restart.
- Legacy files with missing or mismatched local/remote checksums are retained.
  They require operator investigation; the script never guesses or uploads over
  historical evidence during bootstrap.
- The disk guard reports build candidates but does not delete source, release,
  database, backup, or rollback trees. Cleanup still requires exact current and
  rollback identity review.
- Global-candidate archival is not enabled here. It needs a separately reviewed
  one-row production canary and bounded ramp. Reducing logical payload does not
  automatically return PostgreSQL filesystem pages; online maintenance is a
  separate operational decision.

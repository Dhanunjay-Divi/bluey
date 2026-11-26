# Bluey Server — Production Deployment Runbook

> **Codex preflight:** Load `$bluey-ops` from
> `/Users/uno/.codex/skills/bluey-ops/SKILL.md` before any deploy action. Recheck
> the intended commit, artifact provenance, live flags, backups, and this current
> runbook before changing production.

> **Scope:** Stand up `bluey-server` on a single DigitalOcean droplet (or any single Linux host) behind Caddy with auto-TLS. Single binary + SQLite + reverse proxy.
> **Audience:** Operator with sudo access to the deployment host.
> **Estimated time:** 30-45 minutes from a fresh Ubuntu 24.04 droplet.

## 0. Prerequisites checklist

Before starting, you must already have:

- [ ] The **`bluey.sh` domain** with DNS A/AAAA records pointed at the droplet's public IPv4/IPv6.
- [ ] A **DigitalOcean droplet** (or equivalent) running Ubuntu 24.04 with at least 2 GB RAM, 2 vCPU, 25 GB SSD. SSH key set up.
- [ ] **Square production + sandbox application credentials**, location IDs, and webhook signature keys from the Square dashboard. Keep these in a password manager — never commit.
- [ ] **Upstream provider keys** (OpenAI, Anthropic, Deepgram) for the managed lanes.
- [ ] **Resend API key** for transactional email. Bluey uses the Resend HTTPS
  API path because many cloud hosts block outbound SMTP ports.
- [ ] A **64-character JWT secret** generated via `openssl rand -hex 32`.
- [ ] **Backup destination** — a retention-protected S3-compatible prefix is
  required for production.

## 1. One-time host setup

```bash
# As root on the fresh droplet.
apt-get update && apt-get -y upgrade
apt-get -y install ufw curl ca-certificates rsync sqlite3 jq gnupg

# Firewall: only 22 (SSH), 80, 443 open.
ufw default deny incoming
ufw default allow outgoing
ufw allow 22/tcp
ufw allow 80/tcp
ufw allow 443/tcp
ufw --force enable

# Dedicated unprivileged user for the daemon.
useradd --system --create-home --home-dir /opt/bluey-api --shell /usr/sbin/nologin bluey
mkdir -p /opt/bluey-api /var/log/bluey-api /var/backups/bluey-api
chown -R bluey:bluey /opt/bluey-api /var/log/bluey-api
chown root:root /var/backups/bluey-api
chmod 0700 /var/backups/bluey-api
```

## 2. Install Caddy (auto-TLS via Let's Encrypt)

```bash
apt-get -y install debian-keyring debian-archive-keyring apt-transport-https
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' | gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' | tee /etc/apt/sources.list.d/caddy-stable.list
apt-get update && apt-get -y install caddy
```

Caddy auto-renews certificates from Let's Encrypt; no cron needed.

## 3. Drop in the Caddyfile

Copy `ops/Caddyfile.example` from this repo to `/etc/caddy/Caddyfile`, then:

```bash
caddy validate --config /etc/caddy/Caddyfile
systemctl reload caddy
```

## 4. Build + install the server binary

On a build host (your laptop or a CI runner) with the rust toolchain:

```bash
cd /path/to/cue/server
cargo build --release
scp target/release/bluey-server root@<droplet>:/usr/local/bin/bluey-server
ssh root@<droplet> 'chmod 755 /usr/local/bin/bluey-server && chown root:root /usr/local/bin/bluey-server'
```

(Future: replace with a CI-built artifact + `apt`-installable package.)

## 5. Configure environment

Create `/etc/bluey-api/bluey-api.env` with mode 0640 owned by `root:bluey`.
The service runs as the `bluey` group, so group-read is required:

```ini
# Required
BLUEY_PORT=8080
BLUEY_DB_PATH=/opt/bluey-api/bluey.db
BLUEY_JWT_SECRET=<openssl rand -hex 32 output>
BLUEY_PUBLIC_URL=https://bluey.sh

# Square billing. Preprod uses SQUARE_ENVIRONMENT=sandbox; prod uses production.
BLUEY_BILLING_PROVIDER=square
SQUARE_ENVIRONMENT=production
SQUARE_PRODUCTION_APPLICATION_ID=sq0idp_xxxxxxxx
SQUARE_PRODUCTION_ACCESS_TOKEN=EAAA_xxxxxxxx
SQUARE_PRODUCTION_LOCATION_ID=<Square production location id>
SQUARE_PRODUCTION_WEBHOOK_SIGNATURE_KEY=<Square production webhook signature key>

# Optional: keep sandbox values on the host so a preprod env file can switch
# by changing only SQUARE_ENVIRONMENT=sandbox.
SQUARE_SANDBOX_APPLICATION_ID=sandbox-sq0idb_xxxxxxxx
SQUARE_SANDBOX_ACCESS_TOKEN=EAAA_sandbox_xxxxxxxx
SQUARE_SANDBOX_LOCATION_ID=<Square sandbox location id>
SQUARE_SANDBOX_WEBHOOK_SIGNATURE_KEY=<Square sandbox webhook signature key>

# Upstream providers (Bluey owns these; customers pay Bluey)
OPENAI_API_KEY=sk-xxxxxxxx
ANTHROPIC_API_KEY=sk-ant-xxxxxxxx
DEEPGRAM_API_KEY=xxxxxxxx

# Transactional mail for verify + reset emails. Resend is sent over HTTPS
# by bluey-server because many cloud hosts block outbound SMTP ports.
BLUEY_SMTP_HOST=smtp.resend.com
BLUEY_SMTP_PORT=587
BLUEY_SMTP_USERNAME=resend
BLUEY_SMTP_PASSWORD=<resend api key>
BLUEY_SMTP_FROM=Bluey <hello@bluey.sh>
BLUEY_SMTP_STARTTLS=true

# Rate-limit XFF trust — when behind Caddy on the same host this is loopback.
BLUEY_TRUSTED_PROXIES=127.0.0.1,::1
```

```bash
chmod 0640 /etc/bluey-api/bluey-api.env
chown root:bluey /etc/bluey-api/bluey-api.env
```

## 6. Install the systemd unit

Copy `ops/bluey-api.service.example` from this repo to `/etc/systemd/system/bluey-api.service`, then:

```bash
systemctl daemon-reload
systemctl enable --now bluey-api.service
systemctl status bluey-api.service
journalctl -u bluey-api.service --since "5 min ago"
```

## 7. Verify

```bash
curl -fsS https://bluey.sh/admin/health
# {"status":"ok","version":"...","commit":"..."}

curl -fsS https://bluey.sh/pricing/tiers | jq .
# {"reload_amount_cents":1500,"minimum_cue_cents":1,"tiers":[...]}
```

If both succeed, the server is reachable, TLS is live, and the public router responds.

## 8. Configure Square webhook

In the Square dashboard → Developer → Webhooks → Add subscription:

- **URL:** `https://bluey.sh/billing/square/webhook`
- **Events:** at minimum `order.updated`.
- After saving, copy the **Signature key** and update `SQUARE_PRODUCTION_WEBHOOK_SIGNATURE_KEY` in `/etc/bluey-api/bluey-api.env`. `systemctl restart bluey-api.service`.

Run a Square sandbox checkout and verify `journalctl -u bluey-api.service` shows the `order.updated` event processed and the account balance credited.

## 9. Backups

Treat the storage change as a credential and filesystem-boundary migration, not
as a script copy. Live evidence showed the old backup key in the API-readable
environment, `/var/backups/bluey-api` owned by `bluey:bluey`, and the service
unit allowed to write that tree. Stop the API and every storage/log schedule
before changing ownership or credentials. Preserve the disabled files for
audit/rollback; never reactivate them with the old 14+14 policy.

```bash
migration_hold=/root/bluey-storage-migration-hold
install -d -m 0700 -o root -g root "$migration_hold"
systemctl stop bluey-api.service
for policy in \
  /etc/cron.d/bluey-api-backup \
  /etc/cron.d/bluey-log-guards \
  /etc/logrotate.d/bluey-api \
  /etc/logrotate.d/bluey-ops; do
  [ ! -e "$policy" ] || mv "$policy" "$migration_hold/"
done

install -m 0750 -o root -g root ops/backup-bluey-db.sh \
  /usr/local/sbin/backup-bluey-db.sh
install -m 0600 -o root -g root \
  ops/bluey-storage.env.example /etc/bluey-api/bluey-storage.env

# Install the current unit, which does not grant the API access to backup data.
install -m 0644 -o root -g root ops/bluey-api.service.example \
  /etc/systemd/system/bluey-api.service
systemctl daemon-reload
```

In the Cloudflare control plane, create a new backup credential scoped to
list/head/get/put only for the backup prefix. It must have no delete, bucket
administration, lifecycle, or lock-policy permission. Put that new credential,
the real HTTPS alert receiver, and the dedicated operational-log credential in
`/etc/bluey-api/bluey-storage.env`; keep it `root:root` mode `0600`. Remove
`OFFSITE_DESTINATION`, `AWS_*`, `BLUEY_BACKUP_S3_*`, `BLUEY_OPS_LOG_R2_*`, and
the alert webhook from `bluey-api.env`. Application object storage keeps its own
separately scoped `BLUEY_OBJECT_*` credential.
Every root-run storage script rejects this fragment if it is a symlink,
non-regular, not root-owned, or group/world writable; that validation occurs
before the file is sourced.

With the service and old schedules still stopped, prepare the trusted roots and
scripts. `--prepare` changes only the exact directory entries to `root:root`
without recursively rewriting backup payloads, rejects symlinks, installs the
journald cap, validates the installed scripts, and leaves scheduling disabled:

```bash
ops/install-bluey-log-guards.sh --prepare
/usr/local/sbin/backup-bluey-db.sh --check-config
/usr/local/sbin/archive-bluey-logs.sh --check-config
/usr/local/sbin/bluey-disk-guard.sh --check-config
find /var/backups/bluey-api -maxdepth 2 -type l -print -quit | \
  grep -q . && { echo 'unexpected backup symlink' >&2; exit 1; } || true
stat -c '%U:%G %a %n' \
  /var/backups/bluey-api \
  /var/backups/bluey-api/hourly \
  /var/backups/bluey-api/daily \
  /var/backups/bluey-api/.staging \
  /var/backups/bluey-api/deadman \
  /var/backups/bluey-api/.restore-drill-locks \
  /var/lib/bluey-ops

# With the API and every old schedule still stopped, reject unexpected entries
# and migrate only exact backup/proof files by metadata; never recurse through
# an unreviewed tree or follow links.
for snapshot_dir in \
  /var/backups/bluey-api/hourly \
  /var/backups/bluey-api/daily; do
  if find "$snapshot_dir" -mindepth 1 -maxdepth 1 ! -type f -print -quit | grep -q .; then
    echo "unexpected non-regular snapshot entry under $snapshot_dir" >&2
    exit 1
  fi
  if find "$snapshot_dir" -mindepth 1 -maxdepth 1 -type f \
    ! \( -name '*.db' -o -name '*.pgdump' -o -name '*.sha256' \
         -o -name '*.offsite-verified' \) -print -quit | grep -q .; then
    echo "unexpected snapshot filename under $snapshot_dir" >&2
    exit 1
  fi
  find "$snapshot_dir" -mindepth 1 -maxdepth 1 -type f \
    \( -name '*.db' -o -name '*.pgdump' -o -name '*.sha256' \
       -o -name '*.offsite-verified' \) \
    -exec chown root:root -- {} + -exec chmod 0600 -- {} +
done
for control_file in \
  /var/backups/bluey-api/.backup.lock \
  /var/backups/bluey-api/.backup.status; do
  [ ! -e "$control_file" ] && [ ! -L "$control_file" ] && continue
  [ -f "$control_file" ] && [ ! -L "$control_file" ] || {
    echo "unexpected backup control entry: $control_file" >&2
    exit 1
  }
  chown root:root "$control_file"
  chmod 0600 "$control_file"
done
```

Do not start the API yet if any backup payload, sidecar, or proof marker is not
root-owned and write-protected; investigate that exact file instead of applying
a recursive ownership rewrite. The later exact read-back bootstrap revalidates
the content after this metadata-only ownership migration.

The script lives at `ops/backup-bluey-db.sh` in this repo. It auto-detects the
runtime database backend:

- `BLUEY_SERVER_DB_BACKEND=sqlite`: uses SQLite's online `.backup` API.
- `BLUEY_SERVER_DB_BACKEND=postgres`: loads `/etc/bluey-api/bluey-postgres.env`
  by default and writes a `pg_dump --format=custom` archive.

Both modes are safe while `bluey-api.service` is live. Each staged SQLite copy
must pass `PRAGMA quick_check`; each custom Postgres archive must pass
`pg_restore --list` before finalization or proof-marker creation. The script rotates the
configured hourly/daily counts as complete archive/checksum pairs. Production's
58 GB host uses 4 hourly plus 7 daily snapshots, preserves at least 2 of each,
keeps backups under 12 GB and at least 16 GB free, and ships durable copies to
R2. A 2 GiB per-snapshot writer limit plus a pre-write reserve prevents a grown
dump from filling the root filesystem mid-write. Midnight runs reserve two such
allocations because the finalized hourly file and staged daily copy coexist.
The exclusive lock removes every orphaned `.staging/run.*` before capacity work
and prevents overlapping cron/manual backups. A dedicated volume/quota remains
the preferred long-term boundary.

The 12 GB ceiling covers database hot storage (`hourly`, `daily`, and
`.staging`) only. Release/bin/round rollback evidence and log archives are not
paid for by deleting DB restore points; the disk guard accounts for those
components and whole-root pressure independently.

For managed Postgres, install a `pg_dump` client that is the same major version
as the server, or newer. A PostgreSQL 18 server requires `postgresql-client-18`;
older clients abort with a server-version mismatch.

For Cloudflare R2:

```bash
OFFSITE_DESTINATION=s3://<bucket>/bluey-api-backups/
BLUEY_BACKUP_S3_ENDPOINT_URL=https://<cloudflare-account-id>.r2.cloudflarestorage.com
AWS_ACCESS_KEY_ID=<new-prefix-scoped-backup-access-key>
AWS_SECRET_ACCESS_KEY=<new-prefix-scoped-backup-secret-key>
AWS_DEFAULT_REGION=auto
```

New objects are written below explicit `hourly/` and `daily/` sub-prefixes;
legacy objects remain in the flat prefix for read-only bootstrap. Before
activation, record control-plane evidence from a management token that is never
installed on the host:

- an R2 bucket-lock rule for both new prefixes (recommended minimum: 7 days for
  hourly and 35 days for daily);
- lifecycle expiration longer than the lock window (recommended: 14 days for
  hourly and 90 days for daily), plus a documented rule for legacy flat objects;
- read-back of both lock and lifecycle configuration; the prefix-scoped host key
  is expected to receive `AccessDenied` for those administrative APIs; and
- no host-side remote-delete job. Retention is control-plane policy, not ad hoc
  deletion from the backup script.

At the current roughly 0.93 GiB full dump, hourly objects add about 22 GiB/day
or 0.67 TiB/month without lifecycle expiration. Bucket lock takes precedence
over lifecycle deletion, so the lifecycle window must exceed the lock window.

The script verifies remote size, checksum sidecar, and full read-back before it
mints `.offsite-verified`. Neither count nor capacity pruning may delete an
unverified local snapshot when offsite backup is configured. Proof markers are
bound to the current configured destination, and each deletion candidate gets
a fresh local checksum plus remote full read-back immediately before removal.
Normal runs reconcile new-format snapshots left unverified by a transient
upload outage before allocating another dump; historical daily-to-hourly
mappings remain exclusive to the read-only bootstrap below.

Existing production files created before proof markers must be bootstrapped
before the 12 GB ceiling is activated:

```bash
/usr/local/sbin/backup-bluey-db.sh --verify-existing
```

This command is read-only against backup objects and local archives except for
atomic proof-marker creation. It derives historical daily-to-hourly object
names from checksum sidecars and stops on any missing object, size, sidecar, or
full-read-back mismatch. Record its output, then run one normal backup and a
restore drill before relying on automatic retention.

Prove the new host key with a bounded canary: list its prefix, upload a unique
small object and checksum, HEAD both, and fully read both back. A delete attempt
must be denied and the object must remain readable; use the separate management
credential for any later lifecycle cleanup. Then start the API with the updated
unit and API env, inspect only environment variable names (never values), and
prove no backup/ops key is inherited:

```bash
systemctl start bluey-api.service
api_pid="$(systemctl show -p MainPID --value bluey-api.service)"
if tr '\0' '\n' < "/proc/$api_pid/environ" | cut -d= -f1 | \
  grep -Eq '^(OFFSITE_DESTINATION|AWS_|BLUEY_BACKUP_S3_|BLUEY_OPS_LOG_R2_|BLUEY_DISK_GUARD_ALERT_WEBHOOK_URL)'; then
  echo 'root-only storage credential leaked into bluey-api' >&2
  exit 1
fi
```

After that check and a new-key exact backup read-back canary pass, revoke the
old key that had been exposed to the API process and repeat the canary. Rollback
must never restore the old key or the old backup-root write permission.

Production must keep `BLUEY_DISK_BACKUP_HEALTH_REQUIRED=1`. The 15-minute disk
guard warns when the newest active-backend hourly snapshot reaches 120 minutes
and fails at 180 minutes, or immediately for no snapshot, a missing/malformed
checksum sidecar, or missing/inconsistent offsite proof. It compares trusted
root-owned metadata only and does not rehash the full dump on each guard run.
Non-production hosts without a backup schedule may explicitly set the switch to
`0`; do not carry that opt-out into production.

The installer creates the canonical root-owned backup lock inode, and the guard
holds a shared lock throughout its metadata scan. While a legitimate writer holds it, the
guard evaluates the last fully completed snapshot and reports the writer as in
progress; after lock release, an incomplete finalized pair fails immediately.
Production also requires the atomic backup-run status, a daily snapshot below
the 36-hour warning/48-hour hard ages, no unverified backlog, DB hot storage
below its cap, and inode warning/hard thresholds. Alert deduplication includes
safe reason codes, so a new backup fault is delivered even if aggregate state
remains `warn` or `fail`.

Local alert delivery is not a dead-man for a failed host, cron daemon, network,
or configuration. Before release, require both a real webhook transition canary
and an external monitor that alarms if `disk-guard.status` is older than 30–45
minutes. The DigitalOcean agent must also have a 70% root Disk Utilization alert
with a proven operator notification path.

As of 2026-08-30, the exact `bluey-brain` target also has an edit-verified
DigitalOcean policy named `Bluey memory above 85% for 10 minutes` (policy ID
`022ce0ab-510c-4067-8488-8b03a0dc8f7e`) using the existing verified account
email notification. The retained disk-policy ID is
`9c0edae0-c820-46b3-b8d7-5330779ac5c8`; record its current 70% threshold,
target, duration, and a delivered notification alongside each release gate.

Keep these values in root-owned environment/cron config on the server. They must
never be shipped in the desktop app.

### Restore Drills

Backups are not considered production-ready until a restore has been proven
against a disposable target database.

Install the restore drill script next to the backup script:

```bash
cp ops/restore-drill-bluey-db.sh /usr/local/sbin/restore-drill-bluey-db.sh
chmod 750 /usr/local/sbin/restore-drill-bluey-db.sh
chown root:root /usr/local/sbin/restore-drill-bluey-db.sh
```

For Postgres, a separately reviewed external control-plane provider must first
provision a uniquely named empty database from `template0` on a disposable
server/cluster. The target role must directly own it and also connect to the
target server's `postgres` maintenance database for forced teardown. Set this
exact database comment:
`bluey-restore-drill-disposable:v1:<database>:<expiry_epoch>:<hex_token>`.

The same independent provider must own a bounded registration that survives
this host, process, cron, network, and `SIGKILL`. It must monitor the exact
provider resource, drop or quarantine an abandoned target no later than the
declared expiry (at most 24 hours), alert on cleanup failure, and retain its own
armed/expired/closed audit record. Before the restore starts, it must issue one
newline-terminated marker directly under the root-owned mode-0700
`/var/backups/bluey-api/deadman` directory:

```text
bluey-restore-drill-deadman:v1:<target_cluster_sentinel>:<database>:<expiry_epoch>:<hex_token>:<drill_authority>:<provider_identity>
```

The marker must be a regular, non-symlink, `root:root` mode-0600 file. Never
mint it locally or treat its existence as evidence that a provider is armed.
The restore receives all of the following exact bindings:

```text
BLUEY_RESTORE_DRILL_DATABASE_URL=<isolated-disposable-target-url>
BLUEY_RESTORE_DRILL_DATABASE_NAME=<bluey_restore_drill_*>
BLUEY_RESTORE_DRILL_CONFIRMATION=restore:<database>
BLUEY_RESTORE_DRILL_SENTINEL_TOKEN=<32-to-128-hex-token>
BLUEY_RESTORE_DRILL_TARGET_EXPIRES_AT_EPOCH=<future-epoch-within-24h>
BLUEY_RESTORE_DRILL_TEARDOWN_MODE=drop
BLUEY_RESTORE_DRILL_PRODUCTION_CLUSTER_SENTINEL=<independently-read-live-sentinel>
BLUEY_RESTORE_DRILL_TARGET_CLUSTER_SENTINEL=<provider-target-sentinel>
BLUEY_RESTORE_DRILL_AUTHORITY=<reviewed-audit-authority>
BLUEY_RESTORE_DRILL_DEADMAN_PROVIDER_IDENTITY=<provider-registration-identity>
BLUEY_RESTORE_DRILL_DEADMAN_MARKER_FILE=/var/backups/bluey-api/deadman/<safe-name>.lease
```

Phase 622 does **not** select or implement that external provider. Do not run a
production restore drill by hand-creating the marker. A later reviewed phase
must implement provider registration, independent liveness/expiry monitoring,
control-plane cleanup, status read-back, and closure evidence.

The script loads API, root-only storage, and PostgreSQL fragments; selects only
the active backend's extension; and rejects an explicitly mismatched backup.
It requires an absolute non-symlink Bluey backup plus exact sidecar, validates
the checksum and archive catalog, and verifies a zero-user-object baseline,
direct owner, exact disposable comment, expiry, confirmation, and live/target
server/database identity before any destructive restore. PostgreSQL URLs are
converted to a protected temporary libpq service and never passed in argv. The
restore and teardown are bounded; success requires the target database to be
dropped. A same-server/different-database drill is exceptional and additionally
requires `BLUEY_RESTORE_DRILL_ALLOW_SAME_CLUSTER=1` plus a reviewed
`BLUEY_RESTORE_DRILL_SAME_CLUSTER_AUDIT_REF`; the production database always
rejects. SQLite uses a temporary scratch copy and `PRAGMA
integrity_check`.

After the script proves the target absent, it consumes the exact local lease
marker. That local removal is not provider deregistration and cannot prove the
external monitor closed. The provider must independently observe target
absence, close the registration, and preserve that evidence. If the process or
host dies at any earlier point, the provider registration remains armed and
must clean up at expiry without any local heartbeat.

Run this drill after database migrations, before launch, and at least monthly,
but only after the external-provider phase is implemented and reviewed.

### Transactional storage-policy activation

With cron and Bluey's logrotate policy still disabled, bootstrap every retained
legacy snapshot, run one normal backup, prove log-archive upload/read-back and
durable status, and exercise warning/failure/recovery delivery. Record the R2
lock/lifecycle read-back, DigitalOcean 70% disk alert, external 30–45 minute
guard-status dead-man, new-key/revoked-old-key evidence, API credential
isolation, and the existing isolated restore result. These controls may activate
the storage-reliability hotfix, but they do not satisfy the external
restore-target dead-man gate or authorize a product release.

```bash
/usr/local/sbin/backup-bluey-db.sh --verify-existing
/usr/local/sbin/backup-bluey-db.sh
/usr/local/sbin/archive-bluey-logs.sh
cat /var/lib/bluey-ops/log-archive.status
ops/install-bluey-log-guards.sh --activate

backup_cron_tmp="$(mktemp /etc/cron.d/.bluey-api-backup.tmp.XXXXXX)"
printf '%s\n' \
  '0 * * * * root /usr/local/sbin/backup-bluey-db.sh >> /var/log/bluey-ops/backup-cron.log 2>&1' \
  > "$backup_cron_tmp"
chmod 0644 "$backup_cron_tmp"
mv "$backup_cron_tmp" /etc/cron.d/bluey-api-backup

BLUEY_PREFLIGHT_REQUIRE_DISK_GUARD=1 \
BLUEY_PREFLIGHT_REQUIRE_RESTORE_DEADMAN_PROVIDER=1 \
  scripts/bluey-cloud-preflight.sh \
  /etc/bluey-api/bluey-api.env \
  /etc/bluey-api/bluey-storage.env \
  /etc/bluey-api/bluey-postgres.env
```

The final preflight is intentionally expected to fail with
`external restore-drill dead-man provider is not implemented` in this phase.
Do not override the production storage profile to make it green. A later phase
must replace that explicit stop line with a live external provider status check
and its independently retained creation, monitoring, expiry, cleanup, and
closure evidence.

Do not roll back only the scripts or only the policy. First disable both cron
files and both Bluey logrotate policies, preserve all local snapshots and proof
markers, restore a reviewed script/config pair, rerun config checks and a
canary, then re-enable schedules. Never restore the revoked key, service write
access to the backup tree, or the old 14+14 policy.

### Data Requests And Deletes

Authenticated users can download:

- `/account/export` for the existing JSON export.
- `/account/export?format=zip` for a complete zip with structured JSON,
  readable transcript/answer markdown, artifact metadata, and original object
  bytes when object storage is configured.

Zip exports fail closed if referenced object bytes cannot be fetched, if an
object key is outside the account scope, or if the export would exceed
`BLUEY_EXPORT_MAX_OBJECT_BYTES` (default `100 MiB`). Use
`include_objects=false` only when a metadata/text-only bundle is explicitly
acceptable.

Account deletion requires explicit consent fields:

```json
{
  "confirm_text": "DELETE",
  "accept_data_loss": true,
  "accept_credit_loss": true
}
```

If synced artifact objects exist, Bluey deletes those R2/S3 objects first and
only then deletes the account rows. If object storage is not configured or an
object delete fails, the account delete fails closed instead of leaving orphaned
blobs.

Admin-only support and storage endpoints:

- `/admin/storage/health`: application-visible storage readiness only. Because
  the service no longer reads the root-owned backup tree, `latest_backup` may be
  null; backup freshness authority is `/var/lib/bluey-ops/disk-guard.status`
  plus root-only proof, not this API response. Restore/release authority also
  requires the still-unimplemented independent restore-target dead-man provider,
  so production preflight remains blocked in Phase 622.
- `/admin/support/accounts/<account_id>`: redacted account support bundle with
  counts, hashed identifiers, recent provider/cost rows, and artifact object
  metadata. It deliberately excludes transcript text, answer text, document
  previews, source URIs, raw object keys, and raw email.
- `/admin/ops/events`: recent redacted export/delete/support audit events. These
  records store account hashes, actor hashes, status, and small metadata only,
  so delete/export evidence survives account hard-delete without retaining user
  transcripts or documents.

## 10. Monitoring

The server exposes `/admin/metrics` in Prometheus exposition format (admin-only). To scrape:

1. Create an admin account: `curl -X POST https://bluey.sh/auth/signup ...`, then `UPDATE accounts SET is_admin=1 WHERE email='ops@bluey.sh';` directly in the DB (or a future `bluey ops promote` CLI).
2. Mint a long-lived bearer for monitoring; store in your Prometheus auth config.
3. Scrape with `Authorization: Bearer ...` header.

Key metrics to alert on:
- `bluey_mark_complete_failures_estimated` — proxy for billed-but-uncached requests. Alert if >0 sustained 10 minutes.
- `bluey_request_idempotency_in_progress` — alert if growing without bound (deadlocked requests).
- `bluey_balance_cents_sum` — sanity check; sudden negative deltas suggest a billing bug.
- HTTP 5xx rate on Caddy access logs.

## 11. Rolling out a new server version

Default rule: use a signed, auditable release/promotion path. Do not manually
replace the production API binary for normal releases. Manual server binary
replacement is an emergency hotfix path only and must be recorded in the round
doc with the release id, operator, reason, backup proof, smoke output, and
rollback target.

Before any production server rollout:

- Run [`docs/ops/DEPLOY-DISK-STORAGE-CHECK-RUNBOOK.md`](./ops/DEPLOY-DISK-STORAGE-CHECK-RUNBOOK.md).
- Run `scripts/bluey-cloud-preflight.sh` against the production env files.
- Confirm a fresh backup and restore drill if the change touches schema,
  billing, credits, account delete/export, R2/object storage, or retention.
- For billing or credit changes, prove provider payment, local ledger, balance,
  user entitlement, refund/dispute state, and auto-reload state agree for a test
  account.
- For provider/search/STT/routing changes, prove cooldowns, 429 handling, and
  usage charging cannot loop or double-bill.

Emergency-only manual binary replacement:

```bash
# Build the new binary on your build host.
cd cue/server && cargo build --release

# Push.
scp target/release/bluey-server root@<droplet>:/usr/local/bin/bluey-server.new
ssh root@<droplet> '
  set -e
  chmod 755 /usr/local/bin/bluey-server.new
  mkdir -p /var/backups/bluey-api/bin
  if [ -x /usr/local/bin/bluey-server ]; then
    cp -f /usr/local/bin/bluey-server /var/backups/bluey-api/bin/bluey-server.previous
  fi
  mv /usr/local/bin/bluey-server.new /usr/local/bin/bluey-server
  chown root:root /usr/local/bin/bluey-server
  systemctl restart bluey-api.service
  sleep 2
  curl -fsS https://bluey.sh/admin/health
'
```

If `/admin/health` fails, roll back:

```bash
ssh root@<droplet> 'cp /var/backups/bluey-api/bin/bluey-server.previous /usr/local/bin/bluey-server && systemctl restart bluey-api.service'
```

The rollout command copies the old binary to `/var/backups/bluey-api/bin/bluey-server.previous` before replacing it. Keep that step before the `mv`; doing it from `ExecStartPre` would copy the newly deployed binary and make rollback useless.

After the emergency rollout, run the production smoke and add the exception to a
numbered round doc. If the same change needs to become normal production state,
cut a signed release artifact from the same commit and promote it through the
standard path.

## 12. Pre-launch sign-off checklist

Before flipping DNS or announcing the product:

- [ ] `/admin/health` returns 200 over HTTPS with a valid TLS cert.
- [ ] Square webhook fires successfully on a sandbox purchase first, then on a real production purchase.
- [ ] SMTP emails arrive in <30 seconds for both `/auth/verify-email/start` and `/auth/password-reset/start`.
- [ ] `/admin/metrics` is reachable with a bearer + the metrics look sane (accounts >= 1, no in_progress > 0).
- [ ] Backup script runs successfully via `/usr/local/sbin/backup-bluey-db.sh`
      and produces the expected backend file in `/var/backups/bluey-api/hourly/`:
      `.db` for SQLite or `.pgdump` for Postgres.
- [ ] Off-host backup destination receives the snapshot.
- [ ] At least one full money-path smoke: signup → trial → reload via Square → cue dispatch → balance debited → cue response.
- [ ] First `bluey on` from a clean Mac opens browser sign-in and successfully completes the deep-link flow against the production server.
- [ ] systemd unit restarts cleanly on `systemctl restart bluey-api.service` (no orphan PIDs).
- [ ] Caddy auto-TLS renewal log entries visible (`journalctl -u caddy --since "1 hour ago" | grep -i renew`).

## 13. Disaster recovery

If the droplet is destroyed:

1. Provision a new droplet (any region with the same Ubuntu version).
2. Re-run sections 1, 2, 4, 5, 6.
3. Prove the selected snapshot's sidecar, structural catalog/integrity, and
   exact offsite read-back before any restore. Restore only into a newly
   provisioned empty replacement database/volume with an independently checked
   identity; never run an ad hoc `pg_restore --clean` against the former live
   connection string. Have a second operator verify the target identity and
   recovery plan before switching service traffic.
4. Repoint DNS A/AAAA records.
5. Verify section 7.

The logical-backup RPO target is one hour. RTO is not assumed from cron cadence;
record it from a timed, independently verified replacement-host restore.

## 14. Things explicitly NOT in this runbook

- **Multi-host / load-balanced deployment.** The server supports shared
  Redis/Valkey rate/capacity state, but multi-host rollout still needs a
  separate load balancer smoke and deployment runbook.
- **Database replication.** Managed Postgres is the main production DB path;
  this runbook covers hourly logical backups, not cross-region replication or
  managed point-in-time-recovery policy.
- **Ad hoc desktop update server changes.** The distribution path is live at
  `https://bluey.sh`; changes to `latest.json`, installers, and release
  artifacts must go through signed publish/promotion and live verification.
- **Full web app polish on `bluey.sh`.** The same origin should host landing,
  install, link, reload, account, and docs pages. This repo includes the API
  and static landing starter; production page polish can stay in a separate
  web codebase as long as it publishes into `/var/www/bluey`.

# Round 581 - Jobs Global Feed Quarantine Production Deploy

Date: 2026-07-30

Status: deployed and verified

## Scope

This round records the manual production activation of the global-feed row
quarantine implemented and reviewed in Round 580 at source commit
`21d7951f1ae2facaf425b15218a8512345ab8712`.

The deployment changed only:

- `bluey-jobs-api.service`;
- `bluey-jobs-discovery.service`;
- `bluey-jobs-global-discovery.service`.

It did not deploy or restart the main Bluey API, Caddy, the Jobs portal, the
native overlay, audio, STT, or any signed native release.

## Source And Artifact Identity

| Artifact | Identity |
| --- | --- |
| Reviewed source commit | `21d7951f1ae2facaf425b15218a8512345ab8712` |
| Source archive | `/tmp/bluey-jobs-source-21d7951f1ae2.tar.gz` |
| Source archive SHA-256 | `f05523aa7b0dec91107bc06d20747e9379276e20f84ef97360445202a3d75aed` |
| Worker archive SHA-256 | `27e8869cb3bd2422393a8223ef8a66c4a515b85c3d2c03c6d20a748bac132b2e` |
| Jobs API SHA-256 | `790ec7d1c4614edeed4241b8b5fc9eb49d8874227038167905dd3ca383d007b3` |
| Worker release | `/opt/bluey-jobs-workers/releases/jobs-workers-21d7951f1ae2` |
| Worker `package-lock.json` SHA-256 | `49627b6858769236352b41a7a9c392185ff6b992e1709ed31030582b8f38c0bf` |
| Worker `package.json` SHA-256 | `d45f033d1960a4f2157fb6b2bd6b2c5fd0791c855e1cb4dd8acd64e22c8777bb` |

The exact source commit is embedded in the live Jobs health response. The
binary and worker package hashes are the production artifact identities used
for rollback and reconciliation.

## Verification Before Deployment

The Round 580 candidate passed:

- 781 server unit tests;
- 76 server HTTP integration tests;
- focused migration and global-feed quarantine tests;
- strict Rust formatting and Clippy;
- all Jobs TypeScript checks;
- 469 Jobs tests;
- the portal production build;
- privacy, license/provenance, schema-parity, and CI policy guards.

No GitHub Actions workflow was used for build or deployment.

## Backup, Disk, And Preflight

Before activation, old unreferenced Rust build caches were removed. No source,
release, database, portal, or active rollback artifact was deleted. Production
disk usage fell from approximately 83% to approximately 65%.

The fresh PostgreSQL rollback dump is:

| Field | Value |
| --- | --- |
| Path | `/var/backups/bluey-api/hourly/bluey-postgres-20260730T064000Z.pgdump` |
| Size | 712,940,280 bytes |
| SHA-256 | `44ef2ccae99418107b099544c9c72a8f2cb16be0b160ac440c8d225c078029fe` |
| Restore-list entries | 481 |

The strict preflight passed local database, service, binary, environment, and
rollback checks with the existing three object-storage warnings:

1. the object-storage bucket could not be listed;
2. the R2/S3 backup destination could not be listed;
3. the R2/S3 log-archive destination could not be listed.

The attempted backup replication also returned `AccessDenied`. This remains an
explicit infrastructure waiver, not a successful R2 backup claim. The local
production dump and restore inventory are valid.

## Canary And Promotion

The exact Jobs API candidate ran as an isolated one-minute canary. Startup was
clean, 21 real global-discovery batch requests returned 200, and no errors were
observed before the canary terminated normally.

The candidate was then promoted to `/usr/local/bin/bluey-jobs-api`. Both worker
symlinks were atomically moved to the exact retained worker release.

The previous global worker exceeded the 30-second graceful-stop timeout and
required `SIGKILL` during replacement. The new worker resumed from durable
lease and idempotent-ingestion state; it did not duplicate a published
snapshot or lose accepted rows.

## Live Runtime Evidence

| Service | PID | Restarts | State |
| --- | ---: | ---: | --- |
| `bluey-api.service` | 2257183 | 0 | active/running |
| `bluey-jobs-api.service` | 2808528 | 0 | active/running |
| `bluey-jobs-discovery.service` | 2808917 | 0 | active/running |
| `bluey-jobs-global-discovery.service` | 2808932 | 0 | active/running |
| `caddy.service` | 2217438 | 0 | active/running |

The main API and Caddy PIDs remained unchanged. The local Jobs health response
reports exact commit
`21d7951f1ae2facaf425b15218a8512345ab8712` on `linux-x86_64`.

All protected Jobs feature flags remain disabled:

```text
BLUEY_JOBS_MODEL_GENERATION_ENABLED=0
BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED=0
BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED=0
```

## Global Feed Reconciliation

The production health command reported:

```text
bluey-jobs-discovery-health: workers active and sources current
```

The first complete post-deploy runs produced:

| Source | Expected | Accepted | Rejected | Batches | Rejection summary |
| --- | ---: | ---: | ---: | ---: | --- |
| Lever | 67,506 | 67,506 | 0 | 136 | `{}` |
| Ashby | 46,378 | 46,377 | 1 | 93 | `{"missing_identity":1}` |

For each source:

- the run status is `completed`;
- the source is `healthy`;
- failure count is zero;
- no lease or error remains;
- the sum and count of persisted batches match the accepted-row totals;
- expired-row count is zero.

The global manifest reports two sources and 113,884 observed rows. The single
Ashby row is retained as bounded typed quarantine evidence rather than causing
an otherwise valid source snapshot to fail.

No manual database mutation was used to obtain this result.

## Logging And Data Safety

Activation-window logs were checked for panic, fatal error, stack trace, token,
secret, and raw-row leakage. No match was found. The worker logs contain only
bounded aggregate counts and the typed `missing_identity` rejection summary.

## Public And Security Verification

| Request | Result |
| --- | ---: |
| `GET https://bluey.sh/jobs/` | 200 |
| unsigned `GET /api/jobs/workspace` | 401 |
| public `POST /api/jobs/internal/discovery/lease` | 404 |
| native-client `GET /health` | 200 |
| `GET /auth/captcha/config` | 200 |
| unsigned `GET /account/me` | 401 |
| GPTBot `GET /` | 403 |
| direct-origin HTTPS with `Host: bluey.sh` | blocked / timeout |

The public main `/health` response continues to report the unchanged main API
commit. The exact Jobs deployment identity is the loopback Jobs health
response and binary hash above.

## Rollback

The pre-deploy rollback timestamp is `20260730T071310Z`.

| Artifact | Evidence |
| --- | --- |
| Prior Jobs API | `/var/backups/bluey-api/bin/bluey-jobs-api.before-21d7951f-20260730T071310Z` |
| Prior Jobs API SHA-256 | `4193b82c02f0dde2f2a01f83b76a2b3ca31530aad395fc21191c05fd781c5cb8` |
| Prior Jobs environment | `/var/backups/bluey-api/env/bluey-jobs.env.before-21d7951f-20260730T071310Z` |
| Prior Jobs environment SHA-256 | `2db8730603026b88d6576983ef2ef4d913b260760189e417b80b942dadad4741` |
| Paired database dump | `/var/backups/bluey-api/hourly/bluey-postgres-20260730T064000Z.pgdump` |

Rollback requires stopping the Jobs API and both discovery workers, restoring
the paired database, prior binary, prior environment, and prior worker
symlinks, then repeating health, flag, source, route, and edge verification.
Never run the older Jobs binary against the newer authority schema without its
paired database rollback.

## Result

Round 580 is now active in production. One malformed Ashby row no longer
degrades the complete source: it is quarantined with typed evidence while
46,377 valid Ashby rows and all 67,506 Lever rows publish normally.

This round does not enable model generation, local or cloud Browser
distribution, universal unattended submission, or any native runtime change.

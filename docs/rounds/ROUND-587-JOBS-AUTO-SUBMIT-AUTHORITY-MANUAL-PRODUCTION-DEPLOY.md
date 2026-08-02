# Round 587 - Jobs Auto-submit Authority Manual Production Deploy

Date: 2026-08-02

Status: deployed and verified

## Scope

This round records the manual production deployment of the reviewed Round 586
Jobs authority and ATS form read-back implementation from mainline commit
`c4b98bc8034550fe77af2e5b4b36a1472cb04111`.

The deployment changed only:

- `bluey-jobs-api.service`;
- the Bluey Jobs portal static bundle;
- the narrow root-marketing static files represented by the reviewed source.

It did not deploy or restart the main Bluey API, either discovery worker,
Caddy, the native overlay, audio, STT, meeting runtime, or any signed native
release.

## Source And Review Identity

| Item | Identity |
| --- | --- |
| Reviewed feature commit | `98c8213bcde2073cc0583d4e8d800b562e575f1a` |
| Mainline merge commit | `c4b98bc8034550fe77af2e5b4b36a1472cb04111` |
| Pull request | `https://github.com/Dhanunjay-Divi/bluey/pull/22` |
| Exact source archive SHA-256 | `bb0f04dc9d182569a4d5b09fbe7cac8143f6cca1987e503c47e5abb44f70f9a2` |
| Jobs API SHA-256 | `553e37700ad1ef27d1a83f6e567e7e66df79248843ccfe3c285376c74b80b86e` |
| Live Jobs API commit | `c4b98bc8034550fe77af2e5b4b36a1472cb04111` |

The source archive was extracted to an isolated build directory. The exact
commit is embedded in the promoted binary and returned by the loopback Jobs
health endpoint.

## Verification Before Deployment

The reviewed candidate passed:

- all server library tests;
- 16 focused Jobs HTTP integration tests;
- Rust formatting and strict Clippy;
- 489 Jobs package tests;
- complete Jobs TypeScript checks and the production portal build;
- privacy, provenance/license, SQLite/PostgreSQL schema parity, client/server
  boundary, and CI policy gates;
- responsive Settings and Matches QA on desktop and mobile in light and dark
  themes;
- the pull request's required repository checks, including Windows.

No GitHub Actions workflow was used to build or deploy production.

## Backup And Rollback Evidence

A fresh PostgreSQL dump was created before promotion and independently read
back from R2.

| Field | Value |
| --- | --- |
| Local dump | `/var/backups/bluey-api/hourly/bluey-postgres-20260802T120151Z.pgdump` |
| Size | 829,735,743 bytes |
| SHA-256 | `5c8b8fe6e09f647ca58ee14b60d57c938f17cabea495af8e124b1d57861b5824` |
| Restore-list entries | 492 |
| R2 object | `s3://bluey-prod/backups/api/bluey-postgres-20260802T120151Z.pgdump` |
| R2 read-back | exact object and sidecar checksum match |

The pre-promotion rollback set is retained at:

```text
/var/backups/bluey-api/round586-20260802T121254Z
```

| Rollback artifact | SHA-256 |
| --- | --- |
| Prior Jobs API | `790ec7d1c4614edeed4241b8b5fc9eb49d8874227038167905dd3ca383d007b3` |
| Prior Jobs environment | `2db8730603026b88d6576983ef2ef4d913b260760189e417b80b942dadad4741` |
| Prior Jobs portal archive | `b8a5c373f840c3e675bea6b5b116820ae8f87b88356c5cf31f4dcfee8003a0ce` |
| Prior root web archive | `c318da5216eeaca73134150dab4ef7f9b5f8a2dd930f6db54f4ecc0b956c4289` |

Rollback must restore the paired PostgreSQL dump, Jobs binary, Jobs
environment, and static archives before the Jobs service is restarted. An
older Jobs binary must not run against the newer authority schema.

## Exact-artifact Smoke And Promotion

The first simultaneous isolated smoke encountered the fixed production
PostgreSQL connection-pool ceiling while the live service still owned its
pool. No production artifact was changed by that attempt.

A coordinated brief Jobs-only stop then freed the pool. The exact candidate
ran on `127.0.0.1:18081` and passed:

```text
GET /health                 200, exact c4b98bc8 commit
GET /api/jobs/workspace     401 without authentication
```

The candidate was promoted atomically to `/usr/local/bin/bluey-jobs-api` and
the Jobs service returned active with zero restarts. The main API, both
discovery workers, and Caddy retained their existing process identities.

## Static Deployment Identity

The Jobs bundle was replaced atomically from the exact reviewed source. Root
marketing files were copied narrowly; installers, signed update manifests,
release artifacts, backups, and Caddy configuration were excluded.

| Live file | SHA-256 |
| --- | --- |
| Root `index.html` | `d07c8bc063a7283a97865cc29113e4c4529da5f8e9c240605183ea362393c725` |
| Jobs `index.html` | `6858b1e89a2d901aa9c0dd702cba5eedb7a8cdef2fdc1f2d37e63214e8dcc372` |
| Root stylesheet | `abd32a4bcdbcf10aff63c56d926d372c3f12d77c1a67f16e108e4718d1bdb784` |
| Root JavaScript | `b9c40dc595617e775205f3ef42a72c6823291cea2545309917923363f6a836bf` |

All 25 referenced Jobs assets matched the reviewed source and returned 200.

## Live Runtime Evidence

| Service | PID | Restarts | State |
| --- | ---: | ---: | --- |
| `bluey-api.service` | 2257183 | 0 | active/running |
| `bluey-jobs-api.service` | 2920297 | 0 | active/running |
| `bluey-jobs-discovery.service` | 2808917 | 0 | active/running |
| `bluey-jobs-global-discovery.service` | 2808932 | 0 | active/running |
| `caddy.service` | 2217438 | 0 | active/running |

The Jobs API binds only to loopback port 8081. The main API remains on loopback
port 8080. No panic, fatal, failed, or error entry appeared in the Jobs API
activation-window logs.

The following production controls remain disabled:

```text
BLUEY_JOBS_MODEL_GENERATION_ENABLED=0
BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED=0
BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED=0
BLUEY_JOBS_MAILBOX_SYNC_ENABLED=0
```

## Public And Edge Verification

| Request | Result |
| --- | ---: |
| `GET https://bluey.sh/` | 200 in approximately 0.41 seconds |
| `GET https://bluey.sh/jobs/` | 200 in approximately 0.08 seconds |
| native-client `GET /health` | 200 |
| `GET /auth/captcha/config` | 200 |
| unsigned `GET /api/jobs/workspace` | 401 |
| unsigned `GET /account/me` | 401 |
| public `POST /api/jobs/internal/discovery/lease` | 404 |
| missing source-map asset | 404 |
| GPTBot `GET /` | 403 |
| direct-origin HTTPS with `Host: bluey.sh` | blocked / timeout |

Jobs responses include the intended `X-Robots-Tag` policy. The root page
contains the accessible `Apply for Jobs` navigation action.

## Browser QA

The available in-app browser session was signed out and correctly redirected
to:

```text
/login?mode=signup&next=%2Fjobs
```

The live login handoff was inspected at 1,440 x 900 and 390 x 844. The Bluey
Jobs link, account form, Terms, Privacy, header, and footer rendered without
overlap or console errors. Signed-in Jobs workspace behavior was not claimed
from this browser pass; its functional evidence comes from the reviewed tests
and authenticated implementation review in Round 586.

## Result

Round 586 is active in production from exact mainline source. Track-scoped
Auto-submit authority, frozen execution admission, and ATS form read-back are
available to the review-first product without enabling model generation,
local or cloud Browser distribution, mailbox synchronization, or universal
unattended submission.

The signed native release and all native runtime components remain unchanged.

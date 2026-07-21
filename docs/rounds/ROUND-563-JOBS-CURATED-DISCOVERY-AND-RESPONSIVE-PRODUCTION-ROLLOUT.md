# Round 563 - Jobs Curated Discovery And Responsive Production Rollout

Date: 2026-07-21

Status: deployed and verified

## Scope

This round records the production rollout of the Jobs API and global discovery
worker from reviewed source commit
`9c029f5b6e3562e7c23607001df1f7913089e752`, followed by a portal-only mobile
layout correction in commit `33a1fcd9`.

The rollout keeps external feed records as discovery leads. It does not enable
model resume generation, local Bluey Browser distribution, cloud Browser
distribution, or universal unattended submission.

## Build And Artifact Identity

| Artifact | Identity |
| --- | --- |
| Jobs API / worker source | `9c029f5b6e3562e7c23607001df1f7913089e752` |
| Installed source | `/opt/bluey-build-jobs-9c029f5b` |
| Source archive | `/tmp/bluey-jobs-9c029f5b.tar.gz` |
| Source archive SHA-256 | `f3b4f5d714ff1216c9937d0ffa8abcaced09efc7bc05e68ac4d78e2e9ad1a093` |
| Jobs API binary SHA-256 | `c84eb4c3b7e4673698d39b7543bb1d4f29f0ee7f2f2efcc466f2c6f975b2e53f` |
| Responsive portal source commit | `33a1fcd9` |
| Portal archive SHA-256 | `ebac9fb7c9a869f348199d2cc2e757bb0f07ba5d91067e6a8eee3c6930878cc1` |
| Portal index SHA-256 | `f00712d50dfe77882f818511f891e1fb2f7eec3bfbd018a3197b63be8bc0324c` |
| Portal 27-file manifest SHA-256 | `1605211dcf0e415adfad43d6d9b4a768062a68ad6d060a71e340cee972f395da` |
| Responsive CSS SHA-256 | `d8034fad03e08e762829884e497521e1d73aab82bb7a5d129e5a3fe973ae6177` |

The final portal archive contains 27 files, no source maps, no `.DS_Store`
files and no AppleDouble metadata.

## Verification Before Deployment

The reviewed Jobs source passed:

- 440 Jobs TypeScript tests;
- complete Jobs TypeScript checks and portal build;
- Rust formatting and compile checks;
- 201 Jobs library tests;
- 15 Jobs HTTP integration tests;
- 754 server library tests;
- strict Clippy;
- privacy, schema-parity, provenance, dependency and client/server-boundary
  guards.

The responsive portal correction then passed:

- 10 portal test files and 62 tests;
- TypeScript `--noEmit`;
- Vite production build;
- `git diff --check`.

The Vite build emitted only the existing large-chunk advisory. It did not emit
a source map.

## Backup And Preflight

The pre-deployment rollback set is
`/var/backups/bluey-api/round563-20260721T172324Z`.

| Rollback artifact | Evidence |
| --- | --- |
| PostgreSQL dump | 607,591,988 bytes; SHA-256 `a438c6db295b66216f70c3e74c598935a9934b73cd13ff174a98c0b50b6618d2` |
| PostgreSQL restore list | 460 entries |
| Prior Jobs API | SHA-256 `26701f1650d6ac948751699813ec3840d29674ea6eb0d5293269571b0602cd26` |
| Prior Jobs portal | SHA-256 `d615bce95c9d5b9f042681b310ef0ad9e53057ba8d91c0ed5859a7711b84036d` |

The strict preflight completed with three explicit infrastructure warnings:

1. the object-storage bucket could not be listed;
2. the R2/S3 backup destination could not be listed;
3. the R2/S3 log-archive destination could not be listed.

Those warnings remain an operational waiver. This round does not claim that R2
replication or archive access was repaired. The verified PostgreSQL and portal
rollback artifacts exist locally and on the production host.

## Production Deployment

The Jobs API and global discovery worker were deployed from the exact reviewed
source archive. Their live health commit is
`9c029f5b6e3562e7c23607001df1f7913089e752`.

The first portal was deployed with that source. Mobile QA then found retained
desktop minimum-width behavior in the Matches view. The correction:

- gives the mobile application shell and major sections a true bounded width;
- stacks heading actions and metrics at 390 px;
- wraps discovery state and action copy;
- wraps job titles and subtitles;
- stacks the search-policy metrics and action;
- preserves the intentionally horizontal Career Track tab strip.

Only `/var/www/bluey/jobs` was replaced for this correction. No API, worker,
Caddy or native runtime was restarted. The immediately prior portal is retained
at:

```text
/var/www/bluey/jobs.pre-round563-mobile-20260721T183451Z
```

## Live Runtime Evidence

| Service | PID | Restarts | State |
| --- | ---: | ---: | --- |
| `bluey-api.service` | 2257183 | 0 | active/running |
| `bluey-jobs-api.service` | 2345917 | 0 | active/running |
| `bluey-jobs-discovery.service` | 2345919 | 0 | active/running |
| `bluey-jobs-global-discovery.service` | 2345971 | 0 | active/running |
| `caddy.service` | 2217438 | 0 | active/running |

The global worker resolves to `/opt/bluey-build-jobs-9c029f5b`. The separate
direct-source worker remains on `/opt/bluey-build-jobs-5a6e5238-fixed`.

All protected Jobs feature flags remain disabled:

```text
BLUEY_JOBS_MODEL_GENERATION_ENABLED=0
BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED=0
BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED=0
```

## Edge And Browser Verification

Live HTTP checks passed:

| Request | Result |
| --- | ---: |
| `GET /health` | 200 |
| `GET /jobs/` | 200 |
| unsigned `GET /api/jobs/workspace` | 401 |
| public `POST /api/jobs/internal/discovery/lease` | 404 |
| missing Jobs source map | 404 |
| `GET /auth/captcha/config` | 200 |
| unsigned `GET /account/me` | 401 |
| GPTBot `GET /jobs/` | 403 |

All 27 files on the production host byte-match the local portal candidate.

Live browser QA passed in both themes represented by the signed-in preview
state:

- desktop: 1,440 px viewport and document width, no overflow offenders;
- mobile: 390 px viewport and document width;
- mobile primary sections remain inside x=8 through x=382;
- the mobile application shell, metric band, discovery panel, search status and
  job list remain within the viewport;
- Career Track tabs remain horizontally scrollable without widening the page.

Cloudflare injects its challenge-platform script into the public HTML response,
so the public response-body hash is not a stable deployment identity. The
origin file hash and every referenced static asset were compared directly and
match the candidate listed above.

## Rollback

For a portal-only rollback:

1. move the current `/var/www/bluey/jobs` aside;
2. restore
   `/var/www/bluey/jobs.pre-round563-mobile-20260721T183451Z` to
   `/var/www/bluey/jobs`;
3. verify `/jobs/`, its referenced assets, missing-map 404 behavior and mobile
   layout.

For an API or discovery rollback, stop the Jobs API and both discovery workers,
restore the paired database, Jobs API binary, portal and worker targets from the
Round 563 rollback set, and then repeat health, route, flag, lease and browser
checks. Never run an older Jobs binary against the newer authority schema
without its paired database rollback.

## Product Boundary

Production discovery is broad enough to populate and continuously refresh
large candidate sets, but external feeds are not employer submission truth.
Bluey must still revalidate the original posting and use a certified adapter or
review handoff before employer-facing action.

This rollout does not claim every portal can be submitted unattended, does not
bypass portal controls, does not distribute Bluey Browser, and does not enable
AI resume generation. Those capabilities remain gated until their separate
cost, evidence, recovery and cross-platform acceptance requirements pass.

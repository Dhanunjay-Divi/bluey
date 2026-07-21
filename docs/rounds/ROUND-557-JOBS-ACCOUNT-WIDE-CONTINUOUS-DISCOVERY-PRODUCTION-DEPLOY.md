# Round 557: Jobs account-wide continuous discovery production deploy

Date: 2026-07-20

## Outcome

The account-wide continuous discovery implementation from Round 556 is live on
`https://bluey.sh/jobs/`. The production worker fetched 1,819 raw rows from the
four bounded curated feeds and persisted 805 canonical candidate leads after
URL deduplication and Career Track assignment.

The portal now reports active, relevant counts rather than counting historical,
passed, expired, or stale rows. Large result sets render 50 rows at a time and
the final paging action reports its real remainder, such as **Show 25 more** for
a 125-result set.

This is continuous candidate discovery, not universal unattended submission.
Curated-feed rows remain unverified leads until Bluey re-reads the original
employer posting. LinkedIn, Indeed, ZipRecruiter, Dice, CareerBuilder, unknown
ATS systems, access-controlled pages, and unsupported employer forms do not
bypass their access or execution boundaries.

The three execution gates remain disabled:

```text
BLUEY_JOBS_MODEL_GENERATION_ENABLED=0
BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED=0
BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED=0
```

## Production data

The discovery worker's completed report was:

```json
{"event":"discovery_reported","outcome":"complete","source_fingerprint":"source_f2c68bc2d56132197154bbec9bb2e1f8","replayed":false,"job_count":1819}
```

The canonical production rows are:

| Managed feed | Canonical leads |
| --- | ---: |
| PrepAI internships | 227 |
| PrepAI new grad | 268 |
| Simplify new grad | 184 |
| Zapply new grad | 126 |
| **Total** | **805** |

Each row retains its feed provenance and original URL. It is matched to one best
Career Track, deduplicated by canonical URL, marked unverified, and revalidated
against the original employer page before preparation.

## Deployed artifacts

### Jobs API and worker

```text
Jobs API binary:
  /usr/local/bin/bluey-jobs-api
  SHA256 43dff1760cd9b293b152996de59812ed82989e13633d73f4fa915d487bb2298a

Discovery runtime:
  /opt/bluey-build-jobs-5a6e5238-fixed
  selected through /opt/bluey-jobs-discovery/current
```

The first worker runtime omitted the transitive `parse5` package. It restarted
12 times while failing before processing a lease. The deployment was rolled
back, the complete runtime was assembled and checked, and the worker was then
restarted on the fixed path. No snapshot was partially committed during that
failed attempt.

Current service evidence:

| Service | PID | Restarts | State |
| --- | ---: | ---: | --- |
| `bluey-jobs-api.service` | 2286905 | 0 | active/running |
| `bluey-jobs-discovery.service` | 2288357 | 0 | active/running |
| `caddy.service` | 2217438 | 0 | active/running |

### Jobs portal

The first portal archive contained 29 macOS AppleDouble metadata files. The
candidate extraction was inspected before the live symlink was changed, so that
attempt was aborted without mutating the production portal. A clean 27-file
archive with no AppleDouble files and no source maps was then deployed
atomically.

```text
Clean portal archive:
  /tmp/bluey-jobs-round557-clean-20260721T052641Z.tar.gz
  SHA256 885560fe40788af2d7397ef4758bd46ed1f13105d278c332097684100d7d03c1

Live index:
  /var/www/bluey/jobs/index.html
  SHA256 7c80ce1a8e3e2274ee396984aab87271504856a4ea4b39faa2aa1db8ac16c05c

Live Matches bundle:
  /var/www/bluey/jobs/assets/MatchesView-qnQdTfgp.js
  SHA256 4cd578331ee5bf09ecb233fec333033afd42b33f8c234bd6095dbc04998c4a7d
```

The live directory contains 27 files, no `*.map` files, and no `._*` files.

## Backups

All rollback inputs were checked after deployment:

```text
/var/backups/bluey-api/hourly/bluey-postgres-pre-round556-20260721T040339Z.pgdump
  SHA256 44698e1957ab62e4dc3ff426820c4a53fe2d68f0fda1b9fe330e14344825ddcd

/var/backups/bluey-api/bin/bluey-jobs-api-before-round556-20260721T040339Z
  SHA256 94333730f619337c7626d91c48addbc33d8c7cac24c522c6453a82299f74ab

/var/www/bluey/backups/jobs-before-round556-20260721T040339Z.tar.gz
  SHA256 ecd51ec7fb4ae9b71b54e58f179e8a0dc8c11e3446020126e22b0889ae90c206

/var/www/bluey/backups/jobs-before-round557-final-20260721T052757Z.tar.gz
  SHA256 ffc3a7677b9ea1630570617a91bab368d680ebccc321a769711650dbf0cf0c2d
```

## Verification

### Product tests

The deployed portal was built after the active-count and remainder-label fix.
The following focused checks passed against that source and artifact set:

```text
npm test --prefix jobs/portal               # 60 passing
npm run typecheck --prefix jobs/portal
npm run build --prefix jobs/portal
git diff --check
find web/jobs -type f -name '*.map'          # no results
```

Round 556 records the broader package, server, Clippy, discovery-concurrency,
and 406-test Jobs matrix that covered the implementation deployed here.

### Visual QA

Local signed-in preview QA covered:

- 125 relevant matches at 1440 px;
- 50 to 100 to 125 progressive rendering;
- correct **Show 25 more** final paging copy;
- active totals of 125, split 63 and 62 across two Career Tracks;
- no page-level horizontal overflow on desktop; and
- a 390 x 844 mobile viewport with body width equal to scroll width and only the
  intended Career Track strip scrolling horizontally.

The public production landing shell was also checked in a real browser after
deployment. It retained the review-first beta language and did not claim that
local or cloud runners were available. A signed-in production account was not
available in the controlled browser surface for this final pass, so signed-in
live rendering is not claimed; exact artifact hashes and local signed-in visual
QA cover the deployed UI change.

### Live HTTP and edge checks

```text
GET /jobs/                                           200
GET /jobs/assets/MatchesView-qnQdTfgp.js             200
GET /jobs/assets/MatchesView-qnQdTfgp.js.map         404
GET /auth/captcha/config                             200
GET /account/me                                      401
GET /api/jobs/workspace                              401
GET /api/jobs/internal/discovery/lease               404
GET /jobs/ as GPTBot                                 403
X-Robots-Tag                                         noindex, nofollow, noarchive, nosnippet
```

The public Matches bundle downloaded through Cloudflare has the exact local and
origin SHA256. Cloudflare injects its own challenge-platform tag into the public
HTML response, so the public HTML body is not used as a byte-identity assertion;
the origin index and every referenced application asset are exact.

## Rollback

If the portal alone regresses:

1. Extract the Round 557 portal backup into a fresh sibling directory.
2. Verify its archive and index hashes.
3. Atomically repoint the live Jobs portal directory to that verified sibling.
4. Reload Caddy only if configuration, rather than static files, changed.

If the worker regresses:

1. Stop `bluey-jobs-discovery.service`.
2. Repoint `/opt/bluey-jobs-discovery/current` to the previous verified runtime.
3. Start the service and confirm one bounded lease cycle, zero restart growth,
   and no partial snapshot commit.

If the Jobs API regresses:

1. Stop `bluey-jobs-api.service`.
2. Restore the verified pre-Round-556 API binary.
3. Start the service and verify health, authentication, private-route hiding,
   and the three disabled execution flags.
4. Restore the database backup only for a proven schema/data rollback need and
   only together with the matching API/runtime contract.

## Remaining boundary

Bluey can continuously show hundreds of relevant public-feed candidates, and
it can enroll bounded public Greenhouse, Lever, Ashby, SmartRecruiters, and
Workday sources. It does not claim every portal is automatically searchable or
submittable. A production global index, licensed aggregators, additional
provider-specific adapters, resilient browser distribution, and irreversible
submission recovery remain separately gated work.

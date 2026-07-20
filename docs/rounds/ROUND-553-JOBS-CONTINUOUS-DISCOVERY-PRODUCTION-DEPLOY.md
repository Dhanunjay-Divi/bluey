# Round 553: Jobs Continuous Discovery Production Deploy

Date: 2026-07-20

## Outcome

Bluey Jobs now lets a signed-in user search a bounded company directory and
enroll public employer boards for continuous discovery across Greenhouse,
Lever, Ashby, SmartRecruiters, and Workday. The production discovery worker
checks enrolled sources on schedule, commits complete snapshots through the
private worker API, and lets the existing server-owned freshness, eligibility,
deduplication, and Career Track policy decide which jobs may appear.

This is continuous public ATS discovery, not unrestricted portal scraping.
External directory entries are discovery hints only. Every posting is read from
and revalidated against the employer's original public ATS before Bluey can
treat it as current. This round did not add access-control bypasses, CAPTCHA
bypass, stealth/proxy behavior, LinkedIn/Indeed/ZipRecruiter/Dice scraping, or
visual-only Submit behavior.

The deployed source commits are:

- `271156b4` - Career Track, eligibility, category, evidence, and five-ATS
  source-policy P0;
- `aaa5dab2` - authenticated company catalog and continuous source enrollment;
- `c9ff7c08` - strict raw-versus-trusted catalog validation.

Model generation, local Browser distribution, and cloud Browser distribution
remain disabled. The signed Bluey terminal `0.1.104` release and its native
artifacts were not replaced.

## Source Catalog Validation

The owner-supplied `kalil0321/ats-scrapers` link redirects to Jobhive. Round 552
records the exact reviewed commit, license, reused directory fields, and
excluded scraper/evasion material.

The first production catalog request exposed an important upstream-data detail:
the manifest counts raw CSV rows, while a small number of source identifiers do
not pass Bluey's stricter provider grammar. The deployed validator now:

- checks the manifest count against raw rows;
- validates every row against the provider-specific source grammar;
- rejects a catalog with no trusted rows;
- rejects more than 100 or more than 2 percent filtered rows;
- records only aggregate rejection counts and never logs catalog content.

Production validation accepted:

| Provider | Raw rows | Trusted rows | Rejected |
| --- | ---: | ---: | ---: |
| Greenhouse | 4,966 | 4,966 | 0 |
| Lever | 2,113 | 2,083 | 30 |
| Ashby | 2,856 | 2,855 | 1 |
| SmartRecruiters | 2,214 | 2,212 | 2 |
| Workday | 2,604 | 2,604 | 0 |

No rejected identifier can become a Bluey discovery source.

## Production Worker

The production worker runs as `bluey-jobs-discovery.service` beside the
loopback-only Jobs API. The checked-in example unit and `jobs/OPERATIONS.md`
document the deployment without exposing worker credentials.

The unit now includes both `Requires=bluey-jobs-api.service` and
`PartOf=bluey-jobs-api.service`. This fixes a lifecycle issue discovered during
deployment: stopping the Jobs API previously stopped the worker but did not
bring it back after an API binary swap. Jobs API maintenance now propagates the
restart, and operators must verify both services before reopening ingress.

The root-managed worker override contains only loopback origin, worker identity,
and polling configuration. The signing key remains in the shared root-managed
Jobs environment. The production listener is not exposed through the public
hostname.

Production source activity after deployment:

- seven completed discovery runs;
- ten jobs discovered and ten canonical jobs upserted;
- zero jobs closed during the observed window;
- ten active source memberships and four pending memberships;
- all observed Ashby and Lever sources healthy with zero failures.

## Live Product Verification

The live signed-in portal was exercised in Chrome at desktop and mobile sizes.
The company picker returned provider-specific results for `Apex`, including
Workday, Lever, SmartRecruiters, Greenhouse, and Ashby entries. Search, provider
labels, connected state, close behavior, and responsive stacking worked without
console errors or horizontal overflow.

The Matches summary counts only current visible matches after freshness,
availability, pass, and view filters. Career Track tabs retain their broader
record counts, including records filtered from the current verified view.

Evidence:

- `docs/rounds/assets/ROUND-553-JOBS-CONTINUOUS-DISCOVERY-PRODUCTION-DEPLOY/catalog-desktop.jpg`
- `docs/rounds/assets/ROUND-553-JOBS-CONTINUOUS-DISCOVERY-PRODUCTION-DEPLOY/catalog-mobile.jpg`

## Verification Matrix

Passed before production settlement:

- 722 Rust server library tests;
- strict Rust Clippy across all targets;
- 389 Jobs package tests across automation, Browser, runner, workflows, and
  portal;
- Jobs portal typecheck and production build;
- PostgreSQL schema parity;
- privacy, provenance/license, client-boundary, edge-policy, and CI guard tests;
- desktop and 390 by 844 mobile live visual checks;
- no live browser console errors or warnings;
- `git diff --check`.

The production services were active with zero restarts after final settlement:

- `bluey-api`, PID `2257183`;
- `bluey-jobs-api`, PID `2266707`;
- `bluey-jobs-discovery`, PID `2268579`;
- Caddy, PID `2217438`.

The deployed Jobs health payload reports exact source commit
`c9ff7c083aa820beac7c9a93d93b87f8eb652113`.

## Artifact And Safety Evidence

Deployed hashes:

- Jobs API binary:
  `94333730f619337c7626d91c48addbc33d8c7cacac24c522c6453a82299f74ab`;
- unchanged main API binary:
  `484545f3c75894932d9a55ebe04791d50e0ec9e2f66802767cc70b16458e99e5`;
- Jobs portal index:
  `69b283a2cc9842c7953b6f2098622a6e4e2af6e90602d627359e86d7407e68b3`;
- discovery systemd unit:
  `3de27ea503636c3b8a6761da0413bb8969668aa281ae350f3b7287b911b777aa`;
- deployed source archive:
  `89e870dd1595260dc5cadf98001ff5e68e5573c733bbc69f69b2ef5b12597d9e`.

Safety flags remained:

```text
BLUEY_JOBS_MODEL_GENERATION_ENABLED=0
BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED=0
BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED=0
```

Edge and access checks passed:

- browser and native-client `/health`: `200`;
- `/auth/captcha/config`: `200`;
- unsigned `/account/me`: `401`;
- unauthenticated `/api/jobs/workspace`: `401`;
- public `/api/jobs/internal/discovery/lease`: `404`;
- GPTBot request to `/jobs/`: `403`;
- direct-origin HTTPS bypass: failed.

## Backup And Rollback

Verified pre-deploy PostgreSQL backup:

- `/var/backups/bluey-api/hourly/bluey-postgres-pre-round552-20260720T182134Z.pgdump`
- size: `24,876,393` bytes
- SHA-256:
  `b6b80fcae4beb9a0bb2d377ff95a54cd480cdf2afa62690e4f0cc60114e762e9`

Previous Jobs API binary:

- `/var/backups/bluey-api/bin/bluey-jobs-api-before-c9ff7c08-20260720T194304Z`
- SHA-256:
  `94083cf96f79de4753bf9b02e327a11ef41d43137c7b76e94d78d660b8b3107a`

The earlier paired Jobs portal backup is
`/var/www/bluey/backups/jobs-before-aaa5dab2-20260720T183416Z.tar.gz`
with SHA-256
`f216cd94877834f68fe444163456f0d2a00daf33d0e72de9f276ea1ba2559fb`.

Prefer a fix-forward after source enrollment begins. A rollback must deliberately
pair the prior Jobs binary, static portal snapshot, and database backup when
schema or authority state requires it. After restoring the Jobs API, verify
both `bluey-jobs-api.service` and `bluey-jobs-discovery.service` are active
before reopening traffic.

## Remaining Product Boundary

Bluey now has a production continuous-discovery foundation for enrolled public
boards across five ATS families. It does not yet claim universal portal
coverage or unattended employer-facing automation. Broader candidate feeds may
be added only as leads with explicit provenance, bounded readers, canonical
deduplication, and original-source revalidation. Employer-facing Browser work
remains review-first and undistributed until its separate safety and runtime
gates pass.

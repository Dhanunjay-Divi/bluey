# Round 567: Jobs Auto-submit Availability Manual Production Deploy

Date: 2026-07-23

## Goal

Deploy Round 566 without GitHub Actions, prove the server-authoritative
Auto-submit availability contract in a signed-in production account, and leave
an exact rollback path.

This release explains why Auto-submit is unavailable. It does not distribute a
local or cloud runner and does not enable model-generated resume tailoring.

## Source

- implementation commit:
  `e569e4062576ae5c1c83ed5e5986b78a55f68d79`
- branch: `codex/jobs-one-week-launch-20260723`
- production source archive:
  `/tmp/bluey-e569e406.tar.gz`
- source archive SHA-256:
  `5ad6e528b3457abf4571956b5834b01d12f59524be19d9aa0882b6d01836fbd5`
- production portal archive:
  `/tmp/bluey-jobs-portal-e569e406.tar.gz`
- portal archive SHA-256:
  `b6700195f1281198ec93be3eaeb4c4c502b364455815df3f6ac60c1d1420d4c9`

No GitHub Actions, native installer, overlay, audio, meeting-runtime, or desktop
release was used or changed.

## Production Artifacts

The Jobs API was built from the source archive on the production builder:

```text
source: /opt/bluey-build-jobs-round566-e569e406
target: /opt/bluey-cargo-target-round566-e569e406
binary: /usr/local/bin/bluey-jobs-api
binary SHA-256: 124da4c7b7a2ca47fd4952aa3fc7471bb9d61f40034b50c329cc25bbc2f605a5
```

The deployed portal is:

```text
directory: /var/www/bluey/jobs
index SHA-256: bb60cf67bcfa6a17190a74d3449953d158b7c932a440397be6f7833965b4bc21
main asset: /jobs/assets/index-Kvlxmzgg.js
file count: 27
source maps: 0
```

## Build Metadata Correction

The first API build embedded a shortened compile-time commit label
`e569e406257625...`. A direct diff of the archived runtime and portal source
against the committed tree showed no source drift. The mismatch was limited to
`BLUEY_GIT_COMMIT`.

The API was rebuilt with:

```text
BLUEY_GIT_COMMIT=e569e4062576ae5c1c83ed5e5986b78a55f68d79
```

The replacement binary embeds and reports the exact implementation commit.
This was a metadata-only rebuild from the already audited source archive.

## Runtime State

After the corrected binary swap:

```text
bluey-jobs-api MainPID: 2451031
bluey-jobs-api NRestarts: 0
Caddy MainPID: 2217438
Caddy NRestarts: 0
```

Loopback Jobs health reports:

```json
{
  "status": "ok",
  "version": "0.1.5",
  "commit": "e569e4062576ae5c1c83ed5e5986b78a55f68d79",
  "platform": "linux-x86_64"
}
```

The three protected production flags remained off:

```text
BLUEY_JOBS_MODEL_GENERATION_ENABLED=0
BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED=0
BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED=0
```

The post-swap journal contained no warning or error entries. The API reported
the expected 12 migrations, no reconciliation work, no spend cleanup, and
listened only on `127.0.0.1:8081`.

## Public Verification

The Cloudflare-routed production checks passed:

| Check | Result |
| --- | --- |
| `/jobs/` | `200`, TTFB `0.093292s`, total `0.094789s` |
| `/jobs/assets/index-Kvlxmzgg.js` | `200`, TTFB `0.050855s`, total `0.084616s` |
| `/health` with the native client user agent | `200` |
| `/auth/captcha/config` | `200` |
| unauthenticated `/api/jobs/workspace` | `401` |
| public `/api/jobs/internal/discovery/lease` | `404` |
| missing `/jobs/assets/does-not-exist.js.map` | `404` |
| GPTBot request to `/jobs/` | `403` |
| Jobs indexing header | `X-Robots-Tag: noindex, nofollow, noarchive, nosnippet` |
| public edge | `Server: cloudflare` |

The main `/health` route belongs to the main Bluey API and therefore reports
that service's own commit. Exact Jobs source identity is verified on the
loopback Jobs health endpoint.

## Signed-in Production QA

The signed-in production account rendered:

- `$13.53` shared balance;
- `1185` verified matches;
- `51%` average fit;
- `1634` Career Track matches;
- `7 of 9` healthy discovery sources.

The `Software Engineer, Product` Ashby role at Fluidstack showed the complete
reason chain:

- `Beta - Review first`;
- choose a verified application identity;
- employment type is not selected;
- the Career Track does not allow internship;
- the role has not yet been verified as open;
- sponsorship support needs confirmation;
- the ATS is beta and requires packet review.

`Review first` remained usable, `Auto-submit` was disabled, and the primary
action changed to `Blocked by your rules`. This proves the control no longer
appears silently broken: the server-owned reason and the next usable path are
visible before preparation or queueing.

Desktop and mobile QA from Round 566 remain applicable to the identical portal
bundle deployed here.

## Rollback Evidence

The initial release rollback artifacts are:

```text
/var/backups/bluey-api/bin/bluey-jobs-api.before-e569e406-20260723T082354Z
SHA-256: beab5dedb46854dbad9547edb6f46e062048dbdbacd2b70c004d6ce99a8bceeb

/var/www/bluey/backups/jobs-before-e569e406-20260723T082354Z.tar.gz
SHA-256: efbd95dc3836c125f5d0cb6a7f07fdcf26789d3787cb0597d7aeff6a525e24c8

/var/www/bluey/jobs.pre-round566-20260723T082354Z
```

The metadata-correction rollback artifact is:

```text
/var/backups/bluey-api/bin/bluey-jobs-api.before-round566-metadata-20260723T084846Z
SHA-256: 8c84e3a5b394d835a831cc365a9767db25bca43ed413cfb2cd5f394ce09b6140
```

Rollback procedure:

1. restore the previous Jobs API binary;
2. restore the previous Jobs portal directory or archive;
3. confirm the three protected Jobs flags remain `0`;
4. restart only `bluey-jobs-api`;
5. verify loopback Jobs health, public portal `200`, unauthenticated workspace
   `401`, private discovery `404`, missing assets `404`, and the signed-in
   review-first flow.

## Launch Truth

Round 566 is live and suitable for a staged review-first beta. It is not proof
that unattended local or cloud submission is launch-ready.

Before enabling either runner, the owner must provide:

1. two or three authorized Greenhouse test vacancies and two or three
   authorized Lever test vacancies;
2. the canary account IDs, eligible plans, daily limit, and concurrency cap;
3. the named support/on-call owner for stuck and uncertain-submit runs;
4. the week-one runner choice: local, cloud, or neither;
5. signed local Browser installers if local distribution is selected, or an
   authenticated Temporal/browser pool and takeover endpoint if cloud is
   selected;
6. Gmail/Outlook OAuth approval only if launch copy includes outcome tracking.

Until those inputs and canary tests exist, Bluey remains honest: customers can
discover jobs, prepare and inspect application kits, download or hand off
packets, and see the exact reason unattended submission is unavailable.

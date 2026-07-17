# Round 538 - Jobs Locked Bundle Manual Production Deploy

Date: 2026-07-17

Repository: `/Users/uno/Downloads/cue-round536-mainline-refactor`

Source branch: `main`

Deployed source commit: `bf344fe8b02c71dd0e08b1a40f59dff9e2083198`

## Objective

Publish the current Bluey Jobs portal from the locked dependency graph after the
Round 536 source split, without rebuilding or restarting runtime components that
have no source delta. Preserve the signed native release, the main API, the Jobs
API, Caddy, Cloudflare controls, and old content-addressed browser assets.

## Release Decision

The production main API already ran source `e10192bb4919f0cd1ea6dd1e2280c6f0c8b65f07`.
The only later mainline change before this release was evaluator-only commit
`06cf807ba822c20a8d7699c0beb1714b050cef73`; there was no server runtime delta.
The Jobs server also had no source delta requiring a new binary. The release was
therefore deliberately static-only.

The following were not rebuilt, restarted, or replaced:

- `/usr/local/bin/bluey-server`
- `/usr/local/bin/bluey-jobs-api`
- Caddy configuration
- Cloudflare configuration
- `/latest.json`, `/latest.json.sig`, installers, or release artifacts

## Locked Build Verification

Dependencies were installed from `jobs/package-lock.json`, then the current main
source was verified with:

```text
npm ci
npm run typecheck
npm test -- --run
npm run build
```

Results:

- TypeScript typecheck: passed
- Vitest: 8 files, 37 tests passed
- Vite production build: 2,278 modules transformed
- A second clean build produced the same checksum manifest
- `git diff --check`: passed

The reproducible bundle was committed before deployment as:

```text
bf344fe8 Rebuild Jobs bundle from locked dependencies [skip ci]
```

## Published Assets

```text
web/jobs/index.html
  4036255d17e51b359005ecdac9af67fec3e0210ae19767bdff2a3aa369091393

web/jobs/assets/index-DiTCwTOq.js
  49b38b83048c4b10db488c327db2bf236349c1bad6035eabb8f0b4825de73d0c

web/jobs/assets/index-D6l0iYOp.css
  51509e755f6b0553179dd550ef57c6ade9546874c8a9f82442aaa06c785c6270
```

Assets were copied without `--delete`, so users with an already-open tab can
still load the previous hashed entry point. `index.html` was uploaded to a
temporary path and moved into place only after all new assets were present.

## Backup And Rollback

Pre-deploy backup:

```text
/var/www/bluey/backups/jobs-before-bf344fe8-20260717T223244Z
```

It contains the prior `index.html`, its checksum, the prior entry asset name, and
the deployed source identifier. The old assets remain in the production asset
directory.

Rollback requires only restoring the prior entry point:

```bash
cp /var/www/bluey/backups/jobs-before-bf344fe8-20260717T223244Z/index.html \
  /var/www/bluey/jobs/index.html
chmod 644 /var/www/bluey/jobs/index.html
```

No service restart is required for this rollback.

## Live Verification

Production URL: `https://bluey.sh/jobs/`

HTTP and edge checks:

| Check | Result |
| --- | --- |
| Jobs entry page | `200` |
| New JS entry asset | `200` |
| Previous JS entry asset | `200` |
| Missing source map | `404` |
| `/health` with `bluey-cloud-client/0.1.102` | `200` |
| `/auth/captcha/config` | `200` |
| Unsigned `/account/me` | `401` |
| Unsigned `/api/jobs/workspace` | `401` |
| Public `/api/jobs/internal/discovery/lease` | `404` |
| `/llms.txt` | `410` |
| Jobs page with GPTBot user agent | `403` |
| Direct-origin HTTPS bypass | blocked |

Response headers on `/jobs/` retained CSP, HSTS, frame denial, MIME sniffing
protection, strict referrer policy, and:

```text
X-Robots-Tag: noindex, nofollow, noarchive, nosnippet
```

Cloudflare may add its browser telemetry script to edge-served HTML, so the
authoritative byte-for-byte release hash is the origin file hash above. The
content-addressed JavaScript body matched the local build through Cloudflare.

## Browser QA

The local locked build and the live deployment were checked at:

- Desktop: 1280 px viewport
- Mobile: 390 x 844 px viewport

Verified:

- Matches view lazy-loads and renders fully
- Header and mobile bottom navigation remain usable
- Career Track, source-health, metrics, and match content render
- No horizontal overflow at the mobile viewport
- No broken images
- Light/dark-capable Jobs shell remains intact

## Runtime Preservation Evidence

After the static switch:

```text
bluey-api       active, NRestarts=0
bluey-jobs-api  active, NRestarts=0
caddy           active, NRestarts=0
```

Unchanged production binaries:

```text
/usr/local/bin/bluey-server
  ef3933cb11180b95602bbdabca1008ae27defbabc6a6b6b6a9c86ffe76015b33

/usr/local/bin/bluey-jobs-api
  f63745421f0200f76e32d099d4ed516d0f2098fc13fcd32184b0c9111c089688
```

The signed native release remains `0.1.102`, released
`2026-07-16T23:49:35Z`; its manifest and artifacts were not republished.

## Outcome

The current Jobs source split is represented by a deterministic, committed
browser bundle on `main` and is live in production. The release did not disturb
runtime services, signed native artifacts, auth boundaries, crawler controls, or
origin protection.

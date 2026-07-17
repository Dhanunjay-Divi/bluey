# Round 535 - Jobs candidate profile and execution manual production deploy

Date: 2026-07-17

Status: deployed and verified

## Outcome

The Round 534 Bluey Jobs candidate-profile and approved-execution release is
live at [https://bluey.sh/jobs/](https://bluey.sh/jobs/).

The release includes staged PDF, DOCX, and TXT resume import, candidate identity
separation, complete onboarding editors, typeahead suggestions, candidate
feedback events, immutable approved application packets, execution checksum
guards, and receipt validation.

The deployed Jobs source commit is:

`722e6cfe9b0f43e86be4bb263094b035a30109e8`

That commit also wires the candidate-event PostgreSQL migration into the runtime
migration list. It is an ancestor of the current `origin/main`, which advanced
during deployment with an independent main-API answer-contract change. The
concurrent main API was preserved and was not overwritten.

GitHub Actions was not invoked. The signed native release, meeting overlay,
audio runtime, Caddy configuration, and Cloudflare configuration were not
changed.

## Migration correction

Predeployment review found that
`infra/postgres/server-runtime/005_jobs_candidate_events.sql` existed but was not
included in `POSTGRES_MIGRATIONS`. The release added the PostgreSQL target marker,
included migration 005 in `server/src/db/mod.rs`, and added a regression test that
requires candidate events to remain part of the runtime migration set.

The production migration runner completed successfully and idempotently. The
`jobs_candidate_events` table exists and the migration ledger contains its
applied record. Jobs API startup reports five runtime PostgreSQL migrations.

## Verification before deployment

Jobs package verification on the exact candidate:

| Package | Tests |
| --- | ---: |
| Automation | 133 |
| Browser | 34 |
| Runner | 50 |
| Workflows | 34 |
| Portal | 37 |
| Total | 288 |

All five TypeScript packages typechecked and built. The portal production build
passed.

Focused server verification:

- 10 Jobs library tests passed;
- 15 Jobs HTTP integration tests passed;
- the migration-list regression passed;
- `cargo fmt --check` passed; and
- strict Clippy with `-D warnings` passed.

The two owner-authorized resume documents were tested locally through the real
import parser and browser upload path before deployment. Owner documents and
personal contents were removed after verification and were not committed.

## Source build

The production build used a source archive made from exact commit `722e6cfe`.

| Artifact | SHA-256 |
| --- | --- |
| Source archive `/tmp/bluey-jobs-722e6cfe.tar.gz` | `54fc3fffc2f5a48cdd99c2383ac0475d641104efd599ad4c995650f2deb950a6` |
| Jobs API `/usr/local/bin/bluey-jobs-api` | `f63745421f0200f76e32d099d4ed516d0f2098fc13fcd32184b0c9111c089688` |

The archive hash matched before and after transfer. The existing production Rust
toolchain at `/root/.cargo/bin/cargo` was used because the system Cargo 1.75
cannot read the repository's version-4 lockfile. No lockfile or dependency
metadata changed during the build.

## Published portal artifacts

| Artifact | SHA-256 |
| --- | --- |
| Jobs `index.html` | `400a69181b4cdf035e1b4ec355077eddcfe2ff58da31760be7edc3a652337e70` |
| `assets/index-nIj5O4xg.js` | `c0d8e72dad6165cfffce96d19e990f29720b6ca373c6d06eb9fac81024bce829` |
| `assets/index-D6l0iYOp.css` | `51509e755f6b0553179dd550ef57c6ade9546874c8a9f82442aaa06c785c6270` |

Hashed assets were published before the HTML entrypoint, and the entrypoint was
moved into place atomically. Older hashed assets remain available for already
open sessions. No source maps were published.

## Backup and rollback

A fresh PostgreSQL backup was captured before deployment:

`/var/backups/bluey-api/hourly/bluey-postgres-20260717T082509Z.pgdump`

Its SHA-256 is:

`62b3964023788364026040e9aa087d4514b1c8b67a0692759d1dd9a36a6f4d8c`

The backup checksum passed and `pg_restore -l` listed 362 entries.

The prior Jobs binary is preserved at:

`/var/backups/bluey-api/bin/bluey-jobs-api.before-722e6cfe-20260717T082509Z`

The prior portal is preserved at:

`/var/www/bluey/backups/jobs-before-722e6cfe-20260717T082509Z`

Rollback is:

1. restore the saved Jobs binary to `/usr/local/bin/bluey-jobs-api`;
2. restart only `bluey-jobs-api` and require local `/health` to pass;
3. atomically restore the saved portal `index.html`;
4. retain both old and new hashed assets until open sessions age out; and
5. restore the PostgreSQL backup only if an explicit data rollback is required.

Migration 005 is additive and its write paths are backward compatible, so a
normal binary/portal rollback does not require dropping the table.

## Live verification

| Check | Result |
| --- | --- |
| Jobs local `/health` | `200`, exact commit `722e6cfe9b0f43e86be4bb263094b035a30109e8` |
| `/jobs/` | `200`, committed entrypoint and asset hashes match |
| Native-client `/health` | `200`, no JavaScript challenge |
| `/auth/captcha/config` | `200`, Turnstile configuration available |
| Unauthenticated `/api/jobs/workspace` | `401` |
| Public `/api/jobs/internal/discovery/lease` | `404` |
| Missing Jobs source map | `404` |
| `/llms.txt` | `410` |
| GPTBot request to `/jobs/` | `403` |
| Direct HTTPS origin request | timed out before an HTTP response |
| Direct HTTP origin request | `308` redirect only; no protected body |
| Bluey API, Jobs API, and Caddy | active with zero restarts |
| Jobs API listener | loopback only on `127.0.0.1:8081` |
| Jobs API warnings after restart | none |
| Root disk | 65% used, 21 GB free |

The main Bluey API advanced concurrently to deployed source
`cc2075a1457542eeef6dfcc1aaf3b97fc85f3298`. It remained healthy on
`127.0.0.1:8080` and was deliberately left untouched. The Jobs deployment did
not replace or restart it.

## Signed-in browser QA

Chrome was used with the owner's existing signed-in Bluey session. The live
onboarding flow rendered the existing saved Career Profile, work-history
editors, setup navigation, and exit path.

Desktop and 390 by 844 mobile checks confirmed:

- onboarding cards and controls remain inside the viewport;
- the desktop grid and mobile single-column layout align correctly;
- fields retain readable labels and stable dimensions;
- no horizontal overflow appears; and
- the browser console contains no warnings or errors.

Existing saved profile data is not silently reparsed or rewritten by a portal
deployment. The corrected parser applies when a user stages a fresh import and
reviews the proposed replacement or blank-field merge.

## Launch boundary

This remains a staged, review-first Jobs beta. The deployed release strengthens
candidate data and makes approved packets immutable, but it does not certify
unattended submission on every employer site. Production Temporal workers,
signed local/cloud browser delivery, live ATS-family certification, and
Gmail/Outlook outcome workers remain explicit operational gates.

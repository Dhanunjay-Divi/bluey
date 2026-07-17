# Round 532 - Jobs PDF resume parser manual production deploy

Date: 2026-07-16

Status: deployed and verified

## Outcome

The Round 531 PDF resume parser is live at
[https://bluey.sh/jobs/](https://bluey.sh/jobs/).

The deployed portal adds production-path PDF coverage for compact resumes with
inline employer/title rows, inline education/date rows, grouped skills,
projects, bare GitHub links, wrapped bullets, and hyphenated line wraps. The
implementation and generated portal bundle are from mainline commit
`31fd8e839cb7`. Main subsequently advanced to `ac8c750c8d68` with independent
managed-answer work; `31fd8e83` is an ancestor, so both workstreams are present.

GitHub Actions was not invoked.

## Published artifacts

| Artifact | SHA-256 |
| --- | --- |
| Jobs portal `index.html` | `0feb66b0cfb4b5ed54c07d0c03c27a1f8369d04008d079d414f31971808c57c7` |
| `assets/index-CTi78EG3.js` | `d27584b4bd12624407a4512880acc3d139f7eb71a37d977c39e85d907f657132` |
| `assets/index-i3LD_Cjz.css` | `8e88dda86b202f0a21ee41f9ffc9ce198c3d0230ee75ceeeba2a695cf411daeb` |

Assets were synchronized before the HTML entrypoint. The entrypoint was then
moved into place atomically. Older hashed assets remain available for already
open browser sessions.

## Backup and rollback

The complete prior Jobs portal is preserved at:

`/var/www/bluey/backups/jobs-before-31fd8e839cb7-20260717T023917Z`

The saved prior `index.html` SHA-256 is:

`183c9d4e636fd2f1b98ef5c8693512647bbad9a08a2cc1c1b23474fd9516021a`

Rollback requires atomically restoring the saved `index.html` to
`/var/www/bluey/jobs/index.html`, then confirming its referenced assets return
`200`. The newer assets should remain until existing sessions have aged out.
No API or database rollback is required.

## Verification before deployment

- exact owner-provided PDF passed the production-equivalent browser import path;
- extraction matched all three jobs, two schools, three projects, 26 skills,
  location, LinkedIn, and GitHub in the source document;
- wrapped bullets were joined and dehyphenated;
- the committed regression uses a fictional identity and fictional employers;
- no owner resume, filename, personal text, or screenshot was committed;
- complete Jobs package suite: 249 tests passed;
- all Jobs TypeScript packages typechecked;
- portal production build passed;
- generated Jobs output contains no source maps;
- scoped `git diff --check` passed;
- local desktop and mobile layout checks passed.

## Live verification

| Check | Result |
| --- | --- |
| `/jobs/` | `200`, references the new JS and CSS |
| New portal JavaScript | `200`, public SHA-256 matches local artifact |
| New portal CSS | `200`, public SHA-256 matches local artifact |
| Native-client `/health` | `200`, main API unchanged |
| `/auth/captcha/config` | `200` |
| Unauthenticated `/api/jobs/workspace` | `401` |
| Public `/api/jobs/internal/discovery/lease` | `404` |
| Missing Jobs source map | `404` |
| `/llms.txt` | `410` |
| GPTBot request to `/jobs/` | `403` |
| Direct HTTPS origin request | blocked before an HTTP response |
| Direct HTTP origin request | `308` redirect only; protected content not served |
| Bluey API, Jobs API, and Caddy | active with zero restarts |
| Jobs API listener | loopback only on `127.0.0.1:8081` |
| Root disk | 57% used, 26 GB free |

Live browser checks at `1440x1000` and `390x844` showed no horizontal overflow,
clipped controls, console warnings, or console errors. The generated preview
used only synthetic profile data.

## Scope preserved

Only `/var/www/bluey/jobs/assets/` and `/var/www/bluey/jobs/index.html` were
published. The Jobs API, main Bluey API, Caddy, Cloudflare, native installers,
overlay, meeting/audio runtime, database, and workers were not changed.

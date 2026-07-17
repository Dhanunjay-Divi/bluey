# Round 530 - Jobs healthcare resume parser manual production deploy

Date: 2026-07-16

Status: deployed and verified

## Outcome

Round 529 is live at [https://bluey.sh/jobs/](https://bluey.sh/jobs/).
Production now includes corrected DOCX healthcare-resume extraction and compact
career suggestions for locations, roles, work history, skills, and
certifications.

The implementation source is mainline commit `eaa3933d88da`. GitHub Actions was
not invoked.

## Published artifacts

| Artifact | SHA-256 |
| --- | --- |
| Jobs portal `index.html` | `183c9d4e636fd2f1b98ef5c8693512647bbad9a08a2cc1c1b23474fd9516021a` |
| `assets/index-CcFPai-5.js` | `2297b2ccfea8df045c9ae509cc563bdc8f28883400d1e1e9b575700d11457b8f` |
| `assets/index-10kB5eYu.css` | `9cdf10ed28f26087cbca01d7215ef065bf9b888d88b9c662b547b1311542c2f2` |

Assets were synchronized before the HTML entrypoint. Older hashed assets remain
on the origin so already-open browser sessions continue to load safely.

## Backup and rollback

The complete prior Jobs portal is preserved at:

`/var/www/bluey/backups/jobs-before-eaa3933d88da-20260717T015655Z`

Portal rollback:

1. Restore the saved `index.html` atomically to `/var/www/bluey/jobs/index.html`.
2. Confirm its referenced hashed assets return `200`.
3. Keep the new assets in place until existing sessions have aged out.

No database or API rollback is required because this deployment did not replace
the Jobs API or main Bluey API.

## Verification

Before publish:

- complete Jobs package suite: 248 tests passed;
- all Jobs TypeScript packages typechecked;
- portal production build passed;
- exact owner-provided DOCX passed a temporary local production-import test;
- committed regression fixture contains fictional identity and employer data;
- no source maps were generated;
- no owner-resume identity or filename was present in source or bundle;
- desktop and `390x844` mobile interaction checks passed without overflow or
  console errors.

After publish:

| Check | Result |
| --- | --- |
| `/jobs/` | `200`, references the new JS and CSS |
| New portal JavaScript | `200`, public SHA-256 matches local artifact |
| New portal CSS | `200`, public SHA-256 matches local artifact |
| Native-client `/health` | `200` |
| `/auth/captcha/config` | `200` |
| Unauthenticated `/api/jobs/workspace` | `401` |
| Public `/api/jobs/internal/discovery/lease` | `404` |
| Missing Jobs source map | `404` |
| GPTBot request to `/jobs/` | `403` |
| Direct HTTPS origin request | timed out; Cloudflare cannot be bypassed |

## Scope preserved

Only `/var/www/bluey/jobs/assets/` and `/var/www/bluey/jobs/index.html` were
published. The Jobs API, main Bluey API, Caddy, Cloudflare configuration, native
overlay, meeting/audio runtime, database, and signed macOS/Windows release were
not changed.

# Bluey.sh Link + Account Pages — 2026-06-02

## Summary

Implemented the self-service static web routes that do not require additional
operator secrets:

- `/` landing page is the product entry.
- `/link` signs in or creates an account, then calls `/auth/link/mint` and
  redirects to `bluey://link?code=...` for the desktop app.
- `/login` is a web sign-in alias that uses the same account shell.
- `/account` loads `/account/me` and `/account/usage`, shows balance and usage,
  starts a $30 reload through `/billing/checkout`, and lists recently synced
  cloud sessions through `/sync/sessions`.
- `/reload` uses the same account shell for hosted-checkout return states.
- `/verify-email` confirms `/auth/verify-email/confirm`.
- `/password-reset` starts and confirms `/auth/password-reset/*`.
- `/docs/disguise` explains low-profile controls, capture-excluded windows, and
  responsible use.

The implementation stays in `web/index.html` so Caddy's existing SPA fallback
can serve every route from `/var/www/bluey/index.html`.

## API Contracts Used

- `POST /auth/signup`
- `POST /auth/login`
- `POST /auth/link/mint`
- `POST /auth/device/approve`
- `POST /auth/verify-email/confirm`
- `POST /auth/password-reset/start`
- `POST /auth/password-reset/confirm`
- `GET /account/me`
- `GET /account/usage`
- `GET /sync/sessions?limit=8`
- `GET /sync/sessions/:session_id`
- `POST /billing/checkout`

## Session Storage and Continuation Review

Bluey is currently local-first:

- The daemon writes the active recording to private local
  `active-meeting.json`.
- Ended or switched recordings are archived as private JSON files under the
  local `meetings/` directory.
- Generated answer cards and richer dashboard response rows are stored in the
  local `sessions.db`.
- The native overlay session drawer is backed by `MeetingStore::all_meetings()`;
  users can open a previous local session, rename it, and continue from there.
- The CLI local path is `bluey sessions` and `bluey sessions --show <id>`.

Cloud sync is upload/list/show today:

- `bluey cloud sync` uploads sessions, transcript segments, cue responses,
  context artifacts, answer style, summaries, and RAG chunks in idempotent
  batches.
- The server stores account-scoped cloud sessions and exposes
  `/sync/sessions` plus `/sync/sessions/:session_id`.
- The account page now shows synced cloud sessions and can inspect the latest
  transcript, latest answer, attached context, and answer style.
- The managed answer path sends a `session_id`, so server-side RAG can boost
  current-session context and retrieve account memory.

Important remaining bridge:

- If a session is still present locally, the desktop overlay can continue it
  from the session drawer.
- If a session exists only in cloud, the current product can list/show it, but
  it does not yet hydrate the cloud bundle back into a local active
  `MeetingRecord`. The next product step is a desktop `cloud restore/open`
  command that downloads `/sync/sessions/:id`, writes the local meeting archive,
  imports response rows into `sessions.db`, refreshes the overlay session list,
  and opens that restored session.

## Deployment Notes

`ops/Caddyfile.example` already routes API paths to `bluey-server` and serves
all other paths through `try_files {path} {path}/ /index.html`, so no Caddy
change was needed.

Static files to deploy:

```bash
rsync -av web/ /var/www/bluey/
```

Then smoke:

```bash
curl -I https://bluey.sh/
curl -I https://bluey.sh/link
curl -I https://bluey.sh/login
curl -I https://bluey.sh/account
curl -I https://bluey.sh/docs/privacy
curl -I https://bluey.sh/docs/terms
curl -I https://bluey.sh/docs/disguise
```

## Still Depends On Operator Setup

- `bluey-server` running behind Caddy with correct `BLUEY_PUBLIC_URL`.
- Square sandbox/production env vars and webhook registration.
- SMTP env vars for verification/reset email delivery.
- Provider env vars for managed answers/STT/vision.
- Release artifacts hosted at `/install.sh` and `/releases/...`.
- Live browser smoke with a real account and one $30 sandbox checkout.
- Cloud restore/open from a cloud-only session into the active desktop session.

## Review Notes

This is intentionally a simple static alpha surface. A future web app can add a
proper account dashboard framework, usage charts, reload receipts, and
server-rendered support pages without changing the API contract. The account
page already has the first synced-session browser for alpha validation.

## Minimal Web Refresh

Follow-up pass simplified the public landing page into one calm product page:

- Hero explains Bluey, the tagline, and the `bluey on` entry point.
- Flow section explains listen/ask, add context, and continue later.
- Account section explains credits, managed routing, and session memory.
- Install section keeps the one-command setup.

Removed the old animated canvas initializer and the heavy mock app preview from
the public page. Account, login, link, recovery, policy, and session-browser
routes still share the same static file and API contracts.

## Pinky-Style Minimal Landing

Second follow-up pass tightened the public page further after comparing against
`https://pinky.sh/`:

- Single-viewport layout: small nav, left hero, right terminal-style preview.
- Removed the visible multi-section marketing stack from the public route.
- Kept only two primary actions: install and account.
- Added a tiny device-code link form and three compact proof cards.
- Preserved the current Bluey blue/cyan identity and all account/legal routes.

This is intentional. The public page should feel plain, fast, and easy to read;
the product detail now lives in the account, docs, and desktop app surfaces.

## Angular Wordmark Polish

Third follow-up pass added an original angular `BLUEY` wordmark beside the logo
in the shared brand header. The visual direction references sharp AI/security
wordmarks, but the implementation is CSS-only and Bluey-specific rather than a
copy of an external logo.

- Applies to the public nav, account rail, and legal/auth header brand spots.
- Keeps the existing terminal-logo icon as the product anchor.
- Uses clipped letter spans so the mark stays lightweight and scales on mobile.
- Verified against desktop and mobile screenshots plus route smoke for all
  static web routes.

# Bluey.sh Link + Account Pages — 2026-06-02

## Summary

Implemented the self-service static web routes that do not require additional
operator secrets:

- `/` landing page is the product entry.
- `/link` signs in or creates an account, then calls `/auth/link/mint` and
  redirects to `bluey://link?code=...` for the desktop app.
- `/login` is a web sign-in alias that uses the same account shell.
- `/account` loads `/account/me` and `/account/usage`, shows balance and usage,
  and starts a $30 reload through `/billing/checkout`.
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
- `POST /billing/checkout`

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

## Review Notes

This is intentionally a simple static alpha surface. A future web app can add a
proper account dashboard, usage charts, session browser, reload receipts, and
server-rendered support pages without changing the API contract.

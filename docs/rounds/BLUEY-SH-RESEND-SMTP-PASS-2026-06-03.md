# Bluey.sh Resend SMTP Pass — 2026-06-03

## What Changed

- Installed Resend SMTP settings into `/etc/bluey-api/bluey-api.env` on the
  production droplet.
- Restarted `bluey-api` and confirmed `https://bluey.sh/health` still returns
  healthy.
- Updated `ops/bluey-api.env.example` so future operators see the Resend SMTP
  shape instead of the older Postmark placeholder.

No Resend API key or SMTP password is stored in this repository.

## Runtime SMTP Shape

```text
BLUEY_SMTP_HOST=smtp.resend.com
BLUEY_SMTP_PORT=587
BLUEY_SMTP_USERNAME=resend
BLUEY_SMTP_PASSWORD=<resend-api-key>
BLUEY_SMTP_FROM=Bluey <noreply@bluey.sh>
BLUEY_SMTP_STARTTLS=true
```

## Namecheap DNS Records Needed

Add these records in Namecheap for `bluey.sh` and keep the existing A record
for the apex domain.

| Purpose | Type | Host | Value | TTL | Priority |
|---|---|---|---|---|---|
| DKIM | TXT | `resend._domainkey` | Public key from the Resend dashboard | Auto | |
| SPF / MAIL FROM | MX | `send` | `feedback-smtp.us-east-1.amazonses.com` | Auto | `10` |
| SPF | TXT | `send` | `v=spf1 include:amazonses.com ~all` | Auto | |
| DMARC | TXT | `_dmarc` | `v=DMARC1; p=none;` | Auto | |

## Current Verification Status

Public DNS does not yet return the Resend records:

```bash
dig +short TXT resend._domainkey.bluey.sh
dig +short MX send.bluey.sh
dig +short TXT send.bluey.sh
dig +short TXT _dmarc.bluey.sh
```

This means verify/reset email should not be considered production-verified yet.
Once DNS propagates and Resend marks the domain verified, run live smoke:

```bash
POST /auth/verify-email/start
POST /auth/password-reset/start
```

Pass criteria:

- Verification email arrives in under 30 seconds.
- Password reset email arrives in under 30 seconds.
- Links open on `https://bluey.sh/verify-email` and
  `https://bluey.sh/password-reset`.
- Resend dashboard shows DKIM/SPF/DMARC green.

## Security Note

Provider keys pasted into chat or any non-secret channel should be rotated
before production customer traffic.

# bluey-server

Production server for the v0.2 Bluey paid product. Mirrors Pinky's
operational shape (single binary + SQLite + Caddy + LetsEncrypt + droplet)
but written in Rust per the 2026-05-19 single-language-stack decision.

Lives in this repo at `server/` for now; will split to a separate
`bluey-server` repo once it stabilises.

## Endpoints (target)

```
GET  /admin/health           liveness
POST /auth/{signup,login,refresh}      account auth
POST /auth/device/{start,poll,approve} OAuth-style device flow opened by first `bluey on`
GET  /account/me                       license + plan + balance
GET  /account/usage                    rolling-7-day mix + tier projection
POST /router/complete                  managed LLM dispatch
POST /router/embed                     managed embedding dispatch
POST /router/transcribe                managed RIFF/WAV PCM STT (32 MiB, 15 min)
GET  /stt/live                         managed binary PCM16/16-kHz/mono WebSocket (20 min)
POST /usage/event                      per-call metering ingestion
POST /billing/checkout                 Stripe checkout session
POST /billing/webhook                  Stripe webhook handler
GET  /admin/customers                  Bluey-team only
```

Full spec: `../ARCHITECTURE.md`, `../docs/HOW-IT-WORKS.md`,
`../docs/PRICING-MODEL.md`.

## Local dev

```bash
export BLUEY_JWT_SECRET=$(openssl rand -hex 32)
export BLUEY_PORT=8080
export BLUEY_DB_PATH=./bluey-dev.db

# Optional. If unset, verification/reset flows log dev URLs instead.
export BLUEY_SMTP_HOST=smtp.resend.com
export BLUEY_SMTP_PORT=587
export BLUEY_SMTP_USERNAME=resend
export BLUEY_SMTP_PASSWORD=...
export BLUEY_SMTP_FROM="Bluey <hello@bluey.sh>"
export BLUEY_SMTP_STARTTLS=true

cargo run

curl http://127.0.0.1:8080/admin/health
```

## Status

🟡 **v0.2 prelaunch.** Auth, Square billing, managed routing, streaming
responses, embedding, transcription, cloud sync, account export/delete,
metrics, web account pages, and Resend-backed transactional email are
implemented and deployed on `bluey.sh`. Remaining launch gates are operational:
managed provider keys, live inbox/link confirmation, Square hosted-checkout
smoke, backups, and clean-Mac installer smoke.

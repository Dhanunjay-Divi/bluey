# Round 095 - Scalable Infra Deployment Pass - 2026-06-21

## Decision

Bluey should ship the same architecture we want for the first 100 paid users,
without asking users to install databases or provider keys:

- Desktop: UI, local session files, SQLite local history, local RAG cache.
- Bluey server: auth, billing, provider keys, managed AI/STT/vision, provider
  capacity ledger.
- Cloud data services: Postgres + pgvector for server source of truth,
  Valkey/Redis for shared provider capacity/rate state, and Cloudflare
  R2-compatible object storage for release artifacts, backups, support bundles,
  and synced raw artifacts.
- Providers: OpenAI, Anthropic, Gemini, and Deepgram through the server only.

Deepgram is explicitly part of the billing formula. Live captions are paid
upstream API usage and must reserve/settle against the wallet like LLM and
vision calls.

## Current Live Status

Checked from the Bluey droplet without printing secrets:

- `bluey-api` is active.
- Provider keys are configured on the server.
- `BLUEY_REDIS_URL` is configured and Redis responds, but it is droplet-local
  today.
- R2/S3-style backup variables are configured.
- `BLUEY_DATABASE_URL` is not configured.
- `BLUEY_SERVER_DB_BACKEND=postgres` is not configured.
- `psql` is not installed on the droplet.
- A later server build adds the Postgres runtime adapter foundation, but this
  live-status snapshot predates a production Postgres env flip.

This means the live system snapshot was good for single-server alpha readiness,
but not yet a true Postgres-backed scalable runtime.

## What Is Deployable Now

- Keep one API server on the DigitalOcean droplet.
- Use existing Redis/Valkey support for local provider capacity and cooldowns.
- Keep R2/off-host backup configuration active.
- Keep SQLite runtime until the managed Postgres/backfill/smoke cutover is
  completed.
- Use `scripts/bluey-scalable-readiness.sh` to prevent declaring a Postgres
  cutover complete too early.

## Required Before Claiming Fully Scalable Runtime

1. Finish/prove the Postgres runtime adapter behind the existing DB boundary.
2. Add/run SQLite-to-Postgres backfill/parity tooling.
3. Provision managed Postgres + pgvector.
4. Provision managed Valkey/Redis before adding a second API server.
5. Run migrations and preflight against the managed services.
6. Flip `BLUEY_SERVER_DB_BACKEND=postgres` only after the server binary accepts
   and tests that mode.
7. Run live paid smoke: signup, add credits, Listen, Answer, Screen, Docs,
   sessions, auto-reload, webhook crediting, low-balance stop, and
   refund/dispute limits.

## Non-Goals

- Do not use AWS just because the AWS CLI exists locally.
- Do not expose provider keys to the desktop.
- Do not call local BYOK/local-LLM paths part of the customer release.
- Do not mark a paid production cutover complete because infrastructure exists;
  the runtime adapter must be active and verified.

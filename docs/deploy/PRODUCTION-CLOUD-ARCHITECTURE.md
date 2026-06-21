# Bluey Production Cloud Architecture

Date: 2026-06-21

This is the concrete target architecture for paid Bluey. It keeps customer
installs simple while making the server side ready for the first 100 paid users
and a staged worldwide release.

## One-Line Decision

Normal users install only Bluey. They do not install Redis, Postgres, pgvector,
Docker, provider keys, or any cloud credentials.

```text
User laptop
  SQLite + local files + local RAG cache
  no provider keys
        |
        | HTTPS / WebSocket
        v
Bluey server
  Postgres + pgvector source of truth
  Valkey/Redis provider capacity ledger
  R2/S3 object storage for blobs/artifacts/backups
  provider keys and billing live only here
        |
        v
OpenAI / Anthropic / Gemini / Deepgram
```

For the controlled first-100 paid alpha, one DigitalOcean droplet plus SQLite is
acceptable only while the live smoke, off-host backups, provider funding, and
Square webhook tests stay green. The architecture contract below is the path out
of that single-node shape; it does not change the desktop contract.

## Customer Laptop Boundary

The desktop owns only local UX and local cache:

- overlay/pill UI and native capture helpers
- local session/transcript store
- local Markdown copies of attached readable files
- local RAG vector cache for fast per-device retrieval
- queued sync while offline
- account profile/tokens for Bluey, not provider keys

The desktop must not include:

- OpenAI, Anthropic, Gemini, Deepgram, Square, R2, Redis, or Postgres secrets
- BYOK/local-LLM release paths
- Redis/Postgres/Docker/pgvector installers
- cloud database migration logic

If the user uninstalls Bluey, the product should offer two explicit choices:

- **Keep history**: remove binaries and launch agents, leave local sessions,
  summaries, converted Markdown, and RAG cache in app data.
- **Purge local data**: remove binaries, launch agents, tokens, sessions,
  converted Markdown, captures, logs, and RAG cache.

Default retention should keep lightweight session summaries until user deletion.
Heavy captures, raw documents, and temporary artifacts should be pruned by size
and age. A reasonable default is 12 months for paid accounts, with explicit user
delete/export controls before wider launch.

## Server State

Postgres + pgvector becomes the durable server source of truth when we move past
single-node SQLite:

- accounts, email verification, auth refresh tokens, link/device codes
- wallet balances, reserved cents, credit batches, and expiration
- Square/Stripe customer/card/payment identifiers
- reload attempts and payment-success-only wallet credits
- usage events and billing ledger records
- STT reservations and settlement
- request idempotency and replay records
- cloud sessions, transcripts, answers, context artifacts, and tombstones
- cloud RAG chunks with `vector(1536)` embeddings
- audit log, export jobs, deletion jobs, dispute/refund flags

R2/S3 owns large blobs only:

- signed release artifacts, `latest.json`, `latest.json.sig`, install scripts
- database backups and restore drill artifacts
- support zips/log exports
- cloud exports
- synced raw documents/screenshots when the user opts into cloud sync
- optional retained audio chunks if we later keep audio

R2/S3 is never the source of truth for balances, idempotency, auth, provider
capacity, or vector search.

Valkey/Redis owns shared realtime coordination:

- provider/model/key cooldowns
- provider token/request buckets across all server instances
- optional short-lived request/session locks

For one server process, the in-process fallback is acceptable. Before a second
server instance handles live traffic, set:

```bash
BLUEY_REDIS_URL=rediss://...
BLUEY_REDIS_NAMESPACE=bluey-prod
BLUEY_RATE_LIMIT_REDIS_STRICT=1
```

## Provider Boundary

All model/STT calls go through Bluey server:

- OpenAI for fast/vision/embeddings as configured by router policy
- Anthropic for balanced/deep routes as configured by router policy
- Gemini for configured fallback/vision/long-context routes
- Deepgram for live STT relay

The desktop sends authenticated Bluey requests and receives streamed results,
captions, embeddings, and billing metadata. It never sees provider keys.

## Worldwide Release Shape

Do global downloads first, runtime regions later:

1. Serve install scripts, artifacts, and signed manifests from R2/Cloudflare.
2. Keep a single primary API region for the first paid users.
3. Measure typed-answer, audio-caption, and screen-analysis latency by region.
4. Add regional stateless STT/answer ingress when non-US latency is visibly bad.
5. Keep account/billing in one primary Postgres until the business truly needs
   multi-region writable data.

The first worldwide scale split is:

- Cloudflare/R2 for release artifacts and backups.
- Regional stateless gateways for STT WebSockets and answer streaming.
- Central Postgres for accounts/billing.
- Shared Valkey/Redis for provider capacity.
- Provider routing that selects the closest/healthiest allowed provider lane.

## First 100 Paid Users

For the first 100 paid users, the controlled-alpha stack can be:

- one DigitalOcean droplet
- Caddy
- `bluey-server`
- SQLite
- R2 off-host backups
- Redis unset unless multiple server processes are used

Before wider paid alpha:

- run `scripts/bluey-cloud-preflight.sh /etc/bluey-api/bluey-api.env`
- prove Square sandbox and low-dollar production webhooks return 2xx and credit
  only after processor payment success
- prove Deepgram live captions from a clean Mac
- prove OpenAI/Anthropic/Gemini answer/vision routes with funded accounts
- prove signed installer/update artifacts from `bluey.sh`
- run a restore drill from the off-host R2 backup

## Cutover Order To Postgres

Do not flip a fake `BLUEY_DATABASE_URL` until the runtime SQL backend exists.

1. Create managed Postgres 16+ with pgvector.
2. Apply the server-runtime compatibility migrations:
   `scripts/bluey-postgres-migrate.sh /etc/bluey-api/bluey-api.env`.
3. Write or enable the server SQL backend adapter against the
   `infra/postgres/server-runtime` schema.
4. Write a SQLite -> Postgres backfill that preserves account ids, payment ids,
   idempotency keys, usage ids, cloud session ids, tombstones, and RAG chunk ids.
5. Run dual-write or short maintenance-mode migration for billing/idempotency.
6. Compare row counts and ledger totals.
7. Run paid-alpha smoke against Postgres staging.
8. Promote only after Square webhooks, STT reservations, managed answers, RAG,
   export/delete, and admin support flows pass.

## Required Secrets And Services

Keep these in the server secret manager or `/etc/bluey-api/bluey-api.env`, never
in repo or desktop builds:

- `BLUEY_JWT_SECRET`
- `OPENAI_API_KEYS`
- `ANTHROPIC_API_KEYS`
- `GEMINI_API_KEYS`
- `DEEPGRAM_API_KEYS`
- `SQUARE_*_ACCESS_TOKEN`
- `SQUARE_*_WEBHOOK_SIGNATURE_KEY`
- `BLUEY_SMTP_PASSWORD`
- `BLUEY_REDIS_URL`
- `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` for R2 backup/object access
- future `BLUEY_DATABASE_URL`

## Preflight Profiles

Use profiles so we do not confuse a one-server alpha with a scaled production
shape:

```bash
BLUEY_PREFLIGHT_PROFILE=single-server-alpha \
  scripts/bluey-cloud-preflight.sh /etc/bluey-api/bluey-api.env

BLUEY_PREFLIGHT_PROFILE=multi-server \
  scripts/bluey-cloud-preflight.sh /etc/bluey-api/bluey-api.env

BLUEY_PREFLIGHT_PROFILE=postgres-cutover \
  scripts/bluey-cloud-preflight.sh /etc/bluey-api/bluey-api.env
```

`multi-server` requires managed Redis/Valkey and strict Redis behavior.
`postgres-cutover` additionally requires `BLUEY_DATABASE_URL` and a migrated
pgvector schema. It also requires `BLUEY_SERVER_DB_BACKEND=postgres`, so the
cutover profile cannot pass while the deployed runtime is still SQLite-backed.

## Current Repo State

- Desktop local SQLite/RAG cache exists.
- Managed `/router/embed` exists for local RAG embeddings through the server.
- Server Redis/Valkey hooks exist through `BLUEY_REDIS_URL`.
- Off-host backup script supports R2/S3-compatible destinations.
- Postgres/pgvector server-runtime schema is tracked in
  `infra/postgres/server-runtime`.
- Runtime server is still SQLite-backed until the SQL backend migration lands.

That last line matters. The architecture is ready to provision; the runtime DB
cutover is a deliberate follow-up, not an environment-variable trick.

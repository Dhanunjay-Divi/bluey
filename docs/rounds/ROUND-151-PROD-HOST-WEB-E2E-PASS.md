# Round 151 - Production Host And Web E2E Pass - 2026-06-23

## Scope

- bluey.sh static web routes, login/download/reload navigation, and asset loading.
- Caddy API routing and CSP.
- Production bluey-server binary parity with the current web dashboard.
- Postgres, pgvector, Valkey, Square, SMTP, R2 backup, and signed installer preflight.

## Changes

- Added `/account/billing`, `/account/devices`, and `/account/devices/*` to the live Caddy API matcher so dashboard JSON calls no longer fall through to `index.html`.
- Updated live Caddy CSP to allow Square Web Payments SDK script, frame, and tokenization connect endpoints.
- Added `/account/devices` and `/account/devices/*` to `ops/Caddyfile.example`.
- Fixed server build wiring for batch embeddings by exporting `embed_batch_with_key` and using the trace id string shape already extracted by router handlers.
- Built and deployed a new Linux `bluey-server` release binary to `/usr/local/bin/bluey-server` with backup `/usr/local/bin/bluey-server.bak-20260623T044343Z`.
- Updated `scripts/bluey-cloud-preflight.sh` to support multiple env files and skip SQLite path checks when Postgres backend mode is active.

## Verified

- `cargo test --manifest-path server/Cargo.toml`: 151 unit tests, 39 server integration tests, and doc tests passed.
- Production runtime environment:
  - `BLUEY_SERVER_DB_BACKEND=postgres`
  - `BLUEY_DATABASE_URL` set
  - `BLUEY_REDIS_URL` set
  - `BLUEY_REDIS_NAMESPACE=bluey-prod`
  - `BLUEY_RATE_LIMIT_REDIS_STRICT=1`
  - Square production billing selected.
- Production preflight with base env plus Postgres and Valkey env files passed with 0 warnings.
- Postgres connection succeeded, pgvector installed, `cloud_rag_chunks` present, and embedding column uses pgvector.
- Redis/Valkey ping succeeded.
- R2/S3-compatible backup destination reachable.
- Health endpoint reachable.
- Signed update manifest and signature reachable.
- Browser smoke:
  - Home page loads versioned CSS/JS.
  - Login nav click reaches `/login` and renders sign-in.
  - Download nav click reaches `/download` and renders macOS plus Windows coming soon.
  - `/reload` renders the unauthenticated sign-in flow.
  - Browser console has no errors.
- API route checks:
  - `/account/devices` returns API `401` when unauthenticated.
  - `/account/billing` returns API `401` when unauthenticated.
  - `/billing/checkout` returns API `401` when unauthenticated.
  - `/health` returns `200` JSON.

## Follow-Up

- The old Postgres-backed server process panics during systemd shutdown while dropping the sync `postgres` client pool, then the new process starts cleanly. Runtime is healthy after restart, but this confirms the adapter still needs the async Postgres pool or a stricter blocking/drop boundary before frequent rolling restarts.
- The hosted signed manifest is still `0.1.13` with only `darwin-arm64` advertised. This matches the current public Windows coming-soon stance.
- Square checkout header still uses business name because the Bluey Square location does not yet have a hosted logo URL configured.

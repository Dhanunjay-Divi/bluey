# End-to-End Readiness Review By Codex - 2026-06-21

Verdict: NOT READY FOR REAL PAID ALPHA until the live deployment catches up and the paid money/caption/screen path is smoked with real credits.

This review looked at the repo, automated tests, local release hygiene, the live droplet, deployed health/manifest endpoints, production env shape, production SQLite ledger aggregates, and recent live server logs. It does not claim every possible user path is proven; it identifies the highest-risk ways Bluey can still go wrong before tomorrow's live test.

## Findings

### P0 - Live API server is stale versus repo/release

`https://bluey.sh/health` is serving `version=0.1.5` and `commit=4937a4c`, while the public update manifest is `0.1.13` and the branch tip contains the newer Postgres/runtime/safety work. The paid-alpha smoke precondition requires the latest pushed code to be deployed to the droplet/site/release artifacts (`docs/deploy/PAID-ALPHA-SMOKE.md:8`), and the manual deploy script explicitly checks `/health`, `/latest.json`, `install.sh`, and `install.ps1` after publishing (`scripts/deploy-bluey-sh-manual.sh:38`).

Impact: local tests can pass while production still runs old billing, STT, auth, RAG, and provider behavior.

Required action: deploy the latest server binary/site/release artifacts manually, then re-run live health and paid-alpha smoke.

### P0 - Production is still single-server SQLite/local-Redis shape

The droplet env currently classifies as:

- `db_backend=sqlite`
- `database_url=missing`
- `redis_scope=local`
- `redis_strict=off`
- offsite backup destination present

The preflight script intentionally treats SQLite as acceptable only for single-server alpha (`scripts/bluey-cloud-preflight.sh:176`) and requires managed Redis/Valkey plus strict mode for multi-server/postgres-cutover profiles (`scripts/bluey-cloud-preflight.sh:210`, `scripts/bluey-cloud-preflight.sh:236`).

Impact: this is acceptable for a controlled one-server Mac alpha, but not for multi-server scale, worldwide latency, or strong shared provider-capacity guarantees.

Required action: either explicitly launch as one-server alpha, or provision managed Postgres/pgvector + managed Redis/Valkey and run the `postgres-cutover` preflight before increasing scale.

### P0 - Square webhook path still needs proof after the failed-delivery warning

Production DB aggregate check showed `stripe_webhook_events` has:

- `payment.updated`: 16 total, 12 unprocessed
- `order.updated`: 26 total, 0 unprocessed
- `notification.test`: 1 total, 0 unprocessed

The current webhook tracking table is still named `stripe_webhook_events` and records processor events there (`server/src/db/webhook_events.rs:37`). The paid-alpha smoke requires Square webhook logs to show verified signature and 2xx response, balance credit within 30 seconds, and idempotent replay behavior (`docs/deploy/PAID-ALPHA-SMOKE.md:41`).

Impact: the earlier Square failed-delivery email may be resolved by code, but production still has unprocessed payment events and the current live server is stale, so crediting cannot be considered proven.

Required action: after deploy, run Square sandbox webhook replay plus a low-dollar production reload; verify 2xx, idempotency, and balance movement in both Square and Bluey logs.

### P1 - Live managed cloud paths are partly proven, not fully proven

Recent live logs show a managed Anthropic `balanced` request completed and billed, and Deepgram STT relay sessions settled/refunded for microphone/system. That is good evidence that the path can work.

But the same live logs also showed repeated `/router/embed` 401 and `/auth/refresh` 401 around a trace, and this review did not complete fresh user smoke for OpenAI, Gemini, screen/vision, document attach, old-session load/delete, or web reload.

Impact: live captions/answer may work for some current accounts, but a clean account/install can still fail from stale tokens, stale server code, or an untested provider lane.

Required action: run the full clean-Mac paid-alpha smoke after deploy: install, sign in, add credits, Listen mic/system, Answer, Screen, Docs, sessions, balance.

### P1 - Windows install is advertised more than it is proven

Live `latest.json` currently lists only `darwin-arm64`. The prelaunch checklist says Windows paid users require a Windows artifact in the signed manifest and verified `bluey update` before install (`docs/PRELAUNCH-CHECKLIST.md:214`).

Impact: macOS-only alpha is fine, but Windows paid users should not be invited until the signed Windows artifact and install path are live and smoked from a real signed-in Windows desktop session.

Required action: keep the public Windows path clearly gated as coming soon, or publish and smoke a signed Windows artifact.

### P1 - Cloud memory/saved-session restore is not proven in production

Production DB currently has:

- `cloud_sessions=0`
- `cloud_rag_chunks=0`
- `request_idempotency in_progress=0`
- no expired unsettled STT reservations
- no negative-balance accounts

The zero stuck money/idempotency state is good. The zero cloud-session/RAG state means the saved cloud memory path has not been exercised in production.

Impact: local session history may work, but cloud sync/restore/RAG recall is still an unproven live path.

Required action: create, sync, reload, rename/delete, and ask-from-old-session in the paid-alpha smoke.

## Verification Run

Local repo:

- `cargo test --workspace --all-targets` passed.
- `cargo clippy --workspace --all-targets -- -D warnings` passed.
- `cargo fmt --all --check` passed.
- `git diff --check` passed.
- `scripts/check-server-sqlite-boundary.sh` passed.
- Shell syntax passed for deploy/preflight/smoke/release scripts.
- `scripts/release-hygiene-scan.sh` passed with expected dev-flag mentions in docs/scripts.

Live endpoints:

- `https://bluey.sh/health` returned 200 but stale `version=0.1.5`, `commit=4937a4c`.
- `https://bluey.sh/pricing/tiers` returned 200.
- `https://bluey.sh/latest.json` returned 200 and `version=0.1.13`.
- `https://bluey.sh/latest.json.sig` returned 200.
- `https://bluey.sh/install.sh` returned shell script.
- `https://bluey.sh/install.ps1` returned PowerShell.

Droplet:

- `bluey-api` active.
- `caddy` active.
- `/etc/bluey-api/bluey-api.env` present.
- `/var/www/bluey/latest.json.sig` present.
- Single-server preflight passed with warnings.
- Multi-server and postgres-cutover preflight profiles fail as expected until managed Redis/Postgres are configured.

Production ledger aggregates:

- `accounts=20`
- `negative_balance_accounts=0`
- `request_idempotency in_progress=0`
- STT expired/unsettled reservation count was 0.
- Deepgram usage and Anthropic usage exist.
- `payment.updated` has 12 unprocessed events.

## Go/No-Go

Go for internal Mac smoke only after deploying the current code and release artifacts.

No-go for paid alpha until:

1. Live `/health` matches the deployed commit/version.
2. Square reload webhook is proven green with balance crediting.
3. Fresh Mac install/sign-in/listen/answer/screen/docs/session/balance smoke passes without dev flags.
4. Provider routes are smoked with funded accounts and logs show request IDs, trace IDs, billing, and no key leakage.

No-go for Windows users until the signed Windows artifact is in `latest.json` and the Windows desktop session smoke passes.

## Most Likely Things To Go Wrong Tomorrow

1. The website/release manifest is fresh but the API server is stale.
2. A user adds credits but Square webhook replay/crediting is not actually green.
3. Listen starts but a stale token or permissions path prevents live captions.
4. Screen routes to vision but the active provider/model path is untested or unfunded.
5. Saved sessions appear locally but cloud sync/RAG recall is empty.

## Next Commands

After manual deploy:

```bash
curl -fsS https://bluey.sh/health
curl -fsS https://bluey.sh/latest.json
BLUEY_PREFLIGHT_PROFILE=single-server-alpha scripts/bluey-cloud-preflight.sh /etc/bluey-api/bluey-api.env
```

Then run `docs/deploy/PAID-ALPHA-SMOKE.md` exactly, with a fresh Mac install and a new/low-dollar test account.

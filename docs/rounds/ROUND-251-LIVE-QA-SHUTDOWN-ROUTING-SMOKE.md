# ROUND-251 Live QA Shutdown Routing Smoke

Date: 2026-06-30
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Goal

Run human-style live checks after the 0.1.17 deploy, verify logs and edge-case coverage, fix concrete issues found, redeploy, and rerun until the live path is acceptable.

## Fixed

- Fixed production Postgres shutdown panics by wrapping `postgres::Client` in a safe r2d2 connection type. If Axum drops route state from a Tokio worker, the sync Postgres client is dropped on a normal thread instead of starting a runtime inside a runtime.
- Corrected the actual live service deploy path. `bluey-api.service` runs `/usr/local/bin/bluey-server`; earlier test deploys had updated `/opt/bluey-api/bluey-server`, which was not the active executable.
- Stopped sending non-default `temperature` to OpenAI GPT-5-family models. Live API confirmed `gpt-5.5` rejects `temperature: 0.1`.
- Disabled the old Anthropic manual thinking request shape for now. Live API confirmed `claude-opus-4-8` rejects `thinking.type=enabled` and expects the newer adaptive/output-config shape.
- Kept Anthropic answers bounded when manual thinking is disabled, so small `max_tokens` requests do not reserve hidden thinking budget.
- Adjusted AnswerPlan routing so simple code prompts such as tiny Fibonacci/swap snippets still produce a `code_artifact` but route to `balanced` instead of the slower `deep` lane. LRU/cache/backend/debug/system-design style work still routes to `deep`.
- Updated the stale 429 fallback integration-test expectation from OpenAI `gpt-5.5` to current fast fallback `gpt-5.4-mini`.

## Live Results

- `bluey-api.service` clean restart after final deploy:
  - active PID: `890981`
  - executable: `/usr/local/bin/bluey-server`
  - journal showed `Deactivated successfully`
  - no `Cannot start a runtime from within a runtime`
  - no `core-dump`

- Public live checks:
  - `https://bluey.sh/health` returned `status=ok`
  - `https://bluey.sh/latest.json` returned version `0.1.17`
  - install script checksum: `637f06aade486d94f2f18ea1ce8c7f2d0892da19e47096cc97ad787f702f316b`
  - darwin-arm64 artifact checksum verified: `8a91bdc1bd34444fb31f76450461e5d2b287405f205a415611c82d069359451d`

- Live managed answer smoke, before provider-shape/simple-code fixes:
  - request id: `codex-live-route-smoke-1782813238`
  - effective lane: `deep`
  - OpenAI `gpt-5.5` failed with HTTP 400 due unsupported temperature
  - Anthropic `claude-opus-4-8` failed with HTTP 400 due old thinking shape
  - fallback succeeded on Z.AI `glm-5.2`
  - latency: about `6.9s`

- Live managed answer smoke, after provider-shape fix but before simple-code routing:
  - request id: `codex-live-route-smoke-1782814194`
  - effective lane: `deep`
  - provider: DeepSeek `deepseek-v4-pro`
  - `was_fallback=false`
  - latency: about `8.8s`

- Final live managed answer smoke:
  - request id: `codex-live-route-smoke-1782814794`
  - effective lane: `balanced`
  - provider/model: Anthropic `claude-sonnet-4-6`
  - artifact: `code`
  - cost: `1c`
  - `was_fallback=false`
  - server latency: `2228ms`
  - wall-clock curl latency: about `2412ms`

## Verification

```bash
cargo fmt --manifest-path server/Cargo.toml
cargo build --manifest-path server/Cargo.toml --bin bluey-server
cargo build --release --manifest-path server/Cargo.toml --bin bluey-server
cargo test --manifest-path server/Cargo.toml answer_plan -- --nocapture
cargo test --manifest-path server/Cargo.toml temperature -- --nocapture
cargo test --manifest-path server/Cargo.toml anthropic_manual_thinking -- --nocapture
cargo test --manifest-path server/Cargo.toml web_search -- --nocapture
cargo test --manifest-path server/Cargo.toml provider_health -- --nocapture
cargo test --manifest-path server/Cargo.toml router_complete_falls_back_when_preferred_provider_429s -- --nocapture
cargo test -p cue-daemon --test live_transcript_dedup --test live_transcript_emit --test pipeline_integration -- --nocapture
scripts/release-hygiene-scan.sh
```

All listed tests passed.

## Live Config Findings

- `BLUEY_ANSWER_PLAN_ROUTING=1` is live.
- `BLUEY_ROUTE_POLICY=provider_mix` is live.
- Z.AI and DeepSeek key pools are present on the server env and are not stored in repo.
- Managed web-search code/guards exist, but no real web-search provider is configured yet, so web search remains skipped at runtime.
- Production preflight still fails one real security gate:
  - missing `BLUEY_TURNSTILE_SITE_KEY`
  - missing `BLUEY_TURNSTILE_SECRET_KEY`
  - `/auth/captcha/config` currently returns `{"provider":null,"site_key":null}`
- Preflight warnings:
  - Square branding helper script is not present in the droplet build copy.
  - R2/S3 cloud restore object-storage env is incomplete, although backup R2 destination is reachable.

## Remaining

- Create Cloudflare Turnstile keys for `bluey.sh` and add them to `/etc/bluey-api/bluey-api.env`; keep `BLUEY_REQUIRE_TURNSTILE=1` for production signup abuse protection.
- Configure a real managed web-search provider and keys before claiming live web search works.
- Implement Anthropic's newer adaptive thinking/output-config schema before re-enabling manual thinking on Claude flagship lanes.
- Add a lightweight live smoke script that seeds an internal test account, runs one tiny managed answer, prints request id/provider/lane/latency, and verifies the journal has `was_fallback=false`.

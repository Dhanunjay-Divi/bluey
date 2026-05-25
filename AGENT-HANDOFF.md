# Bluey Agent Handoff

Last updated: 2026-05-25

Start here when joining the Bluey repo.

## Read First

1. `docs/rounds/END-TO-END-AGENT-CONTEXT-2026-05-25.md`
2. `docs/MODEL-ROUTING.md`
3. `docs/DEPLOYMENT-SCALING.md`
4. `docs/PRODUCTION-READINESS.md`
5. `docs/PRELAUNCH-CHECKLIST.md`

The first file is the complete current context: product flow, architecture,
implemented state, provider routing, capacity policy, cloud/RAG/storage plan,
environment variables, QA commands, known gaps, and what to tell the next
agent.

## Snapshot

- Branch: `feat/phase-3-round-12`
- Code tip before this docs refresh:
  `3c13ed7 feat(server): add cloud stt fallback routing`
- Product domain target: `bluey.sh`
- Working tree warning: `bluey-dev.db` may appear as local untracked data. Do
  not stage it.

## Product Direction

Bluey is a managed desktop AI overlay for live work.

Customer flow:

```text
install -> bluey on -> link if needed -> compact pill -> expanded overlay
-> listen / attach / analyze screen / ask -> streaming answer + saved session
```

The CLI is the launch/support surface. The overlay is the product surface.

## Current Architecture

- Desktop: `bluey`, `bluey-daemon`, native macOS overlay/audio/whisper helpers,
  local SQLite cache, keychain tokens.
- Server: `bluey-server` in `server/`, with auth, device linking, wallet,
  Stripe billing, managed provider routing, sync/RAG, usage, metrics, GDPR
  export/delete, and STT relay/transcribe endpoints.
- Storage target: local SQLite cache plus cloud Postgres/pgvector/object
  storage/Redis.
- Provider keys are server-side only for production.

## Current Model Routing

See `docs/MODEL-ROUTING.md` for source of truth.

- Instant: OpenAI `gpt-4o-mini`
- Balanced: Anthropic `claude-3-5-sonnet-latest`
- Deep: Anthropic `claude-3-7-sonnet-latest`
- Vision/screen: OpenAI `gpt-4o`
- STT: Deepgram `nova-3`, OpenAI `gpt-4o-mini-transcribe` fallback, hidden
  LocalWhisper dev/offline fallback.

Do not expose `local` as a customer model. Do not expose `vision` as a normal
route choice when the Screen/Analyze action already implies vision.

## Current Capacity Policy

Bluey should not punish paying users for high usage. Normal usage is controlled
by wallet balance and provider capacity, not arbitrary per-account throttles.

Implemented:

- Provider/model buckets for OpenAI, Anthropic, Deepgram, OpenAI STT, embeds.
- Optional emergency per-account guardrails, disabled by default.
- Comma-separated provider key pools for approved capacity.
- Redis shared ledger support through `BLUEY_REDIS_URL`.
- Deepgram -> OpenAI fallback on `/router/transcribe`.

Next capacity hardening: provider-key health scoring in Redis/shared ledger.

## Debug Flag Warning

`BLUEY_HOST_OVERLAY_CAPTURE_VISIBLE=1` is local smoke-test only. It must never be
enabled in customer builds or production launch scripts.

## Full Verification Gate

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo build --all-targets --release
cargo test --all-targets
(cd server && cargo test)
(cd crates/cue-dashboard/ui && npm run build)
swift build -c release --package-path native/macos/cue-overlay
swift build -c release --package-path native/macos/cue-whisper
git -P diff --check
```

Use `docs/deploy/PHASE2-MAC-SMOKE.md` for visual/product smoke.

## Next Best Work

1. Finish production-grade overlay UX: compact pill, uncropped/resizable panel,
   scroll-contained transcript, ChatGPT-like composer, session drawer, attached
   docs chips, style prompt, balance/cost labels, and canvas auto-open only for
   artifacts.
2. Run Mac smoke steps 4-10 with a real managed account/server path.
3. Add Redis provider-key health scoring.
4. Add server-owned model/routing config.
5. Finish `bluey.sh` web/link/reload/account/docs pages.
6. Stand up staging/prod infra with Stripe live, SMTP, Caddy/TLS, backups, and
   monitoring.
7. Complete Windows QA before claiming Windows support.

## Review Cadence

Use the Pinky-style round flow:

1. Implementation doc or recap.
2. Reviewer verdict doc.
3. Fix doc if red.
4. Merge/proceed only when the round is accepted or accepted with explicit nits.

Keep commits scoped. Document every meaningful product/backend/security decision
where a future agent would otherwise need to infer intent from the chat.

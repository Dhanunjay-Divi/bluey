# Round 061 - Managed-Only Customer AI Path

Date: 2026-06-19

Branch: `codex/bluey-ai-site`

Implementation commit: `8b358e7 fix(security): keep customer AI routes managed-only`

## Purpose

Bluey customer installs must not expose or depend on BYOK provider keys, local
LLMs, or local direct-provider routing. The production contract is:

`desktop -> bluey-server -> model/STT/embed providers`

Provider credentials live on `bluey-server`. The desktop can keep local
sessions, local vector/RAG indexes, screenshots, documents, and transcript
history, but provider calls for paid/customer use must route through managed
server endpoints.

## What Changed

- Dashboard provider registry now gates direct OpenAI/Anthropic providers behind
  `BLUEY_DEV_BYOK=1`, and that flag is honored only in debug/dev builds.
- Dashboard local Ollama registration now also requires debug/dev
  `BLUEY_DEV_BYOK=1`. `BLUEY_OLLAMA_HOST` alone is ignored in
  customer/default mode.
- Legacy dashboard single-shot provider construction now returns managed
  `BlueyManagedProvider` when account tokens exist, and otherwise returns no
  provider unless `BLUEY_DEV_BYOK=1`.
- Daemon RAG embedding ignores local `OPENAI_API_KEY` unless running a
  debug/dev build with `BLUEY_DEV_BYOK=1`; linked paid users use managed
  `/router/embed`.
- Daemon auto-recap direct OpenAI construction is gated behind
  debug/dev `BLUEY_DEV_BYOK=1` or `BLUEY_DEV_DIRECT_PROVIDERS=1`.
- The older direct streaming STT factory is also debug/dev-only now; release
  binaries do not honor local Deepgram/OpenAI/Whisper STT env flags.
- Product docs were corrected to describe local Ollama/direct BYOK as
  developer-only fallback tooling, not customer modes.

## Customer-Facing Contract

- No BYOK UI.
- No local LLM customer lane.
- No local provider keys in the desktop bundle.
- No fallback from managed billing/auth failures to unmetered local/direct
  providers.
- Server-side env vars such as `OPENAI_API_KEYS`, `ANTHROPIC_API_KEYS`, and
  `DEEPGRAM_API_KEYS` remain valid on the server only.

## Dev Escape Hatches

These are intentionally explicit and are ignored by customer release binaries.
They are only honored in debug/dev builds:

- `BLUEY_DEV_BYOK=1`
- `BLUEY_DEV_DIRECT_PROVIDERS=1`
- `BLUEY_DEV_DIRECT_STT=1`
- `BLUEY_DEV_DIRECT_VISION=1`

Related direct STT flags such as `BLUEY_STT_LOCAL_WHISPER=1`,
`BLUEY_STT_FALLBACK_OPENAI=1`, and `BLUEY_USE_MOCK_STT=1` are also
debug/dev-only and are not release-binary customer modes.

## Verification

Ran locally on `uno`:

```bash
cargo fmt --all --check
git diff --check
cargo test -p cue-dashboard --all-targets
cargo test -p cue-cloud-client --all-targets
cargo test -p cue-daemon --all-targets
cargo clippy -p cue-dashboard -p cue-daemon --all-targets -- -D warnings
```

Results:

- `cue-dashboard`: 12 passed
- `cue-cloud-client`: 21 passed
- `cue-daemon`: 209 passed, 2 ignored
- Clippy clean for touched desktop crates

## Files Changed

- `crates/cue-dashboard/src/commands.rs`
- `crates/cue-daemon/src/app.rs`
- `crates/cue-daemon/src/rag_indexer.rs`
- `docs/AUTO-ROUTING-USP.md`
- `docs/MODEL-ROUTING.md`
- `docs/PRODUCTION-READINESS.md`
- `docs/SECURITY-HARDENING.md`
- `docs/reviews/REVIEW-MANAGED-ONLY-CUSTOMER-AI-PATH-BY-CODEX-2026-06-19.md`
- `docs/rounds/MANAGED-LOCAL-RAG-EMBEDDINGS-2026-06-19.md`

## Remaining Notes

This round does not remove the provider adapter code itself. The adapters remain
useful for tests, server-side routing, and explicit development mode. The key
point is that the normal customer desktop path cannot accidentally choose them.

Follow-up hardening on 2026-06-19 made the dev flags debug-build-only so a
customer cannot enable direct BYOK/local LLM/STT routing by setting
`BLUEY_DEV_BYOK=1` or the direct STT flags against the shipped release binary.

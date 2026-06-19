# Review: Managed-Only Customer AI Path

Verdict: 🟢 ACCEPT

Date: 2026-06-19

Reviewed commit: `8b358e7 fix(security): keep customer AI routes managed-only`

## Findings

No blocking issues found in the managed-only enforcement patch.

## Checks

- Dashboard provider registry no longer registers direct OpenAI/Anthropic
  providers unless `BLUEY_DEV_BYOK=1` is set in a debug/dev build.
- Ollama is no longer registered from `BLUEY_OLLAMA_HOST` alone, and the dev
  BYOK gate is ignored by release binaries.
- Legacy dashboard single-shot path no longer falls through to
  `OPENAI_API_KEY` when dev BYOK is disabled.
- Daemon local RAG embedding ignores local `OPENAI_API_KEY` unless
  `BLUEY_DEV_BYOK=1` is set in a debug/dev build.
- Daemon auto-recap direct OpenAI path is debug/dev-gated.
- Older direct streaming STT factory paths are debug/dev-gated; release
  binaries stay on managed STT.
- Product docs now match the production contract: customer AI/STT/vision/embed
  provider calls go through `bluey-server`; provider keys stay server-side.

## Residual Risk

- Historical review/archive docs still mention older BYOK/local fallback plans.
  Those are useful project history but should not be treated as current product
  behavior.
- Provider adapter code remains in the repo. That is acceptable because it is
  test/dev/server plumbing; customer release binaries ignore the runtime dev
  env flags and customer routing is gated at selection time.

## Verification

```bash
cargo fmt --all --check
git diff --check
cargo test -p cue-dashboard --all-targets
cargo test -p cue-cloud-client --all-targets
cargo test -p cue-daemon --all-targets
cargo clippy -p cue-dashboard -p cue-daemon --all-targets -- -D warnings
```

All passed locally.

# Review: Model Routing Refresh

Date: 2026-06-17
Reviewer: Codex
Verdict: Accept pending live-account latency smoke

## Findings

No code blockers found in the scoped model-routing refresh.

The main tradeoff is intentional: Deep now starts with Claude Opus 4.8, so hard
questions get a stronger model but cost more. Instant and Balanced remain
cost-aware so routine questions do not pay the Opus/GPT-5.5 tax.

## What I Checked

- OpenAI route candidates use `gpt-5.4-mini` for Instant and `gpt-5.5` for
  accurate/vision fallback.
- Anthropic route candidates use exact dated model IDs for Sonnet 4.6 and
  Opus 4.8.
- Every managed route candidate resolves through `pricing::lookup`.
- GPT-5 family requests continue using `max_completion_tokens`, not the legacy
  `max_tokens` field.
- Anthropic manual thinking support includes Opus 4.8 and excludes Fable 5.
- Pricing docs and implementation agree on the 2026-06-17 snapshot.
- Gemini and Deepgram Flux are documented as evaluated-but-deferred rather
  than half-enabled.

## Verification

```bash
cargo fmt --all --check
cargo test --manifest-path server/Cargo.toml routing::dispatcher
cargo test --manifest-path server/Cargo.toml pricing
cargo test --manifest-path server/Cargo.toml --lib
cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings
git diff --check
```

## Live Smoke Still Needed

Use a signed-in account and funded provider keys on the deployed server:

1. Ask a trivial typed question and confirm Instant starts quickly.
2. Ask a normal technical question and confirm Balanced routes through Sonnet.
3. Ask a hard system-design/coding question and confirm Deep routes through
   Opus 4.8.
4. Run `Screen` and confirm Vision routes through GPT-5.5.
5. Confirm cost labels and usage events reflect the selected provider/model.

## Notes for Kiro

This round deliberately does not add Gemini or Flux. Both are good candidates,
but each needs a full provider integration with pricing, key health, billing,
tests, and capacity behavior. This pass only updates the providers already
wired end to end.

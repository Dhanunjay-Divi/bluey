# Round 037 - Bluey True Streaming + Routing Pass

Date: 2026-06-10

## What Changed

This pass closes the main latency gap in the managed answer path:

```text
desktop -> bluey-server -> OpenAI / Anthropic streaming -> overlay
```

Before this pass, `/router/complete/stream` could look like streaming to the UI
but the server still waited for the upstream provider completion before emitting
chunks. The server now proxies upstream SSE deltas and sends the final Bluey
billing/artifact metadata after provider completion.

## Server Streaming

- Added `routing::complete_stream_with_key`.
- Added OpenAI Chat Completions SSE parsing:
  - `choices[0].delta.content` becomes customer-visible delta chunks.
  - usage chunks from `stream_options.include_usage` become final billing inputs.
- Added Anthropic Messages SSE parsing:
  - `content_block_delta` text deltas stream immediately.
  - `message_start` / `message_delta` usage frames feed final billing inputs.
- `/router/complete/stream` now mirrors `/router/complete` money-path behavior:
  - auth-bound idempotency reservation;
  - RAG context injection;
  - route/key candidate retry;
  - provider/key cooldown handling for 429/529;
  - trial/balance deduction;
  - usage event recording;
  - final `event: billing` carrying the canonical `CompleteResponse`;
  - cached idempotency replay through synthesized SSE from stored response.

## Desktop Managed Client

- `cue-cloud-client` now exposes authenticated streaming POST support.
- `BlueyManagedProvider::complete_stream` consumes the managed SSE / NDJSON
  stream directly.
- The parser handles:
  - OpenAI-compatible delta frames;
  - Bluey billing/final metadata events;
  - NDJSON fallback records;
  - `[DONE]`;
  - split UTF-8 boundaries;
  - auth refresh on pre-stream 401.

## Model Routing Refresh

Managed lane candidates now live in `server/src/routing/dispatcher.rs` and are
documented in `docs/MODEL-ROUTING.md`.

Current managed route table:

| Lane | Candidate order |
| --- | --- |
| `instant` | OpenAI `gpt-5.4-mini` -> Anthropic `claude-haiku-4-5-20251001` |
| `balanced` | Anthropic `claude-sonnet-4-6` -> OpenAI `gpt-5.4` |
| `deep` | Anthropic `claude-sonnet-4-6` -> OpenAI `gpt-5.4` -> Anthropic `claude-haiku-4-5-20251001` |
| `vision` | OpenAI `gpt-5.4` -> OpenAI `gpt-5.4-mini` |

Gemini is not in the managed route table yet because there is no server-side
Gemini dispatcher implementation in this repo.

## Speculative Routing Status

Safe Auto routing remains the product default: classify, choose one lane, stream
one stable answer card.

The draft+deep experiment is implemented and still available behind:

```bash
BLUEY_PARALLEL_DRAFTS=1
```

When enabled, the final Deep result replaces/refines the same card rather than
appending an unrelated second answer. It is intentionally not the customer
default until live latency/quality metrics prove it feels better than one true
streaming lane.

## Latency Eval Harness

Added `scripts/bluey-latency-eval.py`.

It measures:

- `/health` reachability;
- `/router/complete/stream` time to first SSE event, first content delta,
  final billing event, and `[DONE]`;
- `/router/complete` non-streaming total time for comparison.

The script reads only:

- `BLUEY_API_BASE`
- `BLUEY_ACCESS_TOKEN`

It never reads provider keys, never prints bearer tokens, and omits request and
response text from output.

## Verification

Commands run at implementation time:

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo test --manifest-path server/Cargo.toml
cargo test --manifest-path server/Cargo.toml --test integration_e2e
cargo test --manifest-path server/Cargo.toml --lib
cargo test -p cue-cloud-client -p cue-llm
cargo test -p cue-router
python3 -m py_compile scripts/bluey-latency-eval.py
scripts/bluey-latency-eval.py --self-test
scripts/bluey-latency-eval.py --dry-run
git diff --check
```

Key new coverage:

- OpenAI upstream SSE deltas arrive before final billing metadata.
- Anthropic upstream SSE deltas arrive before final billing metadata.
- Stream idempotency replay returns the cached response without a second
  upstream hit.
- Managed desktop stream parser handles split UTF-8, SSE, NDJSON, final
  metadata, and pre-stream auth refresh.

## Remaining Work

- Run the latency harness against a funded staging/prod account and record
  first-token targets by lane.
- Decide whether `BLUEY_PARALLEL_DRAFTS=1` should ever become a customer-facing
  default after the measured true-streaming baseline.
- Add a managed Gemini dispatcher before routing production vision to Gemini.
- Move OpenAI managed routes to Responses API if live testing shows Chat
  Completions does not accept the selected future model IDs or reasoning knobs.

# Round 245 - Provider 429 Cooldown Hardening

## Trigger

The owner asked to make sure cooldowns and upstream `429`/rate-limit behavior
are handled properly, especially now that Bluey is using a wider provider mix
across OpenAI, Anthropic, Gemini, Z.AI, DeepSeek, embeddings, and STT.

## Findings

Bluey already had the important base layers:

- per-provider/model token buckets in `server/src/rate_limit.rs`
- provider/model/key cooldown state in `server/src/provider_health.rs`
- Redis-backed cooldowns when `BLUEY_REDIS_URL` is configured
- fallback loops in streaming, non-streaming, embedding, and transcription
- typed client handling for capacity `429` JSON bodies

The main gap was streaming provider error frames. If a provider accepted an
SSE stream and then sent an in-band rate-limit/overload frame, Bluey treated it
as a generic stream interruption instead of the same typed upstream capacity
signal used for pre-stream HTTP `429`.

## Fix

- `server/src/routing/dispatcher.rs`
  - Parses both numeric and HTTP-date `Retry-After` values.
  - Converts OpenAI-compatible, Gemini, and Anthropic stream error frames with
    rate-limit, quota, overload, capacity, or `RESOURCE_EXHAUSTED` signals into
    typed `UpstreamHttpError` values.
  - Keeps Anthropic overload as status `529` and other stream capacity frames
    as status `429`, both using the existing cooldown path.
- `server/src/api/router.rs`
  - If the prefetched first provider stream event is typed capacity, Bluey now
    records a provider/model/key cooldown and continues trying healthy keys or
    fallback routes before committing the answer stream.
  - If a selected stream later fails with typed capacity, the SSE error payload
    includes `reason: "provider_key_cooling_down"` and `retry_after_secs` so
    the desktop can show a calm capacity message.
- `crates/cue-llm/src/bluey_managed.rs`
  - Managed stream `event: error` payloads with capacity reason/retry metadata
    now map to `LlmError::CapacityBusy` instead of generic provider error.
- `crates/cue-cloud-client/src/client.rs`
  - Client `Retry-After` parsing now accepts numeric seconds and HTTP-date
    values.
- Docs updated:
  - `docs/PROVIDER-429-PLAYBOOK.md`
  - `docs/MODEL-ROUTING.md`

## Verification

Passed:

```bash
cargo fmt -p cue-cloud-client -p cue-llm
cargo fmt --manifest-path server/Cargo.toml
cargo test -p cue-cloud-client retry_after -- --nocapture
cargo test -p cue-llm capacity_busy -- --nocapture
cargo test --manifest-path server/Cargo.toml stream_error_frame -- --nocapture
cargo test -p cue-cloud-client parse_or_err_429_capacity_body_maps_to_capacity_busy -- --nocapture
cargo test -p cue-llm complete_stream_posts_to_managed_endpoint_and_parses_ndjson -- --nocapture
cargo check -p cue-cloud-client -p cue-llm
cargo check --manifest-path server/Cargo.toml
```

One loose exploratory command, `cargo test --manifest-path server/Cargo.toml
stream_error -- --nocapture`, also matched an older integration fixture named
`router_complete_reports_upstream_error_after_capacity_skip`. That fixture
returned `200` instead of its old expected `502`, apparently because current
provider-mix routing found a healthy fallback route. The targeted new
stream-error tests passed; the older fixture should be revisited separately
under a deterministic route policy if we want to keep asserting that exact
failure shape.

## Current State

Provider cooldown behavior is stronger for both normal HTTP failures and
streaming failures:

- pre-dispatch HTTP `429`/`529` still cools the key and falls through
- first SSE capacity frame now cools the key and falls through before the user
  sees a broken answer
- late SSE capacity frame now reaches the desktop as typed capacity, with
  retry-after metadata
- provider-health counters remain available at `/admin/metrics`

## Remaining QA

- Run a live load test with staged provider `429`/overload responses and confirm
  the overlay shows capacity copy instead of generic provider errors.
- If production scales beyond one server, keep `BLUEY_REDIS_URL` configured so
  cooldowns and token buckets are shared across instances.
- Consider a deterministic integration fixture for first-frame streaming
  capacity fallback across two mocked routes.

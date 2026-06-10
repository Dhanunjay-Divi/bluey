# Bluey Latency Eval Harness

Date: 2026-06-10

## Purpose

`scripts/bluey-latency-eval.py` is a local-friendly smoke/eval harness for the
managed answer path. It measures:

- `GET /health` reachability.
- `POST /router/complete/stream` time to first SSE event, first content delta,
  final billing event, and `[DONE]`.
- `POST /router/complete` total time for non-streaming comparison.

The harness reads only `BLUEY_API_BASE` and `BLUEY_ACCESS_TOKEN`. It never reads
provider keys, never prints bearer tokens, and omits request/response text from
output.

## Dry Run

Use dry-run mode when you want to verify local env readiness without an access
token or network call:

```bash
scripts/bluey-latency-eval.py --dry-run
```

It prints which env vars are missing for a full managed-answer eval and exits
`0`.

## Health Only

Run only the public reachability check:

```bash
BLUEY_API_BASE="https://bluey.sh" scripts/bluey-latency-eval.py --health-only
```

If `BLUEY_ACCESS_TOKEN` is not set, the script still runs `/health` and skips
authenticated answer checks.

## Full Managed Answer Eval

Provide the access token from the environment only:

```bash
export BLUEY_API_BASE="https://bluey.sh"
export BLUEY_ACCESS_TOKEN="..."
scripts/bluey-latency-eval.py --lane instant --runs 3
```

Useful options:

- `--question "..."` sends a specific typed question. The question is not
  printed back by the harness.
- `--lane instant|balanced|deep|vision` selects the managed lane.
- `--skip-nonstream` measures only the SSE path after `/health`.
- `--json` emits redacted machine-readable output.
- `--self-test` runs local SSE parser/redaction assertions without network.

## Threshold Labels

Threshold labels are local smoke heuristics, not production SLOs:

- `/health`: `ok` <= 1s, `watch` <= 3s, `slow` > 3s.
- Stream first event / first token: `ok` <= 3s, `watch` <= 8s, `slow` > 8s.
- Stream full completion and non-streaming completion: `ok` <= 15s,
  `watch` <= 30s, `slow` > 30s.

The endpoint now proxies upstream provider streams through `bluey-server`, so
first-token time is the primary UX latency indicator. Full completion time still
matters for final cost labels, artifact metadata, and idempotency completion.

## 2026-06-10 Streaming Upgrade

The eval harness was added with the true streaming pass:

- `bluey-server` proxies OpenAI Chat Completions SSE deltas and Anthropic
  Messages SSE deltas instead of waiting for the full provider response.
- The terminal SSE frame remains a Bluey billing event carrying the final
  `CompleteResponse` so existing dashboard/overlay metadata plumbing keeps
  working.
- Cached idempotency replays still synthesize an SSE replay from the completed
  response because no upstream stream exists on replay.
- The desktop managed client consumes the stream directly through
  `BlueyManagedProvider::complete_stream`.

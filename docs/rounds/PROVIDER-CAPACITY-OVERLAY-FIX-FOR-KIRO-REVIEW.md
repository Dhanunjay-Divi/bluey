# Provider Capacity Overlay Fix — for Kiro Review

Date: 2026-06-13
Branch: `codex/bluey-ai-site`

## Scope

This closes the Codex-owned item from `docs/PROVIDER-429-PLAYBOOK.md` §6:

- Preserve server capacity-busy responses as a typed client error.
- Surface calm overlay copy: "Capacity busy" plus the server retry window when available.
- Do not treat capacity cooling as a generic 429, generic provider failure, or direct-provider failover trigger.

## Implementation

### cue-cloud-client

Files:

- `crates/cue-cloud-client/src/error.rs`
- `crates/cue-cloud-client/src/client.rs`

Changes:

- Added `Error::CapacityBusy { retry_after_secs, reason }`.
- `parse_or_err` now recognizes structured capacity bodies on:
  - `429 Too Many Requests`
  - `503 Service Unavailable`
- Plain `429` with only `Retry-After` remains `RateLimited`.
- `CapacityBusy` is intentionally not retried by the GET retry loop.
- `auth_post_stream` now maps capacity responses before handing a stream to callers.

### cue-llm

Files:

- `crates/cue-llm/src/lib.rs`
- `crates/cue-llm/src/bluey_managed.rs`
- `crates/cue-llm/src/router.rs`

Changes:

- Added `LlmError::CapacityBusy { retry_after_secs, reason }`.
- `BlueyManagedProvider` maps cloud `CapacityBusy` to the typed LLM error.
- `LlmRouter` treats `CapacityBusy` as terminal for that request, not as a failover condition.

Rationale: managed capacity is server-account state, not a cue to fall back into direct/unmetered desktop providers.

### cue-daemon overlay copy

File:

- `crates/cue-daemon/src/app.rs`

Changes:

- `user_facing_answer_error` now recognizes capacity-busy reasons:
  - `provider_key_cooling_down`
  - `provider_capacity`
  - `upstream_spend_guard`
  - structured "capacity busy" text
- The returned overlay message is calm and retry-window aware:
  - `Capacity busy. Bluey is waiting for provider capacity to recover before trying again. Retry in about Ns.`

## Verification

Run:

```bash
cargo fmt --all --check
cargo test -p cue-cloud-client
cargo test -p cue-llm
cargo test -p cue-daemon --all-targets
cargo clippy -p cue-cloud-client --all-targets -- -D warnings
cargo clippy -p cue-llm --all-targets -- -D warnings
cargo clippy -p cue-daemon --all-targets -- -D warnings
git diff --check
```

New regression coverage:

- `parse_or_err_429_capacity_body_maps_to_capacity_busy`
- `parse_or_err_503_capacity_body_uses_retry_after_header`
- `auth_post_stream_maps_capacity_busy_before_streaming`
- `maps_capacity_busy_to_typed_llm_error`
- `test_no_failover_on_capacity_busy`
- `capacity_busy_terminal_helpers`

## Not Changed

- Server provider health and route selection; Kiro's playbook work owns that layer.
- Automatic scheduled retry from the overlay; this change prevents spammy retry/failover behavior and gives the UI a retry window. If we want active queued retry, that should be a separate product decision because it can spend credits after the user has stopped watching.
- Provider key pool configuration.

## Reviewer Focus

1. Confirm `CapacityBusy` is recognized before generic `RateLimited`/`Server`.
2. Confirm `CapacityBusy` does not become `should_failover()`.
3. Confirm the daemon copy is acceptable for the overlay.
4. Confirm no direct-provider/unmetered fallback is introduced.

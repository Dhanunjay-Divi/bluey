# Round 199 - Web Search Credit Metering Sources

## Trigger

Owner asked to implement the concrete web-search behavior after Round 198:

- detect when screen/docs/session memory do not answer the question
- decide that web search is needed
- show clear overlay status such as `Searching web...`, `Reading 3 sources...`, and `Web search used: 1 search, 3 sources`
- charge web search separately from the AI answer
- keep trial search capped, but do not apply a low fixed daily search cap to paid credit users
- show source chips/source details on Mac and Windows

Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`
Workspace: `/Users/uno/Downloads/cue`
Branch: `codex/bluey-overlay-routing-hardening`
Round completed: 2026-06-26 16:00 EDT

## Implementation

Backend:

- Replaced the loose `(sources, attempted)` web-search return with `WebSearchOutcome`.
- Kept real provider/API-based search only. Supported configured providers remain Brave, Tavily, or a generic server-side provider endpoint.
- Added env-tunable web-search pricing:
  - `BLUEY_WEB_SEARCH_CUSTOMER_COST_CENTS`, default `2`
  - `BLUEY_WEB_SEARCH_BLUEY_COST_CENTS`, default `1`
- Added trial search accounting:
  - default `BLUEY_TRIAL_WEB_SEARCHES_PER_DAY=5`
  - durable count comes from `usage_events` where `task_type='web_search'`
- Kept paid users credit-metered instead of a `50/day` product cap.
- Added a short-window repeated identical query guard:
  - `BLUEY_WEB_SEARCH_REPEAT_WINDOW_SECS`, default `600`
  - guard key is hashed in-memory rather than storing raw query text
- Added explicit web-search status copy:
  - `Checking saved context...`
  - `Searching web...`
  - `Reading N sources...`
  - `Web search used: 1 search, N sources`
- Included web-search cost in entry balance checks and upstream spend-guard projections.
- Deducted the combined answer total once after successful completion.
- Recorded LLM answer usage and web-search usage as separate `usage_events`:
  - LLM event keeps only answer token cost
  - `web_search` event records search count, source count, provider, latency, and search cost
- Failed, timed-out, unconfigured, or privacy-blocked searches show a status but do not charge the user.

Daemon / overlay:

- Carried managed source metadata through `LiveProviderAnswer` and `AnswerRouteOutcome`.
- Added a shared `Sources` context card for managed web sources.
- The card includes per-source lines plus `CueCardAttachment` web chips.
- Because it uses the existing shared card protocol, Mac and Windows both receive the same source display path without native UI forks.

## User-Facing Behavior

Expected paid web answer shape:

1. Bluey detects that saved context is insufficient.
2. Server performs one managed search call, capped to configured result count.
3. Overlay sees status events such as:
   - `Checking saved context...`
   - `Searching web...`
   - `Reading 3 sources...`
   - `Web search used: 1 search, 3 sources`
4. Final answer includes source labels such as `[W1]` when factual/current claims use web results.
5. A `Sources` card appears after the answer with web chips for the returned sources.
6. Cost label includes the combined total and the web-search usage copy.

## Verification

Passed:

- `cargo fmt --check --manifest-path server/Cargo.toml`
- `cargo fmt --check -p cue-daemon`
- `git diff --check`
- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
- `cargo test --manifest-path server/Cargo.toml router -- --nocapture`
- `cargo test --manifest-path server/Cargo.toml usage -- --nocapture`
- `cargo test --manifest-path server/Cargo.toml router_cost_label_includes_web_search_usage -- --nocapture`
- `cargo test --manifest-path server/Cargo.toml web_search_usage_event_records_separate_search_cost -- --nocapture`
- `cargo test --manifest-path server/Cargo.toml count_task_events_in_window_counts_recent_matching_task_type -- --nocapture`
- `cargo test -p cue-daemon managed_sources_render_as_context_card_with_web_attachments -- --nocapture`
- `cargo test -p cue-daemon response_artifact_does_not_route_self_intro_to_system_design -- --nocapture`
- `cargo test -p cue-daemon answer_overlay_cost_label -- --nocapture`
- `cargo build --manifest-path server/Cargo.toml`
- `cargo build --release -p cue-cli -p cue-daemon`

Local install refreshed:

- `~/.bluey/bin/bluey off`
- copied `target/release/bluey` to `~/.bluey/bin/bluey`
- copied `target/release/bluey-daemon` to `~/.bluey/bin/bluey-daemon`
- `~/.bluey/bin/bluey on`
- `~/.bluey/bin/bluey status`

Latest local status after install:

- daemon pid `69465`
- active meeting id `89a72895-1931-4990-bc8e-8a6a18dccdba`
- overlay visible `true`
- overlay capture excluded `true`
- overlay opacity `0.92`
- screen capture active `false`

## Current State

- Code is implemented locally and installed for the daemon/CLI.
- The server code is built and tested, but production search still requires server deploy plus search provider env configuration.
- Live web-search provider behavior was not hit in this round because no real search provider key/env was configured locally.
- If search is requested without provider configuration, Bluey will now surface `Web search is not configured yet.` instead of silently pretending it searched.

## Remaining Gates

- Configure production/staging search provider env vars:
  - `BLUEY_WEB_SEARCH_PROVIDER=brave|tavily|generic`
  - provider API key env
  - optional pricing/quota env overrides
- Deploy the server before expecting live cloud web search to work.
- Run a live smoke with a public unknown query and confirm:
  - statuses arrive before answer text
  - final answer cites `[W1]`, `[W2]`, etc.
  - source SSE reaches the daemon
  - overlay shows the `Sources` card
  - usage ledger has both `llm` and `web_search` events
  - paid balance deducts combined cost once
- Build a more polished native source drawer later. Round 199 provides the shared Mac/Windows source-card foundation, not the final drawer UI.

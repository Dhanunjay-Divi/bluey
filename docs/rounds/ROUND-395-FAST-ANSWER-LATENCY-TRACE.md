# ROUND-395 Fast Answer Latency Trace

Date: 2026-07-05
Branch/worktree: `/Users/uno/Downloads/cue`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## User Issue

Bluey answered simple typed questions much slower than expected:

- Example: `Can you explain me the difference between LRU cache and SRU?`
- Overlay showed `started in 10.6 s` and produced a long 855-token answer.
- Similar simple interview/API question showed `started in 4.6 s`.

Expected behavior: these quick conceptual questions used to start in under about 2 seconds and should not pay the latency cost of balanced/deep routing unless the question actually needs it.

## Diagnosis

The local log showed the slow question going through the managed balanced route:

- `route_primary="bluey_managed/balanced"`
- `output_tokens=855`
- `total_tokens=4209`
- completion latency around 16.9s in daemon/provider logs

The main causes were:

1. Desktop Auto routing treated `auto`, `default`, and `general` as managed `balanced`.
2. Short conceptual questions containing coding-adjacent words like `LRU` or `cache` were not separated from implementation/code-generation requests.
3. The server AnswerPlan could still override a desktop `instant` request back to balanced for compact answers.
4. Logs had final completion latency, but not enough split timing to tell whether delay came from context prep, overlay card creation, managed stream connect, first event, first text, provider read, or server lane selection.

## Changes

### Desktop Routing

Updated `/Users/uno/Downloads/cue/crates/cue-daemon/src/app.rs`.

- Added a fast conceptual question detector for short explain/compare/what-is/how-do questions.
- Auto now routes those simple conceptual questions to managed `instant`.
- Auto still routes real code generation, LeetCode-style prompts, Sudoku/LRU implementation, dynamic programming, and backtracking prompts to managed `deep`.
- Screen context still routes to `vision`.
- Instant managed requests now get a smaller output budget of 384 tokens so they do not turn into long balanced answers.

### Server AnswerPlan

Updated `/Users/uno/Downloads/cue/server/src/api/router.rs`.

- Added quick conceptual AnswerPlan detection.
- Short conceptual comparisons now become:
  - intent: `quick`
  - output: `compact`
  - lane: `instant`
- Compact output is capped at 512 tokens server-side.
- Server routing now preserves requested `instant` for compact non-hard answers.
- Server still upgrades requested `instant` to `deep` for coding, coding follow-up, system design, screen, or research when needed.

### Trace Logs

Added privacy-safe timing logs using request ids, short refs, session code, counts, and hashes instead of raw user text.

New daemon log events:

- `answer pipeline route start diagnostics`
  - `request_ref`
  - `session_code`
  - `route_primary`
  - `question_hash`
  - `question_chars`
  - `question_words`
  - `question_intent`
  - `context_was_empty`
  - `context_prepare_ms`
  - `overlay_card_ms`
  - `prep_total_ms`
  - `visible_context_count`
  - `attachment_ids`
- `managed provider stream starting`
  - provider, lane, max tokens, image count, system/user char counts, user hash
- `managed provider stream connected`
  - `stream_connect_ms`
- `managed provider stream first event`
  - `first_event_ms`, status/text/source shape
- `managed provider stream first text`
  - `first_text_ms`
- `managed provider stream finished reading`
  - `stream_total_ms`, answer chars, finished flag, source count, token totals
- `answer pipeline route completed diagnostics`
  - provider, route total, answer-start latency, pipeline total, attempt count, sources count

Existing failure diagnostics now also include `question_hash` so support can correlate failed answers without storing the full question in operational logs.

## Verification

Passed:

- `cargo test -p cue-daemon overlay_auto_routes_short_conceptual_questions_to_instant --quiet`
- `cargo test -p cue-daemon overlay_auto_keeps_code_generation_on_deep_lane --quiet`
- `cargo test -p cue-daemon answer_diagnostics_classify_question_and_text_shape_without_content --quiet`
- `cargo test --manifest-path server/Cargo.toml answer_plan_routing_ --quiet`
- `cargo test --manifest-path server/Cargo.toml answer_plan_short_conceptual_comparisons_use_quick_instant --quiet`
- `cargo check -p cue-daemon --quiet`
- `cargo check --manifest-path server/Cargo.toml --quiet`
- `git diff --check`

## Expected Result

For simple typed questions like:

- `Can you explain me the difference between LRU cache and SRU?`
- `How do you approach API versioning in your project?`

Bluey should choose the managed `instant` lane and emit enough diagnostics to identify:

- whether context prep delayed the request
- whether the server overrode the lane
- how long managed stream connection took
- how long until the first event and first text
- whether provider reading/token count was the slow part

For actual code prompts like:

- `Build me LRU cache in Python`
- `Give me Python code which solves Sudoku`

Bluey should continue using the deeper coding path.

## Not Done

This round does not deploy a production release by itself. It prepares the routing/logging fix and verifies it locally. A release/deploy round should build the signed artifacts, install/smoke the local binary, and promote through the normal Bluey deploy path.

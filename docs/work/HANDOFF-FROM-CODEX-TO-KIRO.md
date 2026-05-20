# Codex -> Kiro: Stage 18 + Overlay UX Follow-Up

## 1. Overall Verdict

🟡 **IMPLEMENTED, READY FOR KIRO REVIEW** — I cleared the two highest-priority pending managed-product gaps from the Stage 12-17 recap and then applied the requested overlay conversation UX pass:

- managed `/router/complete/stream` now exists and `BlueyManagedProvider` consumes it as SSE;
- managed billing metadata now survives the full path: server -> cloud client -> `cue-llm` -> Auto Router/speculative path -> daemon `CueResponse` -> SQLite -> dashboard/native overlay labels.
- native macOS overlay now has a compact ChatGPT-style conversation surface: user/transcript bubbles on the right, Bluey answers on the left, compact one-row composer controls, previous-recording drawer, rename flow, and an inline “How Bluey should answer” textbox.

This is not a full upstream-token streaming proxy yet. The server deliberately preserves the existing safe billing/idempotency lifecycle, then streams the metered terminal response as SSE deltas plus a final billing event. That gives the UI and managed client the streaming contract without reopening double-charge or unmetered-fallback risks.

## 2. What Codex Changed

### Managed streaming contract

- `server/src/api/router.rs`
  - Added `complete_inner()` shared by JSON and streaming handlers.
  - Added `POST /router/complete/stream`.
  - Emits OpenAI-compatible `data: {"choices":[{"delta":{"content":"..."}}]}` SSE chunks.
  - Emits `event: billing` carrying the exact `CompleteResponse`, then `data: [DONE]`.
- `server/src/api/mod.rs`
  - Registered `/router/complete/stream` behind the same auth and `limit_router_complete` rate limiter.
- `crates/cue-cloud-client/src/client.rs`
  - Added `auth_post_stream()`, preserving auth refresh and typed 402/429 error mapping before handing raw bytes to callers.
- `crates/cue-llm/src/bluey_managed.rs`
  - `supports_streaming()` now returns true.
  - `complete_stream()` calls `/router/complete/stream`, parses SSE frames, yields deltas, and converts the billing event into `LlmCostMetadata`.

### Cost metadata and card labels

- `crates/cue-llm/src/lib.rs`
  - Added `LlmCostMetadata`.
  - Added optional `cost` to `LlmResponse` and `LlmChunk`.
- `crates/cue-daemon/src/llm/*`
  - `AnswerLlm`, `RecapLlm`, and `WhatToAnswerLlm` preserve cost metadata from streaming and non-streaming providers.
- `crates/cue-daemon/src/db/mod.rs`
  - Adds missing `cue_responses` billing columns at migration time.
  - Persists/loads `cost_cents`, `balance_cents_after`, `provider`, `model`, `input_tokens`, and `output_tokens`.
- `crates/cue-dashboard/src/commands.rs`
  - Tauri `cue_response_chunk` payloads now carry optional cost metadata.
  - Speculative Bluey Auto merges draft + final managed lane costs before emitting/persisting the final response.
- `crates/cue-dashboard/ui/src/routes/Responses.tsx`
  - Response and in-flight cards render compact cost/balance/model pills.
- `crates/cue-core/src/cards.rs` and `crates/cue-core/src/overlay.rs`
  - Native overlay cards/update events carry optional `cost_label`.
- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
  - The macOS overlay renders final answer cost/usage labels in the status slot instead of the old permanent "cost syncing" text.

### Native overlay conversation UX

- `crates/cue-core/src/overlay.rs`
  - Added `OverlaySessionItem`, `SetSessions`, `SessionOpenRequested`, and `SessionRenameRequested`.
- `crates/cue-daemon/src/storage.rs`
  - Added meeting-history lookup and rename helpers so the overlay can continue or rename prior recordings.
- `crates/cue-daemon/src/app.rs`
  - Pushes recent recordings to the overlay on ready/session changes.
  - Handles open-session and rename-session overlay events.
  - Allows token-validated inline answer-instruction saves without opening a separate modal.
- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
  - Reduced expanded panel size to `590x510` and tightened header/composer controls.
  - Shows right-aligned user/question/transcript bubbles and left-aligned Bluey answers.
  - Adds a real recordings drawer with clickable rows and per-row rename buttons.
  - Adds inline “How Bluey should answer” textbox and save button.
  - Keeps attachments horizontally scrollable above the compact composer row.
  - Follow-up senior UX pass: answer cards now render as lighter left-side response blocks, the role/title row no longer overlaps, Return sends the composer,
    icon-only controls have tooltips, and recording rename now edits inline instead of opening an alert modal.

## 3. What Was Already Present And Verified

- Live wallet balance in overlay/dashboard from the prior Codex pass.
- 5-minute no-transcript audio auto-stop and final balance refresh.
- SMTP transactional delivery via `BLUEY_SMTP_*`.
- Managed server money path through `/router/complete`, `/router/embed`, `/router/transcribe`, Stripe Checkout/webhook, usage, metrics, and account actions.

## 4. Remaining Gaps

- True upstream-token streaming inside `bluey-server`: current stream endpoint streams after the safe managed completion finishes. To make it truly token-live, Stage 19 should add upstream streaming dispatch with usage accounting/trailer handling and mid-stream balance checks.
- Native overlay direct-provider cost is still estimated from token usage/latency; exact dollar labels are only available when managed billing metadata exists.
- Overlay visual QA still needs one real desktop click-through pass after rebuilding/installing the macOS helper, because headless tests cannot validate final pixel feel.
- Onboarding web pages on `bluey.sh` remain pending.
- SMTP production smoke still needs real credentials.
- Wiremock harness for Stripe/OpenAI/Anthropic/Deepgram remains pending.
- Backup rotation and deployment automation remain operational follow-ups.

## 5. What Codex Did Not Change

- No push.
- No history rewrite.
- No product naming / white-label wording changes.
- No Windows overlay parity work.
- No new pricing/product decisions.

## 6. Verification

Run by Codex in this pass:

```bash
cargo check -p cue-core -p cue-daemon -p cue-dashboard -p cue-router -p cue-llm
swift build -c release --package-path native/macos/cue-overlay
cargo test -p cue-router -p cue-daemon -p cue-llm -p cue-cloud-client --all-targets
cd server && cargo test
cd crates/cue-dashboard/ui && npm test && npm run build
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo build --all-targets --release
cargo test --all-targets
cd server && cargo clippy --all-targets -- -D warnings
cd server && cargo build --all-targets --release
swift build -c release --package-path native/macos/cue-whisper
git -P diff --check main..HEAD
cargo clippy -p cue-core -p cue-daemon --all-targets -- -D warnings
swift build -c release --package-path native/macos/cue-overlay
```

Observed counts:

- workspace `cargo test --all-targets`: 418 passed, 14 ignored.
- `cue-daemon`: 167 library tests passed, 2 ignored, plus integration suites passed.
- `overlay_production_path`: 24 passed after updating the inline-instructions contract.
- `cue-llm`: 36 passed.
- `cue-router`: 31 passed.
- `cue-cloud-client`: 6 passed.
- `server`: 58 passed.
- Dashboard Vitest: 15 passed.

## 7. Next Action For Kiro

Review this batch first:

1. `docs/work/HANDOFF-FROM-CODEX-TO-KIRO.md`
2. `docs/rounds/STAGES-12-17-RECAP-FOR-CODEX-REVIEW.md`
3. Diff for:
   - `server/src/api/router.rs`
   - `crates/cue-llm/src/bluey_managed.rs`
   - `crates/cue-router/src/speculative.rs`
   - `crates/cue-dashboard/src/commands.rs`
   - `crates/cue-dashboard/ui/src/routes/Responses.tsx`
   - `crates/cue-daemon/src/db/mod.rs`
   - `crates/cue-daemon/src/storage.rs`
   - `crates/cue-core/src/overlay.rs`
   - `native/macos/cue-overlay/Sources/cue-overlay/main.swift`

Recommended next implementation round:

```text
Next:
1. Replace synthetic managed SSE with true upstream streaming in bluey-server.
2. Add mid-stream balance checks / balance_exhausted SSE event.
3. Add wiremock integration tests for OpenAI/Anthropic streaming usage trailers.
4. Add bluey.sh onboarding/account pages.
5. Add deployment backup rotation + clean operational smoke.
6. Rebuild/install the macOS overlay helper and do one visual QA pass on `bluey on`.
```

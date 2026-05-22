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

## 8. Addendum: Cloud Sync, Cloud RAG, STT Auth Foundation

Date: 2026-05-21

After the overlay/canvas pass, Codex also implemented the first managed-cloud data plane needed for the 1,000-user product shape.

Review doc:

- `docs/rounds/CLOUD-SYNC-RAG-STT-AUTH-FOR-KIRO.md`

What changed:

- Server migration `0012` adds cloud session, transcript, response, context, RAG, and STT session tables.
- Server endpoints now exist for:
  - `POST /sync/batch`
  - `GET /sync/sessions`
  - `GET /sync/sessions/:session_id`
  - `POST /rag/query`
  - `POST /stt/session`
  - `GET /stt/relay`
- Cloud client has typed methods for those endpoints.
- Daemon `CloudSyncNow` now does a real upload from local sessions instead of returning “sync client is scaffolded but not wired”.
- Uploaded data includes local session metadata, transcripts, context previews, persisted answer/cost/artifact metadata, conversation fallback answers, and lexical RAG chunks.
- The default chunked real-audio path now uses managed `/router/transcribe` when a user is logged in and no developer STT key is configured, so customer builds do not need desktop provider secrets.
- CLI now has testable cloud commands:
  - `bluey cloud sync`
  - `bluey cloud sessions`
  - `bluey cloud show <session_id>`
  - `bluey cloud rag <query>`
- Existing account commands now use the account API URL saved by `bluey login`, not only the default production URL.
- `POST /stt/session` establishes the safe contract: server checks balance/trial state and returns a Bluey-scoped session token/relay URL, never a static provider key.
- `/stt/relay` requires the normal Bearer account token plus that session token, single-claims it for the same account, proxies WebSocket frames to Deepgram with the server-held key, records usage, and bills elapsed seconds.

Verification added/run:

```bash
cargo fmt --all
cargo test -p cue-cloud-client
cargo test -p cue-daemon cloud::sync
cd server && cargo test sync_batch_session_bundle_and_rag_roundtrip
cd server && cargo test sync_batch_round_trips_session_bundle_and_rag
cd server && cargo test validate_rejects_empty_or_huge_batches
cd server && cargo test random_token_is_url_safe_and_long
cargo check -p cue-cli
cargo clippy -p cue-daemon --all-targets -- -D warnings
cargo clippy -p cue-cli --all-targets -- -D warnings
cd server && cargo clippy --all-targets -- -D warnings
cargo check -p cue-daemon
cd server && cargo check
cd server && cargo test stt::tests
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo build --all-targets
cargo build --all-targets --release
cargo test --all-targets
cd server && cargo test
cd server && cargo build --all-targets --release
cd crates/cue-dashboard/ui && npm test
cd crates/cue-dashboard/ui && npm run build
swift build -c release --package-path native/macos/cue-overlay
swift build -c release --package-path native/macos/cue-whisper
git diff --check main
```

Still intentionally deferred:

- continuous streaming daemon provider over managed `/stt/relay`;
- Postgres + pgvector production migration;
- dashboard UI for cloud session history / continue-from-cloud;
- background live sync loop;
- cloud binary document storage;
- provider-cost optimizer config in server routing.

## 9. Addendum: Security Hardening + Client Code-Protection Posture

Date: 2026-05-21

User asked for hard protection so customers cannot reverse engineer or read the
desktop code.

Codex's engineering stance: we should not promise that installed desktop code is
unreadable. The durable security model is to treat the desktop as untrusted and
keep provider keys, wallet/metering, routing policy, and authorization on
`bluey-server`.

Review doc:

- `docs/rounds/SECURITY-HARDENING-CODE-PROTECTION-FOR-KIRO.md`

What changed:

- `docs/SECURITY-HARDENING.md` was rewritten to reflect the current managed
  cloud/server authority model instead of the older local-first/BYOK posture.
- `AppPaths::ensure()` now creates Bluey data/config/runtime directories with
  private Unix/macOS permissions (`0700`).
- Meeting archive directory now uses private directory permissions.
- Active and archived meeting JSON files are written with private Unix/macOS
  file permissions (`0600`).
- File-backed local SQLite DBs are created/normalized with private Unix/macOS
  file permissions (`0600`), and WAL/SHM sidecars are normalized when present.
- Workspace release profile now strips symbol tables and uses thin LTO so
  shipped binaries expose less incidental implementation detail.
- Overlay helper verification now checks a colocated SHA-256 sidecar when one
  is present, while keeping local/dev builds usable without a sidecar.
- Added a Stripe webhook regression test proving multi-`v1` signature headers
  are accepted when any `v1` matches.
- Cloud-client, device-flow, Stripe, auth-link, and deep-link diagnostics were
  tightened so tokens/codes/secret URLs are redacted or suppressed by default.
  `BLUEY_DEV_LOG_AUTH_LINKS=1` is the explicit local-dev escape hatch for
  verification/reset URLs when SMTP is not configured.
- Added regression tests for directory, meeting JSON, and SQLite file
  permissions plus log redaction helpers.

Still intentionally not claimed:

- desktop binary/code unreadability;
- full anti-reverse-engineering resistance;
- encrypted local DB;
- signed release manifest;
- certificate pinning;
- binary self-integrity.

Recommended next security round:

1. Signed release manifest with embedded ed25519 public key.
2. `bluey check-update` / update installer verification against that manifest.
3. Local DB encryption using OS keyring-derived key.
4. Windows ACL parity for data/config/runtime directories and local DB files.
5. Dependency audit CI.

Pinky parity call:

- Useful Pinky controls already present or now covered: bcrypt/JWT auth,
  hashed refresh tokens, ownership-gated billing/cloud data, parameterized SQL,
  capture-excluded overlay paths, capture-visible debug gate, Stripe multi-`v1`
  webhook signatures, checksum install path, and helper sidecar verification.
- Better Bluey-specific posture: provider keys and metering are server-owned by
  design, so the desktop has less sensitive material than a BYOK-only product.
- Relevant Pinky leak-review point: operational docs, review files, logs,
  session links, device codes, and support diagnostics are sensitive even when
  customer passwords/provider keys are protected. Keep those private; do not
  publish the full docs/reviews tree as customer-facing docs.
- Still not honest to claim: "unbacktraceable" or "impossible to inspect".
  Customer-safe wording should be: "low-profile native overlay, capture-excluded
  in normal OS capture paths, server-managed provider access, account-scoped
  cloud memory, hardened auth, and audited release controls."

## 10. Addendum: macOS Overlay Command-Bar Polish

Date: 2026-05-21

User reported that clicking Listen made the overlay feel like it was growing
too large, and asked for a more premium bottom bar.

Changed in `native/macos/cue-overlay/Sources/cue-overlay/main.swift`:

- The bottom composer row now reads as a single command bar:
  `Start Bluey`, compact opacity control, rounded `Ask anything...` input,
  `Style`, `Docs`, `Screen`, and larger accented `Answer`.
- The tiny icon-only bottom buttons were replaced with labeled controls so new
  users can understand the flow without memorizing icons.
- The composer input now has its own rounded dark field styling instead of
  floating text inside the bar.
- Canvas auto-open is now stricter: explicit server artifacts still open the
  canvas, but generic document/context/long cards no longer force the whole
  window to expand. Plain listen/recording system cards should not trigger the
  "screen keeps growing" feeling.
- Canvas width was reduced from `360` to `310`, and auto expansion was capped at
  `820px` instead of `980px`.
- The expanded panel default size moved from `590x510` to `720x520` so the
  one-row command bar has enough room without needing an immediate resize.

Verification:

- `swift build -c release --package-path native/macos/cue-overlay`
- `bash native/macos/cue-overlay/build.sh`
- `git diff --check -- native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- Started Bluey normally after QA; status reports
  `overlay_capture_excluded: true`.

Notes for Kiro:

- The local capture-visible QA path remains dev-gated behind
  `BLUEY_DEV_OVERLAY=1`; the normal `bluey on` path is capture-excluded.
- There are several stale unkillable historical `bluey-overlay-macos` processes
  on this machine from earlier experiments. They predate this patch and can
  make visual testing confusing; a reboot will clean them.

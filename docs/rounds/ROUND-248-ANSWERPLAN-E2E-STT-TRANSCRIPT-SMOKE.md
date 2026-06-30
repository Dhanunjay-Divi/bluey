# Round 248 - AnswerPlan E2E, STT Transcript, and Smoke Gates

Date: 2026-06-30
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The owner asked to close the remaining end-to-end gaps from live testing:

- add an AnswerPlan eval suite using the real prompt shapes from screenshots
- add privacy-safe logs for answer routing, transcript shape, canvas detection, web search status, provider, and billing cents
- stop duplicate/empty transcript sends from becoming paid placeholder answers
- make code/canvas behavior more deterministic for code asks and follow-ups
- add live staging smoke coverage before deploy
- reset the active test account balance to `$15.00`

## Root Cause

Several issues came from different layers sharing weak contracts:

- AnswerPlan rules were improving, but the exact live tester prompts were not all pinned by evals.
- The server logged provider and billing details, but not enough answer-plan/transcript/artifact/web-search metadata to debug private user scenarios without reproducing content.
- The overlays accepted low-signal transcript fragments such as one-word ASR output or placeholder live-caption text.
- Windows could still send a generic live-caption fallback when transcript context existed but no usable transcript question was available.
- Managed live STT reserved a 10-minute relay window per source, which is safe but makes quick Listen start/stop tests look like a large temporary balance drop before settlement catches up.

## Changes

### Server AnswerPlan and Logs

- Added privacy-safe request diagnostics:
  - `user_chars`
  - `question_chars`
  - `question_hash`
  - `transcript_chars`
  - `transcript_hash`
  - `transcript_source_labels`
  - `generic_live_transcript_prompt`
- Added those diagnostics to `managed chat answer plan resolved` logs.
- Expanded final `managed chat completed and billed` logs with:
  - `answer_plan_source`
  - `answer_plan_ai_attempted`
  - `answer_plan_ai_reason`
  - `answer_intent`
  - `answer_output`
  - `answer_confidence`
  - canvas artifact type/confidence
  - web search attempted/search count/source count/skipped reason
  - customer and Bluey cost fields already present
- Tightened AnswerPlan priority:
  - missing-context prompts cannot trigger web search just because they include words like `current`
  - system-design intent beats generic code keywords like queue/cache/database
  - topic-reset wording does not make unrelated short questions inherit previous context
- Strengthened code-answer prompt style:
  - explicit code asks should start with fenced working code, then explain
  - code follow-ups should preserve the existing artifact unless a new one is requested
- Added eval coverage for:
  - LRU code
  - Fibonacci topic reset
  - Fibonacci complexity follow-up
  - tell-me-about-yourself behavioral routing
  - Secret Passage Ranch web-research routing
  - empty live-caption prompt
  - live-caption placeholder
  - missing attached docs
  - privacy-safe transcript diagnostics

### macOS Overlay Transcript Lifecycle

- Added a meaningful-transcript gate before a live caption can become an answer question.
- Rejected low-signal fragments such as `sure`, `okay`, placeholders, and one-word filler.
- Kept longer transcript tails usable when they contain enough meaningful content.
- Stopped falling back to raw transcript text if the compact visible question was not meaningful.

### Windows Overlay Transcript Lifecycle

- Added the same meaningful-transcript gate.
- Transcript-derived sends now preserve a `Mic:`, `System:`, or `Audio:` prefix so server diagnostics can count/hash transcript source lines consistently with macOS.
- Removed the generic live-caption fallback from empty Answer sends.
- If transcript context exists but no usable transcript question exists, Windows now emits `ask_answer_blocked` instead of sending a paid placeholder prompt.
- If no text, no transcript, and no context chips exist, Windows blocks locally instead of asking the server.

### Managed STT Reservation UX

- Changed the daemon's default managed STT relay reservation request from 10 minutes to 120 seconds.
- Added operator override:
  - `BLUEY_MANAGED_STT_RELAY_SECONDS`
  - fallback alias `BLUEY_STT_RELAY_SECONDS`
- Clamp remains `30..600` seconds client-side; server hard max remains in place.
- Added log line `live STT relay reservation created` with requested and reserved seconds.
- Existing server settlement still bills actual elapsed audio and refunds unused reserved credit.

### Staging Smoke Gate

- Added `scripts/bluey-e2e-staging-smoke.sh`.
- Default checks are no-cost:
  - server AnswerPlan tests
  - web-search guard tests
  - streaming idempotency guard tests
  - cloud preflight
  - macOS overlay parse
  - Windows overlay syntax when cross compiler exists
  - public `/health`
- Authenticated checks run when `BLUEY_API_TOKEN` is set.
- Paid live provider answer smoke runs only when `BLUEY_RUN_PAID_SMOKE=1`.

### Test Balance Reset

- Live droplet backend checked: SQLite at `/opt/bluey-api/bluey.db`.
- Active test account identified:
  - `codex-smoke-20260608183100@bluey.sh`
  - account id `5fa999b4-fb97-4350-80b6-3b9cc09e27b5`
- Balance was `$12.68`, `reserved_cents = 0`.
- Added a `$2.32` internal credit batch and set balance to `$15.00` in one transaction.
- Live DB did not yet have the newer `balance_ledger_entries` table, so this used the older live-compatible `credit_batches` audit path.

## Files Changed

- `server/src/api/router.rs`
- `crates/cue-daemon/src/app.rs`
- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `native/windows/cue-overlay/main.c`
- `scripts/bluey-e2e-staging-smoke.sh`
- `docs/rounds/ROUND-248-ANSWERPLAN-E2E-STT-TRANSCRIPT-SMOKE.md`
- `docs/rounds/BLUEY-COMPACTION-HANDOFF-2026-06-25.md`

## Verification

Passed:

```bash
chmod +x scripts/bluey-e2e-staging-smoke.sh
bash -n scripts/bluey-e2e-staging-smoke.sh
cargo fmt --manifest-path server/Cargo.toml
cargo fmt
swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift
cargo test --manifest-path server/Cargo.toml answer_plan --quiet
cargo test --manifest-path server/Cargo.toml web_search --quiet
cargo test --manifest-path server/Cargo.toml answer_request_diagnostics --quiet
cargo test --manifest-path server/Cargo.toml generic_live_caption_prompt --quiet
cargo test -p cue-daemon managed_stt_relay_requested_seconds_defaults_and_clamps --quiet
cargo test -p cue-daemon stt_relay_websocket_url --quiet
x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c -Inative/windows/cue-overlay -DUNICODE -D_UNICODE
git diff --check
```

Live balance verification:

```text
credited|5fa999b4-fb97-4350-80b6-3b9cc09e27b5|1268|232|internal:codex-test-balance-reset-20260630:4f9dfdd0d0941c6f
account|5fa999b4-fb97-4350-80b6-3b9cc09e27b5|codex-smoke-20260608183100@bluey.sh|1500|0|0
```

## Current State

- Local deterministic AnswerPlan is stronger and better covered.
- Server logs now provide the metadata needed to investigate private user scenarios without storing private transcript/question text in logs.
- macOS and Windows both block empty or low-signal transcript sends locally.
- Managed STT quick start/stop should cause a much smaller temporary reservation.
- The active test balance is back to `$15.00`.

## Remaining QA and Gates

- Rebuild and hot-install the local macOS binaries before manual overlay QA.
- Deploy server/daemon changes to staging before expecting live users to see them.
- Run `scripts/bluey-e2e-staging-smoke.sh` on staging with production-like env.
- Run paid smoke only with owner approval:

```bash
BLUEY_API_TOKEN=... BLUEY_RUN_PAID_SMOKE=1 scripts/bluey-e2e-staging-smoke.sh
```

- Live manual checks after build/deploy:
  - click Listen then Stop repeatedly and confirm balance settles back after unused reservation
  - speak `Explain LRU cache` and confirm transcript/question text is not the generic live-caption prompt
  - ask `I want the code in Python` and confirm code artifact opens/persists
  - ask unrelated new question and confirm old canvas context is not dragged in
  - ask `Secret Passage Ranch` with web-search provider configured and confirm source statuses/chips

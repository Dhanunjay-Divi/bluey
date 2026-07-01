# Round 273 - Incomplete Answer Guard

## Trigger

The owner shared a screenshot where Bluey answered a refrigeration / ML design question and stopped in the middle of a Markdown table:

```text
Data Architecture & Feature Engineering

| Feature Type | Description | Rationale |
| :--- | :--- | :---
```

This looked like a UI clipping problem at first, but the saved local session showed the answer text itself ended at the table separator. The answer had already been persisted as if it completed.

Continuity anchor: backup thread id `019e133e-d92a-7830-8df0-3a050a4e22f6`.

## Root Cause

The affected saved session was from an older local Bluey build (`0.1.22`) and the provider path reported a completed managed answer. Bluey accepted that final event and saved the answer even though the final text shape was structurally incomplete.

Separate logs around the same period also showed cloud sync / R2 errors, but those did not explain the broken visible answer. The local persisted answer was already truncated.

## Fix

- Added a final-answer integrity guard in `crates/cue-daemon/src/app.rs`.
- The guard now rejects answers ending with:
  - unclosed code fences
  - unfinished Markdown table separators
  - dangling Markdown headings
  - bare list markers
- Added `answer_incomplete_reason` logging to answer completion diagnostics.
- Added provider-path warnings for incomplete final text before saving or marking the answer complete.
- Mapped incomplete-answer errors to the retryable incomplete-stream user message.
- Updated the answer-shape prompt to avoid Markdown tables in streamed chat and prefer short bullets or plain lines.
- Bumped the desktop workspace version to `0.1.27`.

## Verification

- `cargo test -p cue-daemon answer_diagnostics_classify_question_and_text_shape_without_content --locked`
- `cargo test -p cue-daemon provider_prompt_parts --locked`
- `cargo test -p cue-daemon --lib --locked`
  - `281 passed; 0 failed; 2 ignored`
- `cargo test -p cue-llm --locked`
  - `46 passed`
- `cargo check -p cue-daemon --offline`
- `cargo fmt`
- Release dry-run scan passed:
  - dev flag scan
  - secret scan
  - manifest generation
- Published live macOS arm64 artifact:
  - `https://bluey.sh/latest.json` reports `0.1.27`
  - SHA256: `75651a30e1983f1183b3ee32297b0a6e075eba0af712ec4477a8031bd80237aa`
  - `https://bluey.sh/install.sh` serves the shell installer, not HTML
- Local update installed and restarted:
  - `/Users/uno/.local/bin/bluey --version` -> `bluey 0.1.27`
  - `/usr/local/bin/bluey --version` -> `bluey 0.1.27`
  - `bluey status` showed the daemon running after restart

## Current State

Bluey should no longer silently persist an answer that ends mid-table or mid-code as complete. If the provider still cuts off in a structurally incomplete shape, Bluey returns the incomplete-answer retry path instead of saving a bad final answer.

The released artifact is macOS arm64 only. The daemon-side guard is shared Rust code and should apply to Windows daemon builds when the next Windows artifact is produced.

## Remaining QA / Gates

- Add auto-continuation for incomplete answers so Bluey can recover by continuing the same response instead of only asking for retry.
- Add a live regression prompt that intentionally produces table-like content and verifies Bluey uses bullets in overlay chat.
- Audit the separate cloud sync / R2 errors seen in nearby logs; they were not the cause of this broken answer, but they still deserve a storage reliability round.

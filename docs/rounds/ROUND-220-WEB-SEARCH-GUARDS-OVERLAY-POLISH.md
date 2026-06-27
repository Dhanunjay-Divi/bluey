# Round 220 - Web Search Guards and Overlay Polish

Date: 2026-06-27 13:13 EDT
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The owner reported:

- asking about "secret passage ranch" still answered from session context instead of doing useful web search
- the answer-card `A|` paste icon did not make sense
- the History drawer should show a clear loading state as soon as the user clicks it
- mouse cursor inside the overlay should stay as a normal pointer instead of becoming an I-beam over text/input areas
- web search must be protected from excessive/looping usage

## Root Cause

Web search:

- The server already had a managed web-search lane, but it is gated by provider configuration such as `BLUEY_WEB_SEARCH_PROVIDER`, `BLUEY_WEB_SEARCH_API_KEY`, or provider-specific keys.
- If the planner decided a question needed web search but the deployed server had no real search provider configured, the server skipped search and the model only saw local/session context.
- The skip status existed, but the final model prompt did not explicitly tell the model that web search was unavailable for the request, so the final answer could look like Bluey simply failed to search.
- Search had trial caps, repeated-query guard, timeout, source cap, and metering, but it needed a stronger pre-provider safety rail for loops and no-credit paid accounts.

Overlay:

- The paste-into-behind-app action used the `text.cursor` SF Symbol, which rendered like `A|` and looked unrelated to the action.
- History could be opened before the first `set_sessions` event arrived, so the drawer looked empty rather than loading.
- AppKit installs I-beam cursors for text views/fields by default, including overlay composer/canvas/edit areas.

## Fix

Web search behavior:

- When the AnswerPlan needs web search and web search was attempted but returned no usable sources, the server now adds an explicit hidden instruction:
  - do not imply web search succeeded
  - say web search was unavailable for this request when public/current information is required
  - give the next useful step without asking for unrelated session documents
- The existing status stream still emits `Searching web...`, `Reading N sources...`, `Web search used...`, or a friendly skipped reason.

Web search usage controls:

- Paid accounts now preflight search credits before calling the search provider.
- Trial accounts keep a small durable daily cap.
- Paid accounts are not put behind a low daily search count; search remains credit-metered.
- Added a configurable durable hourly safety rail:
  - `BLUEY_WEB_SEARCH_ACCOUNT_HOURLY_LIMIT`
  - default `120`
  - `0` disables the hourly rail
- Added a configurable short-window burst guard before provider calls:
  - `BLUEY_WEB_SEARCH_BURST_LIMIT`
  - default `12`
  - `BLUEY_WEB_SEARCH_BURST_WINDOW_SECS`
  - default `60`
- Existing repeated-identical-query guard remains in place.
- Customer-facing skipped labels stay neutral and do not mention internal abuse/fraud/scraping language.

Overlay UI:

- Replaced the confusing answer-card paste icon with a keyboard-style icon when available, with fallback symbols if needed.
- Added `Loading...` inside the History drawer until the first session list arrives.
- Added arrow-cursor text view/field subclasses and cursor rect overrides for:
  - composer
  - canvas text
  - answer-style text
  - inline rename field
  - base overlay area
- Resize cursors still appear on border resize hit zones.

## Mac / Windows Parity

- Web-search routing, metering, skipped-copy, and safety controls are server-side and shared by Mac/Windows clients.
- The icon/loading/cursor work is macOS native overlay work.
- Windows overlay does not have the same rich answer-card/canvas UI in this branch; Windows native syntax was still checked.

## Verification

Passed:

- `cargo fmt --manifest-path server/Cargo.toml`
- `cargo test --manifest-path server/Cargo.toml web_search --lib`
- `cargo test --manifest-path server/Cargo.toml answer_plan_prompt_explains_unavailable_web_search --lib`
- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
- `git diff --check`
- `BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug native/macos/cue-overlay/build.sh`
- `BLUEY_BIN=/Users/uno/Downloads/cue/target/debug/bluey scripts/bluey-visible-local.sh`
- `/Users/uno/Downloads/cue/target/debug/bluey status`

Latest local visible status:

- daemon pid `26805`
- overlay visible `true`
- overlay capture excluded `false`
- screen capture active `false`

## Current State

- Local visible QA mode is running from the latest debug overlay build.
- Web search will still require a real provider key/config on the deployed server before it can return public sources.
- If the provider is missing or skipped, Bluey should now be honest in the final answer instead of silently acting like only session context exists.
- Excessive search loops are guarded before provider calls.

## Remaining QA / Gates

- Configure a real managed search provider in staging/production before expecting live web-backed answers.
- Run a live `secret passage ranch` style query after provider env is configured and confirm:
  - `Searching web...`
  - `Reading N sources...`
  - source-backed answer with source chips/card
  - cost label includes web-search usage
- Before release/upload, return from visible QA mode to normal capture-excluded mode and verify `overlay_capture_excluded: true`.

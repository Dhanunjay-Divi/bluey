# ROUND-367 Debug Stream Recovery Observability

Date: 2026-07-05
Backup thread: `019e133e-d92a-7830-8df0-3a050a4e22f6`
Branch: `codex/bluey-web-ui-parallel-20260704`

## Why This Round Happened

The overlay appeared to pause or get stuck after a coding follow-up such as "can I get code in go?" The visible chat showed only a short prose lead-in and a dangling `Code` heading while the code panel stayed on the previous artifact. This made it look like Bluey stopped thinking or failed to stream.

## What The Logs Showed

The running app was not the freshly installed release binary. It was the local debug daemon:

- Running process: `/Users/uno/Downloads/cue/target/debug/bluey-daemon`
- Old log version during the failure: `0.1.82`
- Current rebuilt debug version after this round: `0.1.86`

For the failing follow-up request, the daemon logs showed:

- `question_intent="code_or_debug"`
- `route_primary="bluey_managed/vision"`
- `answer_incomplete_reason="unclosed_code_fence"`
- artifact type was `code`
- recovery produced a short visible answer that still had `answer_incomplete_reason="dangling_heading"`
- overlay first visible stream update happened only after the provider/recovery path finished, so the UI looked frozen for roughly the whole provider latency window

Root cause: the managed provider had enough code artifact data, but the overlay recovery path stripped the broken code fence and left a bare `Code` heading. The chat preview also hid normal-size code too aggressively, so a useful recovered answer collapsed to almost nothing.

## Fixes

- Added immediate visible progress cards before provider routing:
  - `Reading screen context...`
  - `Reading attached files...`
  - `Reading live transcript...`
  - `Checking saved context...`
  - `Preparing answer...`
- Added logs for answer pipeline creation and managed provider stream start.
- Added richer recovery logs: raw answer chars, recovered answer chars/lines, recovered incomplete reason, artifact type, and artifact body chars.
- Fixed code recovery so unclosed streaming fences do not leave dangling `Code` headings.
- Relaxed chat code preview limits so normal follow-up code can be shown in chat while very large code stays in the code panel.
- Added C++ language detection for recovered code previews.
- Tightened the internal-disclosure guard to inspect only the actual `Question:` portion when the server/daemon prompt also contains session context, preventing false blocks on normal coding follow-ups.
- Updated behavioral routing guard so explicit "give/write/show/provide/generate/convert/translate ... code" requests do not get treated as behavioral interview answers.

## Tests

Passed:

- `cargo test -p cue-daemon visible_answer_body -- --nocapture`
- `cargo test -p cue-daemon code_artifact_recovery_removes_dangling_code_heading -- --nocapture`
- `cargo test -p cue-daemon internal_disclosure -- --nocapture`
- `cargo test -p cue-daemon answer::tests::allows_coding_followup_when_session_context_mentions_prompt_words -- --nocapture`
- `cargo test -p cue-daemon answer::tests::refuses_explicit_internal_prompt_request_with_context -- --nocapture`
- `cargo test --manifest-path server/Cargo.toml internal_disclosure_guard_allows_coding_followup_context -- --nocapture`
- `cargo test --manifest-path server/Cargo.toml internal_disclosure_requests_are_blocked -- --nocapture`
- `cargo build -p cue-cli -p cue-daemon`

## Local Runtime State

Rebuilt and restarted the local debug daemon the user is actually running:

- Debug CLI: `./target/debug/bluey --version` => `bluey 0.1.86`
- New daemon pid after restart: `31233`
- New meeting id after restart: `93ee6c7c-4fb5-4f21-b212-34159734fc4f`
- Log file now includes `version="0.1.86"` startup rows.

## Follow-Up Notes

- Startup logs also showed a cloud sync `500 Internal Server Error`; this is not the stuck-code root cause, but it should be audited separately because it can affect saved sessions/history.
- The next manual repro should use the rebuilt debug daemon and check for `answer pipeline created visible progress card` plus `managed provider stream starting` before provider output.

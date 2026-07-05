# ROUND-366-CODE-FOLLOWUP-GUARD-RECOVERY

Date: 2026-07-05

Backup thread id: 019e133e-d92a-7830-8df0-3a050a4e22f6

## Why this round happened

The overlay showed normal coding follow-ups failing with:

`Bluey could not complete that answer. Ref: 613D0075`

The visible user request was ordinary coding text such as `can you write go code`, but the request also included attached screen/session context. The internal-disclosure safety guard was scanning the combined text, so words like `prompt` or `instructions` inside the attached problem context could falsely block a coding answer.

The same session also showed answers ending with a dangling `Code` label while the code artifact existed in the code panel. That made it look like Bluey failed to provide code even when a code artifact had been recovered from an incomplete stream.

## Root cause

- Server and local guards treated full composed context as the direct user request.
- Code artifact recovery stripped fenced code for the canvas, but an unclosed streaming fence could leave an empty `Code` heading in chat.
- Normal interview-size code was hidden from the chat too aggressively, forcing the user to notice/open the code panel.

## Changes made

- Restricted internal-disclosure request checks to the explicit `Question:` text when a composed request includes attached context.
- Kept explicit internal-prompt requests blocked even when context is attached.
- Mirrored the guard fix in the daemon local/fallback paths.
- Removed dangling empty `Code` headings when recovering from unclosed streamed code fences.
- Show normal-size code blocks inline in the chat while still preserving the code panel.
- Keep very large code in the code panel and show a clear chat note instead of an empty stub.
- Added C++ language inference for recovered inline code previews.

## Verification

Passed:

- `cd server && cargo test internal_disclosure_guard -- --nocapture`
- `cargo test -p cue-daemon internal_disclosure -- --nocapture`
- `cargo test -p cue-daemon code_artifact -- --nocapture`
- `cargo test -p cue-daemon visible_answer_body -- --nocapture`
- `cargo test -p cue-daemon allows_coding_followup -- --nocapture`
- `cargo test -p cue-daemon refuses_explicit_internal_prompt_request_with_context -- --nocapture`
- `cargo test -p cue-daemon --lib`
- `git diff --check`

Also passed:

- Server unit tests via `cd server && cargo test --lib`
- Release build for local macOS binaries: `cargo build --release -p cue-daemon -p cue-cli`
- Release build for server on Linux droplet: `cd /opt/bluey-builds/round-366/server && cargo build --release`
- Local hot install: backed up old CLI/daemon to `/Users/uno/.bluey/bin/backups/round-366-20260705025343`, installed `bluey 0.1.92` and `bluey-daemon 0.1.92`, then verified `bluey on` and `bluey status`.
- Prod server deploy: built a Linux x86_64 ELF on the droplet, installed it to `/usr/local/bin/bluey-server`, restarted `bluey-api.service`, and verified public `https://bluey.sh/health`.

Known unrelated test gap observed:

- `cd server && cargo test --test integration_e2e` still has mocked router streaming/fallback failures around provider route expectations and SSE error behavior. These failures reproduce outside the internal-disclosure/code-recovery path and should be handled as a separate router integration-test hardening round.

Deployment note:

- A first prod deploy attempt used the local macOS ARM server binary and systemd rejected it with `Exec format error`. The previous prod binary was restored from `/opt/bluey-deploy-backups/round-366/` immediately, health returned to 200, then the correct Linux binary was built and deployed. Current prod health is OK and `/usr/local/bin/bluey-server` is a Linux x86_64 ELF with SHA256 `219de7730d1e0a009bd6fa350c662b43aab204f7042f588600aedfd137ce5a55`.

## Expected behavior after this round

- A prompt like `can you write go code` with screen context mentioning `prompt` or `instructions` should not fail as `internal_disclosure_blocked`.
- Requests explicitly asking for Bluey's private prompts/instructions should still be refused.
- Coding answers should not show a bare `Code` heading with no code.
- Normal-size coding answers should show a fenced code block in chat and keep the code artifact button/panel available.
- Very large code stays in the panel with a clear chat note.
- Direct coding questions inside an interview/domain context should stay code-focused rather than flipping into behavioral-interview answer mode.

## Follow-up watch items

- The logs around this session also showed transient cloud object upload failures and local RAG embedding-key warnings. They were not the direct cause of `613D0075`, but should stay on the reliability audit list.
- If users still see stale code in the right panel, the next fix should make artifact selection strictly follow the latest answer card and clear stale artifact selection on failed answers.

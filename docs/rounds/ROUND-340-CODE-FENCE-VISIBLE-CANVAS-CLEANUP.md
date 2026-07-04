# Round 340 - Code Fence Visible Canvas Cleanup

Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

Branch: `codex/bluey-overlay-spacing-20260626`

## Trigger

The owner showed a coding answer where Bluey's chat card rendered code as smashed prose (`Codecppclass...`) and the code canvas looked like a chopped or awkward fragment instead of a clean full implementation.

## Root Cause

- Round 339 rejected obvious inner-loop fragments, but Bluey still trusted provider markdown too much.
- If a provider emitted a malformed fence such as `Code```cppclass Solution...`, the parser could miss the opening fence or keep a noisy `Code` heading in the visible answer.
- The daemon's final visible-answer cleanup returned the raw answer unchanged whenever a code block existed, so malformed or large code blocks could remain in the chat card even when a clean code canvas was available.
- One-line C-like code blocks were not lightly reflowed before being placed in the code canvas.

## Fix

- Server and daemon code-fence parsing now detects fences that appear after a heading on the same line.
- Malformed language openings like `cppclass Solution` and `pythonfrom typing` are split into a language tag plus actual code.
- Same-line opening and closing fences are handled.
- Code blocks that are C-like and compressed onto one line are lightly reflowed around braces and semicolons so the canvas is readable.
- When a code canvas artifact exists, the final visible chat body strips large/non-trivial fenced code blocks and keeps the readable approach, explanation, complexity, and edge-case prose.
- Small snippets can still be shown inline, but non-trivial interview/algorithm code belongs in the code canvas.
- Managed API final responses now return artifact-aware visible text so the completed card can replace rough streamed deltas with a clean final body.

## Product Rule

For coding answers, Bluey should behave like:

1. Chat: approach, explanation, complexity, edge cases.
2. Code canvas: complete code with wrapper/signature/imports/helper methods.
3. No smashed `Codecppclass` text.
4. No pretending a malformed or unreadable code fence is a good answer.

## Verification

Passed locally:

```bash
cargo fmt --check
cargo test --manifest-path server/Cargo.toml response_artifact_ --lib -- --nocapture
cargo test --manifest-path server/Cargo.toml visible_response_text_strips_code_when_canvas_exists --lib -- --nocapture
cargo test -p cue-daemon answer_overlay_artifact_ -- --nocapture
cargo test -p cue-daemon visible_answer_body_strips_large_code_when_canvas_exists -- --nocapture
cargo check --manifest-path server/Cargo.toml --bin bluey-server
cargo check -p cue-daemon -p cue-cli
BLUEY_UPDATE_PUBKEY="$(cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64)" make package-darwin-arm64
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem PUBLISH_DO=1 PUBLISH_HOST=root@165.227.77.152 PUBLISH_PATH=/var/www/bluey scripts/publish-bluey-release.sh
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem scripts/bluey-release-live-verify.sh 0.1.79
curl -fsSL https://bluey.sh/install.sh | bash
/Users/uno/.bluey/bin/bluey --version
/Users/uno/.bluey/bin/bluey-daemon --version
```

## Deployment

Desktop release `0.1.79` is live on `bluey.sh`.

Darwin arm64 artifact:

`https://bluey.sh/releases/v0.1.79/bluey-0.1.79-darwin-arm64.tar.gz`

Artifact SHA256:

`8eb3d665bd43b2c228bdf41be1ae9702a4cc1d129db979cc6fcc585c6b663fad`

Release verification passed:

- `latest.json` signature verification
- installer MIME checks
- Darwin arm64 artifact SHA verification
- unpacked `bluey` and `bluey-daemon` version checks for `0.1.79`
- public installer smoke installed `0.1.79` locally

Production API rollout is pending the committed-source server build and restart.

## Windows Parity

The daemon parser and final-card cleanup are shared Rust code, so the Windows desktop build receives the same behavior when packaged from this branch. The server-side parser applies to both macOS and Windows clients immediately after API deployment.

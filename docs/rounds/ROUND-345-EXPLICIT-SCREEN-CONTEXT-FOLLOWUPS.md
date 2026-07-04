# Round 345: Explicit Screen Context Follow-Ups

Date: 2026-07-04
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The owner showed a follow-up where Bluey attached `Screen context` again on a typed request, then failed with:

```text
Ref: B2D6A8AD
```

The visible session id was:

```text
A4331D2C
```

## Diagnosis

The local daemon log showed:

- The overlay sent the typed follow-up with `context_ids=0`.
- The daemon still rehydrated saved screen context into the answer request.
- That promoted the request to the managed vision lane.
- The managed server rejected the request with `internal_disclosure_blocked` because the stale screenshot contained Bluey/private-instruction-looking UI.

So the UI looked like it attached the screen again, and the server block became a generic "could not complete" message.

## Product Rule

Sent attachment chips and raw screenshot uploads must be explicit.

- If the overlay sends pending context ids, show those chips and allow the image/document payload.
- If the user asks a follow-up without pending ids, use prior answer/code and saved screen summaries as text memory only.
- Never silently re-upload an old screenshot just because the follow-up says "this", "that", or asks for code.

## Changes

- `question_attachment_ids_for_request` now returns only explicit `visible_context_ids`.
- Sent question cards no longer create fresh attachment chips from inferred answer context.
- Recent sent screen/image follow-up context is now `MeetingMemory`, not `Screenshot`.
- Relevant current-session image matches without pending ids now become retained text memory instead of vision/image context.
- Added a clearer user-facing message for `internal_disclosure_blocked` / private-instruction guard failures.
- Desktop workspace version bumped to `0.1.84`.

## Tests

```bash
cargo fmt --check
cargo check -p cue-daemon -p cue-cli
cargo test -p cue-daemon follow_up_context --lib
cargo test -p cue-daemon inferred_answer_context --lib
cargo test -p cue-daemon relevant_current --lib
cargo test -p cue-daemon internal_disclosure_blocks_get_specific_user_message --lib
```

Covered cases:

- Follow-up can reuse prior screen memory as text.
- Follow-up without explicit pending ids does not resend saved screen images.
- Inferred answer context does not create visible attachment chips.
- Current-session image relevance uses retained memory, not a screenshot payload.
- Internal/private-screen guard failures get a specific user-facing message.

## Deployment

Published desktop release:

```text
0.1.84
```

Live artifact:

```text
https://bluey.sh/releases/v0.1.84/bluey-0.1.84-darwin-arm64.tar.gz
```

Artifact SHA256:

```text
a16c1009ef8f9b51ef75bf0216460bd4f8640cb883366495285f56171ab716a7
```

Verification:

```bash
BLUEY_UPDATE_PUBKEY="$(cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64)" make package-darwin-arm64
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem PUBLISH_DO=1 PUBLISH_HOST=root@165.227.77.152 PUBLISH_PATH=/var/www/bluey scripts/publish-bluey-release.sh
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem scripts/bluey-release-live-verify.sh 0.1.84
curl -fsSL https://bluey.sh/install.sh | bash
/Users/uno/.bluey/bin/bluey --version
/Users/uno/.bluey/bin/bluey-daemon --version
/Users/uno/.bluey/bin/bluey on
/Users/uno/.bluey/bin/bluey status
```

Result:

- `latest.json` signature verified.
- Installer MIME checks passed.
- Darwin arm64 artifact SHA verified.
- Unpacked binaries report `0.1.84`.
- Public installer smoke installed `0.1.84`.
- Local daemon started with pid `88374`.

## Windows Parity

The behavioral fix is in the shared daemon, so it applies to macOS and Windows overlay asks. The native Windows UI was not edited in this round.

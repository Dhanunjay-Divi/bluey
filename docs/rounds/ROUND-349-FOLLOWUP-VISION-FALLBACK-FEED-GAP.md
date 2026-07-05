# Round 349 - Follow-up Vision Fallback and Feed Gap

Date: 2026-07-04
Branch: `codex/bluey-stream-attachments-20260704`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The owner showed Bluey session `AD0A09E8` where a follow-up question, `can u give me go code for that?`, failed with:

```text
Ref: FFAC5F2B
```

The same screenshot also showed an oversized blank vertical gap between the prior answer and the next question.

## Root Cause

The follow-up failure was from daemon `0.1.82`, but the failure pattern was still valid:

- The original answer used screen context and was routed through `bluey_managed/vision`.
- The follow-up was a code request that should have used retained problem context and recent Q&A.
- Because screenshot context was still present in the request, the daemon promoted the follow-up back to `bluey_managed/vision`.
- The managed vision route returned HTTP `400`.
- Bluey did not have a local text fallback for this case, so the user saw the generic failure card.

The UI gap came from the macOS feed stack not explicitly using compact vertical distribution. AppKit could distribute arranged message rows across the viewport, creating large empty space between chat cards.

## Fix

- Added a managed vision text fallback in the daemon:
  - If `bluey_managed/vision` rejects a request with `400`, and screenshot context is present, Bluey converts screenshot context into saved text memory and retries with a chat route.
  - Code/debug follow-ups retry on managed `deep`.
  - Other vision fallback requests retry on managed `balanced`.
  - The fallback logs `managed vision request rejected; retrying with saved text context` with request id, failed provider, fallback provider, and question intent.
- Added a daemon regression test for the exact code-follow-up shape.
- Set the macOS feed stack to `.fill` with vertical hugging/compression resistance so cards stay compact instead of spreading across the window.
- Desktop workspace version bumped to `0.1.88`.

## Intended Behavior

- First screen-context questions can still use vision.
- Follow-ups such as `give me Go code for that`, `same in Java`, or `can you fix the code` should use saved screen text, recent Q&A, and code artifacts instead of failing when the old screenshot route is rejected.
- The chat feed should keep normal message spacing with no large artificial blank gaps between answers and follow-up questions.

## Verification

```bash
cargo fmt --check
cargo test -p cue-daemon managed_vision_bad_request_falls_back_to_text_deep_for_code_follow_up -- --nocapture
cargo check -p cue-cli -p cue-daemon
swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift
cd native/macos/cue-overlay && swift build -c release
BLUEY_UPDATE_PUBKEY="$(cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64)" make package-darwin-arm64
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem PUBLISH_DO=1 PUBLISH_HOST=root@165.227.77.152 PUBLISH_PATH=/var/www/bluey scripts/publish-bluey-release.sh
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem scripts/bluey-release-live-verify.sh 0.1.88
curl -fsSL https://bluey.sh/install.sh | bash
/Users/uno/.bluey/bin/bluey --version
/Users/uno/.bluey/bin/bluey-daemon --version
/Users/uno/.bluey/bin/bluey on
/Users/uno/.bluey/bin/bluey status
```

Result:

- Rust format/check passed.
- Focused daemon fallback regression test passed.
- Swift parse passed.
- Swift release build passed.
- Release artifact dev-flag/secret scan passed.
- `latest.json` signature verified.
- Installer MIME checks passed.
- Darwin arm64 artifact SHA verified.
- Unpacked binaries report `0.1.88`.
- Public installer smoke installed `0.1.88`.
- Local daemon started successfully with pid `88803`.

## Deployment

Desktop release:

```text
0.1.88
```

Live artifact:

```text
https://bluey.sh/releases/v0.1.88/bluey-0.1.88-darwin-arm64.tar.gz
```

Artifact SHA256:

```text
42acefb183d8c0a762679caba7aac375ed25ad29051e5027e17281630aceaa3c
```

## Windows Parity

The follow-up fallback is daemon-side and applies to macOS and Windows.

The feed-gap fix is macOS overlay-specific. Windows should receive the same compact chat-feed rule in its native overlay: message rows should use compact vertical flow, not viewport-distributed spacing.

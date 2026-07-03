# Round 324 - Partial Code Stream Recovery

## Trigger

The owner reported that a coding/screen-context answer started streaming, then dropped and showed only:

```text
Bluey's connection dropped before the answer finished. I did not save that partial answer. Please retry.
Ref: 54B786D8
```

The user expectation is that Bluey should start with a quick explanation while code is generated, and should not throw away useful partial code if the provider stream drops.

Backup thread id remains: `019e133e-d92a-7830-8df0-3a050a4e22f6`.

## Root Cause

The local daemon logs for request `54b786d8-6485-4f42-a1ec-c40881c6a579` showed the managed provider produced useful text but ended with `answer_incomplete_reason="unclosed_code_fence"`. The daemon treated that as an incomplete answer shape and replaced the streamed content with the generic retry card.

This was correct for truly broken empty output, but too strict for recoverable code streams where the only broken shape is a missing closing Markdown fence.

## Fix

- Added repaired partial-answer recovery for `unclosed_code_fence` responses.
- Recovery now closes the open code fence, keeps only meaningful partials, adds a short visible note that the answer was partial, and preserves code canvas detection when possible.
- Applied the recovery to:
  - managed streaming answers
  - managed non-streaming answers
  - direct provider streaming answers
  - direct provider non-streaming answers
- Tightened the provider prompt so first-time coding/build answers must start with one short plain-English approach sentence before the first code fence.
- Added tests for:
  - repaired partial code answers closing the fence and preserving code canvas metadata
  - tiny useless stubs still falling back to retry behavior
  - overlay prompt contract requiring the approach sentence and forbidding starting a coding stream with a code fence

Windows parity: this is shared daemon/provider code, so the same behavior applies to macOS and Windows builds.

## Verification

```bash
cargo fmt --check
cargo test -p cue-daemon incomplete_code_answer_repair -- --nocapture
cargo test -p cue-daemon provider_messages_include_overlay_friendly_answer_shape -- --nocapture
cargo check -p cue-daemon
BLUEY_UPDATE_PUBKEY="$(cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64)" make package-darwin-arm64
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem PUBLISH_DO=1 PUBLISH_HOST=root@165.227.77.152 PUBLISH_PATH=/var/www/bluey scripts/deploy-bluey-sh-manual.sh
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem scripts/bluey-release-live-verify.sh 0.1.63
```

## Current State

- Desktop release `0.1.63` is live on `https://bluey.sh/latest.json`.
- Darwin arm64 artifact:
  `https://bluey.sh/releases/v0.1.63/bluey-0.1.63-darwin-arm64.tar.gz`
- Artifact SHA256:
  `36aee8d6c5934b9bf55c122fab90ea8ca4614b672bbe8982cdaace5e01371d93`
- Artifact size:
  `9190403` bytes
- Live verifier passed:
  - `latest.json` signature verified
  - live version `0.1.63`
  - `install.sh` content type `application/x-shellscript`
  - `install.ps1` content type `application/x-powershell`
  - Darwin arm64 artifact SHA verified
  - unpacked macOS binaries report `0.1.63`

## Remaining QA

- Run a live screen-coding answer from the overlay and confirm the first visible tokens are the approach sentence, not a code fence.
- Force a controlled dropped stream if possible and confirm a useful repaired partial answer is kept instead of a generic retry card.

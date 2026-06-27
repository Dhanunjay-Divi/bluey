# Round 217 - Upload Security Release Guard

Date: 2026-06-27 02:05 EDT
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The owner asked to make sure Bluey is secured and good when uploaded, with no missed release/upload risks.

This was a release-safety pass after recent visible-overlay QA work, because visible/capture debug behavior must never be present in uploaded binaries.

## Findings

- Current `v0.1.16` live manifest points only to `releases/v0.1.16/bluey-0.1.16-darwin-arm64.tar.gz`.
- The live `v0.1.16` artifact hash matched `latest.json`:
  - `c88e6d7faa32c0242f249076d4bf39c43ba61a557460368702d9a0ef62f3a5b1`
- The live active artifact was downloaded and scanned inside the archive; no visible-overlay/dev flag strings were found.
- Live `install.sh` and `install.ps1` were scanned for the same forbidden flags; no matches were found.
- Live `latest.json.sig` exists.
- Fresh local release binaries for `bluey`, `bluey-daemon`, `cue`, and `cue-daemon` were scanned; no forbidden visible-overlay/dev flag strings were found.
- Fresh local release macOS overlay binaries were scanned; no forbidden visible-overlay/dev flag strings were found.
- Old untracked local `dist` artifacts can still contain stale historical builds. They are not the active `latest.json` artifact, and the deploy script uploads only the current version folder, but the upload path needed an archive-level guard so a bad current artifact cannot be published by accident.

## Fix

Added an archive-level dev-flag scan to `scripts/publish-bluey-release.sh`.

The publish script now refuses release artifacts containing any of these production-forbidden markers inside `.tar.gz`, `.zip`, or raw files:

- `BLUEY_OVERLAY_CAPTURE_VISIBLE`
- `BLUEY_HOST_OVERLAY_CAPTURE_VISIBLE`
- `BLUEY_LOCAL_VISIBLE_OVERLAY`
- `BLUEY_ALLOW_CAPTURE_VISIBLE_LOCAL`
- `BLUEY_DEV_OVERLAY`
- `bluey-local-visible-overlay`
- `bluey-overlay-capture-visible`
- `bluey-dev-overlay`

If a bad artifact is present, publishing exits before checksum generation, manifest generation, signing, or rsync upload.

## Security / Abuse Verification

Passed:

- `cargo test --manifest-path server/Cargo.toml --test integration_e2e -- --test-threads=1` (`41 passed`)
- `cargo test --manifest-path server/Cargo.toml --test gdpr_webhook_cleanup` (`2 passed`)
- `cargo test --manifest-path server/Cargo.toml --test connectinfo_real_serve` (`1 passed`)
- `cargo test -p cue-daemon macos_overlay_capture_visible_requires_dev_and_local_gates --lib`
- `cargo test -p cue-cli redact --lib`
- `cargo test -p cue-cloud-client tokens --lib`
- `cargo test -p cue-cli update --lib`
- `cargo test --manifest-path server/Cargo.toml --lib` (`172 passed`)

Covered paths include:

- single-use auth device approval/polling
- Square and Stripe refund/dispute restriction
- Square amount/account-reference/signature mismatch rejection
- billing-restricted router rejection
- upstream spend guard before provider hit
- router stream idempotency and replay
- truncated stream not billed/released
- trial Turnstile gate
- auto-reload saved-card and threshold behavior
- RAG/session sync roundtrip
- GDPR webhook cleanup
- real peer-IP ConnectInfo for rate limiting
- token redaction
- update verification logic

## Release Verification

Passed:

- `bash -n scripts/publish-bluey-release.sh`
- `scripts/release-hygiene-scan.sh dist`
- `cargo build --release -p cue-daemon -p cue-cli`
- `BLUEY_OVERLAY_SWIFT_CONFIGURATION=release native/macos/cue-overlay/build.sh`
- clean publish dry-run with current `v0.1.16` artifact:
  - `Release artifact dev-flag scan passed (12 files checked).`
- poisoned publish test with fake Windows artifact containing `BLUEY_DEV_OVERLAY`:
  - publish exited with code `1`
  - error identified `bluey-9.9.9-windows-x86_64.zip:bin/bluey-daemon.exe: BLUEY_DEV_OVERLAY`
- live current artifact scan:
  - `live artifact dev-flag scan passed (12 files checked)`
- live installer script flag scan:
  - no matches for production-forbidden visible-overlay/dev markers

## Mac / Windows Parity

- The publish guard scans both `.tar.gz` and `.zip`, so macOS/Linux and Windows release artifacts are covered.
- The poisoned-artifact test used a fake Windows zip to verify Windows release binaries are blocked if they contain forbidden visible-overlay/dev markers.
- Fresh macOS overlay release binaries were built and scanned separately.

## Current State

- The current live `v0.1.16` installer path and active release artifact are clean for visible-overlay/dev markers.
- The release publish script now fails closed before upload if the current release artifact contains local visible-overlay or dev overlay flags.
- Server billing, auth, router, sync, GDPR, token, and update security checks passed.
- The local visible QA daemon was stopped after verification and Bluey was restarted normally:
  - daemon pid `66383`
  - overlay capture excluded `true`
  - screen capture active `false`

## Remaining QA / Gates

- Keep using `scripts/publish-bluey-release.sh` for release uploads. Do not manually rsync broad local `dist` folders.
- Before every public upload, run:
  - `scripts/release-hygiene-scan.sh dist`
  - `BLUEY_RELEASE_STAGE="$(mktemp -d)" scripts/publish-bluey-release.sh`
  - the server `integration_e2e` suite
- If old remote release folders ever need cleanup, do it intentionally with a versioned rollback plan because immutable historical release URLs may be referenced by old installers.

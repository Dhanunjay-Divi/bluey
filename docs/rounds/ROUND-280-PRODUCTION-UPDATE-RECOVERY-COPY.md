# Round 280 - Production Update Recovery Copy

## Trigger

A user machine was still running an older installed Bluey binary around `0.1.24`. On `bluey on` / `bluey update`, it printed developer-facing update text:

- `refusing to install unsigned Bluey update`
- `BLUEY_UPDATE_ALLOW_UNSIGNED=1 only for local testing`
- `bluey uninstall` was not recognized because that older binary predates Round 279.

The owner clarified that Bluey should be treated as production-ready, not described to users as local/testing software.

## Root Cause

The old installed binary could not verify signed production updates because it lacked the embedded update public key. That security refusal is correct, but the fallback copy was written for release operators and local development, not production customers.

A binary that is already installed on another machine cannot be retroactively taught a new command, so `bluey uninstall` will only exist after the user installs a current release. The production recovery path needs to say that plainly and safely.

## Fix

- Replaced public updater failure copy with production recovery language:
  - old unverifiable build: reinstall once from the production installer
  - after reinstall: `bluey on` keeps Bluey updated automatically
- Removed `BLUEY_UPDATE_ALLOW_UNSIGNED` and `local testing` from normal user-facing update-blocked output.
- Kept the strict security behavior: Bluey still refuses unverified update installs.
- Changed the installer checksum dependency failure from a local-testing override hint to a production-safe rerun instruction.
- Added a regression test to prevent the local-testing/update-override wording from reappearing in the old-build recovery message.
- Bumped desktop workspace version to `0.1.34` for release.

## Verification

Passed locally:

```bash
cargo test -p cue-cli old_build_update_message_is_production_safe --quiet
cargo test -p cue-cli unverified_manifest_is_not_installable_by_default --quiet
cargo check -p cue-cli --quiet
cargo fmt --all
cargo run -p cue-cli --bin bluey --quiet -- update --check-only
cargo run -p cue-cli --bin bluey --quiet -- uninstall --help
```

The dev-build update smoke correctly skipped with the dev-binary guard.

Passed release/deploy checks:

```bash
BLUEY_UPDATE_PUBKEY="$(cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64)" make package-darwin-arm64
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem PUBLISH_DO=1 PUBLISH_HOST=root@165.227.77.152 PUBLISH_PATH=/var/www/bluey scripts/deploy-bluey-sh-manual.sh
```

- Live `https://bluey.sh/latest.json` reports `0.1.35`.
- Live `latest.json.sig` verifies successfully against the release Ed25519 key.
- Live macOS artifact:
  `https://bluey.sh/releases/v0.1.35/bluey-0.1.35-darwin-arm64.tar.gz`
- Live SHA256:
  `ce9aaebf515f998efc533c81345f3e662035416a2df4c8ff7aeb667504e71190`
- Live `/install.sh` returns `application/x-shellscript`.
- Live `/install.ps1` returns `application/x-powershell`.
- Temp-root installer smoke passed:
  - downloaded `0.1.35`
  - checksum verified
  - installer says `Installing Bluey document tools`
  - installed CLI reports `bluey 0.1.35`
  - installed CLI says `Remove Bluey from this device`

## Current State

Closed and deployed:

- `v0.1.35` is the current public production release.
- `v0.1.34` was published first with the updater recovery wording, then superseded by `v0.1.35` with final install/uninstall copy cleanup.
- Existing very old installs that cannot verify signed updates still need one production reinstall because that old binary cannot be changed remotely.
- After installing `0.1.35`, future `bluey on` update checks should use the signed updater path.

## User Recovery Note

If a customer is already on a very old Bluey build that says it cannot verify updates, they need one production reinstall:

```bash
curl -fsSL https://bluey.sh/install.sh | bash
```

After that, `bluey on` should use the normal signed updater path, and `bluey uninstall` will be available.

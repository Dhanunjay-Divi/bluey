# Round 071 - Auto Update Default On - 2026-06-20

## Why

During live testing, `bluey on` on a `0.1.10` install reported that `0.1.11` was
available but only told the user to run `bluey update`. That behavior is too
manual for the product flow.

## Change

`bluey on` now auto-installs verified signed updates by default:

1. Check `latest.json`.
2. Verify `latest.json.sig` before trusting update fields.
3. Ensure installer and artifact hashes are pinned.
4. Print the update version/size.
5. Give the user 5 seconds to press Esc.
6. Install and relaunch `bluey on`.

Operators can keep notify-only posture with:

- `BLUEY_UPDATE_CHECK_ONLY=1`
- `BLUEY_AUTO_UPDATE=0`

Unsigned manifests remain blocked unless a local dev escape is explicitly set.

## Verification

- `cargo fmt --all --check`
- `git diff --check`
- `cargo test -p cue-cli update::tests`
  - 12 update tests passed, including valid/tampered/missing manifest
    signature coverage.
- Built the signed macOS release artifact with the embedded update public key:
  - `make package-darwin-arm64`
  - `target/aarch64-apple-darwin/release/bluey --version` -> `bluey 0.1.12`
- Published signed release assets to `bluey.sh` manually from this machine:
  - `https://bluey.sh/latest.json` reports `version: 0.1.12`
  - `https://bluey.sh/latest.json.sig` is present
  - `https://bluey.sh/releases/v0.1.12/SHA256SUMS.txt` pins
    `bluey-0.1.12-darwin-arm64.tar.gz`
- Fresh install smoke in a temporary prefix:
  - `curl -fsSL https://bluey.sh/install.sh | bash`
  - installed `bluey 0.1.12`
  - `bluey update --check-only --force` -> `Bluey is up to date (0.1.12).`

## Rollout Note

Installs already running `0.1.10` or `0.1.11` must run one manual
`bluey update` or reinstall once, because those older binaries only notify.
After `0.1.12` is installed, future `bluey on` launches auto-install verified
signed updates unless the user presses Esc during the 5 second prompt or an
operator sets notify-only mode.

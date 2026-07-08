# Round 446 - Stream Transcript Release Deploy

Date: 2026-07-08
Branch: `codex/bluey-web-ui-parallel-20260704`
Release version: `0.1.95`
Release source commit: `d1252aa2b4a070cbf84b3e32f005e0a3d91ecdd3`

## What Shipped

- Desktop release `0.1.95` was signed and published to `bluey.sh`.
- Web/static assets were synced to the droplet through the manual deploy path.
- API server was rebuilt on the droplet from the same source commit and restarted.
- No GitHub Actions were used for this deploy.

## Desktop Release

Published artifact:

- `https://bluey.sh/releases/v0.1.95/bluey-0.1.95-darwin-arm64.tar.gz`
- SHA256: `64caeca5214e9606af10ab634a002ed74c0edd5d5a1a87431ad57e8d3005ca1d`

Live verifier passed:

- `latest.json` signature verified.
- Live manifest version is `0.1.95`.
- `install.sh` MIME type is `application/x-shellscript`.
- `install.ps1` MIME type is `application/x-powershell`.
- macOS arm64 artifact SHA verified.
- Unpacked macOS arm64 binaries report `0.1.95`.

Current manifest note:

- The `0.1.95` manifest includes `darwin-arm64`.
- Windows installer script is published, but no `windows-x86_64` binary artifact was produced in this manual Mac/droplet release path. The Makefile Windows package target currently expects an MSVC Windows builder.

## API Release

Build source on droplet:

- `/opt/bluey-build-codex-round446-stream-transcript`

Runtime binary:

- `/usr/local/bin/bluey-server`
- SHA256: `fc3dffe09999e87fc4399ef6625b8be30f72c4febdfaaa9dacd5474d4688c5ed`

Previous binary backup:

- `/var/backups/bluey-api/bin/bluey-server.previous-20260708T222037Z`

Service check:

- `bluey-api.service` active after restart.
- `NRestarts=0`.
- `https://bluey.sh/health` returned status `ok`.
- Health commit: `d1252aa2b4a070cbf84b3e32f005e0a3d91ecdd3`.

Warning scan:

- `journalctl -u bluey-api.service --since "5 min ago" -p warning --no-pager`
- Result: no warning entries.

## Verification Run

Before release commit:

- `cargo check -p cue-cli --quiet`
- `cargo test -p cue-daemon overlay_history_cards --quiet`
- `cargo test -p cue-daemon user_facing_answer_error --quiet`
- `cargo test --manifest-path server/Cargo.toml sync_batch_round_trips_session_bundle_and_rag --quiet`
- `git diff --check`

Release/build checks:

- `make package-darwin-arm64`
- `scripts/deploy-bluey-sh-manual.sh`
- `scripts/bluey-release-live-verify.sh 0.1.95`
- Remote release API build: `cargo build --release --bin bluey-server`

## Notes For Next Round

- Users on macOS arm64 can update through the published installer/update path.
- If Windows release parity is required for this exact version, run the Windows package target on a Windows/MSVC builder and republish the manifest with the Windows artifact included.
- The API binary version command still tries to load runtime config before printing version, so health endpoint commit verification is the reliable production identity check for now.

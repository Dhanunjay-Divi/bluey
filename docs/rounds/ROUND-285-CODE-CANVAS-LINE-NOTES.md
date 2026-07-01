# Round 285 - Code Canvas Line Notes

## Trigger

The owner asked for coding answers to show light-grey comments explaining what code lines do, without affecting the actual code users copy or paste.

## Root Cause

The code canvas previously rendered one plain artifact body. If Bluey put comments directly inside the code block, those comments became part of the copied code. If Bluey only explained below the code, the canvas did not visually separate that explanation as code guidance.

## Fix

- Updated managed coding instructions:
  - code answers still start with complete working fenced code
  - non-trivial code should include a separate `Line notes:` block outside the code fence
  - line notes are explicitly kept outside code so copied code stays clean
- Updated server artifact formatting:
  - `Line notes:` is split into a dedicated `LINE NOTES` section
  - normal explanation remains in `NOTES`
  - added a regression test for `LINE NOTES` extraction
- Updated the macOS code canvas:
  - `LINE NOTES` renders in a muted grey style
  - existing inline code comments are tinted grey when present
  - the canvas copy button now copies only the `CODE` section for code artifacts
  - non-code canvases still copy the full canvas content
- Bumped desktop workspace version to `0.1.39`.
- Windows parity note:
  - Windows currently has the compact answer surface, not the macOS artifact canvas renderer.
  - The server-side line-note separation is shared, so Windows receives cleaner separated answer text.
  - The grey visual-only code canvas treatment is Mac-only until the Windows artifact canvas exists.

## Verification

Passed locally:

```bash
cargo fmt --all
cargo test --manifest-path server/Cargo.toml response_artifact --quiet
cargo check --manifest-path server/Cargo.toml --quiet
swift build -c debug --package-path native/macos/cue-overlay
cargo check -p cue-daemon --quiet
git diff --check
```

Deploy/release verification:

- Server built on the production droplet from source-only tree:
  `/opt/bluey-build-codex-round285-line-notes/server`
- Installed server binary:
  `/usr/local/bin/bluey-server`
- Previous production server binary backup:
  `/var/backups/bluey-api/bin/bluey-server.previous-20260701T213819Z`
- Installed server SHA256:
  `f201d6f077033eb334e1f090fe09cabd1afa1d6e7b7e5ecc155fe32aa2a4472d`
- `bluey-api.service` restarted active.
- `https://bluey.sh/health` returned `status=ok`.
- macOS release `0.1.39` was packaged and published.
- Live `https://bluey.sh/latest.json` reports:
  - version `0.1.39`
  - `darwin-arm64` artifact:
    `releases/v0.1.39/bluey-0.1.39-darwin-arm64.tar.gz`
  - artifact SHA256:
    `d17f49268f1581815be41ccb5711a60354b6b50edaa3797256e7d10b9a635b92`
- Live `latest.json.sig` verifies successfully against the release Ed25519 key.
- Live `SHA256SUMS.txt` matches the `darwin-arm64` artifact SHA256.
- `/install.sh` returns `application/x-shellscript`.
- `/install.ps1` returns `application/x-powershell`.

## Current State

- Code changes are implemented locally.
- Server deploy is live.
- macOS `0.1.39` release is live for downloadable binaries.

## Remaining QA

- Ask for a non-trivial code answer.
- Confirm canvas shows:
  - clean `CODE`
  - grey `LINE NOTES`
  - normal `NOTES`
- Copy the code canvas and paste into an editor.
- Confirm only the code section is pasted, not line notes or explanation.

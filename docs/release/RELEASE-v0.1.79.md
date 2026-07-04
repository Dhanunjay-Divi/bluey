# Bluey 0.1.79

Code fence visible canvas cleanup.

## Changes

- Repairs malformed same-line code fences such as `Code```cppclass Solution...`.
- Splits malformed language+code openings such as `cppclass` and `pythonfrom`.
- Handles same-line opening and closing fences.
- Lightly reflows compressed C-like one-line code before placing it in the code canvas.
- Keeps non-trivial code in the code canvas and cleans the chat card down to approach, explanation, complexity, and edge cases.
- Applies the parser/cleanup on both the managed server path and the shared daemon path.

## Verification

```bash
cargo fmt --check
cargo test --manifest-path server/Cargo.toml response_artifact_ --lib -- --nocapture
cargo test --manifest-path server/Cargo.toml visible_response_text_strips_code_when_canvas_exists --lib -- --nocapture
cargo test -p cue-daemon answer_overlay_artifact_ -- --nocapture
cargo test -p cue-daemon visible_answer_body_strips_large_code_when_canvas_exists -- --nocapture
cargo check --manifest-path server/Cargo.toml --bin bluey-server
cargo check -p cue-daemon -p cue-cli
BLUEY_UPDATE_PUBKEY="$(cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64)" make package-darwin-arm64
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem scripts/bluey-release-live-verify.sh 0.1.79
curl -fsSL https://bluey.sh/install.sh | bash
/Users/uno/.bluey/bin/bluey --version
/Users/uno/.bluey/bin/bluey-daemon --version
```

## Release

- Desktop release `0.1.79` is live on `https://bluey.sh/latest.json`.
- Darwin arm64 artifact:
  `https://bluey.sh/releases/v0.1.79/bluey-0.1.79-darwin-arm64.tar.gz`
- Artifact SHA256:
  `8eb3d665bd43b2c228bdf41be1ae9702a4cc1d129db979cc6fcc585c6b663fad`
- Release verification passed:
  - `latest.json` signature verification
  - installer MIME checks
  - Darwin arm64 artifact SHA verification
  - unpacked `bluey` and `bluey-daemon` version checks for `0.1.79`
- Public installer smoke installed `0.1.79` locally and both installed binaries report `0.1.79`.
- The non-interactive shell could not provide sudo credentials, so install correctly fell back to the user-local command symlink at `/Users/uno/.local/bin/bluey`.

## API Deployment

- Production API server deployed from `/opt/bluey-build-codex-round340-code-fence-cleanup`.
- Production API health reports commit `e6860a2bf3eb3a12d16126d1857751f1c2769409`.
- Production binary SHA256:
  `f3a33f648af22a04d0f3bd70541bf55558b418712cf176a33eb2755baafb0f7c`
- Previous API binary backup:
  `/var/backups/bluey-api/bin/bluey-server.previous-20260704T180742Z`
- `bluey-api.service` is active with `NRestarts=0`.
- Recent production warning/error journal scan after restart returned no entries.

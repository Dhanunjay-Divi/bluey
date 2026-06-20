# Deploy Live-Test Release - 2026-06-20

## Goal

Deploy the current Bluey code for live paid testing without GitHub Actions. The important release invariant is that installed clients must see a newer signed version, so this round bumps the desktop workspace from `0.1.10` to `0.1.11` before packaging.

## Plan

1. Bump the workspace version and lockfile to `0.1.11`.
2. Build the macOS arm64 release artifact locally with the update public key embedded.
3. Publish `latest.json`, `latest.json.sig`, install scripts, checksums, and artifacts directly to the droplet.
4. Smoke the live install/update endpoints from `bluey.sh`.
5. Leave `bluey-dev.db` untracked and untouched.

## Verification Log

- `cargo fmt --all --check` passed.
- `git diff --check` passed.
- `cargo test -p cue-cloud-client -p cue-llm` passed:
  - `cue-cloud-client`: 21 passed
  - `cue-llm`: 44 passed
- Built with the Ed25519 update public key embedded:
  - `make package-darwin-arm64`
  - `target/aarch64-apple-darwin/release/bluey --version` -> `bluey 0.1.11`
- Published without GitHub Actions:
  - `BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem PUBLISH_DO=1 PUBLISH_HOST=root@165.227.77.152 PUBLISH_PATH=/var/www/bluey bash scripts/publish-bluey-release.sh`
- Live `latest.json`:
  - version: `0.1.11`
  - platforms: `darwin-arm64`
  - signature file size: 88 bytes base64 / 64 bytes raw
- Verified live `latest.json.sig` with the release public key:
  - `openssl pkeyutl -verify -rawin -pubin ...` -> `Signature Verified Successfully`
- Live checksum:
  - `0c6ec6e37490e69fa5c45e29019e80b1735c5f9b501639e100d8821829363416  bluey-0.1.11-darwin-arm64.tar.gz`
- Live route checks returned `200`:
  - `/`
  - `/download`
  - `/login`
  - `/reload`
  - `/docs/privacy`
  - `/docs/terms`
  - `/docs/disguise`
  - `/install.sh`
  - `/install.ps1`
- Temp install smoke passed:
  - `curl -fsSL https://bluey.sh/install.sh | bash`
  - downloaded `v0.1.11`
  - checksum verified
  - `bluey --version` -> `bluey 0.1.11`
  - `bluey update --check-only --force` -> `Bluey is up to date (0.1.11).`
- Static JS live hash matches local:
  - `e7f92d8179709ff4918eb6817e960dee9fa8e0cf01eff67c968910beadd46d9a`
  - SRI: `sha384-vMCEeyJi2Wcd8RUDxf+1KqM9WZaKg1tKXQQCLMqqIk84ffTjXjfxbQ2Fo+fxUSss`
- Server health remains OK:
  - `https://bluey.sh/health`
  - live server commit: `e8f10f38117958bd6a0e7bfcd3316d039679e1e0`
- Pricing endpoint remains OK and reflects the live account/reload policy.

## Notes

No GitHub Actions are used for this deployment. Production release signing remains mandatory for the hosted manifest.

Windows was not republished in this release. The reachable Windows SSH machine has an in-flight dirty worktree, so this deploy intentionally avoids faking a `0.1.11` Windows artifact from older bits. Current live-test target is macOS arm64.

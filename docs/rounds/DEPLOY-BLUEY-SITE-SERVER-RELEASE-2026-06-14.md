# Deploy Bluey Site, Server, And Signed Release - 2026-06-14

## Scope

Codex deployed the current `codex/bluey-ai-site` tip from local/droplet
machines only. GitHub Actions were not used for build or deployment.

## Git

- Branch: `codex/bluey-ai-site`
- Commit: `11ee310`
- Remote: `origin/codex/bluey-ai-site`
- Local untracked file intentionally left untouched: `bluey-dev.db`

## Verification Before Deploy

- `cargo fmt --all --check`
- `python3 scripts/analyze-tracing-calls.py --check-only`
- `cargo test -p bluey-server --manifest-path server/Cargo.toml`
- `cargo test -p cue-cloud-client -p cue-llm`

## Server Deploy

- Host: `root@165.227.77.152`
- Build path: `/opt/bluey-build`
- Built on droplet with `cargo build --release --bin bluey-server`
- Installed atomically to `/usr/local/bin/bluey-server`
- Existing binary backed up as `/usr/local/bin/bluey-server.bak-*`
- Restarted `bluey-api.service`

Post-deploy checks:

- `systemctl is-active bluey-api.service` -> `active`
- `https://bluey.sh/health` -> `status: ok`
- `https://bluey.sh/pricing/tiers` -> returned current tier JSON

## Static Web Deploy

Synced `web/` directly to `/var/www/bluey/` with rsync, preserving release
and installer artifacts:

- `install.sh`
- `latest.json`
- `latest.json.sig`
- `releases/**`

Post-deploy checks:

- `https://bluey.sh/`
- `https://bluey.sh/download`
- `https://bluey.sh/assets/bluey-site.js`

## Signed Release Deploy

Created the first off-repo Ed25519 Bluey release signing key at:

- `~/.bluey/release/bluey-release-ed25519.pem`

The private key is not committed to the repository. The matching public key
was embedded into the local macOS build through `BLUEY_UPDATE_PUBKEY`.

Published:

- `https://bluey.sh/install.sh`
- `https://bluey.sh/latest.json`
- `https://bluey.sh/latest.json.sig`
- `https://bluey.sh/releases/v0.1.10/bluey-0.1.10-darwin-arm64.tar.gz`
- `https://bluey.sh/releases/v0.1.10/SHA256SUMS.txt`

Release artifact:

- Version: `0.1.10`
- Platform: `darwin-arm64`
- SHA256: `d98538caac3d38b032b7075d2e4c6419831a54c36694360513f7d86390a7a562`
- Size: `15490538`

Post-deploy checks:

- `latest.json.sig` exists and is 88 bytes
- hosted tarball checksum matches `SHA256SUMS.txt`
- rebuilt `target/release/bluey` contains the embedded update public key
- `target/release/bluey update --check-only --force` accepted the hosted
  signed manifest and reported up to date

## Local Mac Install Smoke

Installed from the public one-line installer into a temporary root:

```bash
BLUEY_INSTALL_ROOT="$(mktemp -d)/install" BLUEY_CLI_DIR="$(mktemp -d)" \
  bash -c 'curl -fsSL https://bluey.sh/install.sh | bash'
```

Verified the tarball contains both:

- `bin/bluey`
- `bin/bluey-daemon`

Then installed the same hosted artifact onto this Mac under `~/.bluey`.

Post-install check:

- `~/.local/bin/bluey --version` -> `bluey 0.1.10`
- `~/.local/bin/bluey update --check-only --force` -> up to date

## Notes For Kiro

- The capacity 429 hardening round has Codex verdict:
  `docs/reviews/REVIEW-CAPACITY-429-HARDENING-BY-CODEX.md`.
- Codex found and fixed one P1 during that review:
  OpenAI routes bypassed the non-thinking output clamp.
- Deployment was local/droplet only; no GitHub Actions deployment path was
  used.
- Release signing is now live for `latest.json`; do not publish future
  manifests without `latest.json.sig`.
- The release signing private key is operator state and must be backed up
  securely outside the repository.

# Deploy: macOS Ready, Windows Coming Soon - 2026-06-21

## Scope

This pass deployed the current Bluey site, signed macOS release artifact, and
server binary manually from local/droplet machines. GitHub Actions were not used.

## User-Facing Release State

- macOS is the active public alpha install path.
- `latest.json` advertises only `darwin-arm64` for `0.1.13`.
- Windows is intentionally marked "Coming soon" on `/download`.
- The public `install.ps1` exits with a coming-soon message unless
  `BLUEY_WINDOWS_INSTALL_PREVIEW=1` is explicitly set for internal testing.

## Deployed

- Static site synced to `/var/www/bluey`.
- Signed `latest.json` and `latest.json.sig` published.
- `dist/bluey-0.1.13-darwin-arm64.tar.gz` published under
  `/releases/v0.1.13/`.
- `bluey-api.service` rebuilt on the droplet from the current server source and
  restarted with commit `f41d92f` embedded.

## Verification

- `git diff --check`
- `node --check web/assets/bluey-site.js`
- `bash -n scripts/deploy-bluey-sh-manual.sh scripts/publish-bluey-release.sh ops/install/install.sh`
- `cargo test --manifest-path server/Cargo.toml --lib` -> 147 passed
- `make package-darwin-arm64` with `BLUEY_UPDATE_PUBKEY` set
- local archive checksum verified
- live `latest.json.sig` verified against the release Ed25519 public key
- live `darwin-arm64` artifact SHA256 matched the signed manifest
- live `/health` returned `commit: f41d92f`

## Infra Notes

- Production API is still running SQLite mode because no production
  `BLUEY_DATABASE_URL` / `BLUEY_SERVER_DB_BACKEND=postgres` cutover is set.
- Redis/Valkey is configured for shared capacity state, with strict mode off.
- R2/off-host backup destination and hourly backup cron are present.

## Untouched

- `bluey-dev.db` remains local/untracked and was not staged or deployed.

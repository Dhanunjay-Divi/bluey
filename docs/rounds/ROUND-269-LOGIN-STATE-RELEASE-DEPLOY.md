# Round 269 - Login State Release Deploy

## Trigger

Round 268 fixed the local overlay login-state recovery issue. The owner had asked that fixed work be deployed so `curl https://bluey.sh/install.sh | bash` and `bluey update` can receive it from the droplet.

## Release

- Bumped desktop workspace version from `0.1.22` to `0.1.23`.
- Built the macOS arm64 release artifact:
  - `dist/bluey-0.1.23-darwin-arm64.tar.gz`
  - SHA256 `105f3101e2a119e7a19bbee9a70efe8c61f7193556f79c570c527885d6301284`
- Published signed release files to `root@165.227.77.152:/var/www/bluey` with `scripts/publish-bluey-release.sh`.

## Verification

Passed:

```bash
BLUEY_UPDATE_PUBKEY="$(cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64)" make package-darwin-arm64
scripts/release-hygiene-scan.sh
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem \
  PUBLISH_DO=1 \
  PUBLISH_HOST=root@165.227.77.152 \
  scripts/publish-bluey-release.sh
curl -fsS https://bluey.sh/latest.json
curl -fsSI https://bluey.sh/install.sh
curl -fsS https://bluey.sh/releases/v0.1.23/SHA256SUMS.txt
openssl pkeyutl -verify -rawin -pubin \
  -inkey /tmp/bluey-release-pub.pem \
  -sigfile /tmp/bluey-latest.sig \
  -in /tmp/bluey-latest.json
```

Live results:

- `https://bluey.sh/latest.json` reports version `0.1.23`.
- `latest.json.sig` is 88 bytes.
- Signature verified successfully.
- Live `install.sh` returns `content-type: application/x-shellscript` and begins with the shell installer, not HTML.
- Live SHA256SUMS matches the local artifact checksum.
- Temp-home installer smoke installed `bluey 0.1.23`.

Installer smoke caveat:

- The noninteractive temp-home run could not attach sudo to `/dev/tty`, so it correctly fell back to a user-local symlink under `$HOME/.local/bin`. This is expected for the smoke environment and does not block normal terminal installs.

## Current State

The droplet downloadable release is now `0.1.23` for `darwin-arm64`, carrying the Round 268 login-state recovery fix. Windows release artifact was not republished in this round because the live manifest remains macOS arm64 only.

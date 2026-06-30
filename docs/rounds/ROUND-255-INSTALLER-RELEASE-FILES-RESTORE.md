# Round 255 - Installer Release Files Restore

## Trigger

The owner ran:

```bash
curl -fsSL https://bluey.sh/install.sh | bash
```

and bash failed on `<!doctype html>`. That meant the installer URL was serving the website fallback HTML instead of the shell installer.

## Root Cause

The release files on the droplet had been removed from `/var/www/bluey`:

- `/install.sh`
- `/install.ps1`
- `/latest.json`
- `/latest.json.sig`
- `/releases/v0.1.17/*`

With those files missing, Caddy served the SPA/static site fallback for `/install.sh`, so the shell received HTML.

The likely cause was a web-only deploy that synced `web/` to `/var/www/bluey/` with deletion enabled and did not preserve release files. The safe deploy script already avoids this by excluding installer, manifest, signature, and release paths during static web sync.

## Fix

Restored the current release payload from local `dist/publish-bluey-sh/` to the droplet:

- `install.sh`
- `install.ps1`
- `latest.json`
- `latest.json.sig`
- `releases/v0.1.17/RELEASE.md`
- `releases/v0.1.17/SHA256SUMS.txt`
- `releases/v0.1.17/bluey-0.1.17-darwin-arm64.tar.gz`

Future Bluey.sh deploys should use:

```bash
scripts/deploy-bluey-sh-manual.sh
```

or preserve these paths explicitly:

```text
/install.sh
/install.ps1
/latest.json
/latest.json.sig
/releases/**
```

## Verification

Live installer checks passed:

```bash
curl -fsSL https://bluey.sh/install.sh | sed -n '1p'
curl -fsSL https://bluey.sh/install.sh | bash -n
curl -fsSL https://bluey.sh/latest.json
curl -fsSL https://bluey.sh/latest.json.sig | wc -c
shasum -a 256 -c SHA256SUMS.txt
```

Observed:

```text
#!/usr/bin/env bash
manifest=0.1.17 ['darwin-arm64']
sig_bytes=88
bluey-0.1.17-darwin-arm64.tar.gz: OK
```

Temp-home installer smoke passed:

```bash
curl -fsSL https://bluey.sh/install.sh | HOME="$tmp_home" BLUEY_INSTALL_NO_SUDO=1 BLUEY_SKIP_LOCAL_TOOLS=1 bash
"$tmp_home/.local/bin/bluey" --version
```

Observed:

```text
bluey 0.1.17
Bluey installed.
Run: bluey on
```

## Current State

`curl -fsSL https://bluey.sh/install.sh | bash` now serves the actual installer again. After installation, `bluey on` is the intended simple entrypoint: it starts Bluey, opens the browser sign-in/link flow only if needed, and prompts for local permissions on first launch.

## Remaining QA/Gates

- Windows downloadable artifact is still not included in `latest.json`; current live release is `darwin-arm64`.
- Keep release publishing and web-only deploys separate. A static-site deploy must not delete installer or release artifact paths.

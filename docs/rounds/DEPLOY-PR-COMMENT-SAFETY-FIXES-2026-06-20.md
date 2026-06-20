# Deploy PR Comment Safety Fixes - 2026-06-20

## Scope

Codex deployed the PR comment safety batch from local and droplet machines
only. GitHub Actions were not used for build or deployment.

Deployed code commit:

- `e8f10f38117958bd6a0e7bfcd3316d039679e1e0`
- `fix(web,billing): harden account reload safety`

Local untracked `bluey-dev.db` was left untouched.

## What Deployed

Server changes:

- browser logout now has a protected server route and revokes the current
  refresh token
- Square idempotency keys are short enough for Square Cards/Payments APIs
- non-Bluey Square `payment.updated` events are ignored instead of retried
- Gemini streaming requires a real terminal signal before billing

Web changes:

- browser refresh is serialized to avoid refresh-token races
- dashboard load no longer signs users out on generic non-auth failures
- device-code linking requires an explicit signed-in confirmation click
- reload CTA lands on `/reload?checkout=1` and starts checkout once
- `bluey-site.js` SRI was updated in `web/index.html`

## Deployment Steps

1. Synced the local workspace to `/opt/bluey-build` on the droplet while
   preserving the existing Rust release build cache.
2. Built the server on the droplet:

   ```bash
   cd /opt/bluey-build/server
   BLUEY_GIT_COMMIT=e8f10f38117958bd6a0e7bfcd3316d039679e1e0 \
     /root/.cargo/bin/cargo build --release --bin bluey-server
   ```

3. Backed up `/usr/local/bin/bluey-server` with a timestamped `.bak-*` copy.
4. Installed the new binary to `/usr/local/bin/bluey-server`.
5. Restarted `bluey-api.service`.
6. Synced `web/` to `/var/www/bluey/` with release files excluded:

   - `install.sh`
   - `install.ps1`
   - `latest.json`
   - `latest.json.sig`
   - `releases/**`

No release manifest or desktop artifact was republished in this pass because
the code changes were server/web only. The existing signed `latest.json.sig`
remained present and unchanged on the host.

## Live Verification

Server:

- `systemctl is-active bluey-api.service` -> `active`
- `https://bluey.sh/health` returned:
  - `status: ok`
  - `commit: e8f10f38117958bd6a0e7bfcd3316d039679e1e0`
- `https://bluey.sh/pricing/tiers` returned the current `$15` minimum reload
  pricing snapshot.
- `journalctl -u bluey-api.service --since "5 min ago" -p warning` returned no
  entries after restart.

Web:

- `https://bluey.sh/` -> `200`
- `https://bluey.sh/download` -> `200`
- `https://bluey.sh/login` -> `200`
- `https://bluey.sh/reload` -> `200`
- `https://bluey.sh/docs/privacy` -> `200`
- `https://bluey.sh/docs/terms` -> `200`
- `https://bluey.sh/docs/disguise` -> `200`
- Live `assets/bluey-site.js` SHA256 matched local:
  `e7f92d8179709ff4918eb6817e960dee9fa8e0cf01eff67c968910beadd46d9a`
- Live index SRI for `/assets/bluey-site.js` matched the live JS:
  `sha384-vMCEeyJi2Wcd8RUDxf+1KqM9WZaKg1tKXQQCLMqqIk84ffTjXjfxbQ2Fo+fxUSss`

Release safety:

- `/var/www/bluey/latest.json.sig` exists and is 88 bytes.
- `/var/www/bluey/latest.json` still advertises version `0.1.10`.
- Published platforms remain `darwin-arm64` and `windows-x86_64`.

## Notes

The deployment closes the server/web half of the PR comment safety batch. It
does not close broader paid-alpha live smoke items such as real Square checkout,
Deepgram live captions, or Mac overlay click-through verification.

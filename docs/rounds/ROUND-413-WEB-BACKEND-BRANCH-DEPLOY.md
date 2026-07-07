# ROUND-413-WEB-BACKEND-BRANCH-DEPLOY

Date: 2026-07-07
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Goal

Deploy the Bluey web UI changes from `codex/bluey-web-ui-parallel-20260704` together with the current backend changes on `main`, without losing the newer release, overlay, AnswerPlan, billing guard, and Windows installer work already present on `main`.

## Source

- Main before merge: `d9842ffa`
- Web UI branch head: `5e4c7b7d`
- Merged deploy commit: `e9077ef7849f51ea5301ffd6750487e906a32981`

The merge had one conflict in `web/assets/bluey-site.js`. The resolved version keeps:

- web branch add-credits / auto-reload / usage-estimate UI
- main live account balance polling
- main negative billing input protection, merged into the shared billing money guard
- branch dashboard autosave and reload setup modal behavior

## Local Verification

```bash
node --check web/assets/bluey-site.js
git diff --check
cargo check --manifest-path server/Cargo.toml --bin bluey-server
cargo test --manifest-path server/Cargo.toml --lib --quiet
```

Results:

- JS syntax check passed.
- Diff whitespace check passed.
- `bluey-server` cargo check passed.
- Server library tests passed: `267 passed`.

## Web Deploy

Command:

```bash
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem \
PUBLISH_DO=1 \
PUBLISH_HOST=root@165.227.77.152 \
PUBLISH_PATH=/var/www/bluey \
scripts/deploy-bluey-sh-manual.sh
```

Live web verification:

- `https://bluey.sh/health` returned `status=ok`.
- `latest.json` signature verified.
- Live release version is `0.1.90`.
- `install.sh` content type is `application/x-shellscript`.
- `install.ps1` content type is `application/x-powershell`.
- `darwin-arm64` artifact SHA verified.
- unpacked `darwin-arm64` binaries report `0.1.90`.
- Public pages/assets returned 200 for `/`, `/account`, `/download`, `assets/bluey-site.js`, and `assets/bluey-site.css`.
- Live `bluey-site.js` had no merge conflict markers.

## Backend Deploy

Synced the merged source to:

```text
/opt/bluey-build-codex-round413-web-backend
```

Built the Linux release binary on the droplet with:

```bash
BLUEY_GIT_COMMIT=e9077ef7849f51ea5301ffd6750487e906a32981 \
cargo build --release --bin bluey-server
```

Installed:

```text
/usr/local/bin/bluey-server
```

Previous binary backup:

```text
/var/backups/bluey-api/bin/bluey-server.previous-20260707T174952Z
```

Post-restart verification:

- `bluey-api.service` is `active`.
- Local droplet `/health` reports commit `e9077ef7849f51ea5301ffd6750487e906a32981`.
- Public `https://bluey.sh/health` reports commit `e9077ef7849f51ea5301ffd6750487e906a32981`.
- Recent `bluey-api.service` warning log check showed no entries.
- Droplet disk check showed `/` at 28% used.

## Notes

- No provider keys or production secrets were copied into docs.
- Release artifacts remain `0.1.90`; this round deployed web/static changes and the production API binary from the merged backend source.
- The deploy included the branch's backend billing/email changes as well as the web UI changes.

# Distribution Plan — Phase 3 Round 7

## Decision: GitHub Releases as primary distribution

**Why GitHub Releases:**
- Zero infrastructure cost — no CDN, no server, no signing certificates
- Terminal-distributed app: users are developers comfortable with CLI install
- Tauri updater plugin natively supports GitHub Releases as an endpoint
- SHA256 checksums provide integrity verification without code-signing

**Why Homebrew + Scoop:**
- Zero-friction install for power users on macOS and Windows
- Automatic PATH setup, dependency management, clean uninstall
- Formula/manifest auto-update via `bump-formulae.sh` script

## Architecture

```
Tag push (v*) → GitHub Actions matrix build
  ├── macOS arm64 → tar.gz + sha256
  ├── macOS x86_64 → tar.gz + sha256
  ├── Windows x86_64 → zip + sha256
  └── Linux x86_64 → tar.gz + sha256 (best-effort)

Release job:
  → Upload all artifacts to GitHub Release
  → Generate latest.json (Tauri updater manifest)
  → Upload SHA256SUMS.txt
```

## Tauri Updater

- Endpoint: `https://github.com/<org>/bluey/releases/latest/download/latest.json`
- Signing: keypair generated via `tauri signer generate`, private key in GitHub Secrets
- Public key in `tauri.conf.json` → `plugins.updater.pubkey`

## What's deferred

| Item | Reason |
|------|--------|
| Apple notarization / Developer ID | Terminal app, no App Store distribution |
| Windows Authenticode signing | Same — terminal-distributed |
| `.pkg` / `.msi` installers | Overkill for developer audience |
| `.deb` / `.AppImage` / Flatpak | Linux packaging deferred to demand |
| Self-hosted update server | GitHub Releases sufficient for now |
| Mirror / CDN | Not needed at current scale |
| Homebrew core submission | Requires open-source license |

## Next steps for first release

1. `tauri signer generate` → save pubkey in tauri.conf.json, private key in GitHub Secrets
2. Replace `<org>` placeholders with actual GitHub org/user
3. Create separate `<org>/homebrew-bluey` tap repo, copy formula there
4. Create separate `<org>/scoop-bluey` bucket repo, copy manifest there
5. Tag `v0.1.0`, push tag → release pipeline runs
6. Run `infra/scripts/bump-formulae.sh` with SHA256 values from release
7. Push updated formula/manifest to tap/bucket repos

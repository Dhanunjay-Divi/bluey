# Bluey.sh Deploy Pass — 2026-06-02

## Objective

Deploy the current `feat/phase-3-round-12` Bluey server and static site to
`bluey.sh` incrementally, verify each live surface, and record the remaining
operator-only blockers without exposing secrets.

## Commit Deployed

- Local branch: `feat/phase-3-round-12`
- Server commit embedded in `/health`: `bcb6e5a`
- Public server: `https://bluey.sh`
- Droplet: `165.227.77.152`

## Work Completed

1. Verified DNS and host access.
   - `bluey.sh` and `www.bluey.sh` resolve to the droplet.
   - SSH root access works by key.
   - `bluey-api` and `caddy` are active.

2. Prepared the droplet for repeatable builds.
   - Added a 2 GB swap file to protect Rust builds on the 2 GB droplet.
   - Installed build prerequisites and Rust toolchain.

3. Built and deployed `bluey-server`.
   - Synced a minimal build tree instead of the full repo.
   - Excluded local DBs, target directories, reference repos, and other
     non-deployable artifacts.
   - Built `bluey-server` on Linux with `BLUEY_GIT_COMMIT=bcb6e5a`.
   - Backed up the old `/usr/local/bin/bluey-server` before replacement.
   - Restarted `bluey-api` cleanly.

4. Deployed static web.
   - Copied `web/` to `/var/www/bluey`.
   - Verified public routes:
     - `/`
     - `/link`
     - `/login`
     - `/account`
     - `/reload`
     - `/pricing/tiers`
     - `/health`
     - `/docs/privacy`
     - `/docs/terms`
     - `/docs/disguise`

5. Fixed and deployed the installer path.
   - The live site linked to `/install.sh`, but the old installer expected a
     future `Bluey.app` artifact at `v0.2.0`.
   - Updated `ops/install/install.sh` to match the current terminal bundle:
     `bin/bluey` plus native helper binaries.
   - Published:
     - `/install.sh`
     - `/releases/v0.1.0/bluey-0.1.0-darwin-arm64.tar.gz`
     - `/releases/v0.1.0/SHA256SUMS.txt`
   - Verified the public installer into `/tmp`:
     - download ok
     - checksum ok
     - helper bundle installed
     - binaries ad-hoc signed
     - quarantine cleared
     - `bluey --version` prints `bluey 0.1.0`

## Smoke Results

| Surface | Result | Notes |
|---|---:|---|
| `GET /health` | PASS | Returns `commit: a2f5a4c`. |
| Static web routes | PASS | All checked routes return 200. |
| Signup | PASS | Throwaway account created successfully. |
| Account read | PASS | `/account/me` returned account state. |
| Account delete | PASS | Throwaway accounts were cleaned up. |
| Square checkout | PASS with normal email | Sandbox returns `https://sandbox.square.link...`. Reserved/test domains are rejected by Square email validation. |
| Installer | PASS | Public installer works in temp install root. |
| Managed AI answer | BLOCKED | Droplet does not yet have OpenAI/Anthropic provider keys. |
| STT/transcribe | BLOCKED | Droplet does not yet have Deepgram keys. |
| Verify/reset email | PARTIAL | Droplet uses Resend HTTPS API mail. `/auth/verify-email/start` and `/auth/password-reset/start` returned `202` and logged `sent`; inbox/link confirmation is pending operator check. |

## Remaining Inputs Needed From User

Add these to `/etc/bluey-api/bluey-api.env`, then restart `bluey-api`:

```text
OPENAI_API_KEYS=
ANTHROPIC_API_KEYS=
DEEPGRAM_API_KEYS=
```

For production, rotate any provider or billing secrets that were pasted into
chat before real customer launch.

## Next Step

After provider keys are installed and Resend DNS verifies:

1. Restart `bluey-api`.
2. Re-run managed smoke:
   - signup/login
   - Square sandbox checkout
   - `/router/complete`
   - `/router/complete/stream`
   - `/router/transcribe`
   - email verification
   - password reset
3. Then run the Mac app smoke with the temp-installed `bluey` binary.

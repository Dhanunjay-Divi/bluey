# ROUND-457-MANUAL-DEPLOY-PROCESS-IDENTITY-WEB-BACKEND

Date: 2026-07-09
Branch: `codex/bluey-web-ui-parallel-20260704`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Goal

Deploy the current Bluey web, backend, desktop packaging, and installer changes without using GitHub Actions.

This round includes:

- process identity/helper naming parity with Pinky-style installs
- download page command/support-surface polish
- verification email polish
- Try Us monthly device trial window changes
- account/card setup retry fixes already present in the web branch
- Deepgram/STT formatting cleanup currently staged in the backend
- signed macOS release artifact and production API rebuild

## Important Product Invariants

- Manual deploy path only; do not use GitHub Actions for this round.
- Keep a stable traceable commit before publishing.
- Keep old Bluey binary/helper names supported during transition.
- Add generic process names only inside the Bluey install root, with path-filtered cleanup.
- Do not store provider secrets or keys in docs.

## Local Verification

Planned before commit/deploy:

```bash
bash -n ops/install/install.sh scripts/install.sh scripts/build-macos.sh scripts/build-macos-universal.sh scripts/bluey-release-live-verify.sh scripts/bluey-visible-local.sh scripts/macos-overlay-visual-smoke.sh native/macos/cue-overlay/build.sh native/macos/cue-audio/build.sh scripts/deploy-bluey-sh-manual.sh scripts/publish-bluey-release.sh
node --check web/assets/bluey-site.js
git diff --check
cargo check -p cue-cli -p cue-daemon --quiet
cargo check --manifest-path server/Cargo.toml --bin bluey-server --quiet
```

Initial result:

- shell syntax checks passed
- web JavaScript syntax check passed
- `git diff --check` passed
- CLI/daemon cargo check passed
- server cargo check passed

## Deployment Plan

1. Commit the full local batch.
2. Fast-forward/merge to `main` if the branch is clean.
3. Package macOS arm64 with the release public key embedded.
4. Publish web/static files, installers, release manifest, signature, and macOS artifact to the droplet with `scripts/deploy-bluey-sh-manual.sh`.
5. Rebuild `bluey-server` on the droplet from the same commit and restart `bluey-api.service`.
6. Run live checks:
   - `https://bluey.sh/health`
   - `https://bluey.sh/latest.json`
   - installer MIME checks
   - `scripts/bluey-release-live-verify.sh`
   - `bluey-api.service` status/log warnings

## Pending Live Results

To be filled after deployment completes.

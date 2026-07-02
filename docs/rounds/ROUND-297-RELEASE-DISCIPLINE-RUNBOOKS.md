# Round 297 - Release Discipline Runbooks

Date: 2026-07-02
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The user asked Bluey to copy Pinky's release discipline: signed artifacts,
preprod proof, exact production promotion, smoke checks, rollback readiness,
logs, disk/storage checks, billing gates, and dispute evidence packets.

## Work Done

- Read Pinky's release, operations, disk/storage, and dispute evidence runbooks.
- Updated Bluey's release runbook with:
  - deployment execution policy
  - release id format `<version>-<commit12>`
  - preprod readiness gate
  - production promote gate
  - post-production verification gate
  - billing/credit/reload reconciliation requirement
  - search/provider cooldown and 429 proof requirement
- Added `scripts/bluey-release-live-verify.sh`:
  - verifies live `latest.json.sig`
  - checks installer MIME types
  - checks artifact SHA
  - unpacks macOS release and checks `bluey` / `bluey-daemon` versions
  - fails on configured capture-visible dev markers in the shipped daemon
- Updated manual bluey.sh deploy script to run the live verifier when a release
  public key or signing key is available.
- Added Bluey disk/storage deploy runbook:
  `docs/ops/DEPLOY-DISK-STORAGE-CHECK-RUNBOOK.md`
- Added Bluey dispute/refund/credit evidence runbook:
  `docs/ops/DISPUTE-EVIDENCE-RUNBOOK.md`
- Updated Bluey's operations runbook to reflect current production beta state
  instead of stale "not provisioned" language.
- Updated production deploy runbook so manual API binary replacement is
  emergency-only and gated by backup, storage, billing, and reconciliation
  checks.

## Verification

- `BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem scripts/bluey-release-live-verify.sh 0.1.50`
- `bash -n scripts/bluey-release-live-verify.sh scripts/deploy-bluey-sh-manual.sh scripts/publish-bluey-release.sh scripts/release-hygiene-scan.sh`
- `scripts/release-hygiene-scan.sh docs/RELEASE-RUNBOOK.md docs/OPERATIONS-RUNBOOK.md docs/PRODUCTION-DEPLOY-RUNBOOK.md docs/ops scripts/bluey-release-live-verify.sh scripts/deploy-bluey-sh-manual.sh`

## Current State

- No desktop binary or server binary was changed in this round.
- No production deploy was required because this round changed docs and release
  operator scripts only.
- The new live verifier passed against the currently deployed `0.1.50` release.

## Remaining Gates

- Add a GitHub Actions production promotion workflow that promotes an already
  verified artifact instead of rebuilding.
- Add a preprod environment and stored preprod artifact metadata so Bluey can
  enforce exact artifact promotion automatically.
- Add a server release bundle with commit/version/buildTime metadata for API
  deploys, matching the desktop release verifier discipline.

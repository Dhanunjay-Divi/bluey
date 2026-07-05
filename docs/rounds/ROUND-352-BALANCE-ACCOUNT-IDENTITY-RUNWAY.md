# Round 352 - Balance Account Identity And Runway

## Trigger

- The owner showed the web account page for `internal-admin-20260606023943@bluey.sh` displaying `$15.00`, while the desktop overlay showed `$8.25`.
- The same page showed `~477 days at current rate`, which looked unreliable.

## Root Cause

- The desktop was linked to a different account:
  - Dashboard account: `internal-admin-20260606023943@bluey.sh`
  - Desktop account from `bluey account`: `codex-smoke-20260608183100@bluey.sh`
- The runway estimate used exact last-7-day burn-rate math. With only `$0.22` spent recently, `$15.00 / ($0.22 / 7 days)` produced about `477 days`, which is mathematically consistent but bad product copy from a tiny sample.

## What Changed

- Added `projection_label` and `projection_quality` to `/account/usage`.
- Low-sample usage now reports `Light recent usage; estimate needs more activity.` and caps the legacy numeric projection.
- Long runway estimates now cap at `90+ days at recent pace.` instead of claiming overprecise huge values.
- Updated CLI usage output to prefer the server-provided projection label.
- Updated the web dashboard usage card to prefer `projection_label` and avoid `current rate` wording.
- Added account email metadata to desktop balance snapshots and overlay `set_balance` commands.
- Added desktop account email to the macOS overlay balance tooltip and desktop dashboard balance tooltip.
- Mirrored the usage projection contract into the parallel web checkout at `/Users/uno/Downloads/cue` because that account-page source matches the live screenshot.

## Windows Parity

- The server, client, CLI, and web changes are platform-neutral.
- The native tooltip implementation in this round is macOS-specific because the current report came from the macOS overlay. Windows should consume the new optional `account_email` field in its overlay balance affordance when the Windows overlay balance UI is next touched.

## Verification

```bash
/Users/uno/.bluey/bin/bluey account
/Users/uno/.bluey/bin/bluey credits
cargo fmt --check
swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift
cargo check -p cue-core -p cue-cloud-client -p cue-daemon -p cue-cli -p cue-dashboard
cargo check -p bluey-server
cd /Users/uno/Downloads/cue && cargo fmt --check && cargo check -p cue-cloud-client -p cue-cli
cd /Users/uno/Downloads/cue/server && cargo check -p bluey-server
```

## Current State

- Deployed live in this round.
- Desktop account mismatch is now explainable directly from local CLI:
  - dashboard screenshot account: `internal-admin-20260606023943@bluey.sh`
  - current linked desktop account: `codex-smoke-20260608183100@bluey.sh`
- Desktop release `0.1.91` is live on `https://bluey.sh/latest.json`.
- Darwin arm64 artifact:
  `https://bluey.sh/releases/v0.1.91/bluey-0.1.91-darwin-arm64.tar.gz`
- Artifact SHA256:
  `fcc27fcf6eaaedaeeaff8bfd9b5b8b9c74ff32c70b59b25c951ea59f5a9540de`
- Production API health reports commit `a6bbc4604c4b0426dd522c43f5ef9c65f1a8e7b7`.
- Production binary SHA256:
  `028715066c9f82cdcfa73f71f82e6ae51b4ce44a1109bb1e755376b0becdc01b`
- Previous API binary backup:
  `/var/backups/bluey-api/bin/bluey-server.previous-20260705T021937Z`
- Live web JS contains the projection-label display path.
- Public installer smoke installed `0.1.91`, and both installed binaries report `0.1.91`.

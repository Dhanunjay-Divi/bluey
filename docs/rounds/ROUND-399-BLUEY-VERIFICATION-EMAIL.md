# Round 399 - Bluey Verification Email

Date: 2026-07-07
Branch: `codex/bluey-web-ui-parallel-20260704`

## Trigger

Bluey verification emails were arriving as plain text, while Pinky sends a branded, readable code email with a prominent code block.

## Changes

- Added optional HTML support to Bluey's transactional mail sender.
- Kept existing plain-text bodies as fallback content.
- Updated signup OTP email delivery to send a Bluey-branded HTML email:
  - dark email surface
  - Bluey blue wordmark color
  - `Welcome to Bluey.`
  - large spaced verification code block
  - honest expiry copy based on the server's 10-minute OTP TTL
- Added HTML escaping for the displayed code.
- Left email verification links and password reset emails on their existing text-only path.

## Verification

- `cargo fmt --check`
- `cargo test --manifest-path server/Cargo.toml mail::tests -- --nocapture`
- `cargo check --manifest-path server/Cargo.toml --bin bluey-server`
- `git diff --check`

## Current State

New Bluey signup verification emails visually match Pinky's compact branded style while preserving Bluey's existing OTP behavior and text fallback.

## Production Deploy

- Production API server deployed from:
  `/opt/bluey-build-codex-round399-verification-email`
- Production API health returned:
  `{"status":"ok","version":"0.1.5","commit":"50e80b0a","platform":"linux-x86_64"}`
- Installed production binary SHA:
  `c06b98436de87975808ea8cfd33e131e414df741de6bb49f30d6b9124b111a79`
- Previous production binary backup:
  `/var/backups/bluey-api/bin/bluey-server.previous-20260707T043147Z`
- `bluey-api.service`: active
- `bluey-api.service` `NRestarts`: `0`
- Recent warning/error logs after restart:
  `-- No entries --`

## Remaining QA/Gates

- Send a new live signup verification code and confirm Gmail renders the branded HTML view.

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

## Current State

New Bluey signup verification emails should visually match Pinky's compact branded style while preserving Bluey's existing OTP behavior and text fallback.

## Remaining QA/Gates

- Production API needs a backend deploy before live emails change on `bluey.sh`.

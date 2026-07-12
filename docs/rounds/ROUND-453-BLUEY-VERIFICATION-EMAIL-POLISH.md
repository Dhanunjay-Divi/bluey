# Round 453 - Bluey Verification Email Polish

Date: 2026-07-09
Branch: `codex/bluey-web-ui-parallel-20260704`
Backup thread: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The live signup OTP email rendered as a plain light card in Gmail and did not feel like the Bluey product. The user asked to make it use Bluey's colors, logo feel, and a nicer branded layout.

## Changes

- Reworked the signup OTP HTML template in `server/src/mail.rs`.
- Kept the existing plain-text fallback body unchanged.
- Changed the HTML to an email-safe table layout with inline styles.
- Added a small terminal-style Bluey mark using text and borders instead of external images or SVG attachments.
- Added a dark Bluey surface, cyan/blue border accents, a clearer code card, and security copy.
- Preserved:
  - subject
  - OTP TTL copy
  - spaced verification code
  - HTML escaping
  - Resend and SMTP delivery paths

## Verification

- `cargo fmt --manifest-path server/Cargo.toml`
- `cargo test --manifest-path server/Cargo.toml mail::tests --quiet`
- `git diff --check -- server/src/mail.rs docs/rounds/ROUND-453-BLUEY-VERIFICATION-EMAIL-POLISH.md`

## Deployment

Not deployed. The user explicitly asked not to deploy every round unless requested.

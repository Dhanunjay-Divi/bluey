# ROUND-452 Trial Device Monthly Cap

Date: 2026-07-09
Branch: codex/bluey-web-ui-parallel-20260704
Backup thread: 019e133e-d92a-7830-8df0-3a050a4e22f6

## Problem

The Try Us temporary trial used the browser/device as a synthetic email identity. That made the same browser/device effectively limited to one temporary trial forever because the email cap fired before any device-window policy could help.

## Change

- Kept real email trial eligibility at one free trial total.
- Changed default browser/device trial policy to:
  - `BLUEY_TRIAL_MAX_PER_DEVICE=6` lifetime safety cap.
  - `BLUEY_TRIAL_MAX_PER_DEVICE_PER_30_DAYS=1` monthly window cap.
- Added 30-day device-window enforcement for SQLite and Postgres.
- Changed Try Us to use a per-request synthetic email identity so browser/device and IP caps govern temporary trials instead of the real-email lifetime cap.
- Let normal account creation continue without trial minutes when any trial cap is hit. Trial limits now control free usage, not the ability to create a credit-backed account.
- Updated web copy to say the current temporary trial window was used, not that the browser is blocked forever.

## Resulting Policy

- Same email: one free trial total.
- Same browser/device: one temporary trial per 30 days, up to six total by default.
- Same IP/network: three temporary trials per day by default.
- Same IP/user-agent: five temporary trials per day by default.
- Same email domain: twenty-five real-email trial grants per day by default.

## Verification

- `cargo fmt --manifest-path server/Cargo.toml`
- `node --check web/assets/bluey-site.js`
- `cargo test --manifest-path server/Cargo.toml -p bluey-server db::trial_abuse --quiet`
- `cargo test --manifest-path server/Cargo.toml signup_after_account_delete_reuses_email_without_new_trial --quiet`

## Deploy

Not deployed in this round. User asked not to deploy unless explicitly requested.

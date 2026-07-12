# ROUND-458 Account Connect Pill Trial Visibility

Date: 2026-07-09
Backup thread id: 019e133e-d92a-7830-8df0-3a050a4e22f6
Branch: codex/bluey-web-ui-parallel-20260704

## Goal

Make the first-time account and desktop-linking path easier to understand without forcing the large overlay open by default.

## Changes

- Moved the real desktop connection card to the top of the account dashboard, directly below the balance/trial area, so users do not have to discover it inside My Computers.
- Kept the existing desktop-link approval logic; the new top card uses the same pending code, approval, and linked-device refresh flow as the old dashboard card.
- Hid empty desktop-connect shells so the dashboard does not reserve blank space when no desktop action is needed.
- Changed macOS signed-out and sign-in boot handling to keep Bluey pill-first instead of forcing the full overlay open.
- Added one-time shortcut help when the user intentionally expands the pill for the first time, excluding the sign-in gate.
- Changed overlay balance labels to prefer remaining trial time while trial seconds exist, then fall back to dollar balance and low-balance warnings after trial is gone.

## Expected Behavior

- `bluey on` opens a compact pill unless the user explicitly expands it.
- If a desktop needs to be connected, the web dashboard shows the connect card near the top, not hidden below tabs.
- Temporary/trial accounts show a host label like `15m trial` while trial time remains.
- Paid accounts still show `$x.xx` and append `low` only when balance is low after trial is exhausted.

## Not Deployed

This round is local code only. Per current owner preference, no deploy or GitHub Actions release was started.

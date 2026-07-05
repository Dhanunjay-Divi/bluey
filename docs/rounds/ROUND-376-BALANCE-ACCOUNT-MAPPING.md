# Round 376 - Balance Account Mapping

Date: 2026-07-05

## Goal

Explain and reduce confusion when the Bluey web dashboard balance differs from the host overlay balance.

## Finding

Both surfaces read `balance_cents` from `/account/me`.

- The web dashboard shows the account signed into the browser.
- The host overlay shows the account saved in the desktop Bluey account profile.

On the checked machine, the browser was signed into `internal-admin-20260606023943@bluey.sh`, while the desktop account profile was signed into `codex-smoke-20260608183100@bluey.sh`. The `$15.00` web balance and `$7.56` overlay balance were therefore two valid balances for two different accounts, not a cents-formatting bug.

## Changes

- Added a compact balance-card hint: `Overlay different? Connect this desktop.`
- Wired that hint to the Computers tab code connector.
- Updated the desktop-code connector copy to explain that entering the desktop code moves that desktop onto the browser account so the dashboard and overlay balance match.
- Changed the connector button label from `Use code` to `Connect`.
- Bumped the static asset cache key to `2026070505`.

## Scope

Touched only web UI files and this round doc.

- `web/index.html`
- `web/assets/bluey-site.js`
- `web/assets/bluey-site.css`
- `docs/rounds/ROUND-376-BALANCE-ACCOUNT-MAPPING.md`

No native overlay, audio, backend runtime, database, or billing code was edited.

## Verification Plan

- Check JavaScript syntax.
- Check whitespace/diff safety.
- Preview `/account` in dark and light themes.
- Verify the new balance hint focuses the dashboard code connector.

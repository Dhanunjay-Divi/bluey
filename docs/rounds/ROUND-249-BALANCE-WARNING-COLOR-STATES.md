# Round 249 - Balance Warning Color States

Date: 2026-06-30
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The owner asked whether low balance should show orange and balances under `$5` should show red in both the balance text and dot.

## Decision

Use a simple, stable visual rule:

- `>= $10.00`: normal
- `$5.00` through `$9.99`: orange low-balance warning
- `< $5.00`: red critical balance warning

This avoids making normal spending feel alarming while making the under-`$5` state hard to miss.

## Changes

- Added shared macOS overlay balance tone parsing from balance labels such as `$6.67`, `$4.82 low`, and `Login`.
- Collapsed macOS Bluey pill now uses:
  - orange dot for low balance or sign-in-needed warning
  - red dot for critical balance under `$5`
  - green dot for healthy balance
- Expanded macOS header balance label now uses:
  - orange text for low balance
  - red text for under `$5`
  - theme-aware colors for both dark and light modes
- Dashboard floating balance pill now includes a colored dot and switches:
  - orange for low balance
  - red for under `$5`
- Dashboard settings balance value now uses orange/red text for the same thresholds.
- Web account dashboard balance KPI now uses orange/red border/background/text for the same thresholds.

## Windows Parity

No matching Windows overlay balance renderer was present in `native/windows/cue-overlay/main.c` during this round, so there was no Windows overlay surface to update. The dashboard/web changes apply to shared account surfaces.

## Files Changed

- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `crates/cue-dashboard/ui/src/components/BalanceIndicator.tsx`
- `crates/cue-dashboard/ui/src/pages/Settings.tsx`
- `web/assets/bluey-site.js`
- `web/assets/bluey-site.css`
- `docs/rounds/ROUND-249-BALANCE-WARNING-COLOR-STATES.md`
- `docs/rounds/BLUEY-COMPACTION-HANDOFF-2026-06-25.md`

## Verification

Passed:

```bash
swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift
npm --prefix crates/cue-dashboard/ui run build
git diff --check
```

## Current State

Low/critical balance is now visually differentiated in the overlay, desktop dashboard, and web account dashboard.

## Remaining QA and Gates

- Rebuild/hot-install the local macOS overlay to visually confirm the collapsed dot and header balance text.
- Deploy the web assets before expecting the hosted account dashboard to show the new orange/red KPI states.

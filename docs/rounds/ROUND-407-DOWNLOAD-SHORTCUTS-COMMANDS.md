# ROUND-407 Download Shortcuts Commands

Date: 2026-07-07
Backup thread id: 019e133e-d92a-7830-8df0-3a050a4e22f6

## Goal

Make the Bluey download page more self-serve by showing overlay shortcuts and useful CLI commands in the same simple style as Pinky's download page.

## Changes

- Added an always-visible "Overlay shortcuts" section directly under the OS selection cards on `/download`.
- The shortcut section stays visible before and after choosing an OS.
- Mac shortcut labels use `Ctrl+Option`; Windows shortcut labels switch to `Ctrl+Alt` when Windows is selected.
- Added practical shortcut descriptions for pill restore, Ask input, Listen, screen capture, click-through, Answer, Esc, Tab, and scroll keys.
- Changed the Windows download card from "Coming soon" to selectable "Ready" now that Windows binaries are published.
- Added Windows PowerShell install instructions using `irm https://bluey.sh/install.ps1 | iex`.
- Expanded the command grid with production-useful commands:
  - `bluey on`
  - `bluey off`
  - `bluey update`
  - `bluey status`
  - `bluey login`
  - `bluey logout`
  - `bluey usage`
  - `bluey portal`
  - `bluey sessions`
  - `bluey doctor`
  - `bluey support`
  - `bluey logs export`
  - `bluey export`
  - `bluey uninstall`
  - `bluey help`

## Notes

- The download page now makes it clear that `bluey on` starts Bluey, checks updates, and handles sign-in if needed.
- The copy is intentionally beginner-facing: install, start, connect if asked, then use the shortcut/command reference.
- This does not change app shortcut behavior; it exposes the current public controls and commands on the website.

## Verification

- `git diff --check` passed.
- Static markup smoke check passed for Windows install instructions, shortcut copy, and command copy buttons.
- Local `/download` render check passed: Mac defaults, Windows selection switches labels to `Ctrl+Alt`, and Windows install command appears.
- Deployed to `https://bluey.sh` with `scripts/deploy-bluey-sh-manual.sh`.
- Live `/download` verification passed for the shortcut section, Windows PowerShell install command, and expanded command grid.
- Live installer/release checks passed: `latest.json` signature verified, installer MIME types remained correct, and the macOS release artifact SHA/version verified.

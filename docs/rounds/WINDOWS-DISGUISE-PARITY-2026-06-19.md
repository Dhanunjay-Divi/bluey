# Windows Disguise Parity

Date: 2026-06-19
Branch: `codex/bluey-ai-site`

## Goal

Bring Windows dashboard disguise behavior up to the same benign user-facing
standard as macOS:

- persisted disguise mode applies on startup
- tray menu can change the mode
- dashboard windows use the selected mode title
- tray/taskbar icon swaps to the selected mode
- Settings uses platform-native labels

This is not process hiding. Windows Task Manager still shows the shipped
executable image name because Windows does not support safely changing that at
runtime. The Windows equivalent of the macOS surface is AppUserModelID,
window title, tray/taskbar identity, and icon.

## Changes

- `crates/cue-dashboard/src/commands.rs`
  - Added platform-aware embedded disguise icon selection.
  - Windows now uses `icons/disguise/win/*.png`; macOS keeps using
    `icons/disguise/mac/*.png`.

- `crates/cue-dashboard/src/lib.rs`
  - Made the tray disguise labels platform-aware:
    - Windows: Task Manager, Command Prompt, Settings
    - macOS: Activity Monitor, Terminal, System Settings

- `crates/cue-dashboard/ui/src/pages/Settings.tsx`
  - Made Settings disguise labels platform-aware at runtime.
  - Replaced stale OS-keychain wording with the current local account profile
    token-storage wording.

- `crates/cue-dashboard/icons/disguise/README.md`
  - Updated the manual smoke checklist and limitations to match current
    behavior.

## Verification

Run on macOS:

```bash
cargo fmt --all --check
cargo test -p cue-stealth -p cue-dashboard --lib
cargo clippy -p cue-dashboard -p cue-stealth --all-targets -- -D warnings
```

Run on Windows via SSH/build worker:

```powershell
cargo fmt --all --check
cargo clippy -p cue-dashboard -p cue-stealth --all-targets -- -D warnings
cargo test -p cue-stealth -p cue-dashboard --lib
```

Manual Windows desktop smoke must be run from the signed-in interactive desktop
session, not SSH/session 0:

1. Start Bluey dashboard.
2. Open Settings -> Disguise.
3. Select Task Manager, Command Prompt, Settings, then Off.
4. Verify tray/taskbar icon and dashboard title update each time.

## Most Likely Gaps

- The PNGs are still placeholder-quality; replace them with production icons
  before broad Windows release.
- The Windows process image name remains `bluey`/`cue-dashboard` by design.
  Do not add anti-security process hiding to work around this.

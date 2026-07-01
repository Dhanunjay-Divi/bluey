# Round 275 - Hide Collapses To Pill

## Trigger

The owner asked whether Hide should make Bluey a small pill instead of hiding every Bluey window completely.

Continuity anchor: backup thread id `019e133e-d92a-7830-8df0-3a050a4e22f6`.

## Decision

Yes. Normal Hide should mean "minimize Bluey to the pill." A full disappearance feels too much like Bluey broke or quit. The true stop path remains the Turn Off / close confirmation flow.

## Fix

- macOS:
  - `Ctrl+Option+B` now collapses the expanded overlay to the pill instead of hiding all Bluey chrome.
  - Pressing the same shortcut from the pill restores the expanded overlay.
  - IPC `hide` now collapses to the pill.
  - Shortcut copy now says `Minimize to pill / restore`.
  - Signed-out auth gate does not collapse to a pill; the hide control is disabled while signed out so users finish auth first.
- Windows:
  - `Ctrl+Alt+B`, IPC `hide`, and IPC `toggle` now use the existing `collapse_to_pill(...)` path instead of `hide_overlay_completely(...)`.
  - Help and shortcut copy now say Hide minimizes to the small button / pill.
- Bumped the desktop workspace version to `0.1.29`.

## Verification

- `cargo check -p cue-daemon --offline`
- `swift build -c debug --package-path native/macos/cue-overlay`
- `/opt/homebrew/bin/x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c`
- `cargo test -p cue-daemon --lib --locked`
  - `281 passed; 0 failed; 2 ignored`
- `cargo test -p cue-core sign_in_event_serializes --locked`
- Release dry-run scan passed:
  - dev flag scan
  - secret scan
  - manifest generation
- Published live macOS arm64 artifact:
  - `https://bluey.sh/latest.json` reports `0.1.29`
  - SHA256: `17ad3711d96a16d3647cb6a0f16b454fc5ea5cf608cdc4ea0fa1f37c2457b016`
  - `https://bluey.sh/install.sh` serves as `application/x-shellscript`
  - `latest.json.sig` verified successfully
- Local update installed and restarted:
  - `/Users/uno/.local/bin/bluey --version` -> `bluey 0.1.29`
  - `bluey status` showed `overlay_capture_excluded: true`
  - `Ctrl+Option+B` smoke collapsed/restored the overlay state false/true

## Current State

Hide now behaves like a minimize-to-pill action on both native overlay implementations. Users can still turn Bluey off through the close/power path when they actually want it stopped.

## Remaining QA / Gates

- Manual signed-out auth gate smoke should still be run with a clean signed-out profile to confirm it does not collapse into an unusable pill.
- Windows artifact still needs a full build/sign/publish pass before Windows users receive the parity change.

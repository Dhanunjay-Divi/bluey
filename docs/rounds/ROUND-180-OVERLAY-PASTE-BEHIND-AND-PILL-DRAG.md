# Round 180 - Overlay Paste Behind And Pill Drag

## Trigger

Owner asked for the overlay to make it easy to write Bluey answers into the app behind the overlay, since the whole point of the overlay is fast use without manual copy/paste. Owner also asked to keep looking for overlay UI improvements and to carry Mac changes to Windows.

Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`  
Workspace: `/Users/uno/Downloads/cue`  
Round completed: 2026-06-26 02:27 EDT

## Root Cause

- The macOS overlay had answer-card copy controls, but no explicit "place this answer into the app behind Bluey" affordance.
- The daemon had no typed overlay event for this action, so adding it as an ad hoc command would have weakened the overlay security boundary.
- Windows had no equivalent answer paste affordance.
- Windows collapsed-pill drag used a very small movement threshold, making small pointer jitter more likely to expand the pill while dragging.

## Fix

- Added a typed `paste_text_requested` overlay event:
  - active rich overlay schema: `OverlayEvent::PasteTextRequested { text, target_bundle_id }`
  - compact IPC schema: `OverlayIpcCommand::PasteTextRequested { text, target_bundle_id }`
  - production validator rejects oversized `text` and `target_bundle_id`
  - legacy overlay bridge forwards the new event shape for exhaustive handling
- Added daemon paste handling:
  - trims empty requests and rejects empty paste text
  - hides/collapses Bluey before sending the paste shortcut
  - macOS sets clipboard with `pbcopy`, optionally activates the remembered target bundle id, then sends normal Command+V through System Events
  - Windows sets clipboard through an STA PowerShell helper and sends Ctrl+V through `System.Windows.Forms.SendKeys`
  - failures produce a visible warning card instead of silently doing nothing
- Added macOS answer-card paste UI:
  - new compact paste action next to the existing copy icon
  - only appears on completed answer cards with real text
  - uses the sanitized visible answer text
  - remembers the last non-Bluey active macOS application bundle id so paste targets the previous app when possible
- Added Windows overlay parity:
  - native `Paste answer` button for the current visible answer card
  - button emits the same `paste_text_requested` event with the session token
  - button is hidden outside answer cards and in collapsed mode
  - click-through hit testing treats the button as intentional interactive chrome
- Hardened Windows pill dragging:
  - collapsed pill drag threshold is now 4 px
  - release-time movement is checked too, so tiny drag/jitter does not accidentally expand the pill.

## Security And Abuse Notes

- This is not a general remote-control endpoint.
- The overlay can request only one bounded text paste event; it cannot request arbitrary keystrokes or commands.
- Events remain session-token validated by the daemon.
- Text length remains capped at `64 KiB`; target bundle id length is capped.
- The paste action is explicit user chrome, not auto-send or background automation.
- Mac target activation accepts only reasonable bundle identifiers before invoking AppleScript.

## Mac Windows Parity

- macOS: per-answer-card paste icon beside copy, with target-app memory.
- Windows: current-answer `Paste answer` native button with the same event and daemon paste bridge.
- Windows has no macOS-style bundle id target; the helper is best-effort after collapsing the overlay.
- Windows pill drag threshold was also tightened while touching overlay UX.

## Verification

Passed:

- `cargo fmt --manifest-path crates/cue-core/Cargo.toml`
- `cargo fmt --manifest-path crates/cue-daemon/Cargo.toml`
- `cargo test -p cue-core overlay`
- `cargo check -p cue-daemon`
- `cargo test -p cue-daemon overlay_paste_text_event`
- `cargo test -p cue-daemon overlay`
- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c -municode`

Results:

- cue-core overlay tests: `29` passed.
- cue-daemon overlay-focused tests: `46` unit tests plus overlay integration/security tests passed.
- macOS overlay Swift parse passed.
- Windows overlay C syntax passed with MinGW.

## Files Touched

- `crates/cue-core/src/overlay.rs`
- `crates/cue-core/src/overlay_ipc.rs`
- `crates/cue-daemon/src/app.rs`
- `crates/cue-daemon/src/overlay.rs`
- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `native/windows/cue-overlay/main.c`
- `docs/rounds/ROUND-180-OVERLAY-PASTE-BEHIND-AND-PILL-DRAG.md`
- `docs/rounds/BLUEY-COMPACTION-HANDOFF-2026-06-25.md`

## Current State

- Bluey now has an explicit answer paste path from overlay to the app behind it.
- The paste path is visible, bounded, token-validated, and failure-reporting.
- Mac and Windows overlay UX both expose the action.
- Windows collapsed pill dragging should be less likely to open accidentally from small pointer jitter.

## Remaining QA Gates

- Manual macOS paste QA still needs a real GUI pass in common targets such as browser text fields, Notes, and VS Code. The helper may require macOS Accessibility permission for System Events.
- Manual Windows paste QA still needs a real Windows desktop pass because local verification was syntax-only; PowerShell is not installed on this Mac host, but MinGW syntax passed.
- Clipboard content is intentionally set to the answer as part of the paste path, matching familiar copy/paste behavior.

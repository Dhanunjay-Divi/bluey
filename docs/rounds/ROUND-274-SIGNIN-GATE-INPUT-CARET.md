# Round 274 - Sign-In Gate And Input Caret

## Trigger

The owner shared a video and screenshot showing two user-facing issues:

- The Ask input showed two blinking carets at once.
- Clicking Sign in from the overlay could look like it did nothing, and the signed-out pill/expanded UI still exposed controls that should not be usable before account auth.

Continuity anchor: backup thread id `019e133e-d92a-7830-8df0-3a050a4e22f6`.

## Root Cause

- The macOS composer drew a custom cyan caret while `NSTextView` also drew its native insertion caret.
- The signed-out overlay was still following normal click-through/pass-through rules in too many places, so the auth action could feel dead when click-through was enabled or when the browser login flow was already open.
- The daemon accepted the overlay `sign_in_requested` event but discarded the visible status text from the login helper, so repeated clicks did not give fresh overlay feedback.

## Fix

- Removed the custom macOS composer caret and let the native text caret own blinking.
- Added an explicit signed-out gate state in the macOS expanded overlay:
  - signed-out overlay receives mouse events even when click-through is enabled
  - real Sign in buttons remain clickable
  - unusable controls are disabled and dimmed while signed out
  - Listen, Screen, Answer, Attach, and Text shortcuts route to the sign-in flow instead of doing paid or local work
  - successful account state unlock collapses the gate back to the pill
- Changed overlay boot handling so sign-in boot cards expand the auth gate, while non-sign-in boot/status after unlock can collapse back to the normal pill.
- Changed daemon `sign_in_requested` handling to push a visible `Bluey sign-in` status card so users see feedback when the browser/device login is already in progress.
- Bumped the desktop workspace version to `0.1.28`.

## Verification

- `swift build -c debug --package-path native/macos/cue-overlay`
- `cargo check -p cue-daemon --offline`
- `cargo test -p cue-daemon --lib --locked`
  - `281 passed; 0 failed; 2 ignored`
- `cargo test -p cue-core sign_in_event_serializes --locked`

## Current State

The macOS overlay should no longer show duplicate input blinkers. When Bluey is signed out, the full overlay behaves as a sign-in gate instead of a half-usable pill: sign-in clicks work regardless of click-through state, blocked actions re-open the auth flow with feedback, and a successful auth state collapses back to the normal pill.

## Windows Parity

The daemon-side sign-in feedback is shared Rust code and applies to Windows builds. The duplicate caret and expanded signed-out gate behavior patched here are specific to the macOS native Swift overlay. The Windows overlay is a separate C surface and did not receive an equivalent visual caret patch in this round; it should still be checked during the next Windows artifact build before claiming full UI parity.

## Remaining QA / Gates

- Run a live signed-out install smoke from a clean macOS user profile:
  - `bluey on` opens the sign-in gate
  - Sign in opens browser login
  - completing device auth collapses Bluey to the pill
  - Listen and Answer remain blocked until auth state is confirmed
- Add a Windows overlay smoke for signed-out click-through/login behavior before republishing a Windows artifact.
- Consider debouncing repeated sign-in clicks so the overlay cannot stack several login status cards if the user clicks rapidly.

# FIX-585: Readable Light Shortcuts And Clear Desktop Sign-In

> **Codex preflight:** Load `$bluey-ops` before diagnosis or implementation and
> verify its memory against the current repository state.

## Issue

The macOS shortcut help could become unreadable in light mode at low overlay
opacity, successful desktop sign-in did not point new users to the Shortcuts
control, and the browser/device copy made the embedded connection code look
like a second mandatory step.

## Root Cause

- The confirmation surface inherited the user-selected translucent material,
  so light text and controls could wash into the content behind the overlay.
- The first-run help was a full modal shown on expansion rather than a small,
  one-time control-anchored explanation after a visible sign-in transition.
- The desktop and browser described the same device code as a primary manual
  action even though `device_login_url` already carries it to the browser.

## Fix Summary

- Made confirmation and shortcut help surfaces independently opaque in both
  themes and rebuilds the shortcut text when the theme changes.
- Added a one-time, keyboard-accessible coachmark anchored to the real
  Shortcuts button. It appears only after a visible signed-out to signed-in
  transition or the first eligible expansion, never replaces active UI, and
  never steals focus from a text editor.
- Kept the collapsed pill exactly 112 by 30.
- Reworded terminal, daemon, native, and hosted-browser instructions around one
  explicit `Connect this Bluey` confirmation. Manual code entry is retained as
  a fallback without weakening device-authorization security.
- Added metadata-only local lifecycle/action logs for the new UI. Continuous
  opacity and acknowledgement traffic is excluded from the action log.

## Files Modified

| File | Change |
|------|--------|
| `native/macos/cue-overlay/Sources/cue-overlay/main.swift` | Theme-independent help, coachmark policy, lifecycle events, exact pill assertion |
| `native/macos/cue-overlay/Sources/cue-overlay/ShortcutCoachmarkView.swift` | Accessible anchored coachmark UI |
| `crates/cue-cli/src/app.rs` | One-confirmation terminal copy |
| `crates/cue-daemon/src/app.rs` | One-confirmation card copy and discrete metadata-only action logging |
| `web/assets/bluey-site.js` | Connected-device confirmation and fallback-code copy |
| `web/index.html` | Download/account fallback-code instructions |

## Edge Cases Handled

- Switching theme while shortcut help is already open.
- Low overlay opacity in light and dark themes.
- Hidden browser sign-in flows that must not steal focus.
- A user opening Shortcuts, history, tone, delete, or turn-off during the
  delayed coachmark callback.
- A user focusing the composer before the delayed coachmark callback.
- Sign-out while the coachmark is visible.
- Manual code entry when the browser did not retain the embedded code.

## How to Test

```bash
node --check web/assets/bluey-site.js
swift build -c debug --package-path native/macos/cue-overlay
swiftc -D BLUEY_AUTH_UI_POLICY_TESTS \
  native/macos/cue-overlay/Sources/cue-overlay/main.swift \
  native/macos/cue-overlay/Sources/cue-overlay/ShortcutCoachmarkView.swift \
  -o /tmp/bluey-auth-ui-policy-tests
/tmp/bluey-auth-ui-policy-tests
```

Development-only capture-visible QA verified the light coachmark and the full
light shortcut modal. The resulting flag is not part of release configuration.

## Known Limitations

- Physical packaged macOS keyboard, VoiceOver, and pointer certification is a
  release gate, not represented by source-level visual QA.
- The full cross-process diagnostic spine is intentionally a separate batch;
  this fix does not upload local diagnostics or add R2 credentials to clients.

# Round 109 - Copy Selection And Caption Label Cleanup - 2026-06-22

## Why This Round Exists

The overlay made everyday copying feel too custom: copy buttons did not confirm
success, answer body text was rendered like a label instead of selectable text,
and transcript-composed questions could show noisy labels such as `Mic:`.

## Changes

- Made message body fields selectable for normal answer/question reading.
- Updated keyboard routing so selectable text views keep `Cmd+A`/`Cmd+C` instead
  of the composer stealing those shortcuts.
- Added a temporary checkmark state to message copy buttons and canvas copy.
- Expanded copy buttons to normal answer/context/warning cards.
- Trimmed simple `Mic:`, `Microphone:`, `System:`, and `Audio:` prefixes from
  user-facing question cards and provider-facing transcript-composed questions.
- Kept `Mic:`/`System:` labels in the live captions preview, where they help
  users confirm which source Bluey is hearing.

## UX Contract

- Clicking copy should give immediate visual confirmation.
- Users should be able to select/copy answer text with normal macOS gestures and
  keyboard shortcuts.
- Caption labels should stay in the live preview, but not leak into the
  question card or provider-facing follow-up unless multiple sources make the
  label meaningful.

## Verification

- Focused daemon tests cover visible question cleanup.
- macOS overlay build should pass after the selectable text and copy-state
  changes.

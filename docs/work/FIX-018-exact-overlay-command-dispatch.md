# FIX-018: Exact Overlay Command Dispatch

## Issue

Daemon messages could trigger native overlay window actions when command words
appeared anywhere in transcript or card JSON.

## Root Cause

`crates/cue-meeting-overlay/src/ipc.rs` classified incoming NDJSON with substring
checks such as `line.contains("\"hide\"")`. It did not require the value to be
the message's top-level `type`.

## Fix Summary

Each daemon line is parsed once into a small command classification. Window
show, hide, toggle, boot, and meeting-banner actions now match only exact
top-level `type` values. The same parse supplies the banner occurrence key, so
banner queuing does not parse the line a second time.

## Files Modified

| File | Change |
|------|--------|
| `crates/cue-meeting-overlay/src/ipc.rs` | Added typed, single-parse command classification and adversarial tests. |
| `CHANGELOG.md` | Recorded the hardening. |

## Edge Cases Handled

- Command words inside transcript text, card titles/bodies, or nested objects.
- Malformed JSON and missing/non-string top-level `type` fields.
- Similar but unsupported command names such as `show_meeting_banner_suffix`.
- Banner retry deduplication and occurrence tombstones remain unchanged.

## How to Test

```bash
cargo test -p cue-meeting-overlay --lib
```

## Known Limitations

- Unknown daemon command types are still forwarded to the meeting webview for
  forward compatibility, but they cannot perform native window actions.

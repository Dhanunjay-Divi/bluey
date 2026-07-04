# Bluey 0.1.85

Released: 2026-07-04

## Summary

This release adds display-only line numbers to the macOS code canvas so Bluey's `Line notes` can point at visible code lines without polluting copied code.

## Changes

- Code canvas now numbers the rendered `CODE` section.
- `LINE NOTES`, `COMPLEXITY`, and other prose sections remain unnumbered.
- The canvas copy button still copies the raw runnable code section.
- Normal keyboard copy from selected canvas text strips visual line-number gutters before writing to the pasteboard.
- Code comment tinting still works with numbered display lines.

## Verification

```bash
swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift
cd native/macos/cue-overlay && swift build -c release
cargo fmt --check
cargo check -p cue-cli -p cue-daemon
cargo test -p cue-daemon code_artifact_preview_keeps_line_notes_out_of_code_fence --lib
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem scripts/bluey-release-live-verify.sh 0.1.85
curl -fsSL https://bluey.sh/install.sh | bash
/Users/uno/.bluey/bin/bluey --version
/Users/uno/.bluey/bin/bluey-daemon --version
/Users/uno/.bluey/bin/bluey on
/Users/uno/.bluey/bin/bluey status
```

Live artifact SHA256:

```text
7f6c6ebd4740f0a20ab1019849c119e20f0b14fc64b5e7b795a87e90d6a9c15e
```

Round doc:

- `docs/rounds/ROUND-346-CODE-CANVAS-LINE-NUMBERS.md`

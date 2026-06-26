# Round 022 - Overlay Brand Asset Pass - 2026-06-02

## Goal

Carry the refined Bluey logo and lowercase wordmark from the public website into the native overlay so the app UI no longer renders a plain text `>_` mark plus a system-font `Bluey` label.

## Changes

- macOS overlay embeds the current SVG logo and lowercase wordmark directly in `main.swift`.
- Collapsed pill now renders `BlueyLogoView` + `BlueyWordmarkView` with the status dot tucked immediately after the wordmark.
- Expanded overlay header now uses the same logo + wordmark treatment, with the recording/session status underneath.
- Windows overlay visible labels were aligned to lowercase `bluey` pending a fuller Windows vector-wordmark port.

## Verification

Run:

```bash
swift build -c release --package-path native/macos/cue-overlay
bash native/macos/cue-overlay/build.sh
cargo fmt --all --check
git diff --check HEAD~1..HEAD
```

## Follow-Ups

- Port the exact SVG wordmark to Windows Direct2D/GDI instead of using lowercase text.
- Use visual Mac smoke to confirm the pill still feels compact at desktop scale.

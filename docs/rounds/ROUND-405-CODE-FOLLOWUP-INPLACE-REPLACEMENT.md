# ROUND-405 Code Followup Inplace Replacement

Date: 2026-07-07

Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Goal

Make Bluey code follow-ups behave like a reliable workbench:

- If the user asks for a change, language conversion, or "give code for the same", keep the active code canvas and replace it in place.
- Always provide the whole updated code, not only a patch, diff, or changed block.
- Preserve explanation-only follow-ups as chat-only so the current code canvas does not churn.

## Changes

- Updated the server AnswerPlan coding-followup prompt so changed/requested code must be a full in-place replacement with imports, signatures, body, return path, and cleanup/sentinel logic.
- Rejected patch-only or diff-only responses as valid code artifacts in the server artifact detector.
- Updated macOS overlay canvas registration so code follow-up artifacts replace the active code canvas instead of appending a new canvas entry.
- Kept system-design follow-ups separate: design changes still update only the affected design section unless a full redesign is requested.
- Updated daemon prompt parity so Code mode and General mode no longer tell models to prefer patches, changed blocks, or unified diffs.
- Updated tests to lock the new whole-code replacement behavior.

## Verification

Passed:

- `cargo fmt --manifest-path server/Cargo.toml --all`
- `cargo fmt --manifest-path crates/cue-daemon/Cargo.toml --all`
- `cargo test --manifest-path server/Cargo.toml --lib --quiet`
- `cargo test --manifest-path crates/cue-daemon/Cargo.toml --lib --quiet`
- `swift build -c release` in `native/macos/cue-overlay`

## Notes

This round changes source behavior and validates the Mac overlay build. Desktop release `0.1.89` was published through the signed release path so end users receive the overlay-side replacement behavior through `bluey on` updates.

Published desktop release:

- Live manifest: `https://bluey.sh/latest.json`
- Darwin arm64 artifact: `https://bluey.sh/releases/v0.1.89/bluey-0.1.89-darwin-arm64.tar.gz`
- Live release verification passed: `latest.json` signature, installer MIME types, artifact SHA, and unpacked binary versions.

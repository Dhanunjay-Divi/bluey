# Round 116 - Mac Intel Release Support

Date: 2026-06-22

## What Changed

- Release workflow now builds two macOS artifacts:
  - `darwin-arm64` for Apple Silicon Macs.
  - `darwin-x86_64` for Intel Macs.
- macOS Swift helper build scripts now honor `BLUEY_SWIFT_ARCH`, so overlay, audio, picker, and whisper helpers match the artifact CPU architecture.
- Install scripts now select the correct macOS artifact from the current machine architecture instead of rejecting Intel Macs.

## Why

Bluey already had enough source-level support for macOS, but the published release and installer path was Apple Silicon only. Intel users would fail before downloading anything.

## Verification

- `bash -n` passed for the touched install and macOS helper build scripts.
- `.github/workflows/release.yml` parsed successfully with Ruby YAML.
- `git diff --check` passed for the touched files.

## Local Note

The current local install on this Apple Silicon laptop is still arm64 only because it was built locally. The release workflow is what now produces both Apple Silicon and Intel Mac downloads.

# Bluey v0.1.65

Date: 2026-07-03
Branch: codex/bluey-overlay-spacing-20260626

## Summary

This release fixes the macOS attachment picker so valid PDFs are not hidden by the system file-type filter.

## Changes

- Removed the native macOS picker helper's inferred `allowedContentTypes` filter.
- Kept Bluey's explicit extension validator, which allows PDF, DOC/DOCX, Excel/ODS, text, code/data files, and common images.
- Unsupported files are still blocked by Bluey's picker delegate.

## Verification

```bash
native/macos/cue-picker/build.sh
cargo test -p cue-daemon picker_context_filter_rejects_video_and_key_material -- --nocapture
cargo check -p cue-daemon
/Users/uno/.bluey/bin/bluey-doc-converter /Users/uno/Downloads/Interview_Instructions.pdf -o /tmp/bluey-pdf-test.md
```

## Deployment Status

Live on `bluey.sh`.

Live Darwin arm64 artifact:

```text
https://bluey.sh/releases/v0.1.65/bluey-0.1.65-darwin-arm64.tar.gz
```

SHA256:

```text
75191d76c86f8aa5e9632938cd17ae40e865dbdcec78c1bff44805c7ea60ff77
```

Live deployment checks passed:

- `latest.json` signature verified.
- `latest.json` reports `0.1.65`.
- `/install.sh` serves `application/x-shellscript`.
- `/install.ps1` serves `application/x-powershell`.
- Darwin arm64 artifact SHA verified.
- Unpacked `bluey` and `bluey-daemon` report `0.1.65`.
- Local install from `https://bluey.sh/install.sh` completed and Bluey was restarted.

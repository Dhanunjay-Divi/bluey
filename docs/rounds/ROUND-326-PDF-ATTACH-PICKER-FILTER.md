# Round 326 - PDF Attach Picker Filter

Date: 2026-07-03
Backup thread id: 019e133e-d92a-7830-8df0-3a050a4e22f6
Branch: codex/bluey-overlay-spacing-20260626

## Trigger

The owner reported that the attachment picker seemed to allow DOC/DOCX files but not PDF files.

## Root Cause

PDF was already present in Bluey's supported extension lists:

- macOS overlay drag/drop allowlist
- daemon AppleScript fallback picker allowlist
- Windows picker filter
- daemon document conversion support

The remaining fragile layer was the packaged macOS `BlueyFilePicker.app`. It used both:

- Bluey's extension-based panel delegate
- macOS `allowedContentTypes` inferred from filename extensions

The extension delegate already enforces Bluey's supported files. The extra macOS content-type filter was unnecessary and could hide or grey out valid PDFs depending on Finder/UTI behavior, file metadata, or stale helper state.

## Fix

- Removed the native macOS picker helper's `allowedContentTypes` filter.
- Kept Bluey's extension-based `NSOpenSavePanelDelegate` validation, which explicitly allows `.pdf`.
- Kept unsupported files blocked by the delegate validation path.
- Bumped desktop version to `0.1.65`.

Windows parity: Windows already includes `*.pdf` in the picker filter, so no Windows source change was needed for this specific bug.

## Verification

Passed locally:

```bash
native/macos/cue-picker/build.sh
cargo test -p cue-daemon picker_context_filter_rejects_video_and_key_material -- --nocapture
cargo check -p cue-daemon
/Users/uno/.bluey/bin/bluey-doc-converter /Users/uno/Downloads/Interview_Instructions.pdf -o /tmp/bluey-pdf-test.md
```

The converter smoke produced markdown from a real PDF. `pdftotext` is not installed on this Mac, so the successful conversion confirms the bundled Bluey document converter can handle PDFs.

## Deployment

Completed:

- Packaged desktop release `0.1.65`.
- Published `0.1.65` to `bluey.sh`.
- Verified live signed release metadata and installer MIME types.
- Verified the live Darwin arm64 artifact SHA.
- Verified unpacked release binaries report `0.1.65`.
- Installed locally from `https://bluey.sh/install.sh`.
- Restarted Bluey locally.

Live Darwin arm64 artifact:

```text
https://bluey.sh/releases/v0.1.65/bluey-0.1.65-darwin-arm64.tar.gz
```

SHA256:

```text
75191d76c86f8aa5e9632938cd17ae40e865dbdcec78c1bff44805c7ea60ff77
```

Local restart status after install:

```text
bluey 0.1.65
bluey-daemon 0.1.65
overlay_visible=true
```

## Remaining QA / Gates

- Open the attachment picker from the overlay and confirm `.pdf` files can be selected.
- Drag/drop a PDF into the overlay and confirm the file indexes into the session.

# Round 134 - Windows/Mac Attachment Parity - 2026-06-22

## Goal

Keep Windows behavior aligned with the macOS overlay for attachment and context
handling, especially after the Excel/unsupported-file fixes.

## Parity Checked

- Supported context formats are shared at the daemon level:
  PDF, DOC/DOCX, Excel/ODS, CSV/TSV, text, Markdown, code/data files, and
  PNG/JPEG/WebP/GIF/HEIC/BMP/TIFF images.
- macOS drag/drop now pre-filters unsupported files and shows the supported
  formats immediately.
- Windows `WM_DROPFILES` now pre-filters the same formats before emitting
  `attach_files_requested`.
- Mixed drops attach supported files and tell the user how many unsupported
  files were skipped.
- Fully unsupported drops do not start a fake indexing state.
- Fully supported drops show a `Docs loading`/indexing state immediately on both
  macOS and Windows.
- Windows retains its existing capture-exclusion, middle-card click-through,
  header move, edge resize, ask input, and context-chip paths.
- Windows installer already installs Bluey-local document conversion tools into
  `%LOCALAPPDATA%\Bluey\tools\doc-converter`, matching the macOS local-tool
  model.

## Verification

- `cargo check --manifest-path crates/cue-daemon/Cargo.toml` passed.
- `swift build --package-path native/macos/cue-overlay` passed in the attachment
  fix round.
- Windows overlay cross-compiled with `x86_64-w64-mingw32-gcc` after the parity
  changes.
- A temp unsupported `.mp4` attach smoke on the daemon path fails fast with the
  supported-format list.

## Platform-Specific Notes

Exact implementation cannot be byte-for-byte identical because macOS and Windows
use different windowing APIs. The intended product behavior should match:
supported files attach, unsupported files explain why, readable cards pass
through clicks, controls stay interactive, and the overlay can be moved/resized.


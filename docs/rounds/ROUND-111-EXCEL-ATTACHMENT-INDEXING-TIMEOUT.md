# Round 111 - Excel Attachment Indexing Timeout - 2026-06-22

## User-Visible Problem

Dropping an Excel file could leave the overlay showing `Indexing` for several
minutes. The user expected a quick visible acknowledgement, then a ready file
chip, not an indefinite loading state.

## Root Cause

There were three separate failures in the same path:

- Spreadsheet extensions were missing from the daemon context classifier and
  native file-picker filters, so `.xlsx` files were not consistently accepted as
  document context.
- The macOS overlay started the knowledge-indexing indicator during drag/drop
  but had no safety timeout if the daemon skipped or rejected the file before a
  normal ready event returned.
- Production `/router/embed` returned `502` for OpenAI embeddings because the
  embedding response includes prompt usage but no completion-token field; the
  shared usage parser required `completion_tokens`.

## Code Change

- Treated `xls`, `xlsx`, `xlsm`, `xlsb`, and `ods` as document context in the
  daemon.
- Added those spreadsheet formats to the macOS native picker, macOS AppleScript
  fallback, and Windows picker filter.
- Kept spreadsheet conversion inside Bluey's bundled converter path. If the
  bundled MarkItDown converter is unavailable, Bluey now reports that spreadsheet
  conversion needs the bundled converter instead of silently spinning.
- Added a macOS overlay safety timeout so the visible indexing badge is capped
  to a few seconds. If context is visible it becomes `Files ready`; if not, the
  loading strip clears.
- Added immediate unsupported-drop feedback in the macOS overlay. If every
  dropped file is unsupported, Bluey skips indexing and shows the supported
  formats; if a mixed drop contains unsupported files, Bluey attaches the
  supported files and explains what was skipped.
- Added the same unsupported-drop filter to the Windows overlay drag/drop path.
- Centralized the daemon supported-format wording so picker, drop, and CLI
  paths stay consistent.
- Relaxed OpenAI usage parsing for embeddings so production accepts valid
  embedding responses that omit completion tokens.

## Verification

- `cargo test --manifest-path crates/cue-daemon/Cargo.toml doc_conversion --lib`
  passed.
- `cargo check --manifest-path crates/cue-daemon/Cargo.toml` passed.
- `swift build --package-path native/macos/cue-overlay` passed.
- `swift build --package-path native/macos/cue-picker` passed.
- Windows overlay cross-compile with `x86_64-w64-mingw32-gcc` passed.
- `cargo check --manifest-path server/Cargo.toml` passed.
- `cargo test --manifest-path server/Cargo.toml bluey_spend_cents_in_window_sums_recent_provider_cost`
  passed.
- `bluey-doc-converter` converted
  `/Users/uno/Downloads/Harshitha_regression_analysis.xlsx` into a 23 KB
  Markdown table in under a second.
- `bluey context add /Users/uno/Downloads/Harshitha_regression_analysis.xlsx`
  now shows `[document:ready] Harshitha_regression_analysis.xlsx` and local
  status reports `context_items: 1`.
- A temp `.mp4` context-add smoke now fails fast with the supported-format list
  instead of a vague unsupported-file error.
- Production `bluey-api` was rebuilt and restarted active after the embedding
  parser fix.
- A live managed `/router/embed` smoke against `https://bluey.sh` returned
  `HTTP 200`, provider `openai`, model `text-embedding-3-small`, and a
  1536-dimension vector.

## Product Rule

Attachment UI should never leave the user wondering whether Bluey is stuck.
After a drop, show loading briefly, then show ready chips or a clear skip/error
message while indexing continues in the background.

Unsupported files should not start a fake indexing state. Show the accepted
formats immediately, and keep attaching any supported files from the same drop.

# ROUND-448 MarkItDown Discovery

Date: 2026-07-08
Branch: `codex/bluey-web-ui-parallel-20260704`
Backup thread: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Why

PDFs were already accepted by the picker and document pipeline, but local/dev runs could miss the bundled MarkItDown converter. When that happened, Bluey fell back to the native PDF parser, which needs `pdftotext`; on this machine `pdftotext` is not installed.

Installed Bluey had the converter here:

- `/Users/uno/.bluey/bin/bluey-doc-converter`
- `/Users/uno/.bluey/tools/doc-converter/.venv/bin/markitdown`

Those paths were not on shell `PATH`, so a debug binary under `target/debug` could fail to find them.

## Changed

- Kept the existing converter order: explicit env vars, binary-adjacent install paths, PATH fallbacks.
- Added installed-home discovery for:
  - `BLUEY_INSTALL_ROOT`
  - `$HOME/.bluey`
  - `$USERPROFILE/.bluey`
- Added macOS/Linux candidates:
  - `bin/bluey-doc-converter`
  - `tools/doc-converter/bin/bluey-doc-converter`
  - `tools/doc-converter/.venv/bin/markitdown`
- Added Windows parity candidates:
  - `bin/bluey-doc-converter.cmd`
  - `tools/doc-converter/bin/bluey-doc-converter.cmd`
  - `tools/doc-converter/.venv/Scripts/markitdown.exe`
- Added a unit test covering installed converter candidate discovery.

## Result

Bluey still uses MarkItDown first for PDFs, Word, PowerPoint, and spreadsheets. The fix makes local/dev binaries find the installed converter before falling back to native parsers, so PDF attach should not depend on a separate `pdftotext` install when the bundled Bluey converter exists.

## Verification

- `cargo fmt --manifest-path crates/cue-daemon/Cargo.toml`
- `cargo test --manifest-path crates/cue-daemon/Cargo.toml doc_conversion --quiet`

## Not Done

- No deploy.
- No release artifact build.
- No GitHub Actions run.

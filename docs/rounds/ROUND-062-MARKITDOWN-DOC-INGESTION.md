# Round 062 - MarkItDown Document Ingestion — 2026-06-19

## Goal

Make Bluey treat attached documents as clean AI context instead of ad hoc file text. The desired flow is local-first:

1. User attaches or drops a readable file.
2. Bluey converts it to Markdown when possible.
3. The Markdown is saved under the local Bluey data directory.
4. The session stores a short preview for UI and sync metadata.
5. Local RAG indexes the saved Markdown so old/current sessions can recall it.
6. Cloud sync continues to upload session context and RAG chunks when the account/network path is available.

## Implementation

- Added `crates/cue-daemon/src/doc_conversion.rs`.
- `build_context_artifact` now receives `AppPaths` so it can persist local Markdown artifacts.
- Supported attach types are explicitly allowlisted: readable text, Markdown, code, PDF, DOC, DOCX, and RTF. Video/audio/apps/certificates remain rejected before conversion.
- Conversion order:
  - `BLUEY_DOC_CONVERTER_BIN`
  - `BLUEY_MARKITDOWN_BIN`
  - helper binaries next to the Bluey executable: `bluey-doc-converter`, `markitdown`, or their `bin/` variants
  - `bluey-doc-converter` / `markitdown` from `PATH`
  - native fallback for plain text/code and basic platform PDF/DOC extraction
- Local Markdown files are written to:
  - `<bluey data dir>/context-markdown/<artifact_id>.md`
- `ContextArtifact` now carries `markdown_path` for the local persisted Markdown.
- Local RAG now indexes from `markdown_path` when present, falling back to `text_preview` if the file is unavailable.

## Safeguards

- No shell interpolation is used for conversion; Bluey passes the attached path as a process argument.
- MarkItDown input is capped at 25 MB.
- MarkItDown output readback is capped at 2 MB.
- MarkItDown conversion is capped at 20 seconds.
- UI/session preview is capped at 16k characters.
- RAG indexing reads at most 128k Markdown characters per artifact.
- Unsupported files fail with a user-facing skip/error instead of hanging the attach flow.

## Verification

Run from the repository root:

```bash
cargo fmt --all --check
cargo test -p cue-core --all-targets
cargo test -p cue-daemon --all-targets
git diff --check
```

## Known Followups

- Bundle a small `bluey-doc-converter` helper in release artifacts if we do not want customers to install `markitdown` themselves.
- If cloud document recall needs more than the existing synced context preview/RAG chunks, add a managed context upload object rather than sending local file paths.
- Add a visual overlay smoke for Docs loading/loaded states after the next macOS UI pass.

# Session History Drawer Cleanup - 2026-06-21

## Why

The macOS expanded overlay history button opened a full-window drawer. That made the history surface feel like it took over Bluey instead of behaving like a small session picker. Local smoke machines also accumulated hundreds of saved recordings titled `Ad hoc meeting`, which made the history list noisy and hard to trust.

## What Changed

- The macOS session drawer is now a compact, bounded history panel below the fixed header instead of a full-window overlay.
- Drawer height is sized from the number of visible rows and capped to the available space above the composer.
- The drawer title/copy now reads as user-facing history: continue, rename, or delete recordings.
- New locally created sessions default to `New recording` instead of `Ad hoc meeting` / `Ad hoc audio meeting`.

## Local Data Locations

Bluey desktop conversation data is local-first on macOS:

- Saved recordings: `~/Library/Application Support/cue/meetings/*.json`
- Current active recording: `~/Library/Application Support/cue/active-meeting.json`
- Local RAG/vector index: `~/Library/Application Support/cue/rag_vectors.db`
- Captures/screenshots: `~/Library/Application Support/cue/captures/`
- Account/profile settings: `~/Library/Application Support/cue/account.json`

Cloud-synced session snapshots are separate and only appear after account sync; local history continues to live on the device.

## Cleanup Policy

For this pass, local `Ad hoc meeting` files should be moved to a timestamped backup folder rather than hard-deleted. This keeps the user's history UI clean while preserving a restore path if any old test recording matters later.

## Verification

Run:

```bash
cargo fmt --all --check
swift build -c release --package-path native/macos/cue-overlay
cargo test -p cue-core --lib
git diff --check
```

Then launch visible mode and confirm the history button opens a compact drawer that stays below the header and above the composer.

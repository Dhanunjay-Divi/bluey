# Round 167 - R2 Object Byte Sync

## Goal

Restore cloud sessions with the original attached document, image, and screenshot bytes when object storage is configured, while keeping answer-ready text preview hydration as the fallback.

## Changed

- Added a narrow S3-compatible object storage client for Cloudflare R2.
- Added protected artifact object upload and download endpoints.
- Added object storage env config:
  - `BLUEY_OBJECT_ENDPOINT_URL`
  - `BLUEY_OBJECT_BUCKET`
  - `BLUEY_OBJECT_ACCESS_KEY_ID`
  - `BLUEY_OBJECT_SECRET_ACCESS_KEY`
  - `BLUEY_OBJECT_REGION`
  - `BLUEY_OBJECT_KEY_PREFIX`
  - `BLUEY_OBJECT_RETENTION_DAYS`
  - `BLUEY_OBJECT_MAX_BYTES`
- Desktop cloud sync now uploads original local artifact files before syncing metadata.
- Sync metadata now records object key, size, hash, content type, and expiration.
- Cloud hydration now restores original object bytes when available, then falls back to the markdown preview.
- Preflight now checks object storage configuration when required.
- Privacy Policy and Terms now describe cloud-synced object bytes, extracted text, indexes, 12-month retention, and lazy cleanup.

## Behavior

- First attach still converts and indexes locally.
- Cloud sync uploads original bytes once per artifact when storage is configured.
- Future answers should use summaries and snippets, not resend full documents or screenshots unless newly attached.
- A different signed-in device can restore the saved session and download original bytes if the object exists and has not expired.
- Expired objects are treated as gone and deleted lazily on restore attempts.

## Verification

- `cargo check --manifest-path server/Cargo.toml`
- `cargo check -p cue-daemon`
- `cargo test -p cue-daemon cloud::sync::tests`
- `cargo test --manifest-path server/Cargo.toml object_storage`
- `bash -n scripts/bluey-cloud-preflight.sh`

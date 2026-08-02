# ParakeetAI 3.7.0 Windows data and network map

## Local data/security

- `electron-store` settings include auto-detection, private mode, update state,
  API configuration, and encrypted bypass token.
- First-party code uses Electron `safeStorage`; if encryption is unavailable,
  encryption returns `null` and decryption refuses, rather than writing plaintext
  (`dist/main/main.js` offsets 432500-433050).
- Auth cookies are persisted in the Electron session. Static code sets both
  secure and compatibility NextAuth cookie names.
- Logs and Chromium caches remain plaintext/user-profile managed.
- Native audio queues are in-memory and cleared on `stop`; no local raw-audio
  file path is established by the Rust module.

## Client-visible network boundaries

- Product/API web origins under `parakeet-ai.com` and Vercel staging branches.
- GitHub/Bitbucket APIs and repository-based updater.
- Mixpanel ingestion endpoints.
- Object storage/S3 library paths and cloud application APIs in renderer code.

The application contains a bypass-token concept protected by `safeStorage`.
Authorization semantics and production availability are unknown; no token value
is reproduced. Backend storage, transcription, billing, and retention are not
recoverable from the installer.

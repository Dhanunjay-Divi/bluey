# Storage inventory

This inventory combines static API calls with one approved isolated unauthenticated runtime profile. Cloud paths and storage-key semantics come from static resources; the local file families below were confirmed at runtime.

## Observed cloud collections and paths

- `users/{uid}/settings/preferences/copilot_preferences`
- `users/{uid}/interview_presets`
- `users/{uid}/custom_prompts`
- `users/{uid}/sessions/{sessionId}/chat_history` with 50-item pagination
- `users/{uid}/events` filtered for `duo_session_invite`
- `users/{uid}/files`
- User session records are queried for active-session detection.

The client initializes Firebase from bundled configuration names and uses Firebase Auth plus Firestore. Server rules, indexes, retention, tenant enforcement, and encryption beyond provider transport/storage defaults are not present in the client and remain unknown.

## Observed browser storage keys

- localStorage: `lastClerkUserId`, custom-prompt identifiers, Firebase/Clerk persistence data through their SDKs.
- sessionStorage: `pending_helper_id`, helper-access flags keyed by helper ID, and UI/session flags.

## Observed files and directories

- Main-process logs under the Electron application log path; IPC can return log paths and read logs.
- Electron updater cache named `lockedin_desktop_app-updater`.
- Crash reporter is configured with uploads disabled.
- The app reads `.env.local` from the packaged ASAR.

## Runtime-confirmed platform storage

The supplied user-data directory received Cookies, Cache/Code Cache/GPU caches, IndexedDB, Local Storage, Session Storage, WebStorage, Shared Dictionary, Trust Tokens, network state, preferences, Crashpad settings, and `.updaterId`. The supplied HOME received `Library/Logs/lockedin_desktop_app/main.log`. The exact 51-file inventory and modes are in `runtime-unauthenticated.md`.

No use of Electron `safeStorage`, macOS Keychain APIs, or an app-specific encrypted local database was found. The isolated Chromium database/cache files were mode `0600`; main log and updater ID were `0644` under deliberately `0700` disposable parents. Static/runtime evidence still does not establish authenticated persistence behavior or deletion of all Electron/Firebase caches on account deletion.

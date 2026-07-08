# ROUND-431 Account Scoped Session Migration

Date: 2026-07-08
Backup thread id: 019e133e-d92a-7830-8df0-3a050a4e22f6
Branch: codex/bluey-web-ui-parallel-20260704

## Goal

Make Bluey account switching safe and explicit:

- Old cloud chats stay with the old account.
- A new account does not see old account history.
- Moving a desktop link changes auth/billing identity, not chat ownership.
- Local cached sessions are tagged with the account that created or synced them.
- Logout/account switch/delete stops live work, clears visible context, and reloads only current-account sessions.
- Local session migration requires explicit user consent and is never automatic.

## Changes

- Added `cloud_account_id` to local account config, populated by browser login.
- Added `owner_account_id` to local meeting/session records.
- New runtime sessions are tagged with the currently signed-in cloud account when they are created.
- Overlay history, continue latest, open session, delete session, sync upload, and cloud hydration now respect the current account owner.
- Hidden local CLI session listing now uses the same owner filter: signed-in accounts see only their own tagged sessions, and signed-out/local mode sees only unowned local sessions.
- Logout now stops audio/screen capture, pauses listening, clears active session/context/history, stops balance polling, and marks the overlay signed out.
- Cloud sync uploads only sessions owned by the current account and hydrates downloaded cloud sessions with the current owner.
- Added explicit migration command:
  - `bluey sessions --move-local-to-current-account --confirm-move-local-sessions`
  - Moves only unowned local sessions with real content to the currently signed-in account.
  - Skips empty shells and already-owned sessions.
- Added cloud session deletion:
  - Overlay/local session delete removes local state and tombstones the cloud session when signed in.
  - Web Session History delete calls the same cloud tombstone endpoint.
  - Cloud session lists now include recent tombstones for desktop sync, separate from visible session rows.
  - Desktop cloud hydration purges matching local cached sessions for the current account when a web delete tombstone appears.
  - Purge cleanup removes Bluey-owned converted markdown/prepared image cache files, but never deletes the user's original attached documents.
  - Tombstoned sessions do not resurrect from later stale sync batches.

## Delete Semantics

- Deleting one session:
  - Overlay delete removes the local session and best-effort deletes the cloud copy for the signed-in account.
  - Web delete tombstones the cloud copy for the signed-in account and the desktop removes the matching local cached session on its next cloud hydration/sync.
  - Repeated sync should not bring it back.
- Deleting the whole account:
  - Server account deletion already requires explicit typed/checkbox consent for data loss and credit loss.
  - Cloud session/transcript/answer/context/RAG rows are account-scoped and cascade with account deletion.
  - Account-scoped object storage and diagnostic R2 log objects are deleted by the account-delete flow.
  - Local desktop tokens/context are cleared when the desktop receives logout/deleted-account state; local historical files are not silently migrated to another account.

## Tests

Passed:

- `cargo test -p cue-core --lib --quiet`
- `cargo test --package cue-daemon meeting_visibility_is_scoped_to_current_account --quiet`
- `cargo test --package cue-daemon empty_meeting_shell_does_not_sync --quiet`
- `cargo test --package cue-daemon conversation_sync_preserves_code_artifact_fields --quiet`
- `cargo check -p cue-cli -p cue-daemon --quiet`
- `cargo test --manifest-path server/Cargo.toml list_sessions_hides_empty_shells_until_content_arrives --quiet`
- `cargo test --manifest-path server/Cargo.toml tombstoned_session_does_not_resurrect_on_later_sync --quiet`
- `cargo test --package cue-daemon cloud_delete_follows_current_owner_or_legacy_unowned_cache --quiet`
- `cargo test --package cue-daemon cloud_delete_cleanup_removes_only_bluey_owned_context_cache --quiet`
- `cargo check -p cue-cli -p cue-cloud-client -p cue-daemon --quiet`
- `cargo check --manifest-path server/Cargo.toml --quiet`
- `node --check web/assets/bluey-site.js`
- `git diff --check`

## Deploy Status

Not deployed. Per owner instruction, this round did not use GitHub Actions and did not publish a release. It is ready for local testing or a later signed deploy when requested.

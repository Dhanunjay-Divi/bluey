# Account File Tokens And Permission Preflight - 2026-06-20

## Goal

Fix the first-run/customer path where macOS keychain prompts could block Bluey
from reading account tokens, causing managed Listen, Screen, and Answer calls to
surface 401s even after the user had signed in.

Also make `bluey on` more proactive about the permissions Bluey needs for the
normal product flow: microphone, system audio/screen recording, and
accessibility.

## Changes

- `cue-cloud-client` now uses Bluey's private local account profile as the
  default `SecureAccountStore` backend.
- OS keychain / credential-store access is opt-in only:
  - `BLUEY_USE_OS_KEYCHAIN=1`
  - `BLUEY_USE_SECURE_STORE=1`
- Legacy keyring migration remains explicit through
  `BLUEY_LEGACY_KEYRING_FALLBACK=1`.
- `bluey on` now attempts to trigger native macOS prompts through the bundled
  audio helper for:
  - Microphone
  - System Audio / Screen Recording
- `bluey on` still opens the matching System Settings panes and waits for the
  operator to approve missing permissions.
- User-facing copy was updated so docs, CLI output, and privacy text no longer
  claim Keychain/Credential Manager is the default token store.

## What This Means

Normal customer installs should not see macOS Keychain prompts just because they
click Listen, Screen, or Answer. Account tokens are Bluey tokens only; provider
API keys remain server-side and are not shipped to the desktop.

macOS permissions cannot be granted silently by Bluey. The best allowed flow is:

1. `bluey on` checks required permissions.
2. Bluey triggers native permission prompts where possible.
3. Bluey opens System Settings for the permissions macOS requires the user to
   approve manually.
4. Bluey waits and continues once permissions are granted.

## Local Session And Cloud Storage Clarification

Local session history is stored under Bluey's per-user data directory:

- active session: `<data_dir>/active-meeting.json`
- archived sessions: `<data_dir>/meetings/*.json`
- converted document markdown: `<data_dir>/context-markdown/*.md`

Cloud sync stores session metadata/RAG rows in the Bluey server database for
alpha. R2/S3-compatible storage is for release artifacts, backups, support zips,
and large synced blobs as that path is enabled; users do not install or manage
R2, Redis, Postgres, or vector databases locally.

## Verification

```bash
cargo fmt --all --check
cargo test -p cue-cloud-client tokens::tests
cargo test -p cue-cli --lib
git diff --check
./target/debug/bluey account
```

`./target/debug/bluey account` reported `Token: configured` from the account
profile without requiring an OS keychain prompt.

## Most Likely Gaps

- Accessibility cannot be granted silently; macOS still requires the user to
  approve it in System Settings.
- Screen analysis and system-audio capture may involve different helper
  binaries depending on the path. The preflight opens Settings and triggers the
  audio helper prompt, but live smoke must still verify Listen and Screen on a
  clean install.
- R2 is not yet the live source of truth for customer session history.

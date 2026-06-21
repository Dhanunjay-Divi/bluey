# Bluey v0.1.13

First-run auth and permission flow release for paid alpha testing.

## Included

- Bluey account tokens now default to Bluey's private local account profile
  instead of OS keychain / credential-manager storage.
- OS keychain access is opt-in through `BLUEY_USE_OS_KEYCHAIN=1` or
  `BLUEY_USE_SECURE_STORE=1`.
- Legacy keyring migration remains explicit through
  `BLUEY_LEGACY_KEYRING_FALLBACK=1`.
- `bluey on` now tries to trigger native macOS microphone and system-audio /
  screen-recording prompts before opening System Settings and waiting.
- CLI, privacy, and security copy now match the account-file token behavior.

## Operator Notes

- Provider API keys remain server-side. This release only changes desktop Bluey
  account-token storage.
- macOS permissions still require user approval; Bluey can prompt and guide, not
  silently grant.
- Clean-install smoke should verify Listen, Screen, Answer, balance, and session
  history after updating to this build.

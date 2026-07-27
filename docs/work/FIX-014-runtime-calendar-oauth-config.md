# FIX-014: Runtime Calendar OAuth Client Configuration

## Issue

Google and Microsoft public OAuth client IDs were described primarily as
build-time inputs, leaving an already-built release without a clear, tested
runtime configuration and recovery path.

## Root Cause

Provider configuration originally used compile-time `option_env!` values.
Runtime environment precedence had been added during the broader calendar
reliability work, but its release behavior was not isolated by tests and the UI
still exposed a generic backend configuration error.

## Fix Summary

- Resolve a non-empty daemon runtime client ID before the baked release value.
- Keep resolution pure and cover runtime precedence, empty overrides, and
  malformed explicit overrides with unit tests.
- Fail before browser launch with daemon-environment and restart guidance.
- Give unconfigured onboarding rows separate customer and operator recovery
  steps while stating that no client secret belongs on the desktop.
- Document release/runtime precedence, provider registration, redirects,
  scopes, restart requirements, and the external registration blocker.

## Files Modified

| File | Change |
|------|--------|
| `crates/cue-calendar-cloud/src/provider.rs` | Test runtime public-client-ID precedence and validation |
| `crates/cue-meeting-overlay/ui/src/screens/Onboarding.tsx` | Add actionable missing-configuration recovery text |
| `crates/cue-meeting-overlay/ui/src/lib/types.ts` | Describe runtime/build configuration accurately |
| `docs/deploy/CALENDAR-OAUTH.md` | Document secure runtime/release configuration and external registration |
| `docs/work/IMPL-CLOUD-CALENDAR.md` | Clarify that release binaries also accept runtime overrides |
| `docs/work/FIX-004-calendar-oauth-sync.md` | Update live-test instructions for runtime configuration |

## Edge Cases Handled

- A whitespace-only runtime value does not mask a valid baked value.
- A malformed explicit runtime value fails visibly rather than selecting a
  different baked OAuth application.
- Missing configuration does not open a broken provider consent page.
- UI guidance never asks for or displays an OAuth client secret.

## How to Test

```bash
cargo test -p cue-calendar-cloud provider::tests
(cd crates/cue-meeting-overlay/ui && npm run build)
git diff --check
```

## Known Limitations

- Google Cloud and Microsoft Entra applications must still be registered
  externally. This repository intentionally contains no real or invented client
  IDs, so live consent requires operator-supplied registrations and accounts.

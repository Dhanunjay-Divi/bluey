# FIX-004: Calendar OAuth Onboarding and Live Sync

## Issue

Google and Microsoft accounts could not reliably complete onboarding, and a
successful connection did not reliably begin live calendar sync.

## Root Cause

Several independent gaps combined into one broken user flow:

- Meeting/dev/release build paths omitted `cue-daemon/cloud-calendar`, so the UI
  called a daemon compiled to return `cloud calendar not built`.
- Placeholder public client IDs reached the browser instead of failing before
  consent. Browser-launch errors were logged and swallowed, and Windows passed
  an unquoted OAuth URL containing `&` through `cmd.exe`.
- The calendar source was selected only once at daemon startup. Connecting or
  disconnecting during onboarding did not update that retained source.
- Pollers read multiple keychain entries repeatedly; an earlier development
  fallback could leave refresh tokens in plaintext.
- Google delta pages replaced the full snapshot, pagination was ignored, and
  recurring occurrences shared the wrong identity. Microsoft used the wrong
  delta shape and did not safely follow Graph continuation links.
- Onboarding started with an empty local state instead of reading existing
- Connect and disconnect could race, one transient credential-vault failure
  permanently disabled a provider until restart, and token presence alone was
  reported as a healthy connection.
- Calendar account controls disappeared permanently after onboarding.
- Public webhook endpoints did not validate Google channel tokens or Microsoft
  `clientState`; Microsoft subscription validation was modeled as GET + JSON even
  though Graph sends a POST query token with an empty body.

## Fix Summary

- Enabled `cloud-calendar` in all meeting, development, macOS, Windows, Makefile,
  and GitHub release daemon builds. Release CI now requires both public client
  IDs and bakes them into the binary.
- Added provider configuration validation, browser-error propagation, OAuth
  denial handling, bounded network/listener timeouts, PKCE-only public-client
  exchange, sanitized token-endpoint errors, and shell-free default-browser
  launch.
- Replaced plaintext/multi-entry storage with one versioned OS-keychain bundle,
  one-time legacy migration, redacted token diagnostics, and a process-local
  read-through cache.
- Added a dynamic daemon source registry so Google and Microsoft can activate
  immediately after consent, run together, retry after transient initialization
  failure, and stop immediately on disconnect.
- Implemented paginated baseline + incremental merging, deletion handling,
  recurring-event identity, bounded continuation URLs, and periodic rolling
  baseline refreshes.
- Serialized each provider's connect/disconnect lifecycle, validated stored
  authorization before reporting healthy status, restored connection state on
  onboarding mount, and exposed reusable reconnect/disconnect controls in
  persistent account settings.
- Corrected and authenticated the webhook ingress while keeping OAuth tokens and
  calendar content on the device.

## Files Modified

| File | Change |
|------|--------|
| `crates/cue-calendar-cloud/src/{provider,oauth,loopback,tokens}.rs` | Harden native PKCE flow and secure token persistence |
| `crates/cue-calendar-cloud/src/{google,microsoft}.rs` | Correct paginated incremental event sync |
| `crates/cue-daemon/src/{app,calendar}.rs` | Propagate OAuth errors and activate sources dynamically |
| `crates/cue-meeting-overlay/ui/src/screens/{Onboarding,AgentsScreen}.tsx` | Persistent status, connect, error, and disconnect UX |
| `server/src/api/calendar.rs` | Authenticate doorbells and implement Graph POST validation |
| `scripts/*`, `Makefile`, `.github/workflows/release.yml` | Include calendar code in runnable artifacts |
| `ops/bluey-api.env.example` | Document webhook authentication values |

## Edge Cases Handled

- Missing/placeholder client IDs fail before a browser opens.
- User denial returns immediately rather than waiting for the full timeout.
- A missing refresh token requires a clean reconnect instead of failing later.
- Google/Microsoft pagination is bounded to prevent infinite continuation loops.
- Delta deletions remove cached events without dropping unchanged events.
- Expired sync/delta tokens trigger a clean baseline resync.
- Two providers can be connected simultaneously without event-ID collisions.
- A queued disconnect cannot be undone by a late OAuth completion.
- A transient credential-vault read is retried instead of poisoning the source
  registry for the daemon lifetime.
- Revoked or unrefreshable credentials display a reconnect-required error.
- Keychain errors are surfaced as errors, not misreported as disconnected.
- Empty/malformed webhook payloads and missing/mismatched secrets fail closed.

## How to Test

```bash
cargo test -p cue-calendar-cloud
# 55 passed

cargo check -p cue-daemon --features cloud-calendar
# success

cargo test -p cue-daemon --features cloud-calendar calendar::tests
# 4 passed

(cd crates/cue-meeting-overlay/ui && npm run build)
# TypeScript + Vite build succeeded

(cd server && cargo test api::calendar)
# 8 passed

bash -n scripts/build-meeting.sh scripts/reinstall-dev.sh \
  scripts/build-macos.sh scripts/run-local.sh
git diff --check
# success
```

For a live account test, register provider desktop/public clients, set
`BLUEY_GOOGLE_CLIENT_ID` and `BLUEY_MICROSOFT_CLIENT_ID` in the daemon runtime
environment, restart Bluey, then connect and disconnect each provider from
onboarding. Rebuilding is optional because a runtime value overrides the client
ID baked into a release binary. See `docs/deploy/CALENDAR-OAUTH.md`.

An isolated live daemon smoke on `127.0.0.1:57499` also verified that both
providers return their specific missing-client-ID error immediately, without
opening a browser or waiting for the authorization timeout.

## Known Limitations

- This checkout has no real Google/Microsoft public client IDs, so interactive
  provider consent cannot be truthfully completed in an automated run.
- Webhook subscription creation, renewal, and a device-nudge relay remain Phase
  2. The native app's authenticated 45-second incremental poll is the working
  Phase-1 sync path; webhook ingress does not participate in OAuth code exchange.

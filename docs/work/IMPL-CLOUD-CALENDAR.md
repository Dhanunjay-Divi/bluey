# IMPL: CLOUD-CALENDAR — Cloud OAuth Calendar (Google + Microsoft)

> Historical implementation record: this document describes the original A–E
> cloud-calendar landing. The later reliability work supersedes its original
> single-provider/EventKit selection, token-store, polling, and validation
> details. See `IMPL-CALENDAR-OAUTH-RELIABILITY.md` for the current behavior.

A native public-client OAuth calendar integration for Bluey: "Connect Google" /
"Connect Microsoft" in onboarding, so an upcoming meeting on a cloud calendar
fires the existing warm meeting-backend trigger. Built across five work items
(A–E) on branch `agent/cloud-calendar-oauth`; this doc is the integration record
and covers the whole feature. E (this batch) is the final integration: reconcile
D's connect seam and wire the cloud source into `default_source()`.

## Scope

**Does:**

- Add a native public-client OAuth flow (RFC 8252): PKCE (S256) + a transient
  loopback redirect, `client_id` only (NO client secret), tokens stored on-device
  in the OS keychain per provider.
- Add a new crate `crates/cue-calendar-cloud` holding the OAuth core, per-provider
  token store, and a `CalendarSource` impl per provider (Google, Microsoft) that
  keeps a background-refreshed event snapshot.
- Move the shared calendar seam types (`CalendarSource` / `UpcomingEvent` /
  `Participant`) into `cue-core::calendar` so the new crate can implement the trait
  without a `cue-daemon` dependency cycle.
- Keep the integration behind the cross-platform `cloud-calendar` cargo feature
  on `cue-daemon`, and enable that feature in every supported development,
  meeting, and release build path.
- Add IPC (`CalendarConnectStart` / `CalendarConnectStatus` / `CalendarDisconnect`
  → `CalendarStatus`), the daemon handlers, the Tauri commands, and the onboarding
  UI connect buttons + connected-account label.
- Aggregate connected Google and Microsoft sources concurrently through a
  dynamic registry, and activate or deactivate a provider immediately after
  onboarding without requiring a daemon restart.

**Does NOT:**

- Invent or commit real OAuth client IDs. Source builds retain public placeholder
  fallbacks; runtime environment values override build-time values, and release
  CI requires registered public IDs (see **Developer prerequisite**).
- Write events anywhere or request write scopes — read-only calendar access only
  (`calendar.readonly` / `Calendars.Read`).
- Move OAuth codes, bearer tokens, event bodies, or attendee data through the
  Bluey server. Cloud sync remains on-device; the public webhook endpoints are
  authenticated doorbells only.
- Add heavy dependencies. PKCE is hand-rolled from crates already present
  (`sha2`, `base64`, `url`, `getrandom`, `keyring`); HTTP reuses `reqwest`. No
  `oauth2`, no `chrono`/`time` (RFC3339/ISO-8601 is hand-rolled).

## Architecture

**Native public-client PKCE + loopback (RFC 8252).** On connect, the daemon:

1. Generates a PKCE `code_verifier` + `code_challenge = base64url_nopad(sha256(verifier))`
   and a random `state`.
2. Binds a transient single-shot loopback listener. Google desktop clients use
   `127.0.0.1`; Microsoft public clients use the registered `localhost` host.
3. Builds the provider authorize URL (`client_id`, `redirect_uri`,
   `response_type=code`, `scope`, `code_challenge`, `code_challenge_method=S256`,
   `state`, plus Google's `access_type=offline` + `prompt=consent` to guarantee a
   refresh token) and opens it through a shell-free, cross-platform browser
   launcher. The crate stays browser-agnostic through an injected
   `open_browser` closure.
4. The provider redirects back to the loopback listener with `code` + `state`; the
   listener verifies `state`, captures `code`, and serves a "you can close this
   tab" page, then tears down (~2min consent window, bounded by a 120s timeout on
   the daemon side).
5. Exchanges the code at the token endpoint (`grant_type=authorization_code`,
   `code_verifier`, NO client secret) for access + refresh tokens.

**On-device keychain tokens.** Each provider stores one versioned, atomic token
bundle in the OS keychain under its own service
(`bluey_calendar_google` / `bluey_calendar_microsoft`). A process-local cached
store avoids repeated multi-read keychain access, and a one-time migration
removes legacy entries without reintroducing plaintext fallback storage. The
bundle contains access, refresh, absolute expiry, and the connected-account
email. `valid_access_token` refreshes ahead of expiry and atomically persists a
rotated bundle.

**Background-refreshed snapshot source.** The `CalendarSource::upcoming()` seam is
SYNC and must never block the daemon poll, but HTTP + token refresh are async. Each
provider source (`GoogleCalendarSource` / `MicrosoftCalendarSource`) spawns its own
tokio task on construction that fetches events every ~45s and stores the latest
`Vec<UpcomingEvent>` behind a `Mutex`. `upcoming()` just clones the last good
snapshot — sync, non-blocking, no `block_on`, and naturally rate-limited. On a
refresh error the previous snapshot is retained (fail-soft), so a transient network
blip never empties the calendar mid-meeting.

## Crate layout: `crates/cue-calendar-cloud`

| Module | Responsibility |
|--------|----------------|
| `pkce.rs` | RFC 7636 PKCE (S256) verifier/challenge + random state (pure). |
| `provider.rs` | `Provider {Google, Microsoft}` + `ProviderConfig` (endpoints, scopes, public client id, extra authorize params) + `keyring_service()` (pure data). |
| `authorize.rs` | Build the authorize URL (pure). |
| `loopback.rs` | The transient single-shot redirect listener (async I/O). |
| `tokens.rs` | `CalTokens`, the `CalTokenStore` trait, `KeyringCalStore` (per-provider), `MemoryCalStore` (tests), `is_expired`. |
| `oauth.rs` | Flow driver: `exchange_code` / `refresh` / `connect_interactive` / `valid_access_token`. |
| `google.rs` | Google client (`fetch_events`, `fetch_email`) + `GoogleCalendarSource` (`spawn`, `connect`, `impl CalendarSource`). |
| `microsoft.rs` | Microsoft Graph client (`fetch_events`, `fetch_email`) + `MicrosoftCalendarSource` (same shape). |

The crate depends on `cue-core` (for the shared calendar types) and NEVER on
`cue-daemon`, so it is pulled in behind the feature flag without a dependency
cycle.

- **Google:** authorize `accounts.google.com/o/oauth2/v2/auth`, token
  `oauth2.googleapis.com/token`, scopes
  `.../auth/calendar.readonly openid email`, events via
  `calendar/v3/calendars/primary/events` with baseline and incremental
  pagination. Email comes from `oauth2/v2/userinfo`.
- **Microsoft:** authorize/token under `login.microsoftonline.com/common/oauth2/v2.0`
  (tenant `common` = personal + work/school), scope
  `Calendars.Read User.Read offline_access openid profile`, events via
  `graph.microsoft.com/v1.0/me/calendarView` delta queries with UTC timestamps
  and immutable event IDs. Email comes from Graph `/me`
  (`mail` → `userPrincipalName`).

## Feature flag: `cloud-calendar`

`cue-daemon/Cargo.toml`:

```toml
cue-calendar-cloud = { path = "../cue-calendar-cloud", optional = true }
# ...
[features]
cloud-calendar = ["dep:cue-calendar-cloud"]
```

The feature remains optional at the crate level so a minimal daemon can compile
without cloud dependencies. Bluey's supported development, meeting, macOS,
Windows, Makefile, and release workflow paths explicitly enable it so the
onboarding UI never targets a daemon that cannot service calendar requests.

## The seam into `default_source()`

`crates/cue-daemon/src/calendar.rs`. Priority order (the env-fake test hook still
wins first so deterministic tests never race a real calendar):

1. `BLUEY_CALENDAR_FAKE_EVENTS` → `EnvFakeSource`.
2. **(feature `cloud-calendar`)** `DynamicCloudSource`, which merges the current
   snapshots from every connected Google and Microsoft source.
3. `NoopSource`.

`DynamicCloudSource` obtains the Tokio runtime handle without changing the
`default_source()` signature. A provider connected during onboarding replaces
or starts its source immediately; disconnect stops and joins that provider
before clearing credentials. Transient configuration or keychain failures leave
the provider eligible for a later initialization retry. Provider IDs are
namespaced before the two snapshots are merged, preventing cross-provider
dedupe collisions. The meeting-prep scheduler re-reads refreshed snapshots at
least every 30 seconds.

## IPC surface (D)

`crates/cue-core/src/ipc.rs`:

- `DaemonRequest::CalendarConnectStart { provider }` — run the interactive connect
  flow for `"google"` / `"microsoft"`.
- `DaemonRequest::CalendarConnectStatus` — one row per provider.
- `DaemonRequest::CalendarDisconnect { provider }` — clear that provider's tokens.
- `DaemonResponse::CalendarStatus { connections: Vec<CalendarConnection> }` where
  `CalendarConnection { provider, configured, connected, email, error }` is a
  serde wire DTO in `cue-core::calendar`; tokens never appear on the wire.

Daemon handlers `calendar_connect_start` / `calendar_connect_status` /
`calendar_disconnect` live in `crates/cue-daemon/src/app.rs`, feature-gated: a real
body under `#[cfg(feature = "cloud-calendar")]` and a `not(feature)` fallback that
keeps the default build compiling and honestly reports the cloud calendar as not
built / every provider disconnected.

## UI (D)

Tauri commands `calendar_connect` / `calendar_status` / `calendar_disconnect` in
`crates/cue-meeting-overlay/src/commands.rs` bridge the overlay to the daemon IPC.
The onboarding screen (`crates/cue-meeting-overlay/ui/src/screens/Onboarding.tsx`,
with `client.ts` / `tauriClient.ts` / `types.ts`) renders per-provider
configuration, connection health, actionable errors, and connected-account
labels. The same reconnect/disconnect controls remain available later in
persistent account settings.

## Current connected-source activation seam

The source-level `GoogleCalendarSource::connect` and
`MicrosoftCalendarSource::connect` constructors drive the same bounded PKCE
flow, enrich `CalTokens.email`, and persist the provider bundle. The daemon then
passes the returned tokens to `activate_cloud_provider`, replacing any prior
source and beginning sync immediately. Status validates that stored
authorization can still yield an access token instead of reporting health from
token presence alone. Connect, status, and disconnect are serialized per
provider so a late OAuth completion cannot undo a queued disconnect.

## Final validation

Final branch-tip verification completed on 2026-07-26:

- [x] `cargo fmt --all -- --check` — passed for the full workspace.
- [x] `cargo test -p cue-calendar-cloud` — 68 passed, 0 failed.
- [x] `cargo test -p cue-core` — 163 passed, 0 failed, including the calendar
      and IPC serialization coverage.
- [x] `cargo test -p cue-daemon --features parakeet-stt,local-memory,cloud-calendar` —
      423 passed, 0 failed, 18 intentionally ignored; all 8 calendar tests
      passed within this suite.
- [x] `cargo clippy -p cue-calendar-cloud --all-targets -- -D warnings` —
      passed.
- [x] `cargo clippy -p cue-daemon --all-targets --features parakeet-stt,local-memory,cloud-calendar -- -D warnings` —
      passed.
- [x] `(cd server && cargo test calendar)` — 8 passed, 0 failed, 130 filtered;
      server all-target Clippy passed with warnings denied.
- [x] Calendar onboarding/settings UI formatting and production build — all 18
      changed UI files passed Prettier, and Vite built 78 modules successfully
      with three pre-existing non-fatal mixed-import advisories.
- [x] `git diff --check` — passed for staged and unstaged changes.

Live Google and Microsoft consent, refresh rotation, and event retrieval remain
externally gated by registered public client IDs and interactive provider test
accounts. The missing-client-ID path fails fast with actionable errors, and the
external live-consent gate is not a code blocker.

## Developer prerequisite (REQUIRED to authenticate live)

Client IDs are PUBLIC identifiers (there is NO client secret in this native
public-client flow), but they are app-specific. Source builds retain placeholder
fallbacks; release CI requires registered IDs, while local/dev binaries can
receive them from the daemon's runtime environment or at build time.

- **Google:** register an OAuth 2.0 **Desktop app** client in Google Cloud Console
  (APIs & Services → Credentials), enable the Google Calendar API, and add the
  `calendar.readonly`, `openid`, and `email` scopes on the consent screen. Copy
  the client id.
- **Microsoft:** register an app in the Azure Portal (Entra ID → App registrations)
  as a **public client** (mobile & desktop, "allow public client flows" / no
  secret), supported account type `common`, redirect type "Mobile and desktop
  applications" (loopback `http://localhost`), and grant delegated Microsoft Graph
  `Calendars.Read` + `User.Read` + `offline_access` + `openid` + `profile`. Copy
  the application (client) id.

Supply them through the daemon's runtime environment or inject them at build
time (compile-time `option_env!` fallbacks in
`crates/cue-calendar-cloud/src/provider.rs`):

```bash
BLUEY_GOOGLE_CLIENT_ID=<google-desktop-client-id> \
BLUEY_MICROSOFT_CLIENT_ID=<azure-public-client-id> \
  cargo build -p cue-daemon --features cloud-calendar --target aarch64-apple-darwin
```

At runtime, a non-empty environment value takes precedence over the value baked
into any build, including release binaries. Restart the daemon after changing
its environment. Without either source, `provider.rs` falls back to
`PLACEHOLDER_GOOGLE_CLIENT_ID` / `PLACEHOLDER_MICROSOFT_CLIENT_ID` and the live
consent step fails immediately with a configuration error before opening a
broken consent page. See `docs/deploy/CALENDAR-OAUTH.md`.

## Tested vs. needs-a-live-account

**Tested (hermetic, no network / no account):**

- PKCE S256 derivation vs. RFC 7636; random-state uniqueness/url-safety.
- Provider config (endpoints, scopes, offline/consent params, distinct keyring
  services).
- Authorize-URL building (percent-encoding, required params, S256).
- Loopback listener: provider-specific host, bounded request reads, OAuth denial
  handling, capture code + verify state, reject state mismatch, and ignore
  favicon before capture.
- Token endpoint response deserialization; `to_cal_tokens` refresh carry-forward;
  `valid_access_token` cached/expired/no-token branches; `is_expired` skew +
  overflow saturation.
- JSON → `UpcomingEvent` mapping for both providers, including stable provider
  occurrence IDs, conferencing IDs, organizer and attendee email/RSVP data,
  all-day skip, per-item fail-soft, and empty bodies.
- Paginated baseline/incremental merge, deletions, expired-token resync, Graph
  continuation validation, and periodic rolling-window baselines.
- Atomic cached token-store behavior and legacy migration; IPC round-trips; the
  daemon calendar dedupe/window core, with the env-fake hook winning first.

**Needs a live account (not automatable here — requires real client IDs +
per-provider app registration + interactive consent):**

- The end-to-end interactive connect (browser consent → loopback code capture →
  token exchange) against real Google / Microsoft endpoints.
- Real refresh-token rotation over time.
- Real `fetch_events` / `fetch_email` responses (the mapping is unit-tested against
  captured sample bodies, but not against a live endpoint).
- `default_source()` selecting a live cloud source and the background snapshot task
  populating real events that fire the warm trigger.

## Implementation status

The on-device OAuth, token, incremental-sync, dynamic-source, and meeting-prep
paths are implemented. Final-tip validation remains pending in the checklist
above. Real consent still requires registered Google Desktop and Microsoft
public-client applications and interactive test accounts.

## Deviations from Plan

| Deviation | Rationale |
|-----------|-----------|
| `default_source()` signature unchanged (no threaded `Handle` param) | Its daemon call site runs inside Tokio, so the dynamic cloud source can obtain the current handle. Off-runtime callers safely fall through to `NoopSource`. |
| Polling remains authoritative | The server webhook routes authenticate and acknowledge doorbells, but there is no subscription lifecycle or authenticated device-nudge relay. |

## Known Follow-ups

- Register both provider applications and configure real public client IDs before
  a live consent test (see **Developer prerequisite**).
- macOS packaging: the loopback consent flow needs outbound network + the ability
  to open the system browser from the packaged daemon; verify in the signed `.app`.
- Live-account integration/smoke test once real client IDs exist (out of scope for
  a hermetic CI run).

## Review Checklist (for reviewer)

- [ ] Files match the scope described above
- [ ] No unrelated changes included
- [ ] Tests cover acceptance criteria from plan
- [ ] Code style matches AGENTS.md rules
- [ ] No TODOs without linked task IDs

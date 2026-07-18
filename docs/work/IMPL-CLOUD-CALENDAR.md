# IMPL: CLOUD-CALENDAR — Cloud OAuth Calendar (Google + Microsoft)

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
- Gate everything behind a `cloud-calendar` cargo feature on `cue-daemon`
  (cross-platform, off by default — unlike the macOS-only `calendar`/EventKit
  feature).
- Add IPC (`CalendarConnectStart` / `CalendarConnectStatus` / `CalendarDisconnect`
  → `CalendarStatus`), the daemon handlers, the Tauri commands, and the onboarding
  UI connect buttons + connected-account label.
- (E) Select a connected cloud calendar in `default_source()` ahead of EventKit,
  and drive the connect handler through the source-level `connect` constructors so
  the connected-account email is enriched and persisted.

**Does NOT:**

- Ship real OAuth client IDs. The build uses public placeholder client ids; a
  developer must register the app and inject real ids (see **Developer
  prerequisite**). Without them, the live connect flow cannot authenticate.
- Write events anywhere or request write scopes — read-only calendar access only
  (`calendar.readonly` / `Calendars.Read`).
- Touch `web/` or the EventKit path. The EventKit source, the env-fake test hook,
  and the warmup trigger core (dedupe, poll loop, warm-drive) are unchanged.
- Add heavy dependencies. PKCE is hand-rolled from crates already present
  (`sha2`, `base64`, `url`, `getrandom`, `keyring`); HTTP reuses `reqwest`. No
  `oauth2`, no `chrono`/`time` (RFC3339/ISO-8601 is hand-rolled).

## Architecture

**Native public-client PKCE + loopback (RFC 8252).** On connect, the daemon:

1. Generates a PKCE `code_verifier` + `code_challenge = base64url_nopad(sha256(verifier))`
   and a random `state`.
2. Binds a transient single-shot `TcpListener` on `127.0.0.1:0`, reads the bound
   port, and forms `redirect_uri = http://127.0.0.1:PORT`.
3. Builds the provider authorize URL (`client_id`, `redirect_uri`,
   `response_type=code`, `scope`, `code_challenge`, `code_challenge_method=S256`,
   `state`, plus Google's `access_type=offline` + `prompt=consent` to guarantee a
   refresh token) and opens it in the system browser (the daemon shells out to
   `open`/`xdg-open`/`start`; the crate itself stays browser-agnostic via an
   injected `open_browser` closure).
4. The provider redirects back to the loopback listener with `code` + `state`; the
   listener verifies `state`, captures `code`, and serves a "you can close this
   tab" page, then tears down (~2min consent window, bounded by a 120s timeout on
   the daemon side).
5. Exchanges the code at the token endpoint (`grant_type=authorization_code`,
   `code_verifier`, NO client secret) for access + refresh tokens.

**On-device keychain tokens.** Tokens live only in the OS keychain, namespaced per
provider (`bluey_calendar_google` / `bluey_calendar_microsoft`) so a Google and a
Microsoft connection never collide. Stored fields: access, refresh, absolute
expiry (epoch), and the connected-account email (a non-secret UI label). Refresh
is proactive: `valid_access_token` refreshes ~60s ahead of expiry
(`grant_type=refresh_token`) and persists the rotated token.

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
  `oauth2.googleapis.com/token`, scope `.../auth/calendar.readonly`, events via
  `calendar/v3/calendars/primary/events` (`singleEvents=true`,
  `orderBy=startTime`). Email via `oauth2/v2/userinfo`.
- **Microsoft:** authorize/token under `login.microsoftonline.com/common/oauth2/v2.0`
  (tenant `common` = personal + work/school), scope
  `Calendars.Read offline_access openid profile`, events via
  `graph.microsoft.com/v1.0/me/calendarView` with `Prefer: outlook.timezone="UTC"`.
  Email via Graph `/me` (`mail` → `userPrincipalName`).

## Feature flag: `cloud-calendar`

`cue-daemon/Cargo.toml`:

```toml
cue-calendar-cloud = { path = "../cue-calendar-cloud", optional = true }
# ...
[features]
cloud-calendar = ["dep:cue-calendar-cloud"]
```

Off by default — the default build compiles with the cloud code fully `cfg`'d out
(no dead-code/unused warnings). Enable with `--features cloud-calendar`. Cross-
platform, unlike the macOS-only `calendar` feature.

## The seam into `default_source()`

`crates/cue-daemon/src/calendar.rs`. Priority order (env-fake test hook still wins
FIRST so deterministic tests never race a real calendar):

1. `BLUEY_CALENDAR_FAKE_EVENTS` → `EnvFakeSource`.
2. **(new, feature `cloud-calendar`)** a connected cloud calendar via
   `cloud_source()` — Google then Microsoft — when that provider has tokens in its
   keychain (`KeyringCalStore::new(provider.keyring_service()).load()` returns
   `Some`).
3. EventKit (`calendar` feature, macOS).
4. `NoopSource`.

`cloud_source()` obtains the tokio runtime handle via
`tokio::runtime::Handle::try_current()` (the sole call site is inside the daemon's
calendar-poll `tokio::spawn` task — an async context — so `try_current()` resolves
without any signature change; if ever called off the runtime it returns `None` and
falls through rather than panicking). It then `spawn`s the connected provider's
source with an `Arc<dyn CalTokenStore>` over the keychain. Keychain-read errors
read as "not connected" (fail-soft), never a panic. **No signature change to
`default_source()` was needed** — the non-cloud paths (env-fake / EventKit / noop)
do not touch the handle.

## IPC surface (D)

`crates/cue-core/src/ipc.rs`:

- `DaemonRequest::CalendarConnectStart { provider }` — run the interactive connect
  flow for `"google"` / `"microsoft"`.
- `DaemonRequest::CalendarConnectStatus` — one row per provider.
- `DaemonRequest::CalendarDisconnect { provider }` — clear that provider's tokens.
- `DaemonResponse::CalendarStatus { connections: Vec<CalendarConnection> }` where
  `CalendarConnection { provider, connected, email }` (a serde wire DTO in
  `cue-core::calendar`; tokens never appear on the wire).

Daemon handlers `calendar_connect_start` / `calendar_connect_status` /
`calendar_disconnect` live in `crates/cue-daemon/src/app.rs`, feature-gated: a real
body under `#[cfg(feature = "cloud-calendar")]` and a `not(feature)` fallback that
keeps the default build compiling and honestly reports the cloud calendar as not
built / every provider disconnected.

## UI (D)

Tauri commands `calendar_connect` / `calendar_status` / `calendar_disconnect` in
`crates/cue-meeting-overlay/src/commands.rs` bridge the overlay to the daemon IPC.
The onboarding screen (`crates/cue-meeting-overlay/ui/src/screens/Onboarding.tsx`,
with `client.ts` / `tauriClient.ts` / `types.ts`) renders per-provider connect
buttons and the connected-account label from `CalendarStatus`.

## E1: reconciled connect seam

D coded `calendar_connect_start` against A's lower-level `connect_interactive`
(B/C had not landed yet) and left a `// SEAM (E): confirm connect signature`
marker. E switched the handler to the source-level constructors
`GoogleCalendarSource::connect(open_browser)` /
`MicrosoftCalendarSource::connect(open_browser)` (Option 1). These wrap the same
PKCE + loopback flow but ADDITIONALLY (a) enrich `CalTokens.email` via a
userinfo/Graph `/me` call and (b) persist the tokens to the per-provider keychain
themselves — so the handler no longer needs D's separate `KeyringCalStore.save`,
and the follow-up `CalendarConnectStatus` reports a populated email for the
connected-account label. The 120s connect timeout is preserved. Both the connect
constructors and `CalendarConnectStatus` read/write the SAME keyring service
(`Provider::_.keyring_service()`), so status correctly reflects a fresh connect.

## Build & Test

All commands run with `--target aarch64-apple-darwin` (this is an arm64 Mac; the
native toolchain avoids the emulated-x86_64 ort/Parakeet trap).

```bash
cargo build -p cue-core                                    # ✅ success
cargo build -p cue-calendar-cloud                          # ✅ success
cargo build -p cue-daemon                                  # ✅ success (default; cloud fully cfg'd out, no warnings)
cargo build -p cue-daemon --features cloud-calendar        # ✅ success (the real integration)
cargo test  -p cue-calendar-cloud                          # ✅ 42 passed (A/B/C: PKCE, URL, loopback, JSON→UpcomingEvent, token round-trip)
cargo test  -p cue-core --lib ipc                          # ✅ 27 passed (incl. D's calendar_request/status round-trips)
cargo test  -p cue-daemon --lib calendar                   # ✅ 3 passed (env_fake_source_parses_and_windows, fires_once_per_occurrence…, moved_event_rearms…)
cargo test  -p cue-daemon                                  # ✅ default suite green
cargo clippy -p cue-core -p cue-calendar-cloud -p cue-daemon -- -D warnings          # ✅ clean (default features)
cargo clippy -p cue-daemon --features cloud-calendar -- -D warnings                  # ✅ clean
cargo fmt   -p cue-core -p cue-calendar-cloud -p cue-daemon                          # ✅ applied, --check clean
```

## Developer prerequisite (REQUIRED to authenticate live)

Client IDs are PUBLIC identifiers (there is NO client secret in this native
public-client flow), but they are app-specific: the build ships **placeholder**
ids and a developer must register Bluey with each provider and inject the real
public client ids.

- **Google:** register an OAuth 2.0 **Desktop app** client in Google Cloud Console
  (APIs & Services → Credentials), enable the Google Calendar API, and add the
  `calendar.readonly` scope on the consent screen. Copy the client id.
- **Microsoft:** register an app in the Azure Portal (Entra ID → App registrations)
  as a **public client** (mobile & desktop, "allow public client flows" / no
  secret), supported account type `common`, redirect type "Mobile and desktop
  applications" (loopback `http://127.0.0.1`), and grant delegated Microsoft Graph
  `Calendars.Read` + `offline_access` + `openid` + `profile`. Copy the application
  (client) id.

Inject them at build time via env (compile-time `option_env!` fallbacks in
`crates/cue-calendar-cloud/src/provider.rs`):

```bash
BLUEY_GOOGLE_CLIENT_ID=<google-desktop-client-id> \
BLUEY_MICROSOFT_CLIENT_ID=<azure-public-client-id> \
  cargo build -p cue-daemon --features cloud-calendar --target aarch64-apple-darwin
```

Without these, `provider.rs` falls back to `PLACEHOLDER_GOOGLE_CLIENT_ID` /
`PLACEHOLDER_MICROSOFT_CLIENT_ID` and the live consent step will not authenticate.

## Tested vs. needs-a-live-account

**Tested (hermetic, no network / no account):**

- PKCE S256 derivation vs. RFC 7636; random-state uniqueness/url-safety.
- Provider config (endpoints, scopes, offline/consent params, distinct keyring
  services).
- Authorize-URL building (percent-encoding, required params, S256).
- Loopback listener: bind non-zero port, capture code + verify state, reject state
  mismatch, ignore favicon then capture.
- Token endpoint response deserialization; `to_cal_tokens` refresh carry-forward;
  `valid_access_token` cached/expired/no-token branches; `is_expired` skew +
  overflow saturation.
- JSON → `UpcomingEvent` mapping for both providers (organizer folding, all-day
  skip, per-item fail-soft, empty bodies).
- Token store round-trip (`MemoryCalStore`); IPC round-trips (D); the daemon
  calendar dedupe/window core, with the env-fake hook winning first.

**Needs a live account (not automatable here — requires real client IDs +
per-provider app registration + interactive consent):**

- The end-to-end interactive connect (browser consent → loopback code capture →
  token exchange) against real Google / Microsoft endpoints.
- Real refresh-token rotation over time.
- Real `fetch_events` / `fetch_email` responses (the mapping is unit-tested against
  captured sample bodies, but not against a live endpoint).
- `default_source()` selecting a live cloud source and the background snapshot task
  populating real events that fire the warm trigger.

## End-to-end state

Builds green on the DEFAULT feature set and on `--features cloud-calendar`; the
default build has the cloud code fully `cfg`'d out (no dead-code/unused warnings).
All hermetic tests pass; clippy is clean on both feature sets; `cargo fmt` is
applied. The feature **needs real public client IDs + per-provider app
registration** (Google Cloud Console Desktop client, Azure public-client app) to
authenticate a live account.

## Deviations from Plan

| Deviation | Rationale |
|-----------|-----------|
| `default_source()` signature unchanged (no threaded `Handle` param) | The only call site is inside the daemon's calendar-poll `tokio::spawn` task (async context), so `Handle::try_current()` resolves the runtime handle in the cloud branch alone; the non-cloud paths never need a handle. Off-runtime callers fall through to EventKit/noop instead of panicking. |
| Connect handler uses source-level `connect` (not `connect_interactive`) | B/C's `connect` also enriches `CalTokens.email` and persists to the keyring, giving a populated connected-account label and removing the handler's separate `save`. Same PKCE flow + same keyring service underneath. |

## Known Follow-ups

- Inject real client IDs and register the app with both providers before shipping
  (see **Developer prerequisite**); until then the connect flow cannot authenticate.
- macOS packaging: the loopback consent flow needs outbound network + the ability
  to open the system browser from the packaged daemon; verify in the signed `.app`.
- Live-account integration/smoke test once real client IDs exist (out of scope for
  a hermetic CI run).
- A pre-existing `cue-cli` `cargo fmt` drift was flagged by A; it is outside the
  A–E touched files and was intentionally NOT modified here.

## Review Checklist (for reviewer)

- [ ] Files match the scope described above
- [ ] No unrelated changes included
- [ ] Tests cover acceptance criteria from plan
- [ ] Code style matches CLAUDE.md rules
- [ ] No TODOs without linked task IDs

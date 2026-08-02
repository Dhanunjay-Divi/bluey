# Cluely data and network map

## Local storage

| Store | Observed content | Protection/uncertainty | Evidence |
|---|---|---|---|
| `appData/cluely-v2-april22/shared-state.json` | Onboarding and permission flags, display/invisible/screenshot settings, smart mode, notifications, shortcuts, theme, entitlement labels | Plain JSON; schema-sanitized; temporary-file-plus-rename atomicity; no application encryption | `main.js:16451-16589,16939` |
| sibling `appData/cluely` | Legacy v1 flags/keybindings/theme read for migration | Read-only migration path in observed code | `main.js:16370-16449` |
| sibling `appData/cluely-v2/main.log` | Electron application logs | Plain local log; content/rotation only partly visible | `main.js:16940` |
| Chromium userData | Clerk/session cookies, web storage, caches, network state | Electron/Chromium-managed. Application code has no `safeStorage`/Keychain call; OS cookie encryption cannot be established statically | userData path plus Clerk imports |
| localStorage | mode sidebar order, collapsed dashboard brief, theme and library state | Plain renderer origin storage; all localStorage is cleared by sign-out/reset route | `settings-BuvgGbrV.js` ~178121; `chat-BH3ET_qx.js` ~694827; `signed-out-CDvYbWh_.js:1` |
| ASAR/resources | UI, app code, AudioTee/SoX, VAD model/runtime | Signed bundle; ASAR header and every extracted regular-file hash verified | [identity-and-integrity.txt](evidence/identity-and-integrity.txt) |

No local SQL database, durable worker queue, application-browser profile store, secret vault, or encrypted app-specific record store was found. Session transcripts, modes, files, calendar data, people briefs, and billing state are accessed through cloud RPC rather than a visible local database.

## Network architecture

The default RPC client targets `https://api.v2.cluely.com/rpc`, adds a fresh Clerk Bearer token, and includes credentials (`orpc-CSebChCN.js` around byte 49300). Session chat uses `wss://api.v2.cluely.com/agents/chat-agent/<chatAgentName>?_pk=<random-uuid>` (`chat-BH3ET_qx.js` around bytes 260700-271200). The explicit query key appears to be connection plumbing; authorization cannot be inferred from it, and WebSocket cookie/server enforcement is unknown.

| Host/service | Observed purpose | Provenance |
|---|---|---|
| `api.v2.cluely.com` | Authenticated ORPC, agents WebSocket, sessions/transcription/modes/calendar/billing client calls | ORPC and chat chunks |
| `renderer.v2.cluely.com` | Canonical renderer origin served locally from ASAR via Electron protocol handler | `main.js:15814-15844` |
| `v2.cluely.com` | Browser sign-in/product web origin | sign-in/env chunks |
| `ph.cluely.com` | PostHog ingest/API host | `posthog-VTzWjthB.js` |
| `desktop-glass-releases.v2.cluely.com` | Production update feed override | `main.js` updater configuration |
| `20873b71e7c62fabf611d1ee3eb26fd0.r2.cloudflarestorage.com` / `cluely-v2-desktop-glass-releases-prod` | Packaged electron-updater S3/R2 provider | `app-update.yml`; no request made |
| `api.revenuecat.com` | Legacy entitlement migration/client | subscription/settings chunks |
| `js.stripe.com` | Hosted billing flow dependency | pricing/subscription chunks |
| `accounts.google.com` | Google Calendar OAuth | settings/chat chunks |
| `support.cluely.com` | Support navigation | settings chunk |
| `cdn.jsdelivr.net` | VAD URL namespace intercepted and served from local bundle | `main.js:15814-15844` |

Public Clerk/PostHog/client identifiers were statically visible but are redacted from repository evidence. No token, credential, cookie, profile, user record, or binary was uploaded or copied into Bluey.

## Request/data shapes observed

- `transcription.transcribe`: base64 WAV/audio plus language/role context; four-request client cap.
- `sessions.create/update/end/list/delete/resume`: session identity, transcript, heartbeat and post-processing state.
- Agent `uploadScreenshot`: message ID, PNG base64, content type.
- Agent `uploadPartialAudio`: pending audio payload associated with chat/session context.
- Modes/mode files: mode prompt/name, active ordering; presigned upload URL → PUT → server metadata creation → sync-status polling.
- Calendar: connect/disconnect, upcoming meeting list, meeting-linked session, people/attendee brief and summary reads.
- Billing: entitlement/subscription reads and hosted checkout/portal/change/reactivation mutations.

Procedure names and client payload construction do not establish server validation, retention, encryption, tenant isolation, or idempotency.

## Protocol interception and Origin mutation

The main process globally handles `https:` for the default session. Requests for the renderer origin are served from ASAR; a jsDelivr VAD path is served from local resources; all other HTTPS is forwarded. Separately, `onBeforeSendHeaders` forces the renderer Origin value onto every default-session request (`main.js:15814-15844`). This broad rewrite can blur origin-based server controls and complicate debugging. Bluey should not copy it; use an app-scoped protocol and narrowly allowlisted transport instead.

## Upload/persistence boundaries

The product sends transcript text, audio snippets, screenshots, mode files, session/meeting metadata, user identity traits for telemetry, and exception/console context to cloud services. Whether every listed path is enabled for every account depends on runtime flags and user actions. Exact server retention, regional storage, encryption at rest, deletion propagation, and subprocess crash-upload behavior are unknown from the DMG.

## Runtime network result

The bounded probe used a sandbox profile whose outbound-network denial was validated with a failed DNS/HTTPS attempt before the application ran. The app emitted no network log and created no profile files in eight seconds. This is containment evidence, not proof that production startup normally makes no request. See [runtime-assessment.txt](evidence/runtime-assessment.txt).

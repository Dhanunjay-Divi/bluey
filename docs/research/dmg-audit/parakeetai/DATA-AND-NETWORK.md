# ParakeetAI data and network map

## Provenance boundary

This map combines literal static URLs/call shapes with one externally network-blocked unauthenticated startup. The app attempted update, `user.get`, and Mixpanel-token configuration requests; the outer sandbox and blackhole controls prevented completion, and CDP recorded status zero, transfer size zero, and encoded body size zero for every request. No external socket was present. Backend persistence and authorization remain inferred. Reproducible offsets are in [bundle-static.txt](evidence/bundle-static.txt); runtime controls/results are in [runtime-unauthenticated.txt](evidence/runtime-unauthenticated.txt).

## Network destinations

| Destination | Purpose visible in client | Data/boundary visible statically |
| --- | --- | --- |
| `https://www.parakeet-ai.com` | Production service root | tRPC, chat, desktop auth, dashboard links; requests use cookies/credentials. |
| `https://staging.parakeet-ai.com` | Staging service root | Same constructed routes as production. |
| `http://localhost:3000` | Development service root | Same constructed routes; Info.plist allows local insecure HTTP. |
| `<root>/api/trpc` | Typed API transport | User, subscription, configuration, session, transcript, document, resume, scraper, and feedback procedure names. |
| `<root>/api/chat` | Streaming answer generation | Prompt/trigger, session context, pending transcript, direct message and screenshots are visible at call sites. Exact server forwarding is unknown. |
| `<root>/auth/desktop?protocol=parakeetai` | Browser authentication | Returns to custom scheme; optional deployment-bypass query behavior exists. |
| `wss://eu2.rt.speechmatics.com/v2?jwt=...` | Realtime speech-to-text | Ephemeral JWT appears in the WebSocket query string; 16-kHz audio/configuration follows. |
| `https://api-eu.mixpanel.com` | Product telemetry | Backend-supplied project token; identified product events and page views. |
| Google and Cloudflare probes | Connectivity checks | Static no-content/probe URLs; exact runtime cadence was not exercised. |
| GitHub release source | Desktop updates | `parakeetai/parakeetai-desktop-releases` through Electron Updater. |

Literal service roots and offsets are in [bundle-static.txt](evidence/bundle-static.txt). No certificate-pinning implementation was observed in the client. TLS/certificate behavior was not exercised because external networking was denied.

## Client-visible API surface

The following groups are directly supported by procedure names/call sites:

- User: fetch user/country, forced logout state, and update user.
- Subscription: fetch current state.
- Configuration: languages, Mixpanel token, and Vercel region.
- Calls: create/get/list, update live state, metadata, status transitions, ping, takeover, and error reporting.
- Call content: list documents/resumes, get/create transcript batches, get AI messages, scrape a supplied job post, and mint a Speechmatics key.
- Feedback: copied-answer, response rating, and post-call answer.

The complete normalized name list is in [bundle-static.txt](evidence/bundle-static.txt). These are client expectations, not proof that every endpoint was reachable or that authorization is correct.

Observed session-create inputs include call mode, resume/document selection, language, model, automatic-answer and transcript-save flags, extra context, platform/OS version, and a `desktop-app` origin marker. Chat call sites include the call/session identity, recent or pending transcript, trigger kind, direct message, and screenshot data. Static instrumentation attaches some of the same identifiers plus timing/model/region to Mixpanel events. Exact server-side schemas are minified and may evolve independently.

## Local storage

Observed mechanisms:

| Mechanism | Observed contents | Security boundary |
| --- | --- | --- |
| Electron cookie store | `__Secure-next-auth.session-token` and `next-auth.session-token` for the selected API root | Cookies are `secure` and `SameSite=None`, explicitly `httpOnly:false`; renderer-origin script can therefore read them if Electron exposes normal cookie semantics for that origin. |
| Electron Settings | UI/privacy/capture/display/model/development preferences and encrypted deployment-bypass material | Most settings are ordinary plaintext values. The bypass token is passed through `safeStorage` before base64 persistence. |
| Chromium profile/cache | The isolated run created Cookies, DIPS, Local/Session/Shared Storage, Trust Tokens, code/GPU/WebGPU caches, Preferences, and network state under the forced profile. | Cookie row count was zero without authentication. This is ordinary Electron state, not job-application profile isolation. The 1,888-KiB temp tree was deleted. |
| Electron log | Redirected HOME produced `home/Library/Logs/parakeetai-desktop/main.log`; startup/update/meeting-detector state was logged. | The log created an anonymous staging user UUID before login and recorded blocked update errors. Static deep-link handling can also log session identifiers. Production log protection/rotation remains unknown. |
| Updater state/cache | The profile created a 36-byte `.updaterId`; configured cache name is `parakeetai-desktop-updater`. | Startup immediately attempted an update check, which failed `net::ERR_CONNECTION_REFUSED`; no update artifact was downloaded. |

The isolated `settings.json` concretely held `isPrivate=true`, `currentScreenIndex=0`, `locationOnScreen=top-center`, `overlayOpacity=45`, `autoDetectMeetings=unset`, empty encrypted bypass token, and `startHiddenAfterUpdate=false`. The production default path remains platform/library dependent because HOME/user-data were intentionally redirected. `app.asar::dist/main/main.js` bytes 432556–436500; [runtime-unauthenticated.txt](evidence/runtime-unauthenticated.txt).

No SQLite/database file, app-owned durable transcript journal, application queue, or job browser-profile directory was found in packaged resources. Runtime did create ordinary Chromium SQLite/LevelDB stores in the isolated profile. No direct Keychain API usage was found. Electron `safeStorage` may be Keychain-backed on macOS, but that remains a platform inference; no secret was supplied to exercise it.

## Remote storage inference

The client fetches call sessions, transcripts, AI messages, documents, resumes, subscription state, feedback configuration, and user state. It also creates transcript batches and session updates. Therefore remote persistence of at least some of those entities is strongly implied. The DMG does **not** establish:

- physical database or region;
- encryption at rest;
- field-level encryption or key ownership;
- retention/deletion schedule;
- tenant isolation or row-level authorization;
- backups, disaster recovery, or audit logs;
- which model providers receive transcript, screenshot, resume, or job-description content;
- whether Mixpanel identifiers are pseudonymous or joined to other account data.

These must remain unknown until server documentation, contracts, or approved runtime/network inspection is available.

## Secret and identity flow

Browser authentication returns a token through a custom URL payload; main then creates renderer-readable session cookies. A manually entered token follows the same client boundary. The optional deployment-bypass token is encrypted with `safeStorage`; ordinary auth cookies rely on Electron's cookie store rather than an app-owned keychain wrapper. Main bytes 397495–398536 and 432732–434140.

The Speechmatics token is obtained from the Parakeet service for a call session and embedded in the WebSocket URL query. Query placement can expose the token to URL-aware diagnostics/proxies even if short-lived; actual TTL/scope and surrounding log hygiene were not visible. API/chat requests include desktop source, app version, operating system/version, and credentials. Mixpanel identity uses a server-provided user identity/token at app-owned call sites; exact distinct-ID policy is unknown.

## Feature flags and environment controls

Static production/staging/local roots, a Vercel deployment-bypass option, remote languages, Mixpanel token, Vercel region, forced logout, update severity/force state, session limits, and model availability collectively form the visible configuration surface. Some model/plan constants are bundled client defaults, while configuration/subscription procedures are server-sourced. No general third-party feature-flag SDK was identified, but remote API responses can clearly gate behavior.

## Remaining unknowns after the constrained runtime pass

- Successful DNS/TLS destinations, redirects, authenticated cookies, request headers, and response schemas.
- Whether any additional endpoints are constructed dynamically or returned by configuration.
- Cookie accessibility after authentication and Electron fuse values. Context isolation and absent main-world Node globals are now confirmed.
- Production local paths, file protections, log redaction/rotation, cache retention, and updater cleanup. Isolated path structure/default files are now confirmed.
- Transcript retry semantics, de-duplication keys, conflict behavior, and server transaction boundaries.
- Speechmatics JWT lifetime/scope and whether URL values enter logs.
- Mixpanel consent, retention, deletion, event allowlist, and whether any session-recording feature is remotely enabled. Static analysis did **not** find app code enabling recording.
- Backend encryption, tenant isolation, model-provider routing, training/data-use terms, and deletion implementation.

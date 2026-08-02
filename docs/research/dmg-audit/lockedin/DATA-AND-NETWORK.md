# LockedIn data and network analysis

Network destinations and API shapes come from static bundle resources. In the later approved isolated runtime pass, all external egress was directed to a closed local proxy and host resolution was blocked. Firestore, model-configuration, and updater attempts failed; a process socket snapshot found no live TCP/UDP connection, and no external response or payload was captured. Endpoint reachability, server behavior, response schemas, retention, and authorization remain unknown. Sources: [network-inventory.txt](evidence/network-inventory.txt) and [runtime-unauthenticated.md](evidence/runtime-unauthenticated.md).

## Identity and token flow

The static authentication path combines Clerk identity with Firebase:

1. A browser/deep-link flow returns a `locked-in:` URL.
2. The Electron main process extracts `firebaseCustomToken` and `clerkUserId` and delivers them to the renderer (`app.asar:public/electron.js:2466-2585`).
3. The renderer calls Firebase custom-token sign-in and stores `lastClerkUserId` in localStorage.
4. Firebase ID tokens are used for Socket.IO and authenticated first-party HTTP calls.

This flow is observed; token issuance, nonce/state validation, one-time enforcement, rotation, revocation, and backend tenant binding are not present in the client evidence. Process-argument parsing and updater logging are security-relevant because protocol URLs can contain token material; see [SECURITY.md](SECURITY.md).

## Cloud persistence

The renderer names the following Firestore paths:

- `users/{uid}/settings/preferences/copilot_preferences`
- `users/{uid}/interview_presets`
- `users/{uid}/custom_prompts`
- `users/{uid}/sessions/{sessionId}/chat_history`
- `users/{uid}/events`, including `duo_session_invite`
- `users/{uid}/files`

Chat history is paginated in batches of 50, and session records are queried to detect an already-running session. Firestore rules, server-side field validation, indexes, deletion cascades, and retention policy cannot be derived from the DMG. Evidence: [storage-inventory.md](evidence/storage-inventory.md).

## Local persistence

### Observed

- localStorage stores the last Clerk user ID and custom-prompt identifiers; Firebase/Clerk SDKs use browser persistence.
- sessionStorage stores pending helper IDs, helper-access flags, and UI/session flags.
- Main-process code writes logs and exposes log paths/reads over IPC.
- Electron's updater uses cache name `lockedin_desktop_app-updater`.
- `.env.local` is embedded in `app.asar`; only its variable names are recorded in [asar-inventory.txt](evidence/asar-inventory.txt).

### Confirmed in the isolated profile

The unauthenticated launch created Cookies, Cache/Code Cache/GPU caches, IndexedDB, Local Storage, Session Storage, WebStorage/QuotaManager, Shared Dictionary, Trust Tokens, Network Persistent State, Preferences, Crashpad settings, `.updaterId`, and a main log under the supplied HOME/user-data roots. The complete 51-file inventory is in [runtime-unauthenticated.md](evidence/runtime-unauthenticated.md). There is still no static or runtime evidence of `safeStorage`, Keychain, or an app-specific encrypted local database.

## First-party services and API shapes

| Service/area | Static origin or path | Observed data shape/purpose |
|---|---|---|
| Primary API | `https://prod-us-east-1.lockedinai.com` | Authenticated interview, document, billing, event, and account calls |
| Web app | `https://app.lockedinai.com` | Login/sign-up, helper, credits, pricing, discover links |
| Resume product | `https://resume.lockedinai.com` | External resume application |
| Temporary transcription token | `/api/v1/assembly-ai/temporary-token` | Authenticated temporary token request |
| Custom prompts | `/api/v1/custom-prompt/`, `/optimize` | CRUD/streamed optimization wiring |
| Duo | `/api/v1/duo-invite/notify-helper`, `/api/v1/interview-helper/*` | Helper notification, join checks, members, ICE servers |
| Report/history | `/api/v1/interview-report/`, `/generate`, `/api/v1/events/` | Session report/event retrieval and generation |
| Session safety | `/check_running_session`, `/end_session` | Running-session preflight and termination |
| Document index | `/index_material`, `/delete_index` | Auth, user, file, and document-type fields |
| Resume review | `/resume_review` | User/file identity plus job title, company, description, and file link |
| Billing | `/create_payment`, `/retention/subscription_info` | User/price/referral/email/coupon fields; bearer-auth subscription query |
| Survey | `/post_interviews_survey` | Authenticated post-session survey fields |
| Assets | LockedIn S3 image store and CloudFront | Uploaded/static content delivery inferred from call sites |
| Updates | S3 bucket `desktop-app-updates-lockedin-ai`, `ap-southeast-2` | `latest` channel update metadata and packages |

Field names are recorded without values. The complete path list is in [network-inventory.txt](evidence/network-inventory.txt).

## Real-time transport

- Socket.IO is configured for WebSocket-only transport, authenticates with a Firebase ID token, retries indefinitely, and backs off from one to five seconds.
- Duo uses backend-provided ICE data to create WebRTC connectivity. A data channel named `remote-control` can carry input events to the main process when renderer control state permits.

These are observed client behaviors, not evidence of server authorization quality or successful peer connection. Runtime and server unknowns are listed in [limitations.md](evidence/limitations.md).

## Third-party services

Static resources reference Stripe, Clerk, Firebase, Google Secure Token/reCAPTCHA/Fonts/Tag Manager, Microsoft Clarity, Rewardful, LinkedIn Insight, Supademo, Discord, and Unsplash. `build/index.html` directly loads multiple analytics/attribution scripts and contains no CSP meta policy. Server-supplied CSP headers, consent gating, and actual payloads are unknown ([renderer-features.md](evidence/renderer-features.md)).

## Transport policy

Info.plist sets `NSAllowsArbitraryLoads=true`, enables local networking, and defines temporary insecure localhost/127.0.0.1 exceptions with a TLSv1.0 minimum. Static URL inventory is predominantly HTTPS/WSS, so this is an expanded transport capability, not proof that production traffic is plaintext. Provenance: [bundle-identity.txt](evidence/bundle-identity.txt).

## Encryption and secret-storage boundaries

- The DMG is unencrypted. The app bundle is Developer ID-signed/notarized, and the ASAR has an integrity header.
- Firebase/Clerk publishable/configuration values are embedded through `.env.local`; no value is reproduced here. Client-side Firebase configuration is normally public, but any non-public value shipped in this resource would be recoverable by every app recipient.
- No client-specific at-rest encryption boundary for local browser state/logs was found.
- TLS is implied by most static origins; server storage encryption, Firestore rules, and object ACLs are unknown.
- Auth tokens pass through protocol URLs, renderer memory, Firebase persistence, and request headers. No Keychain/safeStorage wrapping was found.

## Queueing, leases, idempotency, and recovery

Client-side recovery includes bounded renderer reloads, audio restart thresholds, Socket.IO reconnects, an active-session preflight, offline Firestore retries, and update state handling. Runtime confirmed repeated Firestore listen retries and static model-tier fallback under blocked egress. No durable queue, lease/heartbeat, idempotency key, side-effect journal, or final-submit state machine was found in the client. A server may implement them, but no supplied evidence demonstrates it. Provenance: [electron-main-observations.md](evidence/electron-main-observations.md), [renderer-features.md](evidence/renderer-features.md), and [runtime evidence](evidence/runtime-unauthenticated.md).

## Deletion and retention

The UI attempts Firebase Auth user deletion. The DMG does not prove deletion of Firestore documents, indexed files, object storage, API records, logs, analytics/attribution profiles, payment records, or Electron browser state. Retention settings and legal holds are also unknown. This should be treated as an unverified boundary, not a claim that deletion fails.

## Feature and environment configuration

Observed environment-variable names select Clerk/Firebase configuration and endpoint mode; main process code also distinguishes development behavior and updater metadata. No remote rollout system, signed flag payload, or documented production/development matrix was present in static resources. Values remain omitted by policy ([asar-inventory.txt](evidence/asar-inventory.txt)).

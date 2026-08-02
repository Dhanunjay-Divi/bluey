# Final Round security assessment

## Overall assessment

The artifact has a sound distribution trust baseline—valid Developer ID signature, hardened runtime, notarization, and verified ASAR header integrity—and several good client controls, including PKCE/state, context isolation, disabled Node integration, navigation/CSP restrictions, and safeStorage on the normal path. Its largest risks are architectural: broad renderer-to-main authority, shared-session permission decisions, sensitive-content telemetry, warning-only live-event validation, and plaintext secret fallback. [Identity evidence](evidence/identity.txt) [Security evidence](evidence/security-static.txt)

## Findings

### P0 — all renderers receive excessive main-process capability

Every preload exposes essentially the full `window.api`, including access-token retrieval, file/resume operations, clipboard, screenshot capture/upload, payment/settings changes, stealth, and external navigation (`preload:47-260`, `preload:388-392`). Main verifies that a sender is a packaged file renderer but does not authorize channels by independently established window role (`main:6278-6408`). The role registry accepts a renderer-supplied identity (`main:7757-7761`).

Observed impact: compromise of any low-complexity widget renderer expands to bearer-token and sensitive device/file capabilities. Exploitation was not attempted. Recommendation: define immutable window capabilities in main at construction, issue per-window unforgeable identities, enforce channel allowlists in the IPC gateway, and minimize each preload surface.

### P0 — interview content reaches a remote-log call path

Structured logger arguments are key-name sanitized and forwarded to Sentry (`main:5100-5194`). `text` is not a sensitive key (`main:3765-3805`). Transcript-final logging includes an 80-character prefix and assistant-final logging includes the complete answer (`main:12104-12115`, `main:12251-12255`).

Observed impact: live interview content can enter Sentry SDK logs. Actual backend ingestion/scrubbing is unknown because outbound traffic was blocked. Recommendation: never log transcript/answer content; replace with length/hash/content class, apply value-level PII/secret scanning before sinks, and add regression tests against every telemetry transport.

### P0 — Bluey should reject malformed remote events, not emulate warn-only validation

Final Round's Socket.IO Zod validation records a warning but still invokes the event handler (`main:5731-5753`). A malformed or version-skewed server message can therefore reach stateful transcript/assistant/session logic. Recommendation: fail closed for security- and state-relevant events, quarantine unknown fields/versions, and use explicit compatibility adapters.

### P1 — plaintext secret fallback

If Electron safeStorage is unavailable, encrypted storage writes plaintext electron-store values (`main:7787-7884`). This includes refresh token and cached user on their storage path (`main:10049-10385`). Recommendation: fail closed for refresh-token persistence, keep the token memory-only, or require a user-visible degraded-security mode; never silently persist plaintext.

### P1 — permission and session scope

The shared default Electron session allows media, display-capture, and notification requests without validating the requesting renderer's role/origin (`main:6742-6765`). No custom session partition was found. Recommendation: bind permission decisions to a main-owned BrowserWindow capability and deny by default; isolate coach/video and general UI partitions where practical.

### P1 — broad native/runtime attack surface

The signed app broadly allows DYLD environment variables, unsigned executable memory, and disabled library validation, and has no App Sandbox entitlement. ATS allows arbitrary loads and insecure loopback/TLS exceptions. These may be required for Electron/native media but increase local compromise impact. Recommendation: remove unused entitlements/exceptions, verify library load paths, and document each remaining exception. [Identity evidence](evidence/identity.txt)

### P1 — fail-open document validation

PDF/DOCX read or parse errors return a valid result and permit upload (`main:11485-11561`). Recommendation: fail closed for unsupported/corrupt files, enforce size/type/magic/parse budgets, and server-scan before making content available to downstream parsers.

### P1 — environment-enabled packaged debugging

A valid `E2E_CDP_PORT` environment variable enables remote debugging in packaged builds (`main:20932-20942`), while the app allows DYLD environment variables. Recommendation: compile debugging out of production or require a signed internal entitlement/build channel and loopback-only random authenticated port.

### P2 — external URL and protocol lifecycle

`openExternal` validates scheme but not host (`main:6719-6740`). Startup removes/sets the `frai` handler; runtime confirmed a stale LaunchServices preference remains after scoped unregister. Recommendation: host/path allowlists for sensitive flows, user confirmation for arbitrary web URLs, and idempotent protocol registration that does not first remove an existing handler.

### P2 — updater assurance is implicit

The current bundle is signed/notarized and the feed is HTTPS. The app's custom authenticode verifier is a non-Windows no-op (`main:6767-6769`); electron-updater may still enforce macOS signing. Recommendation: add explicit release-signing identity/team validation, rollback/version monotonicity, staged rollout, and an integration test that rejects a wrong-team update.

## Positive patterns worth adapting

- PKCE S256, random state, ten-minute flow expiry, and callback state verification (`main:9990-10001`, `main:10387-10560`).
- Content protection reapplication and temporary protection during screenshot capture (`main:5968-5989`, `main:16503-16576`).
- Bounded VAD queue/drop-oldest policy and bounded socket reconnect.
- Navigation/new-window denial and per-page CSPs.
- Proper distribution signing/notarization and ASAR integrity verification.

These should be independently implemented, not copied. [Provenance decision](MANIFEST.md#provenance-and-reuse-decision)

## Bluey security comparison

Bluey's Jobs runner already has stronger irreversible-action controls: durable exclusive submit markers (`jobs/browser/src/irreversible-submit.ts:44-175`), execution leases/heartbeats and a pre-submit irreversible boundary (`jobs/runner/src/execution-lease.ts:66-162`, `196-237`), encrypted profile snapshots with contextual AES-256-GCM (`jobs/runner/src/profile-store.ts:17-75`, `jobs/runner/src/crypto-envelope.ts:43-167`), and HTTPS/public-network browser guards (`jobs/browser/src/browser-network.ts:18-53`). Bluey's native overlay also authenticates local IPC with a per-session token (`crates/cue-daemon/src/app.rs:15416-15532`, `15710-15925`). [Bluey map](evidence/bluey-code-map.txt)

Bluey's relevant remaining risks are delivery rather than adopting Final Round's weaknesses: verify that its production telemetry never records meeting/job contents, finish signed updater/rollback controls, and remove or gate stealth anti-debug/process-masquerading behavior that harms user trust (`crates/cue-stealth/src/macos.rs:1-121`).

## Unknowns requiring controlled validation

- Server-side authorization, telemetry scrubbing, retention/deletion, object-storage policy, secret rotation, and update rejection.
- Keychain ACL and effective production renderer sandbox.
- Authenticated permission prompts and coach/video isolation.
- Whether backend schema enforcement compensates for client warn-only validation.

# Cluely security review

## Security posture

The supplied app has a valid Developer ID signature, hardened runtime, a stapled notarization ticket, strict nested-code verification, and an ASAR header that exactly matches its signed integrity declaration. Its main renderer boundary also validates the sender origin before dispatching registered IPC handlers. Those are concrete strengths ([identity-and-integrity.txt](evidence/identity-and-integrity.txt); `main.js:16668-16688`).

The largest concerns are architectural breadth rather than a proven exploit: an unsandboxed Electron application captures screen and raw audio, exposes generic renderer IPC, rewrites Origin globally, stores app settings in plain JSON, sends sensitive context to multiple cloud services, and grants camera/microphone/JIT entitlements to every helper. No dynamic penetration test or authenticated runtime was performed, so this report does not claim exploitability.

## Trust boundaries

1. **OS → main process.** Microphone, screen-recording and accessibility permissions expose highly sensitive meeting context. The app also declares camera/Bluetooth/audio-capture descriptions. The app is hardened but not App Sandbox-entitled.
2. **Native helpers → main process.** Signed AudioTee and SoX executables stream raw audio over subprocess pipes. The main process supervises termination, but helpers are high-trust code.
3. **Renderer → preload → main.** A generic channel API crosses the bridge. Origin validation narrows who may invoke handlers, but one renderer compromise obtains all currently registered channel capabilities.
4. **Renderer/main → cloud.** Bearer-authenticated RPC, cookies, agent WebSocket, telemetry, uploads, updater, billing and calendar cross the local boundary.
5. **Cloud → renderer.** Session/chat/mode/meeting content is rendered in the Electron UI. Output encoding/content sanitization was not exhaustively proven in minified dependencies.
6. **Updater → installed app.** electron-updater can download and invoke installation; signing-policy enforcement is delegated to the updater/Electron/macOS chain and was not dynamically tested.

## Findings

### C-01 — Generic renderer IPC bridge (medium design risk)

`preload.mjs` exposes arbitrary channel strings for `on`, `send`, and `invoke`. Main-process origin validation (`main.js:16668-16688`) is valuable, but a renderer XSS or compromised same-origin asset can request any registered action, including full-screen screenshot capture, shared-state patch/reset, updater actions, permission calls and window controls (`main.js:16743-16846`). Use a typed, minimal preload API with per-method validation and payload schemas. Do not reproduce this pattern in Bluey.

### C-02 — Global Origin-header rewrite (high design risk)

`main.js:15814-15844` forces one renderer Origin on every default-session request, while the same session forwards arbitrary HTTPS. This can make unrelated origins appear same-origin to servers and obscures the true request initiator. Whether any target is exploitable depends on server checks and CORS/cookie policy, which were not tested. Bluey should retain origin truth, use an app protocol with a restrictive CSP, and allowlist API destinations per client.

### C-03 — Sensitive cloud uploads and telemetry (high privacy impact)

Transcript text, base64 audio, full-display screenshots, mode files, meeting/people context and user identity can cross the device boundary. PostHog identifies users with email/full name, records navigation/launch state/system information, and enables exception/console integration (`posthog-VTzWjthB.js`; `route-BzU-sEaW.js` around byte 602409). The bundle does not establish server retention, regional storage, deletion propagation, or whether console capture is scrubbed. Require explicit per-source consent, visible capture indicators, field-level redaction, documented retention, and telemetry allowlists.

### C-04 — Auth token retrieval through injected page JavaScript (medium design risk)

The auth window retrieves a token by executing JavaScript against `window._globalGetToken`, and the deep-link token is held in main memory until renderer consumption (`main.js:15766-15794,16628-16665`). No token was captured. The in-memory handoff avoids a plain application token file, but executing a page global expands trust in remote auth content. Prefer standards-based loopback/deep-link PKCE with state/nonce, one-time codes, strict origin checks, and keychain-backed refresh material.

### C-05 — Broad helper entitlements (medium hardening gap)

The main app and general/GPU/Plugin/Renderer helpers all receive camera, microphone, JIT and unsigned-executable-memory entitlements. Electron requires JIT-related permissions, but camera/audio access on every helper is broader than least privilege. Remove unnecessary device entitlements from helpers and separately sign only the process that needs them.

### C-06 — Plain local state/logs (low-to-medium privacy risk)

`shared-state.json`, localStorage and `main.log` are not application-encrypted. The schema mainly holds settings and entitlement labels rather than transcript content, but logs/Chromium storage may accumulate identifiers or error context. No application `safeStorage` or Keychain call was found. Minimize logged payloads, apply rotation and permissions, store secrets only in OS keychain, and document which Chromium session data remains on sign-out.

### C-07 — Automatic startup/move/update side effects (medium control risk)

Production startup calls `app.moveToApplicationsFolder()` before normal readiness, initializes login-item startup, and configures immediate/hourly updates (`main.js:15886-15905,16938`). The contained probe stalled without stdout/stderr or profile output and created LaunchServices registration, which was scoped-unregistered. Installation and login-item behavior were not allowed to execute. Installation, launch-at-login and updates should be separate, explicit, reversible user choices with signed-update verification and an enterprise disable policy.

### C-08 — “Invisible” is capture exclusion, not a security boundary (informational)

`setContentProtection` is a platform capture-control signal. It does not prove invisibility to every conferencing, camera, accessibility, EDR, or screen-capture implementation. Treat this as privacy/capture-exclusion UX, never as guaranteed undetectability, and test a disclosed compatibility matrix.

### C-09 — External navigation accepts any syntactically valid HTTPS/mailto URL (low design risk)

New windows are denied, but valid `https:` and `mailto:` targets are delegated to the OS (`main.js:15796-15813`). If remote or rendered content influences a link, this can create phishing/open-redirect exposure. Prefer allowlisted product/support/auth hosts for privileged UI and interstitial confirmation for untrusted destinations.

### C-10 — Server-side authorization, isolation and deletion are unreachable (unknown)

Static client code cannot establish tenant isolation, WebSocket room authorization, upload URL binding, rate limits, encryption at rest, queue idempotency, or deletion completion. The random agent `_pk` query value is not evidence of authentication. These require server documentation, API tests with owned test tenants, and an authenticated privacy/deletion walkthrough.

## Positive controls worth preserving conceptually

- Signed/notarized universal distribution with verified nested code and ASAR integrity.
- Local VAD before cloud transcription, reducing silence upload.
- IPC sender-origin validation.
- Atomic shared-state replacement and schema sanitization.
- Navigation/redirect restriction and denied renderer-created windows.
- Bounded screenshot retry and transcription concurrency.

These are behavioral ideas only. The extracted implementation is proprietary and must not be copied into Bluey.

## Runtime validation still required

- Permission prompt order, cancellation behavior and least-privilege degradation.
- Real cookie/token storage and sign-out residue.
- WebSocket authorization and cross-account room isolation.
- Presigned-upload content type/size/tenant binding and malware handling.
- Update signature/downgrade/channel policy.
- PostHog runtime flags, payload redaction and session-recording/autocapture status.
- Account deletion, transcript/file/calendar revocation and retention.
- Capture exclusion across supported macOS and conferencing applications.

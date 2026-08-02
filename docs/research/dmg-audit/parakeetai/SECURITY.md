# ParakeetAI security review

## Scope

This is a static boundary review plus one externally network-blocked, unauthenticated startup observation—not a penetration test. The runtime pass made no click, permission, login, capture, audio, session, update-install, or bypass action. It confirmed the initial page, context split, main-world globals, processes, attempted startup requests, and isolated files. Authentication, backend authorization, Electron fuses, operating-system permission behavior, and remote update contents remain untested. Artifact identity is fixed by [hashes.txt](hashes.txt); the exact harness and cleanup are in [runtime-unauthenticated.txt](evidence/runtime-unauthenticated.txt).

## Positive controls observed

- The app and nested helpers pass deep strict code-signature validation, Gatekeeper accepts a notarized Developer ID, and the stapled ticket validates. The main executable is hardened-runtime signed. [identity.txt](evidence/identity.txt)
- New-window requests are denied in the Electron window and sent to the system browser, reducing remote content inside the privileged renderer (`app.asar::dist/main/main.js`, bytes 395889–395940).
- The renderer is loaded from a local packaged file rather than a remote web page (`dist/main/main.js`, around byte 395600).
- Private mode invokes `BrowserWindow.setContentProtection`; the app enables it initially (`dist/main/main.js`, bytes 410383 and surrounding private-mode call sites).
- The optional deployment-bypass token is passed through Electron `safeStorage` before persistence (`dist/main/main.js`, bytes 432732–433206).
- ASAR records cover all packed members with SHA-256 and verified with zero mismatches in this audit. [bundle-static.txt](evidence/bundle-static.txt)
- Runtime CDP confirmed an `Electron Isolated Context`; `require`, `process`, and `global` were absent in the renderer main world while the context-bridged API was present. [runtime-unauthenticated.txt](evidence/runtime-unauthenticated.txt)
- Updates use Electron Updater/Squirrel and the supplied application is signed/notarized. Static analysis did not verify the remote release workflow or downgrade policy.

## Findings

### P1 — Generic renderer-to-main IPC bridge has no channel allowlist

**Observed fact.** `dist/main/preload.js` exposes a generic `ipcRendererProxy.invoke(channel, ...args)` and generic event subscription through `contextBridge`. The complete preload is 1,330 bytes and contains no channel validation. Main registers privileged channels including screenshot capture, quit, auth/API configuration, log opening, update installation, clipboard writes, loopback-audio control, and devtools. Reproducible preload/main offsets are in [bundle-static.txt](evidence/bundle-static.txt); examples are main bytes 413511, 418632, 418778, 421646, 423468, and 428059–428128.

**Risk/inference.** A renderer compromise can invoke every present or future registered channel, so a frontend injection or dependency compromise crosses directly into a large native capability set. The renderer is local and external navigation is denied, which lowers exploitability but does not provide least privilege.

**Recommendation.** Reject this pattern for Bluey. Expose typed, purpose-specific bridge methods; validate arguments in both bridge and native handler; split capture/auth/update capabilities by window; add negative tests proving unlisted channels and malformed payloads fail.

### P1 — Session cookies are explicitly not HttpOnly

**Observed fact.** Main writes both `__Secure-next-auth.session-token` and `next-auth.session-token` with `secure:true`, `SameSite=None`, and `httpOnly:false`, then flushes the cookie store (`dist/main/main.js`, bytes 433851–434473).

**Risk/inference.** Disabling HttpOnly removes a standard defense against token theft by script executing in the cookie's web origin. The local `file:` renderer is not automatically proven able to read a different origin's cookie, so this audit does not claim direct `document.cookie` extraction from the overlay. The setting still broadens exposure wherever the API origin renders script and makes the dual-cookie design harder to reason about.

**Recommendation.** Keep session cookies HttpOnly and host-only where possible; use a short-lived, audience-bound desktop exchange code; bind the custom-scheme return to a locally generated state/PKCE verifier; keep durable tokens in OS-protected storage rather than renderer state.

### P1 — Authentication/session deep links carry bearer material

**Observed fact.** The `parakeetai:` protocol accepts base64 JSON containing `authToken` and, for session links, `callSessionId`; main logs that it extracted a session payload and logs session-level identifiers (`dist/main/main.js`, bytes 397495–398553). Base64 is encoding, not confidentiality.

**Risk/inference.** Custom-scheme URLs can enter browser history, inter-process launch arguments, crash reports, or diagnostic logs. A competing scheme handler or forged payload can reach the client; backend validation may prevent use but was unavailable.

**Recommendation.** Return one-time codes rather than bearer tokens, require state/PKCE, expire and consume codes atomically, validate the expected HTTPS origin before initiating auth, and redact payloads and session identifiers from logs.

### P1 — Speechmatics JWT is placed in the WebSocket URL

**Observed fact.** Renderer constructs `wss://eu2.rt.speechmatics.com/v2?jwt=...` after calling `generateSpeechmaticsApiKey` (renderer bytes 149538 and 1218835).

**Risk/inference.** URL query values are more likely than headers/subprotocol values to appear in proxy, crash, or diagnostic URL capture. The JWT may be short-lived and call-scoped, but TTL, audience, permissions, and redaction were not exposed.

**Recommendation.** Prefer an authorization header or supported WebSocket subprotocol; if the provider requires a query token, mint a single-use, minimal-scope, very short-lived token and add URL-redaction tests across logs/telemetry.

### P1 — Broad Electron permission and external-scheme handling

**Observed fact.** The session permission handler approves any request whose permission name is exactly `media`, without checking requesting origin, frame, device kind, or active user gesture. `setWindowOpenHandler` passes the requested URL to `shell.openExternal` and denies the in-app window (`dist/main/main.js`, bytes 395513–395940).

**Risk/inference.** A compromised local renderer inherits media approval and can ask the OS to open arbitrary supported URL schemes. The local packaged renderer and operating-system prompts are mitigations, not origin authorization.

**Recommendation.** Require the packaged renderer origin, expected frame, explicit media type, and an active user-controlled capture state. Allow only `https:` external links to known/validated destinations and require confirmation for unusual schemes.

### P2 — Context isolation is active, but other BrowserWindow hardening relies on defaults

**Observed fact.** The BrowserWindow options specify preload, `experimentalFeatures:true`, and UI properties but do not explicitly set `contextIsolation`, `nodeIntegration`, `sandbox`, or `webSecurity`. Runtime confirmed separate Electron-isolated/default worlds, no main-world Node globals, and the bridged API, so context isolation and Node exclusion are effective in this build. The 200-byte index and runtime DOM have no Content-Security-Policy meta tag. The successful network-sandbox harness required `--no-sandbox`; a prior attempt without that override tried and failed to initialize child sandboxes under the outer sandbox. Production Chromium-sandbox enforcement therefore remains indeterminate. Static offsets are main bytes 394850–395500; [runtime evidence](evidence/runtime-unauthenticated.txt).

**Risk/inference.** Context isolation is a verified mitigation, but relying on defaults for sandbox/web security is fragile across Electron upgrades and packaging/fuse changes. Lack of a CSP means an injection has fewer renderer-side constraints.

**Recommendation.** Set all security-relevant BrowserWindow options explicitly, enable sandbox where compatible, add a restrictive CSP for local assets and specific API/WebSocket origins, disable experimental features unless required, and verify Electron fuses in release CI.

### P2 — Runtime/transport permissions are broader than demonstrated need

**Observed fact.** The app carries unsigned-executable-memory and disabled-library-validation entitlements in addition to JIT. Info.plist globally enables arbitrary network loads and grants local TLS/HTTP exceptions down to TLS 1.0. It also declares Bluetooth and camera descriptions, while the main observed workflow is microphone/system-screen audio and screenshots. [identity.txt](evidence/identity.txt)

**Risk/inference.** Some entitlements are common for Electron, yet disabled library validation and unsigned executable memory enlarge code-injection impact. Global ATS relaxation is broader than the statically observed HTTPS/WSS production endpoints. Unused privacy declarations can confuse consent and increase maintenance surface.

**Recommendation.** Measure release requirements, remove unused permissions/entitlements, scope ATS to necessary local development builds, enforce modern TLS for production, and separate development configuration from signed production artifacts.

### P2 — Mixpanel receives session-linked operational metadata

**Observed fact.** App-owned code initializes EU Mixpanel with a backend token and user identity. Instrumented events include session/call identifiers, model/trigger, screenshot count, response timing/region, recovery, copied/rated responses, and post-call feedback (`dist/renderer/renderer.js`, byte 1151457 and adjacent call sites). Bundled library code supports session recording, but no app invocation enabling it was found.

**Risk/inference.** Even without transcript bodies, joinable session identifiers and interaction timing can be sensitive in interview contexts. Static resources do not expose consent, retention, deletion propagation, or the production event allowlist.

**Recommendation.** Use an explicit minimal event schema, pseudonymous rotating identifiers, no transcript/screenshot/token fields, consent/opt-out where required, bounded retention, deletion propagation, and automated payload tests. Do not state that session recording is active without runtime evidence.

### P2 — Screenshot capture is broad and chat-oriented

**Observed fact.** Main enumerates/captures a selected display and returns encoded image data for chat; renderer permits up to 10 screenshots and 4 MiB encoded total (`dist/main/main.js`, bytes 418778–420000; renderer bytes 1097752–1098035). No static pre-send redaction or durable evidence consent model was found.

**Risk/inference.** Full-display captures can include unrelated notifications, secrets, or other participants' data. Private mode prevents the app itself from appearing in some captures; it does not redact other content.

**Recommendation.** Preserve Bluey's existing preview/confirmation model, prefer region/window selection, show destination and retention at send time, cap dimensions/bytes, strip metadata, and avoid silently persisting screenshots.

## Supply-chain and update boundary

The supplied artifact is signed/notarized and its packed ASAR members verify internally. That does not establish source ownership, reproducible builds, dependency licenses, vulnerability posture, or the security of `parakeetai/parakeetai-desktop-releases`. The shipped native source references a Git dependency by revision, and redistribution rights were not established. No code should be copied into Bluey until ownership, license, dependency, and provenance review is complete.

## Remaining unknowns after the constrained runtime pass

- Electron fuse values and production Chromium-sandbox enforcement. Context isolation and absent main-world Node globals are confirmed.
- Whether authenticated routes change CSP or permission behavior.
- Actual OS permission prompts and origin/device behavior.
- Token lifetime, PKCE/state validation, deep-link replay handling, and forced logout/revocation.
- Cookie accessibility after authentication; the isolated unauthenticated cookie database had zero rows.
- Server authorization/tenant isolation for all listed tRPC procedures, especially `takeOver`, documents/resumes, transcripts, and session mutation.
- Update metadata signatures, downgrade/replay resistance, and release-account controls.
- Production log redaction/rotation, crash reporting, local cache protection, and deletion. The isolated run confirmed the log/cache layout and an anonymous staging UUID generated before login.
- Mixpanel production event payloads, consent, deletion, and any remote session-recording configuration.
- Speechmatics token scope/TTL and URL redaction.

The completed unauthenticated run was sufficient to validate the initial client boundary. The remaining items require authenticated/server evidence or targeted permission/network tests beyond this audit's no-credential/no-permission scope.

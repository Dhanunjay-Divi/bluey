# Littlebird security assessment

## Executive finding

Littlebird has valid Developer ID signing/notarization, hardened runtime, defensive file IPC, external-navigation denial, schema-checked local-tool consent, and a signed update mechanism. Its most important static risks are a successful email OTP being passed to analytics, renderer-accessible token storage protected only by an embedded constant, an overly broad generic IPC bridge, optional relaxed session-recording masking, and production source maps exposing original code/configuration. Evidence: E-LB-SEC-001 through E-LB-SEC-009.

## Findings

### P0 — Successful OTP and email passed to PostHog

The signed `LoginScreen.tsx` sourcesContent calls the Auth0 verification helper and then emits `AUTH0_CODE_VALID` with both email and the raw successful code. The shared analytics path forwards properties to PostHog. This is direct client control-flow evidence, not a marketing inference. No real credential was used. Evidence: E-LB-SEC-001.

Recommended response: remove both values from telemetry, ensure auth factors are denied by analytics schemas, audit/purge the affected production event, review session recordings, and revoke/rotate credentials where confirmed exposure exists.

### P0/P1 — Token storage and renderer reachability

Access/refresh tokens arrive in custom-URL query parameters, are persisted through electron-store with a constant key embedded in the signed bundle, and are exposed through preload getters; the access token is also placed in renderer localStorage. No Keychain path was found. Evidence: E-LB-SEC-003.

Recommended response: move refresh tokens to macOS Keychain with access-control policy, keep access tokens in main-process memory where possible, pass only narrowly scoped operations across IPC, redact custom protocol URLs from all logs, and rotate the store format/key with migration.

### P1 — Generic IPC without sender allowlisting

Preload exposes generic `on/off/send/invoke`; main handlers include auth and collection control, and no shared sender URL/origin validation was found. Context isolation and Node disabling help, but no explicit Chromium sandbox or CSP was found. File operations do include strong path checks. Evidence: E-LB-SEC-004 and E-LB-SEC-009.

Recommended response: expose a frozen, typed API with explicit channel allowlists, validate sender frame/origin and message schema per handler, enable `sandbox: true`, add a restrictive CSP, and remove token-returning IPC.

### P1 — Data-sharing/session-recording boundary

When data sharing is enabled, PostHog masking is relaxed for non-password inputs. Because an OTP is not necessarily a password input, this is high-risk in combination with the telemetry finding, though actual recording of OTP UI was not tested. Evidence: E-LB-SEC-002.

Recommended response: always mask all authentication, identity, compose, meeting, and assistant inputs independent of data-sharing preference; use allowlisted safe UI regions rather than global unmasking.

### P1 — Powerful native capture surface

Accessibility/cross-app parsing, screen/audio capture, calendar/contact/automation permissions, no App Sandbox entitlement, JIT, unsigned executable memory, and disabled library validation create a high-consequence trust boundary. These capabilities align with product behavior and are not evidence of maliciousness. Evidence: E-LB-SEC-007.

Recommended response: split capture into least-privileged signed helpers, remove entitlements not required per process, preserve strict opt-in and visible capture state, test exclusions as a security property, and keep a deny-by-default category/application policy.

### P1/P2 — Client telemetry configuration

The signed client embeds an Axiom ingestion token and multiple client SDK identifiers. Values are redacted and were not validated. Some identifiers are intentionally public; actual privilege is unknown. Evidence: E-LB-SEC-005.

Recommended response: ensure ingestion-only scope, tenant/source constraints, short rotation, quotas/abuse detection, and server-side scrubbing. Use a relay if the token permits more than append-only client telemetry.

### P2 — Production source maps

Original TS/TSX source, route/store structure, and build configuration ship in signed assets. Evidence: E-LB-SEC-008.

Recommended response: upload private source maps directly to error monitoring, strip `sourcesContent` from distributable maps, and prevent `.map` files from entering release artifacts unless explicitly needed.

## Security boundaries

| Boundary | Positive control | Gap / unknown |
| --- | --- | --- |
| DMG → app | Developer ID, hardened runtime, notarization, asar integrity | Broad entitlements remain |
| Renderer → main | Context isolation, Node disabled | Generic IPC, no explicit sandbox/CSP/sender validator |
| Main → native helper | Framed protocol, readiness, PID identity checks, capped restarts | State/queue memory-only; helper has broad entitlements |
| Local tool → user | Zod validation, allow/ask/deny, risk class, expiry | Backend instruction provenance/runtime UI untested |
| App → cloud | TLS/WSS and bearer auth | Token storage/renderer reachability; schemas/retention unknown |
| User → telemetry | Data-sharing setting, password masking | Non-password masking relaxation; OTP event payload |
| Update feed → app | Electron signed updater, notarized bundle | Downgrade allowed; channel authorization server behavior untested |

## Reuse and provenance

Do not copy Littlebird implementation code, bundled parsers, strings, source-map content, or assets into Bluey. The owner authorized inspection for interoperability/research, not relicensing. Architectural ideas may be independently implemented only after ownership, dependency-license, patent, and provenance review. Evidence: E-LB-SEC-008 and E-LB-SEC-010.

## Runtime-required unknowns

- Whether permissions are requested just-in-time and whether denial is graceful.
- Whether CSP is injected dynamically by a server/service worker.
- Whether session recording captures authentication or cross-app content in practice.
- Whether custom-URL tokens appear in local logs or OS history.
- Actual helper file permissions, log redaction, and cleanup.
- Network certificate/payload, server-side authorization, deletion, retention, and billing enforcement.

The bounded unauthenticated run confirmed early local-state and telemetry-attempt behavior but rendered no usable window because Chromium subprocess sandboxing failed inside the outer isolation policy. It granted no privacy permission and used no credential. Evidence: E-LB-RUN-001 through E-LB-RUN-007. No credentialed or permission-granting runtime test should be performed without a separate, explicit test plan and approval.

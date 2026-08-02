# Cluely 2.0.193 Windows data and network map

## Local data

| Store | Observed content | Boundary |
|---|---|---|
| Electron `userData` / `shared-state.json` | onboarding, permissions, window/display, shortcuts, theme, entitlement labels | plain JSON with temp-write/rename; no application encryption observed |
| Chromium profile | Clerk cookies/session, caches, localStorage | Chromium protection is runtime/OS dependent; no app `safeStorage` use found |
| `cluely-v2/main.log` | app/update/lifecycle logs | plaintext local log; production contents require validation |

No local durable job queue, browser-profile vault, receipt database, or Windows
Credential Manager integration was found.

## Packaged network boundaries

- Primary API: `api.v2.cluely.com`.
- Renderer: `renderer.v2.cluely.com`.
- Chat agent WebSocket: agents path under `api.v2.cluely.com`.
- Telemetry: `ph.cluely.com` / PostHog and packaged Sentry integrations.
- Billing/auth dependencies: Clerk, Stripe/Paddle/RevenueCat resources.
- Updates: Cloudflare R2/S3 configuration in `resources/app-update.yml`, with a
  production feed override in the main bundle.

These are client-visible endpoints, not proof of server schemas, authorization,
encryption at rest, retention, or queue semantics.

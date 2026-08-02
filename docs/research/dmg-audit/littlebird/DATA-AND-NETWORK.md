# Littlebird data and network map

## Local data

| Store | Observed contents / role | Protection observed | Evidence |
| --- | --- | --- | --- |
| Electron `userData` | Auth store, PID files, logs/config/cache, helper state | Auth store uses electron-store with an embedded constant key; not device-bound | E-LB-ARCH-004, E-LB-SEC-003 |
| Renderer IndexedDB `littlebird` v15 | Threads, messages, journals, meetings, projects, files, arenas, MCP grants, user config, sync metadata | No at-rest encryption layer found | E-LB-ARCH-009 |
| Renderer localStorage | Access token and UI/store state | Renderer-accessible | E-LB-SEC-003 |
| Browser/session caches | Electron/Chromium Local Storage and code/GPU/WebGPU caches were created before login; no cookie DB or application IndexedDB was created in the bounded run | Main overrides `--user-data-dir` with `appData/Littlebird`; test harnesses must redirect that exact path | E-LB-ARCH-007, E-LB-RUN-002, E-LB-RUN-006 |
| ContextKit state | PID/config/critical state replay; native persistence symbols | Exact on-disk schemas not fully recovered statically | E-LB-ARCH-004, E-LB-ARCH-008 |
| Category seed SQLite | 76,161 domains across six exclusion categories | Signed static seed; no user data | E-LB-ARCH-010 |

No macOS Keychain-backed auth storage path was found. ContextKit symbols reference redaction and exclusion summaries, but their operational completeness is not statically provable. Evidence: E-LB-SEC-003 and E-LB-ARCH-008.

## Network destinations

| Destination | Static role | Confidence / caveat | Evidence |
| --- | --- | --- | --- |
| `https://app.lilbird.co` | Backend API base | Built environment value; shapes inferred from client routes | E-LB-ARCH-006 |
| `wss://ws.lilbird.co` | Authenticated event/tool WebSocket | Main/renderer control flow | E-LB-ARCH-006 |
| `https://app.littlebird.ai` | Public links and desktop-login UI | Built public-link base | E-LB-FEAT-001 |
| `https://downloads.littlebird.ai` | Electron update feed | Main updater configuration | E-LB-ARCH-011 |
| `https://mcp.littlebird.ai/mcp` | Littlebird MCP endpoint | Shipped renderer configuration | E-LB-FEAT-002 |
| Auth0 OAuth endpoints | Google/Apple/email OTP authentication | Exact client values omitted | E-LB-FEAT-001 |
| Axiom API | Client/native telemetry ingestion | Embedded token omitted; scope untested | E-LB-SEC-005 |
| PostHog US ingestion | Product analytics and optional session recording | Client configuration omitted | E-LB-SEC-001, E-LB-SEC-002 |
| Sentry ingest | Crash/error telemetry | Client DSN omitted | E-LB-SEC-005 |
| Singular / Product Fruits | Attribution and onboarding/product analytics | Client identifiers omitted | E-LB-SEC-005 |

No destination was contacted intentionally during static analysis, and no embedded client token was validated. During the bounded no-network launch, main attempted Axiom telemetry before authentication; DNS resolution was blocked and failed with `ENOTFOUND`, so no response or token privilege was tested. Evidence: E-LB-RUN-004.

## Visible API families

Signed renderer/main resources include client routes for:

- chats, files, projects, journals, routines/reports, meetings/transcripts/import, and world-model/knowledge operations;
- integrations and multi-account connection state, including email draft fetch/update/send;
- subscription, payment, invoices, purchased credits, pool/refill controls, and plan changes;
- context exclusions and synchronization;
- account deletion, feedback, and export request;
- local Axon tool frames and consent results over WSS.

These are static route strings and typed client flows, not a server contract capture. Status codes, authorization behavior, idempotency, retention, and production response schemas remain unverified. Evidence: E-LB-FEAT-002, E-LB-FEAT-004 through E-LB-FEAT-006, and E-LB-FEAT-008.

## Authentication flow

Desktop login opens a public browser page and receives access/refresh tokens in the `little-bird://auth-callback` query. Main persists both through electron-store; the access token is synchronized to renderer localStorage, and preload exposes token getters. WSS and HTTP clients use bearer authentication. Evidence: E-LB-FEAT-001, E-LB-ARCH-006, E-LB-SEC-003.

This design crosses four boundaries—external browser, custom-protocol URL, Electron main, and renderer—and makes renderer compromise especially consequential. The embedded electron-store key does not provide a Keychain-equivalent boundary.

## Deletion, export, and recovery

The client exposes deletion of context for the last hour, day, custom range, or all, and sends work to both backend and native helper paths. Account deletion and export-request routes exist. IndexedDB corruption recovery deletes the local database and reboots from server state. Evidence: E-LB-FEAT-006 and E-LB-ARCH-009.

Unknowns include backend retention, backups, processor deletion, export completeness, and whether the helper and cloud acknowledge deletion atomically. Those require authenticated runtime/server evidence.

# Pinky Architecture — Reconstructed from Source Code

**Method**: Read every .go file in cmd/ and internal/ line-by-line. No .md docs consulted.
**Branch**: codex/fresh-review-20260421 at cef44f3 (actual tip: a4352a4)
**Read date**: 2026-05-12

## 1. Package Map (with import graph)

### cmd/pinky-server
**Role**: HTTP API server. The SaaS backend for Pinky — user accounts, sessions, billing, captions.
**Imports**: `internal/api`, `internal/buildinfo`
**Entry flow**: Reads `PINKY_JWT_SECRET` (required), `PINKY_DB_PATH`, `PINKY_PORT`. Creates `api.NewServer(dbPath, jwtSecret)`, starts HTTP on `:port`. Spawns goroutine for inactive-account-deletion queueing (24h ticker, 6-month inactivity threshold, batch of 100). Graceful shutdown on SIGINT/SIGTERM with 5s timeout.

### cmd/pinky (CLI client)
**Role**: Host-side CLI. Manages local sessions, audio, captions, display selection, updates, login.
**Imports**: `internal/audio`, `internal/captions`, `internal/hostoverlay`, `internal/overlay`, `internal/screen`, `internal/session`, `internal/systemmic`, `internal/vieweraudio`
**Commands**: `on`, `off`, `login`, `update`, `status`, `reset`, `audio`, `overlay`, `system-mic`, `listen`, `captions`, `display`, `logs`, `uninstall`, `help`
**Entry flow**: Subcommand dispatch via `os.Args[1]`. On Windows, elevates to admin. `pinky on` does browser-based OAuth login (local HTTP callback on random port with CSRF state), then calls `session.Start()` which daemonizes.

### cmd/signal-relay
**Role**: WebRTC signaling relay server. Bridges viewer offers to host answers via WebSocket.
**Imports**: `internal/buildinfo`, `internal/netutil`, `internal/relayauth`, `internal/session` (for `PinnedHTTPClient`)
**Entry flow**: Listens on `PINKY_RELAY_ADDR` (default `:9000`). Serves viewer HTML from `PINKY_RELAY_WEB_DIR`. Manages host WebSocket connections keyed by session code. Forwards SDP offers from viewers (HTTP POST) to hosts (WebSocket), returns answers.

### internal/api
**Role**: Full HTTP API server — auth, sessions, billing, captions, admin, teams, telemetry, observability.
**Key types**: `Server` (main struct with DB, JWT config, pending joins, viewer leases, telemetry, observability, done channel)
**Concurrency**: `runViewerLeaseReaper()`, `runPendingDeletionProcessor()`, `runHeartbeatReaper()`, `runCaptionAudioUploadLimiterCleanup()`, hourly cleanup+session-expiry+integrity-check goroutine.

### internal/db
**Role**: SQLite persistence layer. Schema migrations, CRUD for users/sessions/plans/devices/teams/captions/billing.
**Key types**: `User`, `Session`, `Plan`, `PendingDeletion`, `CaptionChunk`, `CaptionAudioChunk`
**Persistence**: SQLite with WAL mode, foreign keys on, MaxOpenConns=1, busy_timeout=5000ms, synchronous=NORMAL.

### internal/session
**Role**: Client-side session lifecycle — daemonization, state persistence, heartbeat, API client, auth storage, updates.
**Key types**: `SessionInfo`, `SessionStore`, `AuthInfo`, `APISession`, `APIUser`, `CaptionAllowance`
**Files**: `daemon.go` (supervisor pattern), `manager.go` (Start/Stop), `heartbeat.go`, `state.go`, `apiclient.go`, `pinning.go` (TLS cert pinning), `update.go`, `urls.go`

### internal/webrtc
**Role**: Host-side WebRTC engine. Manages peer connections, video/audio encoding, data channels, captions, viewer audio playback.
**Key types**: `Server` (massive struct — peers map, frame/audio channels, input dispatch, adaptive bitrate, caption storage, relay WebSocket)
**Files**: `server.go`, `peer.go`, `audio_pipeline.go`, `captions.go`, `caption_storage.go`, `adaptive.go`, `viewer_audio.go`, `cursor.go`, `input_dispatch.go`, `host_overlay.go`, `sdp.go`, `state.go`, `diagnostics.go`, `vp8_payloader.go`, `media_clock.go`, `playout_delay.go`, `datachannel_send.go`, `wake_*.go`

### internal/audio
**Role**: System audio capture setup (macOS virtual audio device switching). Permission checks, auto-install.

### internal/captions
**Role**: Client-side caption preference persistence (enabled/disabled state file in ~/.pinky/).

### internal/captionstorage
**Role**: R2 (Cloudflare) object storage client for caption text and audio chunks. S3-compatible signing (AWS Signature V4 manual implementation).

### internal/hostoverlay
**Role**: Host-side overlay UI management (show/hide controls overlay on the shared screen).

### internal/input
**Role**: Permission checks for keyboard/mouse control (Accessibility on macOS).

### internal/netutil
**Role**: Trusted proxy CIDR parsing, client IP extraction from X-Forwarded-For.

### internal/overlay
**Role**: Caption overlay process management (start/stop helper processes for on-screen captions).

### internal/publisher
**Role**: (Not read in detail — likely related to media publishing)

### internal/relay
**Role**: API-side relay client. `KillSession()` sends kill to relay, `StatusSessions()` checks host connectivity.

### internal/relayauth
**Role**: HMAC-SHA256 token issuance and verification for relay authentication. Claims: code, role (host/viewer), exp, iat.

### internal/releaseintegrity
**Role**: Ed25519 signature verification for release binaries (update integrity).

### internal/screen
**Role**: Screen capture abstraction — display enumeration, capture permissions check.

### internal/systemmic
**Role**: Experimental "System Mic" virtual microphone for meeting apps (preprod-only).

### internal/vieweraudio
**Role**: Viewer-to-host audio preferences — enabled state, duck volume percent, echo risk warnings.

### internal/buildinfo
**Role**: Build metadata (version, commit, build time) injected via ldflags.


## 2. Entry Flow (cmd/pinky-server/main.go reconstructed)

1. Read `PINKY_JWT_SECRET` env (fatal if empty)
2. Read `PINKY_DB_PATH` (default `/opt/pinky-api/pinky.db`) and `PINKY_PORT` (default `8080`)
3. `api.NewServer(dbPath, jwtSecret)`:
   - Opens SQLite with WAL, foreign keys, busy_timeout=5000, synchronous=NORMAL
   - Runs all schema migrations (20 versions)
   - Seeds admin accounts from `PINKY_ADMIN_ACCOUNTS` env
   - Runs `cleanupOldData()` (deletes stopped sessions >180 days, expired verifications/resets)
   - Runs database integrity check
   - Spawns 5 background goroutines (hourly cleanup, viewer lease reaper, pending deletion processor, heartbeat reaper, caption audio upload limiter cleanup)
4. Create `rootCtx` with cancel
5. Spawn inactive-account-deletion goroutine (24h ticker, queues accounts inactive >6 months)
6. Spawn signal handler goroutine (SIGINT/SIGTERM → cancel root context → shutdown HTTP with 5s timeout)
7. Log version info, start `ListenAndServe`

## 3. API Surface

### HTTP Routes (every endpoint)

Routes are registered twice: under `/api` and `/api/v1` prefixes.

| Method | Path | Handler | Auth | Body Limit |
|--------|------|---------|------|------------|
| POST | /api/auth/signup | handleSignup | None | 4KB |
| POST | /api/auth/login | handleLogin | None | 4KB |
| POST | /api/auth/logout | handleLogout | None | 4KB |
| POST | /api/auth/refresh | handleRefresh | None | 4KB |
| POST | /api/auth/verify | handleVerifyEmail | None | 4KB |
| POST | /api/auth/forgot-password | handleForgotPassword | None | 4KB |
| POST | /api/auth/reset-password | handleResetPassword | None | 4KB |
| GET | /api/trial/config | handleTrialConfig | None | - |
| POST | /api/trial | handleCreateTrial | None | 4KB |
| POST | /api/trial/convert | handleStartTrialConversion | authMiddleware | 4KB |
| POST | /api/trial/convert/verify | handleVerifyTrialConversion | None | 4KB |
| GET | /api/me | handleMe | authMiddleware | - |
| POST | /api/auth/change-password | handleChangePassword | authMiddleware | 4KB |
| POST | /api/auth/delete-account | handleDeleteAccount | authMiddleware | 4KB |
| POST | /api/legal-acceptances | handleRecordLegalAcceptance | authMiddleware | 4KB |
| POST | /api/sessions | handleCreateSession | authMiddleware | 8KB |
| GET | /api/sessions | handleListSessions | authMiddleware | - |
| GET | /api/sessions/history | handleSessionHistory | authMiddleware | - |
| DELETE | /api/sessions | handleStopAllSessions | authMiddleware | - |
| GET | /api/sessions/{code} | handleGetSession | None | - |
| GET | /api/internal/sessions/{code}/active | handleInternalSessionActive | None (relay secret) | - |
| POST | /api/internal/sessions/{code}/disconnected | handleInternalSessionDisconnected | None (relay secret) | 4KB |
| DELETE | /api/sessions/{code} | handleStopSession | authMiddleware | - |
| POST | /api/sessions/{code}/heartbeat | handleSessionHeartbeat | authMiddleware | - |
| POST | /api/sessions/{code}/telemetry | handleSessionTelemetry | authMiddleware | - |
| POST | /api/sessions/{code}/join | handleJoinSession | None | 4KB |
| POST | /api/sessions/{code}/leave | handleLeaveSession | None | 4KB |
| POST | /api/sessions/{code}/confirm | handleConfirmJoin | None | 4KB |
| POST | /api/sessions/{code}/host-token | handleRefreshHostSignalToken | authMiddleware | 4KB |
| POST | /api/sessions/{code}/token | handleRefreshSignalToken | None | 4KB |
| POST | /api/sessions/{code}/viewer-diag | handleViewerDiagnostic | None | 16KB |
| GET | /api/captions/allowance | handleGetCaptionAllowance | authMiddleware | - |
| POST | /api/sessions/{code}/caption-usage | handleRecordCaptionUsage | authMiddleware | 4KB |
| POST | /api/sessions/{code}/captions | handleStoreCaptions | authMiddleware | 32KB |
| POST | /api/sessions/{code}/caption-audio | handleStoreCaptionAudio | None | 2MB |
| GET | /api/sessions/{code}/recording | handleGetCaptionRecording | authMiddleware | - |
| GET | /api/sessions/{code}/recording/audio/{chunkID} | handleGetCaptionRecordingAudio | authMiddleware | - |
| POST | /api/logs | handleUploadLogs | authMiddleware | 100KB |
| GET | /api/turn-credentials | handleTurnCredentials | None | - |
| GET | /api/devices | handleListDevices | authMiddleware | - |
| DELETE | /api/devices | handleRemoveDevices | authMiddleware | 4KB |
| DELETE | /api/devices/{deviceID} | handleRemoveDevice | authMiddleware | - |
| POST | /api/devices/register | handleRegisterDevice | authMiddleware | 8KB |
| GET | /api/team | handleTeamOverview | authMiddleware | - |
| POST | /api/team/invitations | handleCreateTeamInvitation | authMiddleware | 4KB |
| POST | /api/team/invitations/accept | handleAcceptTeamInvitation | authMiddleware | 4KB |
| POST | /api/team/invitations/{inviteID}/revoke | handleRevokeTeamInvitation | authMiddleware | 4KB |
| PATCH | /api/team/members/{userID} | handleUpdateTeamMember | authMiddleware | 4KB |
| DELETE | /api/team/members/{userID} | handleRemoveTeamMember | authMiddleware | - |
| GET | /api/plans | handlePlans | None | - |
| GET | /api/healthz | handleHealthz | None | - |
| GET | /api/version | handleVersion | None | - |
| GET | /api/release-integrity | handleReleaseIntegrity | None | - |
| POST | /api/billing/checkout | handleCheckout | authMiddleware | 8KB |
| POST | /api/billing/cancel | handleCancelSubscription | authMiddleware | 4KB |
| GET | /api/billing/history | handleBillingHistory | authMiddleware | - |
| POST | /api/billing/webhook | handleWebhook | None | 64KB |
| GET | /api/admin/overview | handleAdminOverview | adminMiddleware | - |
| GET | /api/admin/deployments | handleAdminDeployments | adminMiddleware | - |
| POST | /api/admin/deployments/actions | handleAdminDeploymentAction | adminMiddleware | 4KB |
| GET | /api/admin/disputes/evidence | handleAdminDisputeEvidence | adminMiddleware | - |

### Page Routes (server-rendered HTML)
| Method | Path | Handler |
|--------|------|---------|
| GET | / | handleLanding |
| GET | /admin | handleAdminPage |
| GET | /admin/deployments | handleAdminDeploymentsPage |
| GET | /dashboard | handleDashboard |
| GET | /download | handleDownload |
| GET | /login | handleLoginPage |
| GET | /terms | serveHTMLPage (terms.html) |
| GET | /privacy | serveHTMLPage (privacy.html) |
| GET | /install | handleInstallScript |
| GET | /install.sh | handleInstallScript |
| GET | /install.ps1 | handleInstallPS1 |
| GET | /metrics | adminMiddleware → Prometheus handler |
| GET | /debug/pprof/* | adminMiddleware → pprof handlers |

### Signal Relay Routes (cmd/signal-relay)
| Method | Path | Handler |
|--------|------|---------|
| GET | / | serveViewer (index.html) |
| GET | /join/ | serveViewer (index.html) |
| GET | /join/{code} | serveViewer (index.html) |
| GET | /web/* | serveViewerAssets (static files) |
| WS | /signal/host?code=X | handleHost (WebSocket upgrade) |
| POST | /signal/offer?code=X | handleOffer (SDP relay) |
| GET | /signal/health | "ok" |
| GET | /signal/healthz | JSON version info |
| GET | /signal/metrics | Bearer auth → relay metrics JSON |
| GET | /signal/status?codes=X,Y | handleSignalStatus (host connectivity check) |
| POST | /signal/kill?code=X | Bearer auth → send kill to host |


## 4. Database Schema

**Engine**: SQLite 3 with WAL journal mode, foreign keys enabled, MaxOpenConns=1.
**Migration system**: Go-embedded `schemaMigrations` slice (20 versions). Each migration runs in a transaction. Legacy schema reconciliation for upgrades from pre-migration databases.

### Tables (in migration order)

#### schema_migrations
| Column | Type | Constraints |
|--------|------|-------------|
| version | INTEGER | PRIMARY KEY |
| name | TEXT | NOT NULL |
| applied_at | DATETIME | NOT NULL DEFAULT now |

#### users
| Column | Type | Constraints |
|--------|------|-------------|
| id | TEXT | PRIMARY KEY |
| email | TEXT | UNIQUE NOT NULL |
| password_hash | TEXT | NOT NULL |
| plan | TEXT | NOT NULL DEFAULT 'none' |
| role | TEXT | NOT NULL DEFAULT 'user' |
| stripe_customer_id | TEXT | |
| stripe_subscription_id | TEXT | |
| verified | INTEGER | NOT NULL DEFAULT 0 |
| auth_updated_at | DATETIME | |
| created_at | DATETIME | |
| updated_at | DATETIME | |

#### sessions
| Column | Type | Constraints |
|--------|------|-------------|
| id | TEXT | PRIMARY KEY |
| user_id | TEXT | NOT NULL FK→users(id) |
| code | TEXT | UNIQUE NOT NULL |
| stream_id | TEXT | NOT NULL |
| relay_ip | TEXT | NOT NULL |
| device_name | TEXT | |
| device_id | TEXT | |
| host_platform | TEXT | |
| host_arch | TEXT | |
| host_version | TEXT | |
| host_compatibility | TEXT | |
| remote_id | TEXT | |
| remote_password | TEXT | |
| status | TEXT | NOT NULL DEFAULT 'active' |
| team_id | TEXT | |
| viewer_count | INTEGER | NOT NULL DEFAULT 0 |
| controller_count | INTEGER | NOT NULL DEFAULT 0 |
| stop_reason | TEXT | |
| created_at | DATETIME | |
| last_heartbeat_at | DATETIME | |
| stopped_at | DATETIME | |

**Indexes**: `idx_sessions_user_status(user_id, status)`, `idx_sessions_team_status(team_id, status, created_at)`

#### plans
| Column | Type | Constraints |
|--------|------|-------------|
| name | TEXT | PRIMARY KEY |
| max_concurrent_sessions | INTEGER | |
| max_devices | INTEGER | |
| max_controllers | INTEGER | |
| max_listeners | INTEGER | |
| timeout_hours | INTEGER | |
| price_cents | INTEGER | |
| stripe_price_id | TEXT | |
| team_seat_limit | INTEGER | NOT NULL DEFAULT 1 |

**Seed data**: trial(1/1/1/2/0/$0), starter(1/1/1/2/0/$14.99), pro(3/3/1/5/0/$24.99), unlimited(7/7/1/7/0/$49.99), admin(999/999/999/999/0/$0)

#### devices
| Column | Type | Constraints |
|--------|------|-------------|
| id | TEXT | PRIMARY KEY |
| user_id | TEXT | NOT NULL FK→users(id) |
| device_name | TEXT | |
| device_id | TEXT | NOT NULL |
| platform | TEXT | |
| team_id | TEXT | |
| registered_at | DATETIME | DEFAULT now |
| last_seen | DATETIME | DEFAULT now |
| UNIQUE(user_id, device_id) | | |

#### payments
| Column | Type | Constraints |
|--------|------|-------------|
| id | TEXT | PRIMARY KEY |
| user_id | TEXT | NOT NULL FK→users(id) |
| plan | TEXT | NOT NULL |
| amount_cents | INTEGER | NOT NULL |
| status | TEXT | NOT NULL DEFAULT 'succeeded' |
| stripe_payment_id | TEXT | |
| stripe_customer_id | TEXT | |
| stripe_subscription_id | TEXT | |
| stripe_invoice_id | TEXT | |
| stripe_payment_intent_id | TEXT | |
| stripe_charge_id | TEXT | |
| billing_period_start | DATETIME | |
| billing_period_end | DATETIME | |
| checkout_terms_accepted | INTEGER | NOT NULL DEFAULT 0 |
| created_at | DATETIME | DEFAULT now |

#### payment_disputes
| Column | Type | Constraints |
|--------|------|-------------|
| id | TEXT | PRIMARY KEY |
| payment_id | TEXT | FK→payments(id) |
| user_id | TEXT | FK→users(id) |
| stripe_charge_id | TEXT | |
| stripe_payment_intent_id | TEXT | |
| amount_cents | INTEGER | NOT NULL DEFAULT 0 |
| currency | TEXT | |
| reason | TEXT | |
| status | TEXT | |
| evidence_due_by | DATETIME | |
| created_at | DATETIME | |
| updated_at | DATETIME | |

#### caption_chunks
| Column | Type | Constraints |
|--------|------|-------------|
| id | TEXT | PRIMARY KEY |
| user_id | TEXT | NOT NULL FK→users(id) |
| session_id | TEXT | NOT NULL FK→sessions(id) |
| session_code | TEXT | NOT NULL |
| object_key | TEXT | NOT NULL |
| storage_provider | TEXT | NOT NULL DEFAULT 'r2' |
| bytes | INTEGER | NOT NULL DEFAULT 0 |
| caption_count | INTEGER | NOT NULL DEFAULT 0 |
| created_at | DATETIME | DEFAULT now |
| expires_at | DATETIME | |

#### caption_audio_chunks
| Column | Type | Constraints |
|--------|------|-------------|
| id | TEXT | PRIMARY KEY |
| user_id | TEXT | NOT NULL FK→users(id) |
| session_id | TEXT | NOT NULL FK→sessions(id) |
| session_code | TEXT | NOT NULL |
| object_key | TEXT | NOT NULL |
| storage_provider | TEXT | NOT NULL DEFAULT 'r2' |
| content_type | TEXT | NOT NULL DEFAULT 'audio/webm' |
| bytes | INTEGER | NOT NULL DEFAULT 0 |
| duration_ms | INTEGER | NOT NULL DEFAULT 0 |
| sequence | INTEGER | NOT NULL DEFAULT 0 |
| created_at | DATETIME | DEFAULT now |
| started_at | DATETIME | |
| ended_at | DATETIME | |
| expires_at | DATETIME | |

#### caption_usage
Tracks live caption seconds consumed per session for plan enforcement.

#### trial_session_usage, trial_grants, trial_abuse_events
Trial abuse prevention: IP-based rate limiting, fingerprint cooldowns, abuse scoring.

#### team_workspaces, team_members, team_invitations, team_audit_events
Full team/workspace system with roles (owner/member), invitation flow with token hashes, audit trail.

#### Other tables
`email_verifications`, `login_attempts`, `password_resets`, `pending_deletions`, `processed_webhook_events`, `checkout_evidence`, `legal_acceptances`, `admin_access_audit`


## 5. Session Lifecycle State Machine

```
                    ┌─────────────┐
                    │   CREATED   │  (API: POST /api/sessions)
                    └──────┬──────┘
                           │ host connects to relay WebSocket
                           ▼
                    ┌─────────────┐
          ┌────────│   ACTIVE    │◄────────┐
          │        └──────┬──────┘         │
          │               │                │ reconnect within 30min grace
          │               │                │
          │    ┌──────────┼──────────┐     │
          │    │          │          │     │
          │    ▼          ▼          ▼     │
          │  timeout   user stop  heartbeat│
          │  (plan)    (DELETE)    timeout  │
          │    │          │       (5min)   │
          │    ▼          ▼          ▼     │
          │  ┌─────────────────────────┐   │
          │  │        STOPPED          │   │
          │  └─────────────────────────┘   │
          │                                │
          │  host WS disconnect            │
          └────────────────────────────────┘
               (30min hostDisconnectAPIGrace)
```

**Heartbeat**: Client sends every 60s (`sessionHeartbeatInterval`). Server reaps sessions with no heartbeat for 5min (`sessionHeartbeatStaleAfter`). Reaper runs every 60s.

**Cleanup**: Stopped sessions retained 180 days for dispute evidence, then deleted.

**Daemon supervisor**: `cmd/pinky` daemonizes via `session.Daemonize()`. A supervisor process watches the daemon child, restarts on panic (exit code 2), signal kills (≥128), or watchdog exits (99). Max 6 crash restarts in 10min window, then 60s cooldown. Exponential backoff up to 8s between restarts.

## 6. WebRTC Architecture

**Location**: `internal/webrtc/server.go` + `peer.go`

**Signaling flow**:
1. Host daemon connects to relay via WebSocket (`/signal/host?code=X`) with relay auth token
2. Viewer sends SDP offer via HTTP POST to relay (`/signal/offer?code=X`)
3. Relay forwards offer to host over WebSocket with unique `offerID`
4. Host creates PeerConnection, sets remote description, creates answer
5. Host sends answer back over WebSocket keyed by `offerID`
6. Relay returns answer to viewer's HTTP response
7. ICE candidates gathered via trickle ICE (bundled in SDP)

**Peer connection management**:
- `Server.peers` map keyed by peer ID, protected by `peersMu` RWMutex
- Each peer has: PeerConnection, data channel (`dc`), video/audio tracks, state tracking
- ICE configuration fetched from API (`/api/turn-credentials`) with 10min TTL
- ICE disconnected grace: 30s (connected peers), 8s (never-connected peers)
- ICE failed grace: 25s (connected), 5s (never-connected)
- Stale peer timeout: 15s, hidden viewer timeout: 90s

**Media tracks**:
- Video: VP8 encoded via x264-go, adaptive bitrate with GCC (Google Congestion Control)
- Audio: Opus encoded via pion/opus
- Capture FPS: default 30, adaptive based on network tier
- REMB startup grace: 3s (ignores low estimates during path probing)

**Data channels**:
- `control`: caption state, caption messages, input events, cursor updates
- Caption messages flow viewer→host via data channel, displayed via overlay

**Viewer/Host roles**:
- One controller at a time (`s.controller` string, `ctrlMu` mutex)
- `allowUnattendedControl` flag for auto-granting control
- Viewer slots with release grace (10s)

**Adaptive bitrate** (`adaptive.go`):
- Smoothed bitrate estimation
- Tier-based quality (network tier field)
- Upgrade eligibility tracking with cooldown
- Feedback relief mechanism for viewer-reported issues

## 7. Caption + Caption-Audio Pipeline

**Ingestion flow**:
1. Viewer's browser runs Web Speech API, generates caption text
2. Viewer sends `{type:"caption", text, speaker, final, timestamp}` over WebRTC data channel
3. Host `handleCaptionMessage()` validates: captions enabled? overlay available?
4. Host displays caption via overlay process (`internal/overlay`)
5. If `captionStorageEnabled`: final captions queued to `captionStorageCh` (buffered channel)
6. Background goroutine batches captions (16 per batch, 2s flush interval) and POSTs to API (`/api/sessions/{code}/captions`)
7. API stores caption text as JSON chunks in R2 via `internal/captionstorage`

**Caption audio upload**:
1. Viewer's browser captures microphone audio as WebM chunks
2. Viewer POSTs audio to `/api/sessions/{code}/caption-audio` (2MB limit per chunk)
3. API rate-limits: max 30 uploads per minute per session, max 2 concurrent streams per user
4. Audio stored in R2 with metadata in `caption_audio_chunks` table
5. Stream timeout: 30s, max stream bytes: 100MB

**Replay**:
- `GET /api/sessions/{code}/recording` returns manifest with caption lines + audio chunk URLs
- `GET /api/sessions/{code}/recording/audio/{chunkID}` streams audio from R2
- Max 200 caption chunks, 200 caption lines, 1000 audio chunks per recording

**Plan limits**:
- Starter: 3 hours live captions in 30-day window, 7-day retention
- Pro: unlimited live, 180-day retention, storage enabled
- Unlimited/Team: unlimited live, 365-day retention, storage enabled

**Constants**: `maxStoredCaptionBatch=32`, `maxStoredCaptionTextBytes=1000`, `maxCaptionAudioBytes=2MB`, `captionAudioUploadWindow=1min`, `maxCaptionAudioUploads=30`

## 8. Authentication + Authorization

**Token system**: Custom JWT-like HMAC-SHA256 tokens (not standard JWT library — hand-rolled in `internal/api/auth.go`).
- Access token TTL: 30 minutes
- Refresh token TTL: 30 days
- Key rotation: `PINKY_JWT_KEY_ID`, `PINKY_JWT_ISSUER`, `PINKY_JWT_AUDIENCE`, `PINKY_JWT_PREV_KEYS` for previous key validation

**Auth flow**:
1. Signup: email + password (bcrypt, min 8 chars, max 72) → verification code via email (Resend API)
2. Login: email + password → access token + refresh token
3. Cookie-based auth: `pinky_token` (access), `pinky_refresh`, `pinky_remember`, `pinky_csrf`
4. Bearer token auth: `Authorization: Bearer <token>` header
5. CSRF protection: double-submit cookie pattern (`X-CSRF-Token` header must match `pinky_csrf` cookie)

**Middleware chain** (outer to inner): `observability.wrap` → `requestIDMiddleware` → `logMiddleware` → `corsMiddleware` → `rateLimitMiddleware` → `maxBodyMiddleware` → route handler

**Admin auth**: `adminMiddleware` = `authMiddleware` + check `user.Role == "admin"` + audit log

**Relay auth**: HMAC-SHA256 tokens with claims `{code, role, exp, iat}`. Issued by API, verified by relay. Roles: "host", "viewer".

**Rate limiting**: Token bucket per IP. 200 tokens max, refill 3/second. 10,000 max tracked clients. 5min eviction for idle clients.

**Login brute-force protection**: 5 failed attempts → 15min lockout (per email+IP key).

**TLS cert pinning** (client-side): SHA256 fingerprint of pinky.sh production certificate pinned in `internal/session/pinning.go`. Custom `VerifyConnection` on TLS config.


## 9. Concurrency Map

### Server-side goroutines (cmd/pinky-server)
| Goroutine | Location | Purpose |
|-----------|----------|---------|
| Hourly cleanup | `server.go:NewServer` | `cleanupOldData` + `ExpireOldSessions` + integrity check |
| Viewer lease reaper | `server.go:NewServer` | Sweeps expired viewer leases every 5s |
| Pending deletion processor | `server.go:NewServer` | Retries Stripe cancellation + user deletion every 5min |
| Heartbeat reaper | `server.go:NewServer` | Stops sessions with no heartbeat >5min, every 60s |
| Caption audio upload limiter cleanup | `server.go:NewServer` | Sweeps stale upload state every 5min |
| Inactive account queueing | `main.go` | Queues 6-month-inactive free accounts for deletion, every 24h |
| Signal handler | `main.go` | Waits for SIGINT/SIGTERM |
| Rate limiter cleanup | `middleware.go:init()` | Evicts stale rate limit buckets every 5min |

### Relay goroutines (cmd/signal-relay)
| Goroutine | Location | Purpose |
|-----------|----------|---------|
| Host ping loop | `handleHost` | Sends WebSocket ping every 30s per host |
| Host read loop | `handleHost` | Reads answers from host WebSocket |
| Disconnect cleanup timer | `scheduleDisconnectedSessionStop` | 30min timer per disconnected host, then notifies API |
| Signal handler | `main` | Graceful shutdown |

### Client-side goroutines (internal/session, internal/webrtc)
| Goroutine | Location | Purpose |
|-----------|----------|---------|
| Daemon supervisor | `daemon.go:runDaemonSupervisor` | Watches child daemon, restarts on crash |
| Session maintenance | `heartbeat.go:runSessionMaintenance` | Heartbeat (60s) + log sync (15s) |
| WebRTC relay connection | `webrtc/server.go` | Maintains WebSocket to relay |
| Telemetry flusher | `webrtc/server.go` | Batches telemetry events (250ms interval, batch of 8) |
| Caption storage flusher | `webrtc/caption_storage.go` | Batches captions to API (2s interval, batch of 16) |
| RTCP read loop | per peer | Reads RTCP feedback for adaptive bitrate |
| Wake detector | `wake_*.go` | Detects sleep/wake for reconnection |
| Input dispatch | `input_dispatch.go` | Coalesces mouse moves |

### Shared state + synchronization
| Resource | Protection | Location |
|----------|-----------|----------|
| `Server.peers` | `peersMu` RWMutex | webrtc/server.go |
| `Server.controller` | `ctrlMu` Mutex | webrtc/server.go |
| `Server.ws` (relay WS) | `wsMu` Mutex | webrtc/server.go |
| `Server.offerMu` | Mutex | webrtc/server.go (serializes offer handling) |
| `Server.capMu` | Mutex | webrtc/server.go (capture state) |
| `hosts` map | `hostsMu` RWMutex | signal-relay/main.go |
| `hostsByIP` | `hostIPMu` Mutex | signal-relay/main.go |
| `host.answers` | `answerMu` Mutex | signal-relay/main.go |
| `host.conn` | `mu` Mutex | signal-relay/main.go |
| `Server.pendingJoins` | `pendingMu` Mutex | api/server.go |
| `Server.activeViewerLeases` | `activeViewerMu` Mutex | api/server.go |
| `captionAudioUploadRecent` | `captionAudioUploadMu` Mutex | api/captions.go |
| `captionAudioStreams` | `captionAudioStreamMu` Mutex | api/captions.go |
| Rate limiter clients | `limiter.mu` Mutex | api/middleware.go |
| `relayMetrics` | `mu` Mutex | signal-relay/main.go |

### Shutdown pattern
- `Server.done` channel (closed once via `stopOnce`)
- All background goroutines select on `<-s.done`
- `pendingJoinWG` WaitGroup for in-flight join operations
- HTTP server graceful shutdown with 5s context timeout

## 10. Persistence Patterns

- **Single writer**: `db.SetMaxOpenConns(1)` — serializes all writes through one connection
- **WAL mode**: Allows concurrent reads during writes
- **Transactions**: Used for migrations only (each migration in its own tx). Normal operations use direct ExecContext/QueryContext
- **No prepared statements**: All queries are inline SQL strings
- **UUID generation**: Custom `newUUID()` using crypto/rand (UUID v4 format)
- **Timestamps**: All stored as `DATETIME` strings in SQLite (ISO format via `datetime('now')`)
- **Retention**: Stopped sessions kept 180 days, then bulk-deleted. Legal acceptances have explicit `retain_until`.

## 11. External Integrations

### Cloudflare R2 (Caption/Audio Storage)
- **Package**: `internal/captionstorage`
- **Auth**: AWS Signature V4 (manual implementation, not AWS SDK)
- **Env vars**: `R2_ACCOUNT_ID`, `R2_ACCESS_KEY_ID`, `R2_SECRET_ACCESS_KEY`, `R2_BUCKET`, `R2_ENDPOINT`, `PINKY_CAPTION_STORAGE_PREFIX`
- **Operations**: PUT (upload chunks), GET (stream for replay), presigned URLs
- **Timeout**: 10s default HTTP client

### Cloudflare Turnstile (Bot Protection)
- **Package**: `internal/api/trial.go`
- **Purpose**: Protects trial account creation from abuse
- **Env var**: (implied `TURNSTILE_SECRET_KEY`)
- **Flow**: Client sends turnstile token → API verifies with Cloudflare → allows trial creation

### Stripe (Billing)
- **Package**: `internal/api/billing.go`
- **Env vars**: `STRIPE_SECRET_KEY`, `STRIPE_WEBHOOK_SECRET`, `PINKY_STRIPE_3DS_MODE`
- **Operations**: Create checkout sessions, cancel subscriptions, process webhooks (payment events, disputes)
- **3DS modes**: automatic, any (default), challenge
- **Webhook idempotency**: `processed_webhook_events` table prevents double-processing
- **Dispute handling**: Auto-collects evidence (checkout metadata, session history, legal acceptances)

### Resend (Email)
- **Package**: `internal/api/email.go`
- **Env var**: `RESEND_API_KEY`
- **Purpose**: Verification codes, password resets
- **From**: `Pinky <noreply@pinky.sh>`
- **Fallback**: Logs to console if no API key configured

### TURN Server (WebRTC relay)
- **Endpoint**: `/api/turn-credentials` returns TURN server credentials
- **Auth**: HMAC-SHA1 time-limited credentials (standard TURN REST API pattern)
- **Env vars**: `PINKY_TURN_URLS`, `PINKY_TURN_SECRET`
- **TTL**: Credentials valid for ICE config TTL (10min)

### Prometheus (Metrics)
- **Package**: `internal/api/observability.go`
- **Metrics**: `pinky_api_http_requests_total{method,route,status}`, `pinky_api_http_request_duration_seconds{method,route}`
- **Endpoint**: `GET /metrics` (admin-only)
- **Also exposes**: Go runtime metrics, process metrics, pprof endpoints

## 12. Build + Deploy

**No Dockerfile found** in repo root (likely deployed via direct binary copy).

**CI** (`.github/workflows/ci.yml`):
- Runs on: ubuntu-latest, windows-latest
- Tests: `go test ./internal/api ./internal/session ./internal/webrtc ./internal/screen ./cmd/signal-relay`
- Linting: golangci-lint v2.5.0
- Security: govulncheck
- Viewer integrity: SRI hash verification script

**Deployment** (`.github/workflows/deploy-production.yml`):
- Manual workflow dispatch with release ID + preprod session codes for gate
- SSH-based deployment to preprod and production hosts
- Audio/reconnect gate: requires passing macOS + Windows session tests on preprod
- Concurrency group: `production-release` (no cancel-in-progress)

**Build flags**: Version, Commit, BuildTime injected via `-ldflags` at build time. `ReleasePublicKey` for Ed25519 update verification.

**Dependencies** (from go.mod):
- `pion/webrtc/v4` — WebRTC stack
- `pion/opus` — Opus audio codec
- `gen2brain/x264-go` — H.264/VP8 video encoding
- `gen2brain/malgo` — Cross-platform audio I/O (miniaudio bindings)
- `gorilla/websocket` — WebSocket for relay signaling
- `mattn/go-sqlite3` — SQLite driver (CGo)
- `prometheus/client_golang` — Metrics
- `golang.org/x/crypto` — bcrypt, TLS
- `golang.org/x/term` — Terminal raw mode for interactive menus

## 13. Config (Environment Variables)

| Variable | Default | Used In |
|----------|---------|---------|
| `PINKY_JWT_SECRET` | (required) | API server JWT signing |
| `PINKY_DB_PATH` | `/opt/pinky-api/pinky.db` | SQLite database location |
| `PINKY_PORT` | `8080` | API server listen port |
| `PINKY_API` | `https://pinky.sh` | Client API base URL |
| `PINKY_RELAY_ADDR` | `:9000` | Relay listen address |
| `PINKY_RELAY_SECRET` | (empty=no auth) | Relay↔API shared secret |
| `PINKY_RELAY_WEB_DIR` | `relay/web` | Viewer static files |
| `PINKY_API_BASE` | `https://pinky.sh` | Relay→API base URL |
| `PINKY_RELAY_IP` | `167.71.175.146` | Default relay IP for sessions |
| `PINKY_ENV` | (empty=production) | Environment identifier |
| `PINKY_ENVIRONMENT_BANNER` | (empty) | Non-prod banner text |
| `PINKY_AUDIT_LOG` | `/opt/pinky-api/audit.log` | Audit log file path |
| `PINKY_TRUST_PROXY_CIDRS` | `127.0.0.0/8,::1/128` | Trusted proxy CIDRs |
| `PINKY_ADMIN_ACCOUNTS` | (empty) | `email:password,...` seed |
| `PINKY_ALLOW_LOCALHOST_ORIGINS` | `false` | Dev CORS relaxation |
| `PINKY_CAPTION_STORAGE` | `false` | Enable R2 caption storage |
| `PINKY_PINNED_CERT_FINGERPRINTS` | (hardcoded) | Additional TLS pins |
| `PINKY_TURN_URLS` | (implied) | TURN server URLs |
| `PINKY_TURN_SECRET` | (implied) | TURN credential secret |
| `PINKY_ENABLE_SYSTEM_MIC` | `false` | Enable system mic CLI |
| `PINKY_JWT_KEY_ID` | `current` | JWT key identifier |
| `PINKY_JWT_ISSUER` | `pinky.sh` | JWT issuer claim |
| `PINKY_JWT_AUDIENCE` | `pinky-api` | JWT audience claim |
| `PINKY_JWT_PREV_KEYS` | (empty) | Previous JWT keys for rotation |
| `R2_ACCOUNT_ID` | | Cloudflare account |
| `R2_ACCESS_KEY_ID` | | R2 access key |
| `R2_SECRET_ACCESS_KEY` | | R2 secret |
| `R2_BUCKET` | | R2 bucket name |
| `R2_ENDPOINT` | (derived from account ID) | R2 endpoint |
| `STRIPE_SECRET_KEY` | | Stripe API key |
| `STRIPE_WEBHOOK_SECRET` | | Stripe webhook verification |
| `RESEND_API_KEY` | | Email service key |

## 14. Telemetry

**API-side** (`internal/api/telemetry.go`):
- In-memory telemetry store tracking peer transport paths (direct vs relay/TURN)
- First-frame latency samples (1h window)
- Reconnect event counting
- Admin realtime dashboard: direct/relay peer ratio, TURN usage %, avg first frame ms, reconnect events

**API-side audit log** (`internal/api/middleware.go`):
- Structured JSON via `slog.NewJSONHandler`
- Fields: `request_id`, `method`, `path`, `status`, `user`, `ip`, `code`, `user_agent`
- Buffered writer (32KB) with flush on newline
- Path: `PINKY_AUDIT_LOG` (default `/opt/pinky-api/audit.log`)

**Relay-side** (`cmd/signal-relay`):
- Structured JSON logging via `slog`
- Metrics: inactive session rejects, offer-no-host, host-unreachable, host-timeouts (1h sliding window)
- Exposed via `/signal/metrics` (bearer auth required)

**Host-side** (`internal/webrtc/server.go`):
- Telemetry event queue (128 capacity, batch of 8, flush every 250ms)
- Event types: `transport`, `peer_closed`, `first_frame`, `reconnect`
- Posted to API: `POST /api/sessions/{code}/telemetry`

**Prometheus metrics** (API server):
- `pinky_api_http_requests_total` (counter, labels: method, route, status)
- `pinky_api_http_request_duration_seconds` (histogram, labels: method, route)
- Go runtime + process collector metrics

## 15. Frontend

**API server frontend** (`web/`):
- Server-rendered HTML templates: `landing.html`, `dashboard.html`, `login.html`, `download.html`, `admin.html`, `deployments.html`, `terms.html`, `privacy.html`, `404.html`
- Static JS: `api-pages.js` (client-side dashboard logic)
- No framework — vanilla HTML/JS with server-side rendering
- CSP: strict, allows Cloudflare challenges iframe

**Viewer frontend** (`relay/web/`):
- Vanilla JavaScript modules (no framework, no build step)
- Files: `index.html`, `viewer-core.js`, `viewer-session.js`, `viewer-input.js`, `viewer-audio.js`, `viewer-captions.js`, `viewer-controls.js`, `audio-worklet.js`
- Communicates with relay via: HTTP POST for SDP offers, WebRTC data channels for control/captions
- Audio: Web Audio API with AudioWorklet for viewer-to-host audio
- Input: keyboard/mouse events sent over data channel
- Captions: Web Speech API for speech-to-text, sent over data channel

## 16. Notable Code Patterns

1. **Supervisor pattern** (`internal/session/daemon.go`): Three-level process hierarchy — foreground CLI → supervisor (detached) → daemon. Supervisor auto-restarts daemon on crash with exponential backoff.

2. **TLS certificate pinning** (`internal/session/pinning.go`): Custom `VerifyConnection` callback on TLS config pins SHA256 fingerprint of pinky.sh cert. Only applies to production hostnames, custom/self-hosted endpoints use normal PKI.

3. **Token bucket rate limiter** (`internal/api/middleware.go`): In-memory per-IP rate limiting with automatic refill. Caps tracked clients at 10,000 to prevent memory exhaustion.

4. **Relay auth tokens** (`internal/relayauth/token.go`): Lightweight HMAC-SHA256 tokens (not JWT) — just base64(payload).base64(signature). Minimal overhead for high-frequency relay operations.

5. **Adaptive bitrate** (`internal/webrtc/adaptive.go`): GCC-based congestion control with smoothed estimates, tier-based quality levels, upgrade eligibility cooldowns, and viewer feedback relief.

6. **Caption storage queue** (`internal/webrtc/caption_storage.go`): Buffered channel with drop-on-full semantics. Logs dropped count at powers of 100. Background flusher batches to API.

7. **Schema migration with legacy reconciliation** (`internal/db/db.go`): Handles databases created before the migration system existed by detecting legacy tables and synthesizing migration records.

8. **Reusable session codes**: Codes persist across restarts via `~/.pinky/reusable_code` file and `state.json` PersistCode field. Preserved during updates.

9. **Host disconnect grace** (signal relay): 30-minute grace period before notifying API that a host disconnected. Prevents viewer "session not found" during Wi-Fi roaming, sleep/wake, or relay deploys.

10. **CSRF double-submit cookie**: Cookie-based auth requires matching `X-CSRF-Token` header for unsafe methods. Token stored in non-HttpOnly `pinky_csrf` cookie for JS access.

## 17. Questions + Ambiguities

1. **No Dockerfile**: Deployment appears to be direct binary SCP to VPS hosts (evidenced by SSH-based deploy workflows). No containerization visible.

2. **internal/publisher**: Package exists but was not deeply read. Likely related to media track publishing in WebRTC context.

3. **Stripe price IDs**: The `stripe_price_id` column exists in plans table but no seed data populates it — likely set via admin or env.

4. **TURN server**: Credentials are generated but the actual TURN server infrastructure is external (not in this repo).

5. **RustDesk references**: `internal/session/rustdesk.go` exists — suggests historical or alternative remote desktop backend, but WebRTC is the primary path now.

6. **x264 vs VP8**: go.mod includes x264-go but WebRTC server references VP8 payloader. Both codecs may be supported depending on negotiation.

7. **Team billing**: Teams share the owner's plan. No per-seat billing visible — seat limits are plan-based constants.

## 18. Summary: What Pinky IS (in 10 sentences)

Pinky is a remote desktop sharing product built as a Go monorepo with three deployable binaries: an API server, a signaling relay, and a host CLI client. The host CLI captures screen (via platform-native APIs) and system audio, encodes them as VP8/Opus, and streams to viewers over WebRTC peer connections brokered through the signaling relay. Viewers connect via a vanilla JavaScript web app served by the relay, receiving video/audio and sending keyboard/mouse input back over WebRTC data channels. The API server manages user accounts (email/password with bcrypt), subscription billing (Stripe), session lifecycle (create/heartbeat/stop), and caption storage (Cloudflare R2). Sessions are identified by short alphanumeric codes (8 chars) and support one controller at a time with multiple viewers watching. The system includes a live caption pipeline where viewers' browsers run Web Speech API, send transcriptions to the host via data channel, which displays them as an overlay and optionally stores them for replay. The host client runs as a supervised daemon with automatic crash recovery, heartbeat-based liveness detection, and graceful handling of sleep/wake cycles. Authentication uses custom HMAC-SHA256 tokens with 30-minute access / 30-day refresh TTLs, plus TLS certificate pinning for the production API. The product has tiered pricing (trial/starter/pro/unlimited) with plan-enforced limits on concurrent sessions, devices, viewers, and caption hours. Deployment is SSH-based to VPS hosts with a manual promotion workflow requiring passing audio/reconnect integration tests on preprod before production release.

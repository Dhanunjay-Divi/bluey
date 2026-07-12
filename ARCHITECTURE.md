# Bluey Architecture

> Historical architecture snapshot. It records the `0.1.0` system and rollout
> plan, not the current release matrix. As of July 12, 2026, the public signed
> manifest reports `0.1.99` with `darwin-arm64` and `windows-x86_64` artifacts.
> Use `README.md`, `INSTALL.md`, and `https://bluey.sh/latest.json` for current
> product, platform, and install claims.

> **Source of truth for system shape, server topology, deploy paths,
> platform status, and the staged monetization rollout.**
>
> Living doc. Updated each round that lands cross-layer changes.
> Last updated: 2026-05-19, post v0.1.0 GA on `c34592a`.

This is the first doc to read. Everything else (`SERVER-REFERENCE.md`,
`DECISIONS.md`, `AGENT-HANDOFF.md`, `FUTURE-IMPLEMENTATIONS.md`,
`docs/*RUNBOOK*.md`, round docs) cross-references back to here.

---

## 1. System diagram

```
                                                                       LAYER 3
+-------------------------------------------------------------------+ PRODUCT
|  bluey-server (FUTURE: separate repo, not yet stood up)           | SERVER
|  ----------------------------------------------------------------  | (cloud,
|   POST /auth/{signup,login,refresh,reset}                          |  paid)
|   POST /billing/{checkout,webhook}                                 |
|   GET  /account/me                                                 |
|   POST /router/complete   (managed Auto Router endpoint)           |
|   GET  /admin/* (Bluey-team only)                                  |
+-------------------------------------------------------------------+
                              ^   HTTPS, auth tokens
                              |
+-------------------------------------------------------------------+ LAYER 2
|  bluey distribution (FUTURE: not yet stood up; design = R14.8)    | DISTRIBUTION
|  ----------------------------------------------------------------  | (cloud,
|   GET /install                                                     |  public)
|   GET /install.sh                                                  |
|   GET /install.ps1                                                 |
|   GET /latest.json                                                 |
|   GET /downloads/v<ver>/                                           |
|   GET /admin/health                                                |
+-------------------------------------------------------------------+
                              ^   HTTPS, public, no auth
                              |
+-------------------------------------------------------------------+ LAYER 1
|  bluey local client (this repo, SHIPPING in v0.1.0)               | LOCAL
|  ----------------------------------------------------------------  | CLIENT
|   bluey-cli  ──┐                                                   |
|                ├─ NDJSON IPC ─►  bluey-overlay-macos (capture-     |
|   bluey-daemon ┘                  excluded NSWindow pill UX)       |
|   bluey-whisper-macos (whisper.cpp on-device transcription)        |
|   bluey-audio-macos   (system + mic capture via CoreAudio)         |
|                                                                    |
|  Workspace crates:                                                 |
|   cue-cli, cue-daemon, cue-llm, cue-router, cue-rag,               |
|   cue-stealth, cue-dashboard, cue-core                             |
+-------------------------------------------------------------------+
```

Three distinct layers. Each is a separate codebase, separate deploy
story, separate tradeoffs. Conflating them produces over-engineered
infra you won't use OR under-engineered infra that blocks
monetization. Keep them separate.

---



---

## 1.5. What runs where (v0.1 reality)

A common question: "do we need a server for the models?" Bluey today
runs the **classifier + routing + storage** on the laptop, but **the
actual model inference is in the cloud** — at the upstream provider
(Anthropic / OpenAI) — using the user's BYOK API key.

| Component | Location v0.1 (dev BYOK) | Location v0.2+ (managed-only) | Local fallback (offline / privacy) |
|---|---|---|---|
| LLM inference | provider cloud, user's API key | provider cloud via `bluey-server` (Bluey's API key) | local Ollama on laptop |
| Embeddings | OpenAI cloud, user's API key | provider cloud via `bluey-server` | local sentence-transformer on laptop (R14.x) |
| RAG vector storage + search | laptop SQLite | laptop SQLite + optional cloud RAG (cross-device) | always laptop |
| Whisper STT (audio → text) | **laptop** (whisper.cpp) | cloud STT via `bluey-server` (Deepgram / Realtime) | local whisper.cpp on laptop |
| Audio capture (system + mic) | **laptop** (CoreAudio) | **laptop** | **laptop** |
| Auto Router classification | **laptop** (heuristic) | laptop heuristic + optional server-side tiny-model | always laptop heuristic |
| Overlay + daemon + dashboard | **laptop** | **laptop** | **laptop** |

**Key product decision (2026-05-19):** Bluey is **managed-only**. The
customer's BYOK keys are NOT exposed in the production UI. Every paid
inference goes through `bluey-server` so Bluey is the billing entity.
**Local models (whisper.cpp, Ollama) exist as an offline / privacy
fallback only** — the customer is still using a paid Bluey account
during fallback; the inference cost just shifts to their hardware. See
`DECISIONS.md` for the rationale.

**Practical implication for a customer's laptop:** modern Macs handle
the local pieces fine. Whisper tiny.en runs in real-time at <10% CPU
on Apple Silicon. RAG bounded-heap top-k is sub-second up to 50k
chunks. The daemon + overlay use <100 MB RAM. The cloud pieces are
just network I/O — bytes in, bytes out.

**Practical implication for monetization:** in v0.1 the user pays the
LLM provider directly (BYOK). Bluey gets nothing. The Auto Router
crate (`cue-router`) was built so a future `BlueyManagedProvider`
implementation can dispatch through `bluey-server`, which holds
Bluey-owned API keys, charges the customer, and pays the upstream
providers — taking a margin. **That is the monetization handle.**
See Section 5 (Layer 3 product server) and Section 6 (Auto Router
plug-points).

## 2. Servers (target topology)

> **State 2026-05-19:** No Bluey servers are stood up yet. v0.1.0 is
> distributed by manual file transfer. The table below is the **target**
> shape after R14.8 (distribution) and R14.9 (product server scaffold)
> land.
>
> Bluey infrastructure is **separate from Pinky** per user direction
> 2026-05-19. They are two products; they share no servers.

| Server | IP / DNS | Role | Stack | Status |
|---|---|---|---|---|
| Distribution | TBD (path C: a fresh DigitalOcean droplet) | binary downloads, install scripts, version manifest | nginx + LetsEncrypt + static files | not provisioned |
| Product API (preprod) | TBD | auth, billing, license check, managed Auto Router endpoint, admin | Go + SQLite + Caddy (mirrors Pinky pattern) | not provisioned |
| Product API (prod) | TBD | same as preprod, live data | Go + SQLite + Caddy | not provisioned |

Per-environment droplet count after rollout: 1 distribution + 1 product
preprod + 1 product prod = 3 droplets (+ 2 if we mirror Pinky's API/Relay
split, but Bluey has no relay-shaped concern today).

See `SERVER-REFERENCE.md` for the canonical paths once these are stood
up.

---

## 3. Layer 1 — Local client (this repo)

**State:** v0.1.0 GA shipped on 2026-05-19. macOS arm64 + universal
binary (arm64+x86_64 lipo). BYOK (user supplies own provider keys).

**Workspace crates:**
- `cue-core` — shared types (CueCard, OverlayCommand, OverlayEvent, OverlayUiState)
- `cue-cli` — `bluey on/off` and friends
- `cue-daemon` — supervisor, IPC, audio pipeline, transcription orchestration
- `cue-llm` — provider impls (Anthropic, OpenAI, Ollama)
- `cue-router` — Bluey Auto Router (classifier + policy + speculative dispatch)
- `cue-rag` — local SQLite vector store with bounded-heap top-k
- `cue-stealth` — process masquerading + anti-debug helpers
- `cue-dashboard` — Tauri UI (developer tool, NOT shipped in v0.1.0)

**Native helpers:**
- `native/macos/cue-overlay`  — Swift NSWindow pill UX
- `native/macos/cue-audio`    — CoreAudio capture
- `native/macos/cue-whisper`  — whisper.cpp via SwiftPM
- `native/windows/*`          — equivalents (NOT shipped; see `docs/rounds/PHASE-3-WINDOWS-BRIEF-FOR-CODEX.md`)

**Future additions:**
- `cue-cloud-client` — when Layer 3 lands; thin HTTP client for auth +
  managed Auto Router dispatch.

---

## 4. Layer 2 — Distribution server (forward-looking)

**State:** architecture decided (R14.8); implementation pending user
pick on path A/B/C and domain.

**Job:** serve binaries to anyone with a URL. No auth. No customer state.
Not coupled to monetization.

**URL contract (stable across implementations):**

```
GET /install            templated bash
GET /install.sh         alias for /install
GET /install.ps1        templated PowerShell (Windows)
GET /latest.json        release manifest (version + URLs + sha256)
GET /downloads/v<ver>/  immutable versioned release dir
GET /admin/health       liveness check
```

**Three paths to implement, ordered by complexity:**

| Path | When | Effort |
|---|---|---|
| **C — nginx + static + droplet** | now (v0.1 internal testing) | 1.5–2 hr |
| **A — standalone Go server** | when distribution needs templated installers, A/B install scripts, etc. | 4–6 hr + deploy |
| **B — CDN + object store** | global edge cache becomes the bottleneck | 2–3 hr if account exists |

See `docs/BLUEY-DISTRIBUTION-ARCHITECTURE.md` for the full path
comparison. URL contract is stable across paths so a Path-C → Path-A
migration is a routing change, not a client change.

---

## 5. Layer 3 — Product server (forward-looking)

**State:** REQUIRED for v0.2 launch (per 2026-05-19 no-BYOK decision).
Scaffold tracked as R14.9 in `docs/rounds/PHASE-3-ROUND-14-PLAN.md`. v0.1
ships without it (dev BYOK only); v0.2 cannot ship without it because
managed-only is now the default.

**Job:** the actual paid SaaS. Sells the value Bluey wraps the user's
provider keys with (or replaces them entirely with Bluey-managed keys).

**Endpoints (target):**

```
POST /auth/signup, /auth/login, /auth/refresh, /auth/reset
POST /billing/checkout              (hosted credit reload checkout)
POST /billing/webhook               (billing-provider webhook handler)
GET  /account/me                    (license + plan)
POST /router/complete               (managed Auto Router endpoint)
GET  /admin/customers               (Bluey-team only)
```

**Lives in a separate repo (`bluey-server`).** Different deploy cadence,
different security review surface, **same language as the rest of Bluey
(Rust).** Mirrors Pinky's API droplet operational shape but uses Rust
so the team is not maintaining two stacks for one product family.

**Why this is necessary for monetization:** if the user supplies their
own OpenAI/Anthropic keys, what are they paying Bluey for? The honest
answer: nothing, until Bluey holds the keys via the managed Auto Router
endpoint. That's the monetization handle the Auto Router crate
(`cue-router`) was designed to plug into.

---

## 6. Auto Router monetization plug-points

`cue-router` (this repo) already supports monetization without rewriting:

```rust
// cue-router/src/policy.rs
pub trait RoutingPolicy { fn route(&self, ...) -> ProviderRoute; }

// v0.1 dev mode (BYOK, internal-only, NOT exposed to customers):
StaticPolicy::defaults()  // hard-coded lane → provider, daemon
                          // reads keys from env/keyring directly

// v0.2 production default (managed-only, what customers see):
ManagedPolicy::from_bluey_account(token)
//   - daemon authenticates with bluey-server using the customer's
//     Bluey account token (stored in keyring after the first `bluey on` sign-in)
//   - bluey-server picks lane + provider + model
//   - bluey-server uses Bluey-owned upstream API keys
//   - bluey-server enforces tenant budget + rate limits
//   - bluey-server meters usage for billing
//
// Local fallback (offline / privacy-only):
LocalFallbackPolicy::default()  // routes all lanes to local Ollama
                                // + local whisper.cpp; customer still
                                // using a paid Bluey account
```

The `SpeculativeProvider` trait is the integration point. The dashboard's
`ProviderRegistry` impls it today; tomorrow a `BlueyManagedProvider` impl
dispatches over HTTPS to the product server.

**Implication:** `cue-router` does NOT need a rewrite when monetization
launches. Just add a new `Policy` impl + a new `Provider` impl that
both know how to talk to the cloud service.

---

## 7. Staged rollout (Timeline Y)

### Stage 0 — v0.1.0 GA (DONE 2026-05-19)

- macOS arm64 + universal binary, smoke-tested, tagged.
- Auto Router shipping with classifier observation + speculative
  default-ON.
- BYOK only. No Layer 2 or Layer 3 yet.

### Stage 1 — distribution server (NEXT)

- Pick Path A/B/C + domain.
- Stand up the chosen path; deploy artifact.
- Update `scripts/install.sh` to discover via `latest.json`.
- Internal testers run `curl bluey.dev/install.sh | sh`.
- **Time to land:** 1.5–6 hr.

### Stage 2 — product server (BLOCKING for v0.2 launch)

- New repo `bluey-server`, **Rust** (per 2026-05-19 lang decision).
- All endpoints required for v0.2 paid launch (managed Auto Router
  endpoint, auth, account-credit billing, license check). No more "scaffold + flip
  later" — the no-BYOK decision (2026-05-19) makes Stage 2 the gating
  dependency for v0.2.
- Billing-provider sandbox stays enabled through Stage 3 dogfooding; production
  credentials are flipped on at Stage 4.
- **Time to land:** ~1–2 weeks dedicated work.

### Stage 3 — daemon talks to product server (this repo)

- New `cue-cloud-client` crate.
- `bluey on` opens sign-in when needed; `bluey off` stops the product.
  Support-only account commands remain hidden.
- `cue-daemon` calls cloud-client for auth + license + managed router
  dispatch.
- Dev-mode BYOK remains gated for internal testing only (BYOK is NOT exposed in the production UI per DECISIONS.md no-BYOK decision).
- **Time to land:** ~3–5 days.

### Stage 4 — paid alpha launch

- Flip billing provider to production mode.
- First paying customers.

### Stage 5 — public GA

- Open signup, marketing, pricing page, support docs, privacy/legal.
- Out of scope for v0.1.x; this is v0.3.

---

## 8. Pending decisions (user-owned)

| Decision | Options | Default if you don't pick |
|---|---|---|
| Distribution path | A (Go server) / B (CDN) / C (nginx static) | C — smallest infra to unblock |
| Distribution domain | `bluey.dev` / other / "no domain yet" | "raw IP for v0.1 testing" |
| Monetization timeline | X (slow) / **Y (medium, recommended)** / Z (aggressive) | Y |
| Product server language | **Rust (locked-in 2026-05-19)** | Rust |
| Managed Router endpoint | proxy upstreams as-is / Bluey-specific protocol | proxy as-is for v0.2 |

---

## 9. Update protocol

- **Every round** that lands changes touching cross-layer concerns,
  update Sections 1–3 in this file as part of the same commit.
- **Every round** with a new pending decision, append to Section 8.
- **Every round** with a new monetization plug-point, update Section 6.
- This doc is the home for "where are we headed and why" — keep it
  durable enough that a future Kiro/Codex agent picking up the work
  six rounds from now can orient quickly.

## 10. Cross-references

- `SERVER-REFERENCE.md` — exact paths on each server once stood up.
- `DECISIONS.md` — historical decisions + dead ends ("what not to retry").
- `AGENT-HANDOFF.md` — operational notes for the next agent.
- `FUTURE-IMPLEMENTATIONS.md` — canonical tracker for deferred work.
- **`docs/HOW-IT-WORKS.md` — end-to-end v0.2 customer flow (signup,
  login, request, billing, hard-stop, fallback).**
- **`docs/PRICING-MODEL.md` — locked pricing tiers, per-question
  cost table, usage profiles, UI surfaces.**
- `docs/AUTO-ROUTING-USP.md` — Auto Router product framing.
- `docs/BLUEY-DISTRIBUTION-ARCHITECTURE.md` — Layer 2 path comparison.
- `docs/PRODUCTION-READINESS.md` — what ships vs what's pending matrix.
- `docs/release/RELEASE-v0.1.0.md` — release notes.
- `docs/rounds/PHASE-3-ROUND-14-PLAN.md` — current-round work items.

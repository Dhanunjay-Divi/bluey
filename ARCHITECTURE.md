# Bluey Architecture

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
- `native/windows/*`          — equivalents (NOT shipped; see `docs/work/PHASE-3-WINDOWS-BRIEF-FOR-CODEX.md`)

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

**State:** scaffolded as R14.9, parked until monetization is greenlit.

**Job:** the actual paid SaaS. Sells the value Bluey wraps the user's
provider keys with (or replaces them entirely with Bluey-managed keys).

**Endpoints (target):**

```
POST /auth/signup, /auth/login, /auth/refresh, /auth/reset
POST /billing/checkout              (Stripe checkout session)
POST /billing/webhook               (Stripe webhook handler)
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

// Today (v0.1 BYOK):
StaticPolicy::defaults()  // hard-coded local lane → provider mapping

// Future (v0.2 paid):
ManagedPolicy::from_bluey_account(token)
//   asks bluey-server which provider/model to use
//   bluey-server enforces tenant budget + rate limits
//   bluey-server returns a ProviderRoute pointing the daemon at
//   bluey-server itself, which proxies to the upstream provider
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

### Stage 2 — product server scaffold (parallel with Stage 1)

- New repo `bluey-server`, Go (mirrors Pinky operational pattern).
- Endpoints stubbed; Stripe in test mode.
- No real billing yet.
- **Time to land:** ~1 week dedicated work.

### Stage 3 — daemon talks to product server (this repo)

- New `cue-cloud-client` crate.
- `bluey login` / `bluey logout`.
- `cue-daemon` calls cloud-client for auth + license + managed router
  dispatch.
- BYOK still works when no account is logged in.
- **Time to land:** ~3–5 days.

### Stage 4 — paid alpha launch

- Flip Stripe to live mode.
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
- `docs/AUTO-ROUTING-USP.md` — Auto Router product framing.
- `docs/BLUEY-DISTRIBUTION-ARCHITECTURE.md` — Layer 2 path comparison.
- `docs/PRODUCTION-READINESS.md` — what ships vs what's pending matrix.
- `docs/release/RELEASE-v0.1.0.md` — release notes.
- `docs/work/PHASE-3-ROUND-14-PLAN.md` — current-round work items.

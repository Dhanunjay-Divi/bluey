# Bluey Architecture (forward-looking)

> **What this doc is:** the source of truth for how Bluey's architecture should
> evolve from v0.1.0 (local-first BYOK) toward a paid commercial product.
> Captures the layered server model, monetization plug-points, and the
> staged rollout that gets us to revenue without overcommitting infra
> spend before we have user-validated demand.
>
> **Living doc:** updated each round as decisions land.
> Last updated: 2026-05-19, post v0.1.0 GA on `c34592a`.

---

## 1. Layered architecture

Bluey has three distinct layers. Each layer has a separate codebase, separate
deploy story, and separate tradeoffs. Conflating them produces
over-engineered infra you won't use or under-engineered infra that blocks
monetization. Keep them separate.

```
+------------------------------------------------------------+
|  LAYER 3 — PRODUCT SERVER (cloud, monetization)            |
|    auth + Stripe + license + managed Auto Router endpoint  |
|    cloud RAG + sync + account/admin dashboards             |
|    NEW REPO: bluey-server (Go or Rust, your call)          |
+------------------------------------------------------------+
                       ^ HTTPS, auth tokens
                       |
+------------------------------------------------------------+
|  LAYER 2 — DISTRIBUTION SERVER (cloud, public read)        |
|    /install, /install.sh, /install.ps1, /latest.json,      |
|    /downloads/v<ver>/                                      |
|    OPTION C (recommended v0.1): nginx + static + droplet   |
|    OPTION A (later): standalone bluey-server Go service    |
|    OPTION B (later): CDN + object store                    |
+------------------------------------------------------------+
                       ^ HTTPS, public, no auth
                       |
+------------------------------------------------------------+
|  LAYER 1 — LOCAL CLIENT (this repo, what users run)        |
|    bluey, bluey-daemon, bluey-overlay-macos,               |
|    bluey-whisper-macos, dashboard (Tauri, dev only)        |
|    cue-router (Auto Router classifier + speculative)       |
|    cue-llm (Anthropic / OpenAI / Ollama providers)         |
|    cue-rag (local SQLite vector store)                     |
+------------------------------------------------------------+
```

### Layer 1 — Local client (this repo)

**State:** v0.1.0 GA shipped. macOS arm64 + universal binary. BYOK (user
supplies their own Anthropic/OpenAI keys). Pill-first overlay UX. Local
SQLite for transcripts + RAG. Auto Router classifier observable, speculative
draft+final routing default-ON.

**Stays in this repo.** All workspace crates (`cue-cli`, `cue-daemon`,
`cue-llm`, `cue-router`, `cue-rag`, `cue-stealth`, `cue-dashboard`, etc.)
plus the native helpers (`native/macos/cue-{overlay,audio,whisper}`,
`native/windows/cue-{overlay,audio,whisper}`).

**Future additions to this layer:**
- Layer 3 client integration: when product server lands, add an HTTP client
  in `cue-daemon` that talks to Bluey cloud (auth handshake, license check,
  managed router dispatch when a Bluey account is logged in).
- Cloud-vs-local toggle: `bluey login` switches the daemon to managed
  routing; `bluey logout` falls back to BYOK.

### Layer 2 — Distribution server (separate, public)

**State:** architecture chosen (R14.8 doc); implementation pending user
pick on path A/B/C and domain.

**Job:** serve binaries to anyone with a URL. No auth. No customer state.
Not coupled to monetization.

**URL contract (stable across implementations):**

```
GET /install              templated bash
GET /install.sh           alias
GET /install.ps1          templated PowerShell
GET /latest.json          release manifest (version + URLs + sha256)
GET /downloads/v<ver>/    immutable versioned release dir
GET /admin/health         liveness check
```

**Three paths to implement, ordered by complexity:**

| Path | When | Effort |
|---|---|---|
| **C — nginx + static + droplet** | now (v0.1 internal testing) | 1.5–2 hr |
| **A — standalone Go server** | when distribution needs to evolve (signed installers, A/B-able install scripts) | 4–6 hr + 2 hr deploy |
| **B — CDN + object store** | global edge cache becomes the bottleneck | 2–3 hr (account already exists) |

**Recommendation:** Path C now. Migrate to A or B when Path C's limits hit
(single region, single droplet, manual cert renewal).

### Layer 3 — Product server (separate, monetization)

**State:** scaffolded as R14.9, parked until monetization is greenlit.

**Job:** the actual paid SaaS. Sells the value Bluey wraps the user's
provider keys with (or replaces them entirely with Bluey-managed keys).

**Features by milestone:**

| Capability | v0.2 (paid alpha) | v0.3 (public GA) |
|---|---|---|
| Auth (signup, login, refresh, password reset, device registration) | ✓ | ✓ |
| Stripe (test mode → live) | test → live | live |
| Managed Auto Router endpoint (Bluey-owned API keys, customer pays Bluey) | ✓ | ✓ |
| License/entitlement check (daemon phones home periodically) | ✓ | ✓ |
| Account dashboard (web, customer-facing) | ✓ | ✓ |
| Admin dashboard (Bluey-team only) | ✓ | ✓ |
| Cloud RAG with citations and tenant scoping | optional | ✓ |
| Encrypted transcript sync across devices | — | ✓ |
| Team plans (shared transcripts, SSO, audit log) | — | future |
| SOC2 / data residency | — | future |

**Lives in a separate repo (`bluey-server`).** Reasons:
- Different deploy cadence (Bluey-the-app ships fortnightly; cloud
  service ships daily once live).
- Different security review surface (server holds API keys, customer
  PII, billing data).
- Different language is fine (Go matches Pinky pattern; Rust if you want
  to share types via Protobuf or wire types from `cue-core`).
- Local Bluey doesn't need to compile the server's billing code.

---

## 2. Monetization plug-points

The Auto Router (`cue-router`) is already built in a way that supports
monetization without rewriting:

```rust
// cue-router/src/policy.rs
pub trait RoutingPolicy { fn route(&self, ...) -> ProviderRoute; }

// Today (v0.1 BYOK):
StaticPolicy::defaults()  // hard-coded local lane → provider mapping

// Future (v0.2 paid):
ManagedPolicy::from_bluey_account(token)
//   asks bluey-server which provider/model to use
//   bluey-server enforces tenant budget + rate limits
//   bluey-server returns ProviderRoute with bluey-server URL as
//   the provider_name; the daemon dispatches to bluey-server which
//   proxies to the real upstream provider
```

The `SpeculativeProvider` trait (`cue-router/src/speculative.rs::SpeculativeProvider::provider_for`)
is the integration point. Today the dashboard's `ProviderRegistry` impls
this; tomorrow a `BlueyManagedProvider` impl can dispatch over HTTPS to
the product server.

**Implication:** cue-router does NOT need a rewrite when monetization
launches. Just add a new `Policy` impl + a new `Provider` impl that
both know how to talk to the cloud service.

---

## 3. Staged rollout (Timeline Y, recommended)

### Stage 0 (DONE) — v0.1.0 GA

- macOS arm64 + universal binary built, smoke-tested, tagged.
- Auto Router shipping with classifier observation + speculative dispatch
  default-ON.
- BYOK only.

### Stage 1 (NEXT) — distribution server live for internal testing

- Pick distribution path (A/B/C) + domain.
- Stand up the chosen path; deploy artifact to it.
- Update `scripts/install.sh` to discover via `latest.json`.
- Internal testers run `curl bluey.dev/install.sh | sh` (or whatever
  domain we use).
- **Time to land:** 1.5–6 hr depending on path picked.

### Stage 2 — product server scaffold (parallel)

- New repo `bluey-server` (Go, mirrors Pinky pattern, but Bluey-only).
- Endpoints:
  - `POST /auth/signup`, `POST /auth/login`, `POST /auth/refresh`
  - `POST /billing/checkout` (Stripe test mode)
  - `POST /billing/webhook` (Stripe webhook handler)
  - `GET /account/me` (license + plan)
  - `POST /router/complete` (managed Auto Router endpoint stub, returns
    a pass-through to upstream provider; later this is where budget
    capping + provider selection happens server-side)
  - `GET /admin/customers` (Bluey-team only)
- Stripe stays in test mode; no real billing yet.
- Deploy alongside Pinky on the same DigitalOcean account but different
  droplet (or ECS task / Fly.io app — pick what's familiar).
- **Time to land:** ~1 week dedicated work.

### Stage 3 — daemon talks to product server (Bluey-app side)

- New `cue-cloud-client` crate in this repo.
- `bluey login` / `bluey logout` flows.
- `cue-daemon` uses cloud-client to fetch auth token, refresh, license
  check.
- `cue-router::ManagedPolicy` impl asks server which lane to use.
- `cue-router::BlueyManagedProvider` dispatches through server.
- BYOK still works when no account is logged in.
- **Time to land:** ~3–5 days.

### Stage 4 — paid alpha launch

- Flip Stripe to live mode.
- Sign up first paying customers.
- Watch metrics.
- **Time to land:** 1 day flip + however long it takes you to find users.

### Stage 5 — public GA

- Open signup, marketing, pricing page, support docs, privacy/legal pages.
- **Out of scope for v0.1.x; this is v0.3.**

---

## 4. Decisions still pending (user-owned)

| Decision | Options | Default if you don't pick |
|---|---|---|
| Distribution path | A (Go server) / B (CDN) / C (nginx static) | C — smallest infra to unblock |
| Distribution domain | `bluey.dev` / other / "no domain yet" | "no domain, raw IP for now" |
| Monetization timeline | X (slow), Y (medium, recommended), Z (aggressive) | Y |
| Product server language | Go (Pinky pattern) / Rust (workspace fit) | Go (your team knows the Pinky pattern, faster to operate) |
| Managed Router endpoint shape | proxy upstreams as-is / Bluey-specific protocol | proxy as-is for v0.2; Bluey-specific later |

I'll proceed on the defaults if you go silent. Tell me explicitly to
override any of them.

---

## 5. What this doc is NOT

- **NOT a roadmap** — see `docs/ROADMAP.md`.
- **NOT a release plan** — see `docs/release/RELEASE-v0.1.0.md` and per-round
  plans under `docs/work/`.
- **NOT the production-readiness matrix** — see `docs/PRODUCTION-READINESS.md`.
- **NOT the Auto Router USP framing** — see `docs/AUTO-ROUTING-USP.md`.

This doc cross-references those; it does not replace them.

---

## 6. Update protocol

- **Every round** that lands changes touching cross-layer concerns
  (anything beyond Layer 1 alone), update Section 1 and Section 3 in
  this file as part of the same commit.
- **Every round** with a new pending decision, append to Section 4.
- **Every round** with a new monetization plug-point, update Section 2.
- This doc is the home for "where are we headed and why" — keep it
  durable enough that a future Kiro/Codex agent picking up the work
  six rounds from now can orient quickly.

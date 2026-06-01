# Bluey Future Implementations

> **Canonical tracker for deferred work.** Mirrors Pinky's
> `FUTURE-IMPLEMENTATIONS.md` shape but Bluey-only.
>
> If an item is "we should do this but not now," it lives here. If an
> item is in active development, it lives in the corresponding
> `docs/rounds/PHASE-3-ROUND-N-PLAN.md`. New items get added here; items
> get **removed** when they ship (linked to the commit that landed
> them).
>
> Last updated: 2026-05-19, post v0.1.0 GA.

---

## Layer 1 — Local client (this repo)

### R14.1 — sqlite-vec / usearch ANN for RAG

**Where deferred:** R12 carry-over → R13 → R14.1.
**Why deferred:** the bounded-heap top-k landed in R13.3 step 1 gives
~37 ms at 10k chunks / 1536-dim. Past 50k chunks per database the scan
itself dominates; that's when we want a real ANN index.
**Estimate:** 1 day.
**Recommended approach:** usearch (pure Rust, sidecar file) — see
`docs/rounds/PHASE-3-ROUND-14-PLAN.md::R14.1` for the full A/B/C
tradeoff.
**Trigger to ship:** any per-user corpus exceeds 50k chunks OR Linux/Windows
support requires a new RAG path.

### R14.2 — Windows native whisper.cpp

**Where deferred:** R10 → R12.5 → R14.2.
**Why deferred:** macOS whisper integration is shipping; Windows
needs its own port (whisper.cpp built via CMake + statically linked
into `cue-whisper.exe`) AND a clean Windows test machine for QA.
**Estimate:** 2–3 days code + clean-Windows QA.
**Owner:** **codex** (per user direction 2026-05-19; uno's Tailscale
reaches the Windows test machine).
**Brief:** `docs/rounds/PHASE-3-WINDOWS-BRIEF-FOR-CODEX.md` (W1).

### R14.3 — Linux x86_64 build

**Where deferred:** R12 cross-platform finding → R13.5 → R14.3 (Linux
portion).
**Why deferred:** v0.1.0 GA stayed macOS-only honestly. Linux needs
cross-compile + audio capture validation on PulseAudio + PipeWire and
the overlay layer is macOS-specific (Linux gets CLI-only at first).
**Estimate:** 3 hours code + bench validation.
**Trigger to ship:** at least one Linux user requests it.

### R14.6 — Telemetry counters for overlay-reader rejections

**Where deferred:** codex R12 review.
**Why deferred:** the production overlay reader thread logs warnings
on rejected events (token / length / state). For ops we want a counter
so spikes are visible. Gated on having a telemetry sink + privacy
review.
**Estimate:** 30 min code + privacy review.
**Trigger to ship:** Stage 2 product server lands (it provides the
telemetry sink) AND a privacy policy is drafted.

### R14.7 — Clean-machine validation of `scripts/install.sh`

**Where deferred:** R13 partially shipped; codex R12 nit.
**Why deferred:** uno is the dev box, so the installer was tested on
the same machine that built it. Real validation needs a clean Apple
Silicon Mac that has never run any Bluey build.
**Estimate:** 1 hour given a clean Mac.
**Trigger to ship:** before the v0.1.x distribution server URL goes
public-facing (Stage 1).

---

## Layer 2 — Distribution server

### R14.8 — Stand up the distribution server

**Status:** architecture decided; implementation pending user pick on
Path A / B / C and domain.
**See:** `docs/BLUEY-DISTRIBUTION-ARCHITECTURE.md` and
`ARCHITECTURE.md` Section 4.
**Estimate:** 1.5–6 hr depending on path.
**Trigger to ship:** user picks path + domain.

### Process masquerading on Windows

**Where deferred:** R8 → W6 (Windows brief).
**Why deferred:** macOS argv-overwrite mechanism doesn't apply on
Windows. Windows path exists in `cue-stealth/src/windows.rs` but is
not exercised on a real Windows machine.
**Estimate:** half a day given clean Windows.
**Trigger to ship:** v0.1.x Windows GA.

---

## Layer 3 — Product server / monetization

### R14.9 — Product server (BLOCKING for v0.2)

**Status:** **REQUIRED for v0.2 launch** per the no-BYOK decision
(2026-05-19, `DECISIONS.md`). No longer optional.
**See:** `ARCHITECTURE.md` Section 5 + Section 7 Stage 2.
**Estimate:** ~1–2 weeks dedicated work in a separate `bluey-server`
repo.
**Language:** Rust (locked in 2026-05-19; `DECISIONS.md`). Operational
shape mirrors Pinky's API droplet pattern (single binary + SQLite +
Caddy + LetsEncrypt) but the language stays in the Bluey family.
**Endpoints required for v0.2:**

- `POST /auth/{signup,login,refresh,logout,reset}`
- `POST /billing/{checkout,webhook}` (hosted billing provider; sandbox through
  Stage 3, live at Stage 4)
- `GET  /account/me` (license + plan + usage summary)
- `POST /router/complete` (managed LLM dispatch — this is the
  monetization handle)
- `POST /router/embed` (managed embedding dispatch)
- `POST /router/transcribe` (managed STT dispatch)
- `POST /usage/event` (per-call metering ingestion from daemon)
- `GET  /admin/customers` (Bluey-team only)
- `GET  /admin/health`

**Trigger to ship:** v0.2 launch is the trigger. Until then, BYOK
remains a dev-only path.

### Daemon talks to product server (Stage 3) — superseded by R14.10

**Status:** Superseded by R14.10–R14.13 (the no-BYOK decision broke
this single "daemon talks to server" item into four concrete pieces).
Kept here as a pointer for git-history-divers.

**Status:** parked until R14.9 lands.
**Approach:** new `cue-cloud-client` crate in this repo;
first-`bluey on` sign-in plus `bluey off` lifecycle; `cue-router::ManagedPolicy` impl
that asks bluey-server which lane to use; `cue-router::BlueyManagedProvider`
dispatches over HTTPS to the product server.
**Estimate:** 3–5 days.
**Trigger to ship:** product server has a working stub
`POST /router/complete` endpoint + an auth flow.

### R14.10 — `cue-cloud-client` crate + first-`bluey on` sign-in flow (BLOCKING for v0.2)

**Status:** required for v0.2 launch. Daemon must authenticate with
`bluey-server` instead of using BYOK keys.
**Where deferred:** spawned by 2026-05-19 no-BYOK decision.
**Approach:**

- New `crates/cue-cloud-client/` Rust crate.
- HTTPS client with refresh-token handling, retry on 401, exponential
  backoff on 5xx.
- Token stored in keyring under `bluey_account` (separate namespace
  from the dev BYOK `llm_*` keys).
- `bluey on` opens the browser/device flow when no token exists, receives
  the `bluey://` callback, and stores the issued token.
- Hidden support/logout paths can clear the keyring entry when needed.

**Estimate:** 3–4 days code + tests.
**Trigger to ship:** R14.9 server has working auth + token endpoints.

### R14.11 — `BlueyManagedProvider` + `ManagedPolicy` (BLOCKING for v0.2)

**Status:** required for v0.2 launch.
**Approach:**

- New `cue_llm::bluey_managed::BlueyManagedProvider` impl that
  speaks HTTPS to `bluey-server::POST /router/complete` and
  `/router/embed`. Reuses the existing `LlmProvider` /
  `EmbeddingProvider` traits so callers do not change.
- New `cue_router::policy::ManagedPolicy` impl that calls
  `bluey-server::POST /router/route` (or embeds the policy in the
  ManagedProvider response) to pick lane + provider + model.
- The daemon's existing `ProviderRegistry` is rewired so the
  production code path uses `BlueyManagedProvider` exclusively.
  BYOK providers stay behind a `BLUEY_DEV_BYOK=1` env flag for
  dogfooding only.
- `cue_router::policy::StaticPolicy` is renamed
  `LocalFallbackPolicy` and used only when offline / privacy-mode
  is active.

**Estimate:** 2–3 days code + integration tests.
**Trigger to ship:** R14.10 cloud client + R14.9 server endpoints.

### R14.12 — Local-fallback mode + offline detection

**Status:** required for v0.2 launch (so offline customers do not
just see errors).
**Approach:**

- Daemon tracks bluey-server reachability via the cloud-client.
- When unreachable for >N seconds, switches to local-fallback mode:
  - LLM lane → local Ollama (`llama3.1` or whatever model is on disk)
  - STT → local whisper.cpp
  - Router policy → `LocalFallbackPolicy`
- Daemon emits a clear UI signal (LaneBadge "OFFLINE" tag, a status
  chip in the dashboard).
- When connectivity returns, daemon switches back automatically.
- An explicit `bluey privacy-mode on/off` toggle pins the daemon to
  local-fallback regardless of connectivity.

**Estimate:** 2 days code + tests.
**Trigger to ship:** R14.11 ManagedProvider + LocalFallbackPolicy land.

### R14.13 — Account credits + per-use metering (server side)

**Status:** required for v0.2 launch.
**See:** `DECISIONS.md` 2026-05-19 account-credit entry,
`docs/HOW-IT-WORKS.md` Sections 0, 4-7.

**Approach:**

- Account schema gains `balance_cents`, `trial_seconds_remaining`,
  and per-batch credit tracking with 12-month (365-day) expiry.
- Per-model pricing table (input cents/1M tokens, output cents/1M
  tokens) with Bluey markup 100-200% over upstream provider cost.
- `/router/complete` flow:
  1. Estimate cost from `input_tokens * markup_in + max_output_tokens * markup_out`.
  2. **Entry check:** if `balance_cents < estimated_cost`, return
     `402 Payment Required { balance_cents, estimated_cost_cents,
     reason: "insufficient_balance", reload_url }`.
  3. Stream upstream provider response.
  4. **Mid-stream check** (every N tokens or every chunk): if running
     cost exceeds remaining balance, cut the upstream stream, emit a
     final chunk with `balance_exhausted: true`, deduct the
     remaining balance (NOT the overrun), log overrun for audit.
  5. On clean completion: atomic `UPDATE accounts SET balance_cents =
     balance_cents - actual_cost WHERE id = ?`. Return new balance in
     response trailer.
- Free trial: during trial, skip balance deduction; decrement
  `trial_seconds_remaining` by request duration.
- Reloads: hosted checkout credits the account through a verified webhook.
  Saved-card auto-reload is a future feature and is not part of the live v0.2
  customer promise.
- Daemon polls `/account/me` every 30s OR receives WebSocket push to
  keep the live balance in the overlay top strip current.
- Daemon emits the `RouterMeta` per-cue with `cost_cents` so the UI
  can render the per-card cost label.

**Hard guarantees baked into the implementation:**
1. Customer cannot rack up debt. Overruns are absorbed by Bluey.
2. No surprise charges. Customer explicitly reloads credits and always sees
   the balance.
3. No silent failures. 402 is always accompanied by a clear reason.

**Estimate:** 5-7 days code + tests + billing integration.
**Trigger to ship:** R14.9 server + R14.11 ManagedProvider land.

### R14.14 — Cost-label UX + tier visibility (daemon + dashboard)

**Status:** partially shipped in Stage 12-17 Codex follow-up. Live balance,
dashboard cost labels, persisted response billing metadata, and macOS overlay
answer status labels are implemented. Remaining work is the richer account
usage dashboard and web onboarding screen.
**See:** `docs/PRICING-MODEL.md` Section 4 for the exact mockups.

**Approach:**

- Server-side: `/account/usage` returns rolling-7-day mix + projected
  duration + which tier the user falls into. Computed nightly, cached
  for 60s. Hits SQLite `usage_events` table aggregated by
  `task_type`/`lane`.
- Daemon-side: `bluey usage` CLI command formats and prints the
  server response. `bluey credits` shows per-batch expiration dates.
- ✅ Overlay top strip: live balance display from account polling.
  Compact, minimal, no tier info.
- ✅ Cue card / dashboard response cards: per-card cost label rendered
  from managed response metadata (`cost_cents`, provider/model, token
  counts, post-request balance when available).
- Dashboard `/account/usage` page: rolling-7-day breakdown chart,
  tier comparison panel, "your $X.XX lasts ~N days" projection.
- Onboarding screen `/onboarding/welcome` (after first reload):
  static tier table from `PRICING-MODEL.md` + manual reload + credit
  validity disclosure.

**Estimate:** 3-4 days (UI work split across overlay, dashboard,
CLI, plus the server-side aggregation endpoint).
**Trigger to ship:** R14.13 account credits + R14.11 ManagedProvider land.

### Billing-provider production mode (Stage 4)

**Status:** parked until product server scaffold + Stage 3 daemon
client are tested against the billing sandbox.
**Estimate:** 1 day flip + however long the first paying users take
to find.
**Trigger to ship:** dogfooding through Stage 3 confirms the
managed-router happy path works end-to-end.

### Cloud RAG with citations + tenant scoping

**Status:** v0.3 territory.
**Approach:** encrypted transcript upload to product server; cloud-side
embedding + retrieval; per-tenant scoping; citations in the response.
**Estimate:** 1–2 weeks.
**Trigger to ship:** Stage 4 paying users are asking for cross-device
search.

### Encrypted transcript sync across devices

**Status:** v0.3 territory.
**Estimate:** 1 week.
**Trigger to ship:** Stage 4 multi-device requests.

### Team plans (shared transcripts, SSO, audit log)

**Status:** future.
**Estimate:** 2–3 weeks.
**Trigger to ship:** company customers.

### SOC2 / data residency

**Status:** future.
**Trigger to ship:** enterprise sales conversation.

---

## UX polish (mostly Layer 1)

### Markdown / code rendering in overlay cards

**Where deferred:** UX direction doc.
**Estimate:** half a day.

### Answer / code copy controls

**Where deferred:** UX direction doc.
**Estimate:** 2 hours.

### Status chips for route / mic / system / provider / context

**Where deferred:** UX direction doc.
**Estimate:** 4 hours.

### Attachment drawer

**Where deferred:** UX direction doc.
**Estimate:** half a day.

### Warning-card patterns

**Where deferred:** UX direction doc.
**Estimate:** 2 hours.

### Visual QA harness for capture-excluded NSWindow

**Where deferred:** UX direction doc.
**Why hard:** capture-excluded NSWindow does not show up in screen
captures, so automated visual diff tools cannot see it. Need either an
unexcluded debug build OR a manual eyeballing checklist.
**Estimate:** 1 hour for the checklist; longer for any automation.

---

## Distribution / packaging

### Code signing + notarization

**Where deferred:** R12 user decision.
**Why:** v0.1 ships terminal-only, distribution is `curl | sh`, signing
adds operational overhead without much UX benefit at internal-tester
scale.
**Estimate:** 1 day setup + ongoing per-release runtime.
**Trigger to ship:** before non-internal users get the binary.

### Auto-update mechanism

**Where deferred:** v0.1 scope.
**Approach:** client polls `latest.json`, prompts user when newer
version is available, downloads + replaces in place.
**Estimate:** 1–2 days.
**Trigger to ship:** when v0.1.x has shipped at least 2-3 point
releases and manual reinstall friction shows up in feedback.

### `scripts/install.ps1` (PowerShell installer for Windows)

**Where deferred:** Windows brief W4.
**Status:** spec lives in the Windows brief; codex picks up when
W1–W3 land.
**Estimate:** 2 hours.

---

## Cleanup / archived

These were on the future list and have shipped. Kept here briefly so
git-history-divers can find the trail; remove after one round.

### ✅ R12 pill-first overlay UX (shipped 2026-05-17, `c34592a`)
### ✅ R13.1 OverlayUiStateScope cancel/error guard (shipped 2026-05-17, `31b794d`)
### ✅ R13.2 generate_session_token returns Result (shipped 2026-05-17, `31b794d`)
### ✅ Auto Router crate `cue-router` (shipped 2026-05-17, `96fc208`)
### ✅ Auto Router daemon wiring + speculative dispatch (shipped 2026-05-18, `4bf7a73`)
### ✅ RAG bounded-heap top-k (shipped 2026-05-18, `7c91d80`)
### ✅ R14.3 macOS x86_64 universal binary (shipped 2026-05-19, `0340aad`)
### ✅ R14.4 replace_body for clean draft → final (shipped 2026-05-19, `c34592a`)
### ✅ R14.5 Dashboard LaneBadge (shipped 2026-05-19, `c34592a`)
### ✅ Speculative routing default-ON (shipped 2026-05-19, `8a3051d`)
### ✅ Stage 17 live balance bridge (shipped 2026-05-20, `7c01c93`)
### ✅ Managed SSE stream contract + cost labels (shipped 2026-05-20, this Codex batch)

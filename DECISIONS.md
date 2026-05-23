# Bluey Decisions — what not to retry

> **Historical decisions and dead ends.** Keep this short and high-signal.
> Mirrors Pinky's `DECISIONS.md` shape.
>
> Pattern: state the decision, link the round/commit, capture the
> rationale, and (where relevant) call out what NOT to retry without
> new context.

---

## 2026-05-19 — Pricing tiers visible in product UI

**Source:** user direction.
**Decision:** the pricing model is not a back-end-only concern. The
three usage tiers (Light / Typical / Heavy), the per-question cost
table, and the customer's current tier projection MUST be visible in
the product. Specifically:

- Onboarding screen after first $30 reload shows the three tier table.
- `/account/usage` dashboard shows the customer's rolling-7-day mix,
  which tier they fall into, and how many days $30 lasts at their
  current rate.
- `bluey usage` CLI command prints the same.
- Each cue card in the overlay shows a per-card cost label below the
  body (e.g. `$0.04 · 412 in / 89 out · 1.8s`).
- Overlay top strip shows live balance only (intentionally minimal).

**Markup tiers locked:**
- Easy / Medium → 200% markup (×3 over Bluey's upstream cost)
- Deep speculative → 150% markup (×2.5)
- Vision → 150% markup (×2.5)

**Why "always visible" matters:** transparent metering is the trust
contract. Customers must never be surprised by their balance, never be
unsure what a cue cost, and never have to guess how long $30 lasts.

**Don't retry without new context.** Hiding cost from customers is the
fast path to chargebacks + churn. The Bluey product UX explicitly
opts into transparency.

See `docs/PRICING-MODEL.md` for the locked numbers + UI mockups.

---

## 2026-05-19 — Prepaid wallet + auto top-up + 10-min free trial

**Source:** user direction.
**Decision:** v0.2 monetization model is a **prepaid wallet** with
**auto top-up** plus a **10-minute free trial** for new accounts.

Specifics:

- New account starts with `balance_cents = 0` and
  `trial_seconds_remaining = 600`.
- During trial: `/router/complete` returns successfully without
  deducting balance; trial seconds decrement by request duration.
- After trial ends: customer must load $30 to continue. No partial
  reloads; minimum is $30.
- Each request is metered server-side: `cost = input_tokens *
  markup_in + output_tokens * markup_out` with **100-200% markup
  over the upstream provider cost**.
- Auto top-up: when balance drops below $5, bluey-server triggers a
  $30 Stripe charge against the saved card. Customer can disable in
  Settings (default ON).
- **Hard stop:** if balance is insufficient for the *estimated*
  cost of a request, server returns 402 Payment Required and
  daemon shows "Add $30 to continue" — no streaming starts. If
  balance becomes insufficient mid-stream, server cuts the stream
  and Bluey eats the overrun (customer is NEVER put in the red).
- **Customer always sees:** live balance at the top of the overlay,
  per-card cost label after each cue, LaneBadge with provider/model.

**Pricing rationale (target):** the locked markup tiers
(`docs/PRICING-MODEL.md`) are 200% on Easy/Medium / 150% on Deep
speculative / 150% on Vision. Customer prices range from $0.0003
(Easy) to $0.104 (System design speculative). The typical user
(~50 cues/day mixed) burns ~$21/month and reloads the $30 wallet
roughly once a month. Heavy users (~200 cues/day) reload 2-3 times
per month. **Gross margin per request is ~67% on 200%-markup lanes
and ~60% on 150%-markup lanes** (see `docs/PRICING-MODEL.md` Section 5
for the per-lane breakdown). The earlier 87-99% claim in this doc
was wrong — it conflated Bluey-vs-upstream with Bluey-vs-customer-cost.
The 60-67% number is the correct one to quote and is what the
PRICING-MODEL.md table proves out per-lane.

**What this changes for the codebase:**

- Server-side: prepaid balance state in account record. Every
  `/router/complete` does an entry check (balance >= estimated_cost)
  and a mid-stream check. Atomic deduction via SQL UPDATE on
  completion.
- Daemon-side: live-balance display in the overlay top strip.
  Per-card cost label below each cue (already supported by
  `RouterMeta`; just add `cost_cents` field).
- Hard-stop UI: "balance_exhausted" event from server →
  daemon shows banner with reload button.
- Auto top-up: Stripe SetupIntent at first $30 reload to save the
  card, subsequent charges use saved PaymentMethod.

**Don't retry without new context:**
- Don't allow debt. Customer overrun is on Bluey, not the customer.
- Don't surprise-charge. Auto top-up is opt-out; customer always
  sees the balance.
- Don't hide cost. Per-card cost label is non-negotiable.

See `docs/HOW-IT-WORKS.md` for the full v0.2 customer flow including
free trial, hard-stop, and offline fallback.

---

## 2026-05-19 — No BYOK; managed-only with local fallback

**Source:** user direction.
**Decision:** Bluey ships **managed-only**. Customer pays Bluey for use.
Bluey owns all upstream API keys (Anthropic, OpenAI, Deepgram, etc.).
The customer's daemon dispatches every LLM call, every embedding,
every cloud STT call through `bluey-server`, which proxies to upstream
providers using Bluey-owned keys. There is **no BYOK exposure to
customers** in the product UI.
**Local models are an offline / privacy fallback only.** When the
customer is on a paid Bluey plan but momentarily disconnected, OR they
explicitly toggle on a privacy-only mode, the daemon falls back to
local whisper.cpp + local Ollama. The customer is still on a paid
Bluey subscription; the inference cost just shifts to their hardware
during the fallback window.
**Earlier proposal (rejected):** BYOK as a default with an optional
managed mode. Rejected because:

- Two billing models means two integrations, two support stories, two
  pricing pages. Each adds friction without proportional revenue.
- BYOK means Bluey only sells "the wrapper." Margin is thin and the
  product differentiation is harder to defend.
- Cluely / Pluely / similar overlays are already managed-only; that is
  the working pattern in this category.

**What this changes for the codebase:**

- `cue-llm`'s `OpenAiProvider` / `AnthropicProvider` / `OllamaProvider`
  are demoted to **dev-mode-only**. The daemon no longer reads user
  API keys from keyring or env in production; those code paths stay
  but are gated behind a `dev` feature OR `BLUEY_DEV_BYOK=1`.
- A new `BlueyManagedProvider` becomes the production default. It
  speaks HTTPS to `bluey-server` and authenticates with the customer's
  Bluey account token (stored in keyring after the first `bluey on` sign-in).
- `cue-router::StaticPolicy` is renamed `LocalFallbackPolicy` and
  becomes the offline-mode fallback. The new default is
  `cue-router::ManagedPolicy` which delegates lane choice to
  `bluey-server`.
- `bluey on` / `bluey off` are the customer-facing lifecycle commands.
  The first `bluey on` opens sign-in if no account token is stored.
  Support-only account commands remain hidden.
- Per-use metering happens server-side; the daemon emits usage events
  to `bluey-server` after every cue request.

**What this changes for the rollout:**

- Layer 3 (`bluey-server`) is **REQUIRED for v0.2**, not optional.
  Previously framed as "parallel scaffold during Layer 2 testing"; now
  it is the gating dependency for monetization.
- v0.1 (BYOK) keeps working for internal dev / dogfooding only. We do
  not invite external users to the BYOK flow because that creates
  expectations we won't honor in v0.2.

**Don't retry without new context.** If we ever offer a self-hosted
or "bring your own keys for compliance reasons" enterprise tier, that's
a deliberate revisit, not silent drift.

---

## 2026-05-19 — bluey-server is Rust, not Go

**Source:** user direction.
**Decision:** the future Bluey product server (Layer 3) is written in
Rust, not Go. Lives in a separate repo (`bluey-server`) but stays in
the Bluey language family.
**Earlier proposal (rejected):** Go, to mirror the Pinky daemon stack
for operational familiarity.
**Why rejected:** Bluey is end-to-end Rust today (workspace crates,
native helpers via Swift on macOS, C on Windows). Introducing Go for
one new component would mean two stacks for one product family: extra
toolchain, extra build pipeline, extra deploy story, extra hire-able
profile. The marginal upside (Pinky-style operational familiarity)
does not pay for the friction.
**What carries over from Pinky:** the operational shape (single binary
+ SQLite + Caddy + LetsEncrypt + droplet) is fine to mirror. The
language changes.
**Don't retry without new context.** If a future requirement makes Rust
genuinely awkward (e.g. a vendor SDK only exists for Go), revisit
deliberately. Don't drift back to Go silently.

---

## 2026-05-19 — Bluey distribution is separate from Pinky

**Source:** user direction.
**Decision:** Bluey gets its own distribution server, its own product
server (when monetization lands), its own domain. Pinky and Bluey are
two products; they share no servers and no codebase.
**Earlier proposal (rejected):** piggyback Bluey artifacts on Pinky's
`/downloads/` static-file route to avoid standing up new infra.
**Why rejected:** product separation matters more than infra economy at
this stage; conflating two products operationally creates friction when
either of them grows or breaks.
**Don't retry without new context.** If we ever revisit (e.g. one
product is acquired, or shares billing infra), it has to be a deliberate
re-decision, not silent drift.

---

## 2026-05-19 — Speculative routing default-ON for v0.1

**Source:** user direction.
**Decision:** `BLUEY_SPECULATIVE_ROUTING` is default-ON. Hard questions
fire Instant + Deep in parallel out of the box. Disable explicitly with
`BLUEY_SPECULATIVE_ROUTING=0/false/off`.
**Why:** v0.1 audience is internal testers, cost is BYOK, the Auto
Router USP demos better with draft+final firing immediately.
**Cost guardrail:** none yet. Telemetry counter (R14.6) + per-tenant
cost cap will land in Stage 2 product server before this default
matters at scale.
**Reverse if:** any external user flips, OR speculative cost concerns
appear in dogfooding metrics.

---

## 2026-05-19 — Monetization timeline = Y (medium)

**Source:** monetization conversation.
**Decision:** ship distribution server now (Layer 2); scaffold product
server (Layer 3) in parallel; flip to live billing at v0.2 paid alpha.
**Rejected timelines:**
- **X (slow):** wait on product server until v0.2/v0.3. Reasoning we
  rejected: leaves 2-3 months of testing without a billing path, slows
  validation of the actual monetization model.
- **Z (aggressive):** build full product server + ship Bluey v0.2 paid
  immediately, skip BYOK v0.1 entirely. Rejected: pre-billing-ready
  before actual user feedback on the local product is poor sequencing;
  too much risk on one big bet.

---

## 2026-05-17 — Code signing / notarization deferred

**Source:** R12 user decision; codex 🟢 with softened wording.
**Decision:** v0.1 ships unsigned tarballs. Distribution is terminal-only
(`curl | sh`) for internal testers. Code signing + notarization are a
v0.2-class concern, not blocking v0.1 GA.
**Caveat:** docs (`INSTALL.md`, release notes) explicitly say signing is
deferred and document the `xattr -d com.apple.quarantine` workaround.
We do NOT promise Gatekeeper bypass behavior.
**Don't retry without new context.** If we ship to non-internal users,
revisit. Signing + notarization is ~1 day of one-time setup.

---

## 2026-05-17 — Single `Arc<Mutex<OverlayUiState>>` in daemon

**Source:** R12.2 codex review.
**Decision:** the Daemon's overlay state mirror was collapsed into a
single shared `Arc<Mutex<OverlayUiState>>` cloned across the daemon and
the production reader thread. Handlers transition the state directly;
the gate observes live transitions.
**Don't retry the split-brain pattern** (Daemon-side mirror + reader-thread
copy). It worked by accident under R11 because inner-form events were
never emitted unless the panel was open, but the architecture was
broken-by-design.

---

## 2026-05-17 — `generate_session_token` returns `Result`

**Source:** R12.3 codex review.
**Decision:** `cue_daemon::overlay::generate_session_token` returns
`Result<String, getrandom::Error>` instead of `.expect("OS random source
unavailable")`. Daemon startup propagates failure with anyhow context
instead of panicking.
**Don't retry the panic pattern.** Even though `getrandom` failure is
extremely rare in practice, panicking the whole daemon on startup is
worse than gracefully refusing to launch.

---

## 2026-05-17 — True 256-bit OS-entropy session token

**Source:** R12.3 codex review nit.
**Decision:** `generate_session_token` uses `getrandom::getrandom` to
draw 256 bits of OS entropy directly. The previous impl concatenated
two `Uuid::new_v4()` values which gave 244 bits of randomness.
**Don't retry the UUID concat trick.** The 244-bit version was
plausibly secure but had subtle structure (bit position 12 of every
token was forced to '4' for UUIDv4 version). Use the OS entropy source
directly.

---

## 2026-05-17 — Pill-first overlay UX

**Source:** R12 user/codex direction.
**Decision:** `bluey on` shows the compact 112x28 pill; the feed/composer
panel only opens on pill click or via `OverlayCommand::Show`. Earlier
behavior auto-expanded the full panel on startup.
**Don't retry auto-expand-on-startup.** Pill-first is the documented
product contract (capture-excluded, click-to-open, drag-to-move).

---

## 2026-05-16 — v0.1.0 scoped to macOS arm64 only

**Source:** codex R12 nit.
**Decision:** v0.1.0 GA ships macOS arm64 (and a universal slice that
also includes x86_64, link-tested only). Linux and Windows are
**explicitly not** in v0.1.0.
**Why:** any platform we claim to support must be smoke-tested on a
clean box. We can only do that for macOS arm64 from uno; Linux + Windows
need their own bench machines.
**Cross-platform expansion is R14.3.** Until that lands and is
clean-tested, INSTALL.md and the release notes do NOT advertise
non-macOS platforms.

---

## 2026-05-16 — RAG: bounded-heap top-k now, sqlite-vec/usearch later

**Source:** codex R11 carry-over (R13.3).
**Decision:** for v0.1 we keep the SQLite scan but use a bounded
min-heap of size k instead of sort+truncate. ~37 ms for 10k chunks /
1536-dim / top-10 on Apple Silicon. Real ANN (sqlite-vec or usearch)
is R14.1, deferred until per-user corpora exceed ~50k chunks.
**Don't retry full sort + truncate.** It's correct but wastes work.

---

## 2026-05-15 — Auto Router as a separate crate

**Source:** R12 user direction.
**Decision:** the task classifier + routing policy + speculative router
live in `crates/cue-router/` as a standalone crate, depended on by
`cue-dashboard` for daemon wiring. Trait-based so a future managed
classifier or managed policy can be plugged in without rewriting the
heuristic baseline.
**Don't merge cue-router into cue-llm or cue-daemon.** The separation
is deliberate: `cue-router` is the monetization hinge (managed
endpoint plugs into `RoutingPolicy` + `SpeculativeProvider` traits).
Keeping it isolated keeps the contract clean.

---

## Format

When adding a new entry: date, source (round / review / user), one-line
decision, why, and what NOT to retry. Newest first. Keep entries short
(max ~20 lines) — link out to round docs for detail.

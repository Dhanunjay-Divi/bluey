# Bluey Decisions — what not to retry

> **Historical decisions and dead ends.** Keep this short and high-signal.
> Mirrors Pinky's `DECISIONS.md` shape.
>
> Pattern: state the decision, link the round/commit, capture the
> rationale, and (where relevant) call out what NOT to retry without
> new context.

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
  Bluey account token (stored in keyring after `bluey login`).
- `cue-router::StaticPolicy` is renamed `LocalFallbackPolicy` and
  becomes the offline-mode fallback. The new default is
  `cue-router::ManagedPolicy` which delegates lane choice to
  `bluey-server`.
- `bluey login` / `bluey logout` flows are added. v0.2 launch is
  gated on these.
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

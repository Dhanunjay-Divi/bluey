# PHASE-3-POST-GA-DOC-LOCK-HANDOFF-FOR-CODEX-REVIEW.md

> **Branch:** `feat/phase-3-round-12` tip `883e11e` on uno.
> **Tag:** `v0.1.0` GA still anchored at merge `c34592a` (unchanged).
> **Reviewer ask:** review the 16 commits since the last codex
> handoff (`bd10762`). Almost all docs + strategy + one runtime
> change (speculative default flip). No code regressions expected;
> pipeline still green at 392 cargo + 15 vitest.

---

## 1. What changed since `bd10762`

### Code (1 commit)

```
8a3051d feat(router): flip BLUEY_SPECULATIVE_ROUTING default to ON
```

Speculative draft+final dispatch is now ON by default. Disable
explicitly with `BLUEY_SPECULATIVE_ROUTING=0/false/off`. Per user
direction 2026-05-19 — internal testers, BYOK, USP demos better
out of the box. Cost guardrail (R14.6 telemetry counter +
per-tenant cap) still pending; not a v0.1 internal-tester concern.

### Doc reorganization (mirrors Pinky structure)

```
0ef2b18 docs: promote ARCHITECTURE + add SERVER-REFERENCE / DECISIONS / AGENT-HANDOFF / FUTURE-IMPLEMENTATIONS
b2dd967 docs: reorganize round + review docs into docs/rounds and docs/reviews
1e232d6 docs: remove stale docs/ARCHITECTURE.md (canonical lives at repo root)
acc800d docs: add operational runbooks (delivery, ops, release, security)
876c336 docs(readiness): refresh post v0.1.0 GA + Auto Router daemon wiring
07c703d docs(R14): mark R14.3 partial / R14.4 / R14.5 done; add R14.8 + R14.9
```

Repo-root canonical docs (5 new):

- `ARCHITECTURE.md` — 3-layer model, server topology, monetization plug-points, staged rollout, pending decisions
- `SERVER-REFERENCE.md` — exact paths on each server when stood up (forward-looking)
- `DECISIONS.md` — historical decisions + dead ends, newest first; 13 entries
- `AGENT-HANDOFF.md` — read-this-first for the next agent (operational basics, pipeline, codex/Kiro split)
- `FUTURE-IMPLEMENTATIONS.md` — canonical tracker, Layer 1/2/3 organized

`docs/` reorganization:

- `docs/rounds/` (45 files) — per-round PHASE-* / FIX-* / IMPL-* / HANDOFF-* docs moved out of `docs/work/`
- `docs/reviews/` (38 files) — REVIEW-PHASE-* docs moved out of `docs/work/`
- `docs/work/` — now contains only rolling state files + templates
- `AGENT-ONBOARDING.md` — promoted to repo root (sibling of `AGENT-HANDOFF.md`)

`docs/` operational runbooks (4 new):

- `docs/DELIVERY-LIFECYCLE.md` — environments, build pipeline, promotion, rollback, branch model, CI plan
- `docs/OPERATIONS-RUNBOOK.md` — secrets inventory, smoke tests, restart/inspect/kill, common failure modes
- `docs/RELEASE-RUNBOOK.md` — step-by-step checklist for cutting a release
- `docs/SECURITY-HARDENING.md` — threat model, what's in place, honest gaps, verification commands

### Strategy locks (5 new product decisions, captured across DECISIONS.md + ARCHITECTURE.md + multiple targeted docs)

```
ef02cfc docs: lock bluey-server in Rust (not Go) - single-language stack
e3180ce docs(arch): add Section 1.5 - what runs where (laptop vs cloud)
6d579f4 docs: lock no-BYOK strategy (managed-only with local fallback)
0443b57 docs: propagate no-BYOK strategy to downstream docs
c4b4eb3 docs: lock prepaid-wallet billing + add HOW-IT-WORKS.md
883e11e docs: lock pricing tiers + always-visible cost UX (PRICING-MODEL.md)
```

Five product decisions locked, all dated 2026-05-19, all captured
in `DECISIONS.md` with rationale + don't-retry guidance:

1. **Bluey distribution is separate from Pinky.** Two products, two
   infrastructures. Earlier piggyback proposal withdrawn.
2. **No BYOK; managed-only with local fallback.** Customer pays
   Bluey; Bluey owns all upstream API keys; local models
   (whisper.cpp, Ollama) are an offline / privacy fallback only.
   Layer 3 (`bluey-server`) is now BLOCKING for v0.2 (was optional).
3. **`bluey-server` is Rust, not Go.** Single-language stack across
   the Bluey product family. Operational shape (single binary +
   SQLite + Caddy + LetsEncrypt + droplet) still mirrors Pinky.
4. **Prepaid wallet model: $30 first reload, $30 auto top-up at
   <$5, 1-year credit validity, hard stop at $0.** No debt; mid-stream
   cuts absorbed by Bluey; customer always sees balance.
5. **Pricing tiers visible in product UI.** 200% Easy/Medium / 150%
   Deep / 150% Vision markup. Onboarding screen + `/account/usage`
   dashboard + `bluey usage` CLI + per-cue cost label all required
   for v0.2. Three usage tiers (Light ~2,850 cues, Typical ~1,380,
   Heavy ~825 per $30) shown to customers explicitly.

Distribution architecture:

```
847024a docs: Bluey distribution architecture (piggyback on Pinky API server)
78b171c docs: replace piggyback proposal with standalone Bluey distribution
962492a docs: forward-looking Bluey architecture (3-layer model + monetization timeline)
```

Three distribution paths laid out in `docs/BLUEY-DISTRIBUTION-ARCHITECTURE.md`
(A: Rust server / B: CDN / C: nginx static). User pick still pending.

New supplementary docs:

- `docs/HOW-IT-WORKS.md` — 12-section v0.2 customer flow walkthrough
  (signup, install, login, normal request, hard speculative, hard
  stop, mid-stream cut, free trial, offline fallback, single billing
  relationship, cross-references, customer-vs-server table)
- `docs/PRICING-MODEL.md` — locked numbers, per-question cost table,
  three usage tier breakdowns, UI mockups for 5 surfaces, gross
  margin, competitive positioning, tuning levers

### Updated existing docs

- `docs/PRODUCTION-READINESS.md` — Auto Router section refreshed with
  shipping state (26 router tests, daemon wiring done, speculative
  default-ON, ProviderRegistry, LaneBadge, replace_body, vision
  keyword narrowing, AutoRouter coordinator); Scope section updated
  with shipped artifacts (arm64 + universal); P0 Cloud /
  Commercial section reframed as REQUIRED for v0.2 per no-BYOK.
- `FUTURE-IMPLEMENTATIONS.md` — R14.9 product server bumped from
  optional to BLOCKING for v0.2; new R14.10–R14.14 added:
  - R14.10 `cue-cloud-client` crate + `bluey login` flow (3–4 days)
  - R14.11 `BlueyManagedProvider` + `ManagedPolicy` (2–3 days)
  - R14.12 local-fallback mode + offline detection (2 days)
  - R14.13 prepaid wallet + per-use metering (5–7 days)
  - R14.14 cost-label UX + tier visibility (3–4 days)

---

## 2. Pipeline status

```
Branch: feat/phase-3-round-12  tip 883e11e
Tag:    v0.1.0 (anchored on merge c34592a, unchanged)

cargo fmt --all --check                                          ✅
cargo clippy --all-targets -- -D warnings                        ✅
cargo build --all-targets --release                              ✅
cargo test --all-targets                                         ✅ 392 passed, 0 failures
(cd crates/cue-dashboard/ui && npm test)                         ✅ 15 vitest tests
(cd crates/cue-dashboard/ui && npm run build)                    ✅
swift build -c release --package-path native/macos/cue-overlay   ✅
swift build -c release --package-path native/macos/cue-whisper   ✅
make package-darwin-arm64                                         ✅
make package-darwin-universal                                     ✅
git -P diff --check main..HEAD                                    ✅
```

Test count unchanged at 392 cargo + 15 vitest because nothing in
this batch added tests — only the speculative default flip is
runtime, and that path was already covered by the existing
`cue-router` and `cue-dashboard` tests.

---

## 3. Reviewer ask (per area)

### 3.1 Speculative default flip (`8a3051d`)

The only runtime change. Confirm the env-var semantics make sense:
unset = ON, `0`/`false`/`off` = OFF, anything else = ON. This is
slightly unusual; some prefer "explicit opt-in" via `=1`. User
direction was default-ON for v0.1 internal testing.

### 3.2 No-BYOK strategy (`6d579f4`, `0443b57`)

Confirm the Section 6 plug-point design in `ARCHITECTURE.md`
correctly captures how `cue-router` swaps from BYOK
(`StaticPolicy::defaults` + `OpenAiProvider`) to managed
(`ManagedPolicy::from_bluey_account` + `BlueyManagedProvider`)
with no rewrite. The trait-based design was specifically intended
to support this swap; this commit just documents it formally.

### 3.3 Pricing model (`c4b4eb3`, `883e11e`)

Confirm:

- The cost math in `docs/PRICING-MODEL.md` Section 2 (per-question
  cost table) is correct given OpenAI/Anthropic list prices.
- The three usage tier mixes (Light/Typical/Heavy) and the resulting
  cues-per-$30 numbers (~2,850 / ~1,380 / ~825) are reasonable for
  what we expect tech users to do.
- The hard-stop + mid-stream-cut design in `docs/HOW-IT-WORKS.md`
  Section 6–7 is financially safe (customer never goes negative;
  Bluey eats overruns).
- The 1-year credit validity per-batch FIFO accounting in
  `FUTURE-IMPLEMENTATIONS.md::R14.13` is correct (each $30 reload is
  its own row; credits expire 365 days from purchase regardless of
  later reloads).

### 3.4 Doc reorganization (`0ef2b18`, `b2dd967`, `1e232d6`, `acc800d`)

Confirm the Pinky-style layout is usable:

- Repo-root canonical docs (`ARCHITECTURE.md`, `SERVER-REFERENCE.md`,
  `DECISIONS.md`, `AGENT-HANDOFF.md`, `AGENT-ONBOARDING.md`,
  `FUTURE-IMPLEMENTATIONS.md`) — read order documented in
  `AGENT-HANDOFF.md` Section 1.
- `docs/rounds/` (45 files) and `docs/reviews/` (38 files) — moved
  with `git mv` so history is preserved per file. Verify by spot-
  checking a few via `git log --follow`.
- `docs/work/` — now intentionally minimal (rolling state +
  templates); not a dump-everything dir anymore.
- Operational runbooks (`docs/DELIVERY-LIFECYCLE.md`,
  `OPERATIONS-RUNBOOK.md`, `RELEASE-RUNBOOK.md`,
  `SECURITY-HARDENING.md`) — currently mostly forward-looking
  (no servers stood up yet); will be filled in with real paths +
  hashes once R14.8 lands.

### 3.5 R14 plan (`07c703d`)

Confirm the new R14.10–R14.14 split is the right granularity. The
no-BYOK decision broke the original "daemon talks to server" item
into four concrete pieces (cloud-client crate / managed
provider+policy / fallback mode / metering+wallet) plus the UX
piece (R14.14). Total Bluey-engineering work to v0.2 is
~3–4 weeks dedicated.

---

## 4. What's NOT in this batch

- No Pinky-side changes. Pinky and Bluey are two products per
  the 2026-05-19 decision; nothing in this commit touches
  `/tmp/pinky-full/` or any Pinky deploy path.
- No new code crates. `cue-cloud-client`, `bluey-server`, the
  `BlueyManagedProvider` impl, etc. are all R14.10–R14.13 work and
  haven't been started yet.
- No infra changes. No droplet provisioned, no DNS, no Stripe
  account. All of that waits on user pick of Path A/B/C + domain.
- No Windows work. `docs/rounds/PHASE-3-WINDOWS-BRIEF-FOR-CODEX.md`
  is the canonical Windows W1–W7 spec for the codex Tailscale-Windows
  agent; nothing has been done against it from this side.

---

## 5. Re-review request (paste-ready)

> Doc + strategy lock-down since the v0.1.0 GA tag. Branch
> `feat/phase-3-round-12` tip `883e11e` on uno.
>
> 16 commits, almost all docs + 5 product decisions locked
> (no-BYOK, prepaid wallet, pricing tiers visible, distribution
> separate from Pinky, bluey-server in Rust). 1 runtime change
> (speculative default flip ON).
>
> Pipeline unchanged: 392 cargo + 15 vitest, all builds + smoke green.
>
> Repo now mirrors Pinky structure: 5 canonical docs at root
> (ARCHITECTURE, SERVER-REFERENCE, DECISIONS, AGENT-HANDOFF,
> FUTURE-IMPLEMENTATIONS), reorganised `docs/rounds/` (45) +
> `docs/reviews/` (38), four operational runbooks in `docs/`.
>
> Two new substantive docs: `docs/HOW-IT-WORKS.md` (12-section
> v0.2 customer flow) + `docs/PRICING-MODEL.md` (locked numbers,
> tiers, UI mockups).
>
> Reviewer asks in `docs/work/PHASE-3-POST-GA-DOC-LOCK-HANDOFF-FOR-CODEX-REVIEW.md`
> Section 3 — confirm:
>   1. speculative env-var semantics (unset = ON) is the right default
>   2. plug-point design in ARCHITECTURE.md §6 captures the BYOK→managed swap
>   3. pricing math + tier mixes + hard-stop semantics check out
>   4. doc reorg is usable (read order, history preserved on moves)
>   5. R14.10–R14.14 split is the right granularity
>
> R14.10–R14.14 are ~3–4 weeks of code work, all tied to monetization.
> Not in this batch; gated on user picking distribution path + domain
> (Layer 2) and explicitly greenlighting Layer 3 scaffold.

# Bluey Future Implementations

> **Canonical tracker for deferred work.** Mirrors Pinky's
> `FUTURE-IMPLEMENTATIONS.md` shape but Bluey-only.
>
> If an item is "we should do this but not now," it lives here. If an
> item is in active development, it lives in the corresponding
> `docs/work/PHASE-3-ROUND-N-PLAN.md`. New items get added here; items
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
`docs/work/PHASE-3-ROUND-14-PLAN.md::R14.1` for the full A/B/C
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
**Brief:** `docs/work/PHASE-3-WINDOWS-BRIEF-FOR-CODEX.md` (W1).

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

### R14.9 — Product server scaffold (Stage 2)

**Status:** parked until monetization is greenlit (the user's call).
**See:** `ARCHITECTURE.md` Section 5 + Section 7 Stage 2.
**Estimate:** ~1 week dedicated work in a separate `bluey-server` repo.
**Trigger to ship:** user explicitly greenlights the monetization
track.

### Daemon talks to product server (Stage 3)

**Status:** parked until R14.9 lands.
**Approach:** new `cue-cloud-client` crate in this repo;
`bluey login` / `bluey logout` flows; `cue-router::ManagedPolicy` impl
that asks bluey-server which lane to use; `cue-router::BlueyManagedProvider`
dispatches over HTTPS to the product server.
**Estimate:** 3–5 days.
**Trigger to ship:** product server has a working stub
`POST /router/complete` endpoint + an auth flow.

### Stripe live mode (Stage 4)

**Status:** parked until product server scaffold + Stage 3 daemon
client are tested in Stripe test mode.
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

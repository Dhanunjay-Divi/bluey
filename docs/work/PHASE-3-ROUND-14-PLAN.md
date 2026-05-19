# PHASE-3-ROUND-14-PLAN.md

**Status:** In progress. v0.1.0 GA tagged on `c34592a` (macOS arm64 +
universal). R14.3 (macOS x86_64), R14.4 (replace_body for clean draft -> final),
and R14.5 (LaneBadge UI) **shipped** in commits `0340aad`, `c34592a`. Auto
Router default flipped to ON (`8a3051d`). Distribution server architecture
chosen (`78b171c`).

Outstanding: R14.1 ANN, R14.2 Windows whisper, R14.3 Linux portion, R14.6
telemetry, R14.7 clean-machine validation, R14.8 distribution server build,
R14.9 product server scaffold.

R13 left these items deliberately. Round 13 prioritised local correctness +
Auto Router observability + heap-based top-k for the existing RAG store. This
round picks them up after the user has a chance to test v0.1.0 end-to-end.

---

## R14.1 — Vector index (sqlite-vec or usearch) for RAG

**Source:** R13.3 deferred from R13 sweep.

**Today (post R13.3 step-1):** `crates/cue-rag/src/store.rs::query` does an
O(N) embedding scan with bounded min-heap top-k. At 10k chunks / 1536-dim /
top-10 the query runs in ~37 ms on Apple Silicon (`tests/rag_scaling.rs`).
Past ~50k chunks per database the scan starts to dominate and we want a
proper ANN index.

**Options:**

- **A. sqlite-vec virtual table.** Native SQLite extension that adds a
  `vec0(embedding float[1536])` column type with `MATCH` operator. Plus:
  no separate index file, queries are SQL. Minus: requires loading a SQLite
  extension at runtime, complicates packaging — we currently use `rusqlite`
  with `bundled` which doesn't expose `loadable_extension`. Need a feature
  switch and clean-machine validation of the extension binary.
- **B. usearch Rust crate.** Pure-Rust HNSW index, persisted to a sidecar
  file. Plus: zero packaging complexity, very fast at scale. Minus: index
  is separate from the SQLite chunks table; need atomic upsert + a
  consistency story for crash recovery.
- **C. hnsw_rs Rust crate.** Same shape as usearch, simpler API.

**Recommend:** **B (usearch)** because the packaging story is the cleanest and
we already have an opinionated Rust workspace. Migration path:

1. Add `usearch` to workspace deps with the `static` feature.
2. Build the index from `rag_chunks` + `rag_embeddings` on first start.
3. Persist `~/.cache/bluey/rag.usearch` next to the SQLite db.
4. Switch `query()` to `index.search(query_emb, k)`; keep the SQLite scan as
   fallback if the index is missing.
5. Update `chunk_count()` to read from index size.
6. Add a benchmark at 100k chunks proving ≥10x speedup.

**Estimate:** 1 day including migration + crash-recovery tests.

---

## R14.2 — Windows real whisper.cpp

**Source:** R12.5 / R13.4. Stub today.

**Today:** `native/windows/cue-whisper/main.c` documents `BLUEY_WHISPER_MODEL`
and emits a "not implemented" error JSON. macOS has the real
`SwiftWhisper` integration shipped in R10.

**Approach:**

1. Build whisper.cpp on Windows via CMake or its bundled MSBuild project.
2. Static-link the resulting `whisper.lib` from the cue-whisper Windows
   binary (C, no Swift on Windows).
3. Speak the same JSON IPC the daemon already speaks to the macOS variant.
4. Smoke test on a clean Windows 10/11 machine with model files in
   `%LOCALAPPDATA%\bluey\whisper\`.

**Blocker:** Windows test machine. User has Tailscale-reachable Windows; that
unblocks development but final QA needs a clean Windows install (no dev
toolchain pollution).

**Estimate:** 2-3 days.

---

## R14.3 — Cross-platform support matrix expansion

**Status:** 🟡 PARTIAL. macOS x86_64 + universal lipo done in `0340aad` (`scripts/build-macos-universal.sh`, Makefile `package-darwin-universal` target, link-tested only). Linux x86_64 and Windows still pending.

**Source:** R12 review cross-task finding / R13.5.

R13 honestly scoped v0.1.0 to macOS arm64. R14 expands:

- **macOS x86_64 (Intel):** `cargo build --target x86_64-apple-darwin` plus
  `lipo -create` for a universal binary. Smoke test on a clean Intel Mac.
  ~1 hour code + 30 min validation.
- **Linux x86_64:** cross-compile via `cross` or build on a Linux box.
  Verify CPAL audio capture on PulseAudio + PipeWire. Verify daemon
  + overlay (no native overlay on Linux today; Linux gets a CLI-only
  build until X11/Wayland overlay lands). ~3 hours.
- **Windows x86_64:** blocked on R14.2.

Once each platform passes its own smoke test, update `INSTALL.md`,
`web/index.html`, and `docs/release/RELEASE-v0.1.0.md` to reflect the
actual support matrix. Until then v0.1.0 stays macOS arm64-only.

**Estimate:** ~6 hours total code; per-platform validation extra.

---

## R14.4 — Replace draft "[refined]" tag with real card replacement

**Status:** ✅ DONE in `c34592a`. Daemon emits `replace_body: true` on the deep Final chunk; reducer + LaneBadge handle the swap. Vitest tests cover the contract.

**Source:** R13.x speculative wiring follow-up.

**Today:** when `BLUEY_SPECULATIVE_ROUTING=1` and a Hard question fires the
deep lane, the Deep `Final` chunk is emitted as a synthetic chunk prefixed
with `\n\n[refined]\n` so the existing dashboard reducer (which only does
delta-append) preserves the deep answer alongside the draft. This is a
visual hack.

**Fix:** add a new `cue_response_replace` Tauri event that signals
"replace card body with this text" and wire `responseReducer` to honour it.
Or extend `CueResponseChunkPayload` with a `replace_body: bool` field.

**Estimate:** 30 minutes including UI test.

---

## R14.5 — Dashboard lane-badge UI

**Status:** ✅ DONE in `c34592a`. `LaneBadge.tsx` renders above each in-flight card showing latency lane (color), task type, provider/model, confidence, and a REFINED tag once the deep lane has replaced the draft. Tooltip shows full RouterMeta JSON.

**Source:** R13.x router-meta wiring follow-up.

`request_cue` already emits `router_meta { task_type, latency_lane,
provider_lane, provider_name, model, confidence }` on the first chunk of
every cue stream. The dashboard does NOT yet render this.

**Fix:** add a small `<LaneBadge meta={...} />` component shown above each
in-flight card. Tooltip on hover shows full classification. ~1 hour.

---

## R14.6 — Telemetry counter for overlay-reader rejections

**Source:** codex R12 review, deferred.

`crates/cue-daemon/src/app.rs::validate_and_decode_overlay_line` logs `warn!`
on every rejected line (token / length / state). For ops we want a counter
so spikes are visible at scale.

Gated on having a telemetry sink + privacy review. Schedule when those land.

**Estimate:** 30 min code + privacy review.

---

## R14.7 — Clean-machine installer validation

**Source:** R13.7 partially done; codex flagged clean-machine validation as
a v0.1.0 GA blocker.

`scripts/install.sh` was tested locally on uno (the dev box). It still needs
validation on a clean Apple Silicon Mac that has never run any Bluey build:

1. Wipe `~/.local/bluey`, `~/.local/bin/bluey*`, `~/Library/Application
   Support/bluey/`.
2. `curl … | sh` against the GA tarball URL (or `BLUEY_ARCHIVE=…`).
3. `bluey on` → expect pill + daemon running.
4. Drive a real cue request (configured OpenAI key), verify streaming.
5. `bluey off` → expect clean exit.
6. Re-install and verify upgrade path.

Output: a checklist that becomes the GA gate.

**Estimate:** 1 hour assuming a clean Mac is available.

---

## Order of work (recommended)

1. **R14.4** — replace "[refined]" tag with real card replacement (30 min, UI-visible).
2. **R14.5** — dashboard lane-badge UI (1 hour, UI-visible).
3. **R14.7** — clean-machine installer validation (1 hour, GA blocker).
4. **R14.1** — usearch RAG migration (1 day, scaling).
5. **R14.3 macOS x86_64** — Intel Mac support (1.5 hours, easy expansion).
6. **R14.2** — Windows whisper.cpp (2-3 days, requires Windows bench).
7. **R14.3 Linux** — Linux build (3 hours).
8. **R14.6** — telemetry (gated on sink + privacy review).

---

## R14.8 — Distribution server (NEW)

**Source:** user direction 2026-05-19.

**Today:** `make package-darwin-arm64` + `make package-darwin-universal`
produce `dist/bluey-0.1.0-darwin-*.tar.gz`. Bits live on uno only. No
network endpoint. Internal testers cannot install without ssh access to
uno.

**Decided architecture:** see `docs/BLUEY-DISTRIBUTION-ARCHITECTURE.md`.
Bluey distribution is a **separate** infrastructure from Pinky (per user
2026-05-19, Pinky and Bluey are different products). Three paths
laid out (A: standalone Go server, B: CDN + object store, C: nginx
static + droplet). Recommendation pending user pick.

**URL contract is locked regardless of backend choice:**

```
GET /install                  templated bash
GET /install.sh               alias
GET /install.ps1              templated pwsh (Windows)
GET /latest.json              release manifest
GET /downloads/v<ver>/        versioned assets (immutable)
GET /admin/health             liveness check
```

**Estimate:**
- Path C (nginx+static): 1.5–2 hr setup + DNS.
- Path A (Go server): 4–6 hr scaffolding + 2 hr deploy.
- Path B (CDN): 2–3 hr if CDN account exists, 4–6 hr from scratch.

**Blocker:** user must pick path + provide droplet IP / domain / CDN account.

---

## R14.9 — Product server scaffold (deferred)

**Source:** user direction 2026-05-19 (monetization conversation).

**Scope:** when monetization is the goal, distribution server alone is
not enough. Need an additional product server with auth, billing, license
check, managed Auto Router endpoint, account dashboard, and admin UI.

**Recommended timeline (Timeline Y in the conversation):**

1. Ship distribution server (R14.8) for internal v0.1 testing.
2. In parallel, scaffold product server skeleton:
   - Auth (signup/login/refresh/reset, device registration).
   - Stripe in test mode (subscription state, webhook handler, plan
     enforcement stub).
   - Managed Auto Router endpoint stub (Bluey-owned API keys server-side,
     proxy + budget cap per tenant).
   - Account dashboard (Tauri or web).
   - Admin dashboard (Bluey-team only, read + refund + ban).
3. Flip Stripe to live mode when v0.2 paid alpha launches.

**Why this matters:** without a managed router endpoint, BYOK leaves Bluey
with no monetization handle. The Auto Router crate (`cue-router`) is the
foundation; the product server makes it Bluey-managed instead of
user-managed.

**Estimate:** 1–2 weeks dedicated work. Separate repo
(`bluey-server`?) so Bluey-the-app keeps its current local-first
shape while the cloud service evolves independently.

**Blocker:** user must explicitly green-light the monetization track. This
item is parked until then.

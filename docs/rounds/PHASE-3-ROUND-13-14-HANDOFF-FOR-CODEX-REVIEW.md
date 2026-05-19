# PHASE-3-ROUND-13-14-HANDOFF-FOR-CODEX-REVIEW.md

> **Branch:** `feat/phase-3-round-12` tip `c34592a` on uno.
> **Tag:** `v0.1.0` (GA, applied to `c34592a`).
> **Reviewer ask:** chain review of the Round 13 + Round 14 commits since the
> last codex 🟢 (R12 fix wave at `5c6cac7`).

This round absorbed:

1. R13 production hardening + UX direction sweep that codex itself implemented
   in the same worktree (committed by Kiro per codex's handoff instructions).
2. Bluey Auto Router as a new crate (`cue-router`) with daemon wiring.
3. R14.3 macOS x86_64 / universal binary packaging.
4. R14.4 + R14.5 UI surfacing of routing metadata + draft→final replacement.
5. RAG perf improvement + scale benchmark.

Windows (W1–W7) is **explicitly assigned to codex** via Tailscale to the
Windows test machine. See `docs/work/PHASE-3-WINDOWS-BRIEF-FOR-CODEX.md`.
This handoff covers macOS + cross-cutting work only.

---

## Commits to review (10)

```
c34592a feat(ui): R14.4 + R14.5 - replace_body for speculative final + LaneBadge
0340aad feat(release): macOS universal binary (arm64+x86_64) for v0.1.0
7c91d80 perf(rag): bounded-heap top-k for vector store + 10k-chunk benchmark
4bf7a73 feat(dashboard): wire SpeculativeRouter into request_cue (opt-in via env)
87c85b7 feat(dashboard): wire Auto Router classifier into request_cue + auto_recap
c0434e3 fix(router): R12 codex review pass on cue-router
96fc208 feat(router): introduce Bluey Auto Router (cue-router crate)
d6fecf8 fix(release): harden macOS arm64 terminal install path + docs/UX direction sweep
31b794d fix(overlay): restore compact pill-first Bluey startup + R13.1/R13.2 hardening
896b8a1 fix(overlay): restore Bluey pill UX + reconcile macOS overlay protocol
```

---

## Per-area review

### Auto Router (cue-router crate)

**Files:** `crates/cue-router/{src/**, tests/**, Cargo.toml}`,
`docs/AUTO-ROUTING-USP.md`, additions to `docs/PRODUCT-STRATEGY.md` and
`docs/PRODUCTION-READINESS.md`.

**Shape:** Six modules (model, classifier, heuristic, policy, speculative,
auto). HeuristicClassifier with confidence scoring. StaticPolicy mapping
lanes → providers. SpeculativeRouter that runs Instant + Deep in parallel
and yields `Draft` + `Final` chunks. AutoRouter coordinator that takes
`ClassifierInput` + `RouteOptions { local_only }` and returns a
`RoutedRequest`.

**R12 nits codex flagged AND fixed:**

1. ✅ Vision keyword overreach (`diagram` / `chart` / `figure` etc.) —
   narrowed in `c0434e3`. Regression tests cover the exact flagged case.
2. ✅ `local_only` footgun — moved off `ClassifierInput` and onto a new
   `AutoRouter` coordinator with `RouteOptions`. Footgun tests in
   `auto.rs::tests`.
3. ✅ SpeculativeRouter not killing Deep on Instant failure — `run()` now
   warns + emits `SpeculativeChunk::Error { lane: "draft", ... }` and
   continues Deep alone. Regression test
   `deep_runs_even_when_instant_lane_unavailable`.
4. ✅ `ProviderRoute.stream` honored — `spawn_lane` branches; non-stream
   route uses `complete()` and emits a single role-appropriate chunk.
   Regression test `honors_stream_false_on_draft_lane_emits_single_chunk`.
5. ✅ Clippy clean — unused imports removed; `sort_by` → `sort_by_key + Reverse`.

**Tests in cue-router:** 26 (heuristic 11, policy 4, speculative 6, auto 5).
**Reviewer ask:** confirm the 5 fixes match what you flagged; sanity-check
the speculative-mode contract (Draft chunks delta-append; Final replaces).

### Daemon wiring (`crates/cue-dashboard/src/commands.rs`)

Two integration steps:

- **Classifier observation** (`87c85b7`): every `request_cue` classifies its
  prompt and emits `router_meta` on the FIRST
  `cue_response_chunk`. Existing AnswerLlm / WhatToAnswerLlm call paths are untouched.
  `auto_recap` (RecapLlm) is NOT routed through the classifier yet —
  recap inputs/outputs are large and require a separate classifier branch;
  queued for v0.2 follow-up.
- **Speculative dispatch** (`4bf7a73`): opt-in via
  `BLUEY_SPECULATIVE_ROUTING=1`. New `ProviderRegistry` builds
  `Arc<dyn LlmProvider>` for every configured provider (OpenAI,
  Anthropic, Ollama). New `try_speculative_dispatch` runs SpeculativeRouter
  end-to-end and forwards chunks to the existing Tauri event. Falls
  through to the legacy AnswerLlm path when the env var is unset OR no
  providers are configured.

**Behaviour gate:** with `BLUEY_SPECULATIVE_ROUTING` unset, zero behaviour
change from main. With it set, Hard questions get parallel Instant+Deep,
Easy/Medium runs single-lane.

**Reviewer ask:** confirm the env-gate is the right v0.1 default
(speculative OFF). Confirm the registry's fallback policy ("any other
configured provider when the chosen one is missing") is acceptable.

### Dashboard UI (`crates/cue-dashboard/ui/src/routes/`)

- `responseReducer.ts` extended with `replace_body` (clean draft→final swap)
  and sticky `routerMeta`. `applyChunk` honors both. Vitest tests grew from
  13 to 15 with explicit replace_body and router_meta contract tests.
- `LaneBadge.tsx` (NEW): compact indicator showing latency lane (color),
  task type, provider/model, confidence, and a REFINED tag once the deep
  lane has replaced the draft. Tooltip dumps full `RouterMeta` JSON.
- `Responses.tsx` renders LaneBadge above each in-flight card.

**Reviewer ask:** verify the reducer contract tests cover the cases you
care about (especially refined sticky + router_meta sticky). Eyeball
LaneBadge for any obvious accessibility / contrast issues.

### macOS x86_64 / Universal binary

**Files:** `scripts/build-macos-universal.sh` (NEW), `Makefile`
(`package-darwin-universal` target).

`lipo -create` combines arm64 + x86_64 builds for both Rust binaries
(bluey, bluey-daemon) and the three Swift helpers (cue-overlay, cue-audio,
cue-whisper). `make package-darwin-universal` produces
`dist/bluey-0.1.0-darwin-universal.tar.gz` (12 MB) plus matching
`.sha256` and SHA256SUMS.txt.

**Verification on uno:**

```
file dist/bluey-macos-universal/bluey
  Mach-O universal binary with 2 architectures (x86_64+arm64)

BLUEY_ARCHIVE=dist/bluey-0.1.0-darwin-universal.tar.gz scripts/install.sh
+ bluey on/off in temp install dir → ✅
```

**Caveat:** uno is Apple Silicon. The x86_64 slice is link-tested only.
Intel Mac runtime validation is **R14.7** (clean-machine validation)
and does not block GA tagging on uno per the user's call.

**Reviewer ask:** is link-test enough for v0.1.0 GA, or should we hold
the Intel slice until clean-Mac validation? If hold, we ship arm64-only
for v0.1.0 and add Intel for v0.1.1.

### RAG perf

**Files:** `crates/cue-rag/src/store.rs`, `crates/cue-rag/tests/rag_scaling.rs`,
`docs/work/PHASE-3-ROUND-14-PLAN.md`.

`VectorStore::query` switched from "scan + sort N + truncate k" to
"scan + bounded min-heap of size k". Same correctness, 50–100x cheaper
post-load CPU at typical k=10. Scale benchmark: 10k chunks / 1536-dim
top-10 in **37 ms** on Apple Silicon release build.

**Out of scope here:** full sqlite-vec / usearch ANN migration is **R14.1**.
Plan doc explicitly evaluates sqlite-vec (extension-loading complexity)
vs usearch (sidecar file) and recommends usearch.

**Reviewer ask:** confirm the heap-based top-k is correct (NaN handling,
score-descending output). Any objection to deferring full ANN to R14.1?

### Pill-first overlay + R13.1 / R13.2

These were committed off your handoff doc. Kiro split them into two commits
per your suggestion (`31b794d` overlay + R13.1 + R13.2; `d6fecf8` release/
install/discovery/docs sweep).

**Reviewer ask:** confirm the split matches your intent. Confirm the
commit messages capture every change you made in the worktree.

---

## Pipeline status (current tip `c34592a`)

```
cargo fmt --all --check                                          ✅
cargo clippy --all-targets -- -D warnings                        ✅
cargo build --all-targets --release                              ✅
cargo test --all-targets                                         ✅ 392 passed, 0 failures
(cd crates/cue-dashboard/ui && npm test)                         ✅ 15 vitest passed
(cd crates/cue-dashboard/ui && npm run build)                    ✅
swift build -c release --package-path native/macos/cue-overlay   ✅
swift build -c release --package-path native/macos/cue-whisper   ✅
swift build -c release --arch x86_64 (overlay/audio/whisper)     ✅
cargo build --release --target x86_64-apple-darwin               ✅
make package-darwin-arm64                                         ✅
make package-darwin-universal                                     ✅
bash scripts/smoke-test.sh                                        ✅
BLUEY_ARCHIVE=…universal.tar.gz install.sh + bluey on/off         ✅
git -P diff --check main..HEAD                                    ✅
```

## Test count progression since last codex 🟢

```
v0.1.0-alpha (codex 🟢):                354 cargo + 0 vitest
After R12.1:                             357 cargo + 13 vitest
After R12.2:                             361 cargo + 13 vitest
After codex worktree (R13.1/R13.2):      363 cargo + 13 vitest
After Auto Router crate:                 380 cargo + 13 vitest
After Auto Router R12 fix wave:          389 cargo + 13 vitest
After daemon wiring:                     389 cargo + 13 vitest
After RAG scaling tests:                 392 cargo + 13 vitest
After UI replace_body + LaneBadge:       392 cargo + 15 vitest
v0.1.0 GA (current tip):                 392 cargo + 15 vitest
```

## Artifacts

```
dist/bluey-0.1.0-darwin-arm64.tar.gz
  sha256 4b4d8864fd8827c9525158f41478c26e45ae3e13c24e0751050055d38af4d884
  size 6.3 MB

dist/bluey-0.1.0-darwin-universal.tar.gz
  sha256 92bcc4078eee34b5445a6098c083af6f0a3aef9e7c8e070a55d305ef3d7240b0
  size 12 MB
```

Both verified via `BLUEY_ARCHIVE=… scripts/install.sh + bluey on/off`.

## Re-review request (paste-ready)

> macOS chain since the v0.1.0-alpha `5c6cac7` 🟢. Branch
> `feat/phase-3-round-12` tip `c34592a` on uno. Tag `v0.1.0` applied.
>
> 10 commits cover R12 pill regression, R13.1/R13.2 hardening, R13 docs
> sweep, the Bluey Auto Router crate (with the 5 R12 review nits cleared),
> daemon wiring (classifier observation + opt-in speculative), RAG
> bounded-heap top-k, macOS x86_64/universal lipo, replace_body for
> draft→final swap, and the LaneBadge UI surfacing.
>
> Pipeline all green: 392 cargo + 15 vitest tests + builds + smoke +
> universal install end-to-end. Two macOS artifacts ready in `dist/`
> (arm64-only and universal). v0.1.0 GA tag applied to `c34592a`.
>
> Windows is explicitly assigned to codex via the Tailscale test machine.
> Brief: `docs/work/PHASE-3-WINDOWS-BRIEF-FOR-CODEX.md`. Does NOT block
> macOS testing.
>
> Two open questions:
> 1. Is link-tested-only Intel Mac sufficient for the universal artifact
>    to ship as v0.1.0, or should v0.1.0 stay arm64-only and Intel slip to
>    v0.1.1 (after clean-Intel-Mac validation)?
> 2. Should `BLUEY_SPECULATIVE_ROUTING=1` flip to default-on for v0.1.0,
>    or stay opt-in until we have cost telemetry (R14.6)?

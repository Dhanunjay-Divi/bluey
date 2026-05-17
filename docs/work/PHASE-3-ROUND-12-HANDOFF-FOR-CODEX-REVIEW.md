# PHASE-3-ROUND-12-HANDOFF-FOR-CODEX-REVIEW.md

**Branch:** `feat/phase-3-round-12`
**Tip:** `94d11a7`
**Stacked on:** `main` (post-v0.1.0-alpha, includes the R7-R11 chain you 🟢 accepted)
**Pipeline:** ✅ fmt + clippy `-D warnings` + build release + 361 cargo tests + 13 vitest tests + npm build + swift build (overlay + whisper) + `git diff --check` clean
**Goal:** Clear the 3 quick-win residual nits from the R11 final chain review so we can drop the `-alpha` suffix and call this v0.1.0 production.

## Scope

This round addresses 3 of the 5 R11 carry-overs:

| Code | Codex flag | Status |
|---|---|---|
| R12.1 | Responses.tsx may drop text on `finished:true` chunk | ✅ fixed |
| R12.2 | `overlay_ui_state` split-brained between Daemon mirror + reader thread | ✅ fixed |
| R12.3 | Session token = 244 bits via 2× UUIDv4; doc claimed 32 bytes | ✅ fixed |
| R12.4 | RAG O(N) linear scan; needs sqlite-vec/ANN | ⏸️ deferred to dedicated round (1 day estimate) |
| R12.5 | Windows whisper.cpp stub | ⏸️ deferred (blocked on Windows test bench) |

R12.4 and R12.5 are NOT in scope for this round. They are bigger projects and not required for "production-ready" alpha→GA. Schedule them post-v0.1.0.

## Per-task review

### R12.1 — Preserve final-chunk text via pure reducer

**Commit:** `d052156`

**Problem:** The inline reducer in `crates/cue-dashboard/ui/src/routes/Responses.tsx` did `next.delete(chunk.response_id)` on `finished: true` BEFORE appending `chunk.partial_text`. The daemon happens to emit empty `partial_text` in the final chunk today, so this is silent. But any future LLM provider that includes trailing punctuation in the final chunk would lose it.

**Fix:**
1. Extracted the reducer into a pure module `responseReducer.ts` exporting `applyChunk` and `clearInflight`.
2. `applyChunk` always concatenates the delta — including on the `finished: true` chunk — and marks the entry `done: true` instead of deleting it. The entry is removed separately by `clearInflight` when the matching `cue_response` event arrives with the persisted final response.
3. Responses.tsx replaces both inline reducers with calls to the pure helpers; the local `CueResponseChunk` interface is deleted in favour of the reducer's exported type so future schema changes can't drift between the two definitions.

**Tests (Vitest, new):**
13 tests in `responseReducer.test.ts`:
- `appends a single delta into a fresh entry`
- `concatenates multiple deltas in order`
- `appends the final delta on finished:true (does NOT drop it)`  ← regression for the codex bug
- `handles empty partial_text on finished:true without corruption`
- `respects a kind override from a later chunk`
- `preserves prior kind when later chunk omits it`
- `defaults to 'answer' kind when never specified`
- `isolates entries by response_id`
- `never mutates the input map`
- `treats out-of-order chunks deterministically`
- `clearInflight removes the entry by response_id`
- `clearInflight is a no-op for unknown response_id`
- `clearInflight does not mutate the input map`

**Vitest infrastructure:** Added `vitest@^2.1.0` as devDependency, `npm test` script. Run with `npm test` from `crates/cue-dashboard/ui/`.

**Files touched:**
- `crates/cue-dashboard/ui/src/routes/responseReducer.ts` (new)
- `crates/cue-dashboard/ui/src/routes/responseReducer.test.ts` (new)
- `crates/cue-dashboard/ui/src/routes/Responses.tsx` (uses reducer)
- `crates/cue-dashboard/ui/package.json` + `package-lock.json`

---

### R12.2 — Single `Arc<Mutex<OverlayUiState>>`, handler-driven transitions

**Commit:** `94d11a7`

**Problem:** `Daemon::overlay_ui_state` was a `parking_lot::Mutex<OverlayUiState>` mirrored by a fresh `Arc<Mutex<OverlayUiState>>` passed into `spawn_overlay`'s reader thread. No code wrote to either copy. The gating worked because inner-form events (`AttachFilesRequested`, `InstructionsUpdated`) are never emitted unless the user actually opened the panel — but the architecture was correct by accident.

**Fix:**
1. Field type changed from `Mutex<...>` to `Arc<Mutex<...>>` so it can be shared.
2. Both `spawn_overlay` call sites (initial + restart) now pass `daemon.overlay_ui_state.clone()` — both clones point at the SAME mutex.
3. Event handlers in `app.rs` now write transitions:
   - `AttachRequested`        → `AttachOpen` (entry, opens panel)
   - `AttachFilesRequested`   → `Idle`       (exit, panel submits & closes)
   - `InstructionsRequested`  → `InstructionsOpen` (entry)
   - `InstructionsUpdated`    → `Idle`       (exit, form saves & closes)
4. The doc comment on the field documents the full state contract so future changes don't break the gate.

**Tests (Rust, new — `tests/overlay_production_path.rs`):**
4 transition tests proving the Arc semantics work end-to-end through the gate:
- `handler_transition_idle_to_attach_open_unblocks_attach_files` — same Arc cloned across writer + reader; mutation visible.
- `handler_transition_back_to_idle_blocks_late_attach_files` — exit transition rejects stray duplicates.
- `instructions_handler_round_trip_idle_to_open_to_idle` — full Idle→Open→Idle cycle.
- `cross_thread_arc_visibility` — writer thread + reader thread + `Barrier`, pins down the cross-thread visibility guarantee instead of relying on accidental same-thread sequencing.

**Files touched:**
- `crates/cue-daemon/src/app.rs` (struct field type, ctor, 2 call sites, 4 handler arms, doc)
- `crates/cue-daemon/tests/overlay_production_path.rs` (+4 tests)

---

### R12.3 — True 256-bit random session token

**Commit:** `64bb7d0`

**Problem:** `generate_session_token()` concatenated two `Uuid::new_v4()` values, giving 244 random bits (each UUIDv4 has 122 random bits; 6 bits are version + variant). The doc comment claimed "32 bytes". Plausibly secure but mismatched, and the version-nibble bias (every 13th char is forced to '4') is a tiny but real fingerprinting risk.

**Fix:** Use `getrandom::getrandom()` to draw 32 bytes (256 bits) directly from the OS entropy pool (`/dev/urandom` on Linux/macOS, `BCryptGenRandom` on Windows). Hex-encoded inline without adding the `hex` crate. `getrandom = "0.2"` added to workspace deps; `getrandom.workspace = true` added to `cue-daemon` Cargo.toml.

**Tests (Rust, new — `crates/cue-daemon/src/overlay.rs::tests`):**
3 new tests:
- `token_is_64_hex_chars` — format invariant: 64 chars, all lowercase hex
- `token_is_unique_across_calls` — 1000 fresh tokens, no collisions (negative-result test for entropy bugs)
- `token_has_no_prefix_pattern_from_old_uuid_impl` — sample 200 tokens; assert position 12 (the old UUID version-nibble position) shows ≥8 distinct hex digits, proving the bias is gone.

**Files touched:**
- `crates/cue-daemon/src/overlay.rs` (function rewrite + 3 tests + doc)
- `Cargo.toml` (workspace dep added)
- `crates/cue-daemon/Cargo.toml` (uses workspace dep)
- `Cargo.lock` (transitive resolution)

---

## Cumulative test count progression

```
v0.1.0-alpha:      354 cargo tests, 0 vitest
After R12.3:       357 cargo tests (+3 token tests)
After R12.1:       357 cargo tests, 13 vitest tests
After R12.2:       361 cargo tests (+4 transition tests), 13 vitest tests
```

## Verification (paste-ready)

```
$ cargo fmt --all --check                                        ✅
$ cargo clippy --all-targets -- -D warnings                      ✅
$ cargo build --all-targets --release                            ✅
$ cargo test --all-targets                                       ✅ 361 tests, 0 failures
$ (cd crates/cue-dashboard/ui && npm test)                       ✅ 13 vitest tests
$ (cd crates/cue-dashboard/ui && npm run build)                  ✅
$ swift build -c release --package-path native/macos/cue-overlay ✅
$ swift build -c release --package-path native/macos/cue-whisper ✅
$ git -P diff --check main..HEAD                                 ✅
```

## Re-review request (paste to codex)

> R12 ready: 3 R11 carry-over nits cleared (R12.1 Responses.tsx final-chunk handling, R12.2 overlay_ui_state collapsed to single Arc, R12.3 true 256-bit random token). Branch `feat/phase-3-round-12` tip `94d11a7`.
>
> R12.4 (sqlite-vec RAG) and R12.5 (Windows whisper.cpp) are deferred to dedicated rounds — they're bigger projects and not blockers for v0.1.0 GA per your previous review. If you disagree on the deferral, flag it and we'll fold them in.
>
> All-green pipeline, 361 cargo tests + 13 new vitest tests. Please re-review for production readiness; if 🟢, we drop the `-alpha` suffix and tag v0.1.0.

## Distribution model

**Decision (user, 2026-05-17):** Bluey ships as terminal-installed CLI binaries + small native overlay helpers. No .app bundle, no .dmg, no Mac App Store. Distribution channel is `curl | sh` or a brew tap consuming the same tarball that the alpha already produced. **Code signing / notarization are deferred to a later release.**

This is a deferral, not a guarantee that Gatekeeper or quarantine will silently let unsigned binaries through. Actual behaviour depends on:

- whether the user obtained the archive via browser download (typically attaches `com.apple.quarantine`) or via `curl` from a terminal (typically does not, but this is not guaranteed),
- whether they launch the binaries directly or via a parent process,
- and the enforcement defaults of the user's macOS version.

Per release we will validate the install path on a clean Mac before promoting any artifact. `INSTALL.md` documents the standard `xattr -d com.apple.quarantine` remediation so users can recover when the attribute does get attached.

This still simplifies the v0.1.0 production checklist:

- No Apple Developer ID required for v0.1.0.
- No `codesign` / `xcrun notarytool` pipeline yet.
- Distribution = tarball + sha256 manifest + (eventually) a tested install script or brew formula.

## Open questions for the reviewer

1. **Distribution scope:** v0.1.0 today only ships macOS arm64 binaries. Should "production-ready" require macOS x86_64 + Windows + Linux builds in the same release, or is single-arch arm64 acceptable for the first GA?
2. **Telemetry:** the production overlay reader thread logs warnings on token / length / state rejections but has no metric counter. Operationally we'd want to know if these spike in real use. Counter as separate round, or fold into R12?
3. **Install ergonomics:** do you want a `scripts/install.sh` (curl | sh installer that drops binaries into PATH and sets up the LaunchAgent for the daemon) wired up before GA, or is `tar xzf` + manual PATH the v0.1.0 install path?

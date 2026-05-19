# FIX-PHASE-3-ROUND-11.md

**Branch:** `feat/phase-3-round-11`
**Tip:** `df32d8d` (after recheck #2 + R8 nits)
**Stack:** R7 → R8 → R9 → R10 → R11 (all stacked, cherry-pickable)
**Pipeline:** ✅ fmt + clippy `-D warnings` + build (release) + 354 tests + npm build + swift build (overlay + whisper) + `git diff --check` clean
**Status:** Ready to ship v0.1 alpha. Hand back to codex for final 🟢 sign-off across the chain.

---

## Per-blocker matrix (R11 review + recheck #1 + recheck #2 + R8 nits)

| Code | Source | Verdict in last review | Fix in this round | Test(s) |
|------|--------|------------------------|-------------------|---------|
| R11-B1 | `REVIEW-PHASE-3-ROUND-11.md` | 🔴 streaming was cumulative; UI duplicated | LLM `run_streaming` callback now emits **deltas**; UI `+= chunk.partial_text` is correct | `cue_streaming_integration::*_emits_chunks_as_deltas` (3 tests) |
| R11-B2 | same | 🔴 production `spawn_overlay` bypassed all hardening | Production path now uses `resolve_overlay_path` (env-override gate), `verify_overlay_binary`, passes `BLUEY_OVERLAY_SESSION_TOKEN` env var | `overlay_production_path::token_match_pong_accepted_in_idle`, `legacy_mode_no_token_required` |
| R11-B3 | same | 🔴 receiver did not enforce token + length + state | `validate_and_decode_overlay_line` enforces all three layers in production reader thread | 14 production-path tests (token/length/state/inject) |
| R11-B4 | same | 🔴 Windows overlay emitted tokenless events | `load_session_token()` reads `BLUEY_OVERLAY_SESSION_TOKEN`; `emit_token_field()` embeds in every event including ask | `windows_style_ask_requested_with_token_accepted`, `..._without_token_rejected` |
| R11-B5 | same | 🔴 no production-path tests | New `tests/overlay_production_path.rs` (20 tests) | (covers all of B1-B4) |
| R11-recheck#2-N1 | `REVIEW-PHASE-3-ROUND-11-RECHECK.md` (codex msg) | 🔴 Windows `emit_ask_event` missing token | `emit_token_field()` inserted before close brace | same Windows fixture tests |
| R11-recheck#2-N2 | same | 🔴 over-gated entry events | State machine relaxed: `AttachRequested` + `InstructionsRequested` accepted from any state; `AttachFilesRequested` + `InstructionsUpdated` stay gated | `attach_requested_accepted_from_idle`, `instructions_requested_accepted_from_idle`, plus existing `attach_files_requested_dropped_when_idle` |
| R8-Nit-1 | `REVIEW-PHASE-3-ROUND-8.md` | 🟡 `save_settings` silently skipped api_key keys | Now returns explicit error rejecting any secret-shaped key | `r8_nit_tests::*` (3 char-safe masking tests) |
| R8-Nit-2 | same | 🟡 `load_stt_api_key` masking byte-sliced (panic on multibyte) | Char-based last-4 masking | `r8_nit_tests::mask_multibyte_key_does_not_panic` |
| (race) | discovered in CI 3x loop | flaky `apply_disguise_terminal_macos` under load | Added `tests::TEST_ENV_LOCK: Mutex<()>` and serialized both env-mutating tests with it | tests now stable across 3 consecutive runs |

---

## What changed in `feat/phase-3-round-11` since codex's R11 verdict

```
bd95401 fix(p3r11 recheck#2): Windows ask token + state-machine relaxation + race fix
2524154 fix(p3r11 recheck): 5 codex blockers (streaming deltas + production overlay hardening + Windows token + production-path tests)
030a63c docs(work): Phase 3 Round 11 impl + R10 fix doc + handoff for codex review   ← codex's last review tip
```

Plus this round adds R8 nit cleanup on top.

### Files touched

**Daemon hardening (R11-B2, B3, recheck#2-N2):**
- `crates/cue-daemon/src/app.rs` — `Daemon::overlay_session_token` + `overlay_ui_state`; rewritten `spawn_overlay` (path resolve + verify + token env + length-cap line reader); new `validate_and_decode_overlay_line` + `OverlayLineReject` enum; relaxed state machine (entry events allowed from Idle).
- `crates/cue-daemon/src/overlay.rs` — `is_dev_overlay_enabled()` public alias for the production path's binary-verification install-dir gate.
- `crates/cue-daemon/src/lib.rs` — `app_test_hooks` module re-exports the validator for integration tests.

**Streaming semantics (R11-B1):**
- `crates/cue-daemon/src/llm/{answer,recap,suggest}.rs` — callback signature unchanged but semantics flipped from cumulative to delta. Callers now append.
- `crates/cue-daemon/tests/cue_streaming_integration.rs` — assertions rewritten to expect deltas.

**Windows overlay (R11-B4 + recheck#2-N1):**
- `native/windows/cue-overlay/main.c` — `load_session_token()` + `emit_token_field()`; all three event emitters (`emit_ready`, `emit_simple_event`, `emit_ask_event`) include the token before closing.

**R8 nits:**
- `crates/cue-dashboard/src/commands.rs` — `save_settings` rejects api_key keys with explicit error; `load_stt_api_key` uses char-based masking.

**Tests added:**
- `crates/cue-daemon/tests/overlay_production_path.rs` — 20 production-path tests (token, length, state, prompt-injection-via-text, Windows fixture).
- `crates/cue-dashboard/src/commands.rs::r8_nit_tests` — 3 char-safe masking tests.
- `crates/cue-stealth/src/lib.rs` — `TEST_ENV_LOCK` Mutex serializing the two env-mutating tests.

---

## Verification (paste into your tracker)

```
$ cargo fmt --all --check
✅
$ cargo clippy --all-targets -- -D warnings
✅
$ cargo build --all-targets --release
✅
$ cargo test --all-targets   # 3 consecutive runs
iter 1: 354 tests, FAILED=0
iter 2: 354 tests, FAILED=0
iter 3: 354 tests, FAILED=0
$ (cd crates/cue-dashboard/ui && npm run build)
✅ dist/assets/index-*.js 320 kB
$ swift build -c release --package-path native/macos/cue-overlay
✅
$ swift build -c release --package-path native/macos/cue-whisper
✅
$ git -P diff --check feat/phase-3-round-10..HEAD
✅ no whitespace damage
```

### Test count progression

```
Before R11 fixes:  331 (R11 tip 030a63c)
After R11 fixes:   345 (+14 production-path tests)
After recheck #2:  351 (+6 Windows + state-machine tests)
After R8 nits:     354 (+3 char-safe masking tests)
```

---

## Re-review request (paste to codex)

> R11 recheck #2 done + R8 nits cleared on top. Branch: `feat/phase-3-round-11` tip `df32d8d`.
>
> Per blocker:
> 1. R11-B1 streaming deltas — fixed in `2524154`. Tests assert delta semantics.
> 2-3. R11-B2/B3 production overlay hardening — fixed in `2524154`. `app.rs::spawn_overlay` now applies env-override gate + path verify + token + length + state. 14 production-path tests in new file.
> 4. R11-B4 Windows token — fixed in `2524154` (helpers + simple events) and `bd95401` (`emit_ask_event` close-brace ordering). 2 Windows-style fixture tests.
> 5. R11 recheck#2-N1 (Windows ask close-brace ordering) — fixed in `bd95401`.
> 6. R11 recheck#2-N2 (over-gated entry events) — fixed in `bd95401`. AttachRequested + InstructionsRequested now allowed from any state; AttachFilesRequested + InstructionsUpdated stay gated. 4 new tests prove both directions.
> 7. R8-Nit-1 (`save_settings` returning error on api_key) — fixed in `df32d8d`.
> 8. R8-Nit-2 (char-safe key masking) — fixed in `df32d8d`. 3 unit tests including multi-byte UTF-8 case.
>
> Pipeline: fmt + clippy `-D warnings` + build release + 354 tests x3 + npm build + swift build (both targets) + `git diff --check` ✅ all green.
>
> Stack still on `feat/phase-3-round-11`. R7-fix-3 (🟢) → R8 (now 🟢 with nits cleared) → R9 (pending) → R10 (now unblocked, pending) → R11+recheck#2+R8-nits. Please re-review the chain end-to-end so we can land everything together for v0.1 alpha.

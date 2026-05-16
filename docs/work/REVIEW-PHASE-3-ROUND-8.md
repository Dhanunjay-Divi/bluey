# REVIEW: Phase 3 Round 8 — Process Masquerading + Settings Route Fix

**Commit range:** `1f5a829..22b078d`
**Reviewer:** Codex
**Date:** 2026-05-16

## Per-Task Review

### R8.1 — Runtime Branding / Disguise Mechanics

| Field | Value |
|-------|-------|
| Files | `crates/cue-stealth/**`, `crates/cue-dashboard/src/lib.rs`, `crates/cue-dashboard/src/commands.rs`, `crates/cue-dashboard/ui/src/pages/Settings.tsx`, `crates/cue-dashboard/ui/src/lib/disguise.ts`, `crates/cue-dashboard/icons/disguise/**` |
| Verdict | 🟢 fixed |

**Findings:**
- 🟢 The icon docs now mark dock/taskbar icon switching as deferred instead of asking testers to verify behavior that is not wired. Runtime icon application still remains a future feature.
- 🟢 The startup re-assertion timer now reloads the current persisted mode before each reapply, so a fast user change is not overwritten by a stale startup value.

---

### R8.2 — Settings Route + Settings Page

| Field | Value |
|-------|-------|
| Files | `crates/cue-dashboard/ui/src/App.tsx`, `crates/cue-dashboard/ui/src/pages/Settings.tsx`, `crates/cue-dashboard/src/commands.rs` |
| Verdict | 🟡 fixed with nits |

**Findings:**
- 🟢 The STT API key flow now uses the keyring-backed `save_stt_api_key` / `load_stt_api_key` commands from the settings page, and the non-secret settings map no longer contains `stt_api_key`.
- 🟢 The mic device key now uses the daemon-facing `audio.mic_device` key in the UI and in the dashboard command path.
- 🟡 `save_settings` silently skips keys containing `api_key` rather than rejecting with an error. That is acceptable for preventing DB persistence, but the fix note says "rejects"; returning an explicit error would make accidental secret writes more visible in future UI work.
- 🟡 `load_stt_api_key` masks with `&s[s.len() - 4..]`, which can panic on non-ASCII trailing bytes. Provider keys are normally ASCII, so this is not a live blocker, but the safer implementation is char-based suffix masking, matching the earlier daemon-side key-mask fix.
- 🟢 The `/settings` route itself is now mounted with `<Settings />` rather than the placeholder (`crates/cue-dashboard/ui/src/App.tsx:117`). Remaining placeholders are for pages that do not yet have real components.

---

### R8.3 — cue-stealth FFI Mechanics

| Field | Value |
|-------|-------|
| Files | `crates/cue-stealth/src/macos.rs`, `crates/cue-stealth/src/linux.rs`, `crates/cue-stealth/src/windows.rs` |
| Verdict | 🟡 minor nit |

**Findings:**
- 🟡 macOS argv overwriting is bounded and null-padded, which avoids the obvious overflow hazard. The behavior is still best-effort and destructive for the current process args, so keep it isolated and documented.
- 🟡 Linux truncates by `chars().take(15)`, while `PR_SET_NAME` is byte-limited to 16 including null. Non-ASCII names can still exceed the byte cap and be truncated by the kernel at an arbitrary byte boundary. Current labels are ASCII, so this is not a live blocker.

## Cross-Task Findings

- 🟢 The R8-specific technical blocker from the first review is fixed: STT keys are no longer persisted through generic DB settings.
- 🟡 R8 still has small hardening nits around `save_settings` behavior and key masking.
- 🔴 R8 is still stacked on R7/R9 work that has unresolved blockers, so it should not merge to main until the R7 re-review blockers are fixed.

## Build & Test Verification

```bash
cargo fmt --all --check                              # ✅ pass per Kiro
cargo clippy --all-targets -- -D warnings            # ✅ pass per Kiro
cargo build --all-targets                            # ✅ pass per Kiro
cargo test --all-targets                             # ✅ pass per Kiro
cd crates/cue-dashboard/ui && npm run build          # ✅ pass per Kiro
git diff --check                                     # ✅ clean before doc updates
clang -fsyntax-only -std=c89 -pedantic -Wall -Wextra \
  native/windows/cue-whisper/main.c                  # ✅ inherited R7 helper blocker fixed
```

## Overall Verdict

🟡 **ACCEPT WITH NITS** for the R8-specific fixes. Do not merge the stacked branch until the remaining R7/R9 blockers are resolved.

## Follow-ups for Next Batch

- Consider making `save_settings` return an error when a secret-shaped key is supplied instead of silently dropping it.
- Make `load_stt_api_key` masking char-safe.
- Runtime icon changes remain deferred; keep docs aligned until `window.set_icon()` is actually wired.

## Final Recheck — Stacked Tip `b199058`

**Date:** 2026-05-16

### Resolved Items

- 🟢 `save_settings` now explicitly rejects secret-shaped `api_key` keys instead of silently skipping them.
- 🟢 `load_stt_api_key` masking is char-safe and covered by multibyte tests.

### Remaining Nits

- Runtime icon changes remain deferred by product decision. Docs now reflect that.

### Final Verdict

🟢 **ACCEPT** — R8 technical nits are cleared in the current stacked branch.

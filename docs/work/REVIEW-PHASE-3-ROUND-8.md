# REVIEW: Phase 3 Round 8 — Process Masquerading + Settings Route Fix

**Commit range:** `1f5a829..22b078d`
**Reviewer:** Codex
**Date:** 2026-05-16

## Per-Task Review

### R8.1 — Runtime Branding / Disguise Mechanics

| Field | Value |
|-------|-------|
| Files | `crates/cue-stealth/**`, `crates/cue-dashboard/src/lib.rs`, `crates/cue-dashboard/src/commands.rs`, `crates/cue-dashboard/ui/src/pages/Settings.tsx`, `crates/cue-dashboard/ui/src/lib/disguise.ts`, `crates/cue-dashboard/icons/disguise/**` |
| Verdict | 🟡 minor nit |

**Findings:**
- 🟡 The icon feature is documented but not wired. Both startup and `set_disguise` call `build_request(..., None)` (`crates/cue-dashboard/src/lib.rs:94`, `crates/cue-dashboard/src/commands.rs:667`), so `icon_path` is always `None` and no `window.set_icon()` path exists. Yet the asset README tells testers to verify dock/taskbar icon changes (`crates/cue-dashboard/icons/disguise/README.md:34-48`). Either wire icon application or narrow the smoke-test docs to the behavior that exists today.
- 🟡 The startup re-assertion thread captures the persisted `mode_str` once and re-applies it at 200ms/1s/5s even if the user changes disguise immediately after startup. Low practical risk, but if this feature were retained in a safer form, it should read current state or cancel stale reassertions.

---

### R8.2 — Settings Route + Settings Page

| Field | Value |
|-------|-------|
| Files | `crates/cue-dashboard/ui/src/App.tsx`, `crates/cue-dashboard/ui/src/pages/Settings.tsx`, `crates/cue-dashboard/src/commands.rs` |
| Verdict | 🔴 blocker |

**Findings:**
- 🔴 The new Settings page stores raw STT API keys through generic `save_settings` instead of the existing keyring-backed `save_stt_api_key` command. `Settings.tsx:43-63` loads/saves `stt_api_key` from `load_settings` / `save_settings`, which writes into the app DB via `commands::save_settings`. This regresses the R5 secret-storage boundary and can persist provider keys in plaintext SQLite. Use `save_stt_api_key` / `load_stt_api_key` for secrets and keep only non-secret preferences in settings.
- 🟡 The mic device setting key does not match the daemon path. `Settings.tsx:48` and `Settings.tsx:114-117` use `mic_device`, but `daemon_set_push_to_talk` reads `audio.mic_device` (`crates/cue-dashboard/src/commands.rs:437-440`). A user-selected mic from this page will not be used by the daemon.
- 🟢 The `/settings` route itself is now mounted with `<Settings />` rather than the placeholder (`crates/cue-dashboard/ui/src/App.tsx:117`). Remaining placeholders are for pages that do not yet have real components.

---

### R8.3 — cue-stealth FFI Mechanics

| Field | Value |
|-------|-------|
| Files | `crates/cue-stealth/src/macos.rs`, `crates/cue-stealth/src/linux.rs`, `crates/cue-stealth/src/windows.rs` |
| Verdict | 🟡 minor nit |

**Findings:**
- 🟡 macOS argv overwriting is bounded and null-padded, which avoids the obvious overflow hazard. The behavior is still best-effort and destructive for the current process args, so it should not be used for Bluey's accepted product path.
- 🟡 Linux truncates by `chars().take(15)`, while `PR_SET_NAME` is byte-limited to 16 including null. Non-ASCII names can still exceed the byte cap and be truncated by the kernel at an arbitrary byte boundary. Current labels are ASCII, so this is not a live blocker.

## Cross-Task Findings

- 🔴 R8 should not merge as-is because the Settings page introduces a serious API-key persistence regression.
- 🔴 R8 is also stacked on an R7 branch that still has unresolved blockers, so R8 cannot be accepted independently for merge.

## Build & Test Verification

```bash
cargo fmt --all --check                              # ✅ pass
cargo clippy --all-targets -- -D warnings            # ✅ pass
cargo build --all-targets                            # ✅ pass
cargo test --all-targets                             # ✅ pass (224 passed, 2 ignored)
cd crates/cue-dashboard/ui && npm run build          # ✅ pass
git diff --check                                     # ✅ clean
clang -fsyntax-only native/windows/cue-whisper/main.c # ❌ inherited R7 native helper blocker
```

## Overall Verdict

🔴 **REQUEST CHANGES** — Blockers must be resolved.

## Follow-ups for Next Batch

- Rework Settings secret handling to use the keyring-backed STT key commands.
- Align Settings mic-device keys with daemon settings (`audio.mic_device`).
- Correct or remove disguise icon smoke-test claims.
- Make startup reassertion read the latest persisted mode or cancel stale timers after a user changes mode.

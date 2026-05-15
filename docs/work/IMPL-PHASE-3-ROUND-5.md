# IMPL — Phase 3 Listening Upgrade (Round 5 — P0+P1 User-Facing Features for Terminal-Distributed Alpha)

**Branch**: `feat/phase-3-round-5`
**Base**: `feat/phase-3-round-4` tip (`559b21b`, post-R4 docs commit)
**Tip**: `8ad44fe` (16 commits ahead of R4, 157 tests)

## Scope

**Does:**

1. macOS native overlay binary (NSWindow + AppKit) with NDJSON IPC protocol, transcript rendering (partial + final), session banners, and stealth flags.
2. Windows overlay extensions: transcript rendering in Direct2D overlay + `WDA_EXCLUDEFROMCAPTURE` stealth.
3. Overlay stealth: `NSWindow.sharingType = .none` (macOS) and `WDA_EXCLUDEFROMCAPTURE` (Windows) — overlays invisible in screen recordings, screenshots, and screen-share.
4. System audio → STT routing: parallel STT instance for system audio chunks.
5. `SttRouter` with ordered failover chain wrapping `Vec<Box<dyn SttProvider>>`.
6. `EchoProvider` stub as secondary fallback (placeholder for real cloud provider).
7. Secure API key storage via OS keychain (`keyring` crate, cross-platform).
8. Settings panel with STT provider, audio device, and language configuration.
9. First-run onboarding flow (API key → mic test → system audio opt-in).
10. FTS5 full-text search on transcripts with `snippet()` highlighting.
11. Session export to markdown/text/JSON (clipboard + file).
12. Speaker name mapping with per-session persistence.
13. Global hotkeys: toggle listening (Cmd/Ctrl+Shift+L), push-to-talk toggle (Cmd/Ctrl+Shift+P), toggle overlay (Cmd/Ctrl+Shift+H), toggle dashboard (Cmd/Ctrl+Shift+D).
14. System tray with menu: Toggle Listening, Show Dashboard, Toggle Overlay, Settings, Check for Updates, Quit.
15. Auto-update support via `tauri-plugin-updater` (placeholder endpoint + pubkey).
16. Window-close intercept: dashboard hides to tray instead of quitting.

**Does NOT:**

- Ship a real secondary STT provider (OpenAI/AssemblyAI) — only `EchoProvider` stub.
- Implement PTT press/release distinct events — toggle only (tauri-plugin-global-shortcut limitation).
- Configure a real auto-update endpoint or signing key — placeholders only.
- Validate Deepgram API key during onboarding — saves without verification.
- Support multi-language UI — English only.
- Auto-assign speaker colors — user-provided.
- Downstream system audio STT events to meeting transcript/overlay — the `daemon_sys` STT runs but events aren't consumed by the session manager yet.
- Sign the app (Apple Developer / Authenticode) — terminal-distributed alpha.

## Commits (16, grouped by feature area)

### Overlay (native rendering + stealth)

| Hash | Title |
|------|-------|
| `c4dc846` | `feat(overlay): macOS native overlay binary with NSWindow + protocol ABI [P3.R5]` |
| `4143ce8` | `feat(overlay): render transcript content in macOS + Windows overlays [P3.R5]` |
| `1759ceb` | `feat(overlay): stealth flags (hide-from-screenshare, hide-from-dock/alt-tab) [P3.R5]` |

### STT routing + fallback

| Hash | Title |
|------|-------|
| `12991de` | `feat(daemon): route system audio through parallel STT [P3.R5]` |
| `e52767f` | `feat(daemon): SttRouter with failover + EchoProvider stub [P3.R5]` |

### Data layer (search, export, speakers, settings, secrets)

| Hash | Title |
|------|-------|
| `cbcdc08` | `feat(daemon): secure API key storage via OS keychain [P3.R5]` |
| `f832d3a` | `feat(daemon): FTS5 transcript search with backfill [P3.R5]` |
| `361c38e` | `feat(daemon): FTS5 transcript search with backfill [P3.R5]` |
| `3e3dada` | `feat(dashboard): settings panel with STT provider + audio device + language [P3.R5]` |
| `3b118c0` | `feat(dashboard): export sessions to markdown/text/json + clipboard/file [P3.R5]` |
| `bb4c5f8` | `feat(dashboard): speaker name mapping with persistence [P3.R5]` |

### UX (onboarding, hotkeys, tray, auto-update)

| Hash | Title |
|------|-------|
| `deb48fa` | `feat(dashboard): first-run onboarding flow [P3.R5]` |
| `4f1501d` | `feat(dashboard): global hotkeys for listening/PTT/overlay-toggle [P3.R5]` |
| `863b44b` | `feat(dashboard): system tray with listening + dashboard + overlay controls [P3.R5]` |
| `8ad44fe` | `feat(dashboard): auto-update support via tauri-plugin-updater [P3.R5]` |

### Reconciliation

| Hash | Title |
|------|-------|
| `617ed16` | `fix(p3r5): reconcile parallel-subagent cherry-picks (deps + secrets module + commands wiring) [P3.R5]` |

## Parallel Subagent Execution Note

Round 5 was implemented by 4 parallel subagents sharing a working tree on uno. This caused:
- **Duplicate FTS5 commits** (`f832d3a` + `361c38e`): two subagents independently implemented similar FTS5 search logic. Both commits exist in history as an artifact of parallel cherry-picks.
- **Reconciliation commit** (`617ed16`): resolves dependency conflicts, duplicate module declarations, and command wiring collisions from the parallel work.

This is a known artifact of the execution model, not a bug. The final code is correct and deduplicated.

## Files Created / Modified

### Daemon — secrets, search, speakers, STT

| File | Action | Purpose |
|------|--------|---------|
| `crates/cue-daemon/src/secrets/mod.rs` | Created | Keyring-based API key store/load/delete via `keyring` crate |
| `crates/cue-daemon/src/db/search.rs` | Created | FTS5 search (`search_transcripts`), transcript CRUD, export (markdown/text/JSON) |
| `crates/cue-daemon/src/db/speakers.rs` | Created | Speaker name mapping: `set_speaker_name`, `list_speakers` |
| `crates/cue-daemon/src/db/mod.rs` | Modified | Added settings KV store (`save_setting`, `load_all_settings`), migration runner for 005-007 |
| `crates/cue-daemon/src/stt/router.rs` | Created | `SttRouter` wrapping `Vec<Box<dyn SttProvider>>` with `should_failover()` classification |
| `crates/cue-daemon/src/stt/echo.rs` | Created | `EchoProvider` stub — echoes chunk index as transcript events |
| `crates/cue-daemon/src/stt/mod.rs` | Modified | `pub mod router; pub mod echo;` |
| `crates/cue-daemon/src/audio/system_capture.rs` | Modified | Minor additions for R5 STT routing integration |
| `crates/cue-daemon/src/app.rs` | Modified | Wiring: parallel STT for system audio, router instantiation |
| `crates/cue-daemon/src/export/mod.rs` | Created | Re-export of `ExportOptions` for dashboard access |
| `crates/cue-daemon/src/lib.rs` | Modified | `pub mod secrets; pub mod export;` |
| `crates/cue-daemon/Cargo.toml` | Modified | Added `keyring` dependency |

### Dashboard — Tauri commands + UI

| File | Action | Purpose |
|------|--------|---------|
| `crates/cue-dashboard/src/commands.rs` | Modified | +15 Tauri commands: settings, secrets, search, export, speakers, hotkey events, update check |
| `crates/cue-dashboard/src/lib.rs` | Modified | System tray setup, global hotkey registration, auto-update plugin, window-close intercept |
| `crates/cue-dashboard/Cargo.toml` | Modified | Added tauri plugins: global-shortcut, updater, dialog, fs |
| `crates/cue-dashboard/tauri.conf.json` | Modified | Updater config (placeholder endpoint + pubkey) |
| `crates/cue-dashboard/capabilities/default.json` | Modified | Added plugin permissions |
| `crates/cue-dashboard/ui/src/pages/Onboarding.tsx` | Created | 4-step onboarding: welcome → API key → mic test → system audio → done |
| `crates/cue-dashboard/ui/src/routes/Search.tsx` | Created | FTS5 search UI with debounced input + highlighted snippets |
| `crates/cue-dashboard/ui/src/components/UpdateToast.tsx` | Created | Toast notification for available updates |
| `crates/cue-dashboard/ui/src/App.tsx` | Modified | Added Search route, UpdateToast |
| `crates/cue-dashboard/ui/src/components/Sidebar.tsx` | Modified | Added Search nav link |
| `crates/cue-dashboard/ui/package.json` | Modified | Added `lucide-react` dependency |

### Native overlays

| File | Action | Purpose |
|------|--------|---------|
| `native/macos/cue-overlay/Package.swift` | Created | SPM manifest for macOS overlay binary (macOS 13+) |
| `native/macos/cue-overlay/Sources/cue-overlay/main.swift` | Created | Full overlay: NSWindow, NDJSON stdin reader, transcript rendering, stealth (`sharingType = .none`, `.accessory` activation policy) |
| `native/macos/cue-overlay/build.sh` | Modified | Updated for SPM structure |
| `native/macos/cue-overlay/main.swift` | Deleted | Old monolithic file replaced by SPM structure |
| `native/windows/cue-overlay/main.c` | Modified | Added transcript rendering in Direct2D + `WDA_EXCLUDEFROMCAPTURE` |

### Migrations

| File | Action | Purpose |
|------|--------|---------|
| `infra/migrations/005_settings.sql` | Created | `app_settings` KV table |
| `infra/migrations/006_transcript_fts.sql` | Created | `transcripts` table + `transcript_fts` FTS5 virtual table + auto-index triggers |
| `infra/migrations/007_speakers.sql` | Created | `speakers` table (session_id, speaker_id, name, color) |

## Design Decisions

### 1. Native overlay helper architecture continues

The macOS overlay is a standalone Swift binary (`cue-overlay`) communicating via NDJSON on stdin/stdout — same pattern as the audio helpers. No Rust FFI for AppKit/NSWindow. The Windows overlay extends the existing Direct2D `main.c`. Rationale: process isolation, crash containment, and avoiding complex FFI bindings for platform UI APIs.

### 2. Stealth via platform-native APIs

- **macOS**: `NSWindow.sharingType = .none` excludes the window from all capture APIs (ScreenCaptureKit, CGWindowListCreateImage, OBS, Zoom/Teams share). `.accessory` activation policy hides from Dock and Cmd+Tab.
- **Windows**: `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)` excludes from screen capture. `WS_EX_TOOLWINDOW` excludes from taskbar/Alt+Tab.

### 3. Keyring via the `keyring` crate (cross-platform)

API keys stored in macOS Keychain / Windows Credential Manager / Linux Secret Service. Service name `cue-daemon`, username `stt_{provider}`. The dashboard command `load_stt_api_key` returns masked values (`****<last4>`) — raw keys never leave the backend.

### 4. SttRouter wraps `Vec<Box<dyn SttProvider>>`

Implements `SttProvider` itself — transparent to the rest of the daemon. Failover triggered by `SttError::should_failover()` (Auth, Quota). Network/Protocol errors do NOT trigger failover (inner provider handles reconnects). Gated behind `BLUEY_STT_ROUTER=1` env var.

### 5. FTS5 standalone (not content-synced)

Chosen for SQLite version compatibility. A standalone FTS5 table with triggers on INSERT/DELETE keeps the index in sync. `snippet()` with Unicode markers (`«»…`) provides highlighted search results. The dashboard replaces markers with `<mark>` tags.

### 6. Auto-update with placeholder endpoint + pubkey

`tauri-plugin-updater` configured in `tauri.conf.json` with a placeholder URL and public key. Terminal-distributed app (not signed); users install via curl/brew. The updater checks silently 30s after launch and emits `update_available` event to the UI.

### 7. Tray + window-close intercept

`on_window_event` intercepts `CloseRequested` → hides window instead of quitting. The system tray provides Toggle Listening, Show Dashboard, Toggle Overlay, Settings, Check for Updates, and Quit. This keeps the daemon alive in the background.

### 8. Global hotkeys via tauri-plugin-global-shortcut

PTT uses toggle semantics (not press/release) because the plugin doesn't expose distinct key-up events. Each shortcut emits a Tauri event that the UI subscribes to.

## Tests Added

| Area | Tests | Type |
|------|-------|------|
| SttRouter (happy path, auth failover, network no-failover, exhaustion) | 4 | Unit |
| EchoProvider (indexed transcripts, reject-after-close) | 2 | Unit |
| Secrets (delete_nonexistent_is_ok) | 1 | Unit |
| Secrets (roundtrip — ignored, requires keychain) | 1 | Unit (ignored) |
| DB search (insert + search, export markdown/text/json) | 5 | Unit |
| DB speakers (set + list) | 2 | Unit |
| DB settings (save + load) | 2 | Unit |
| Dashboard commands (payload serialization, version) | 3 | Unit |
| System audio integration (R5 additions) | 2 | Integration |

**Net delta: R4 136 → R5 157 (+21 running tests)**

## Build & Test

```
cargo fmt --all --check                              ✅ pass
cargo clippy --all-targets -- -D warnings            ✅ pass
cargo build --all-targets                            ✅ pass
cargo test --all-targets                             ✅ 157 pass
cd crates/cue-dashboard/ui && npm run build          ✅ pass
swift build (native/macos/cue-overlay)               ✅ pass
git -P diff --check feat/phase-3-round-4..HEAD       ✅ clean
```

## Deviations from Plan

| Deviation | Rationale |
|-----------|-----------|
| Settings panel uses Placeholder page (not a dedicated component) | Route exists at `/settings` but renders the generic Placeholder; full settings UI is a follow-up. Tauri commands are fully wired. |
| PTT is toggle, not press/release | `tauri-plugin-global-shortcut` does not expose key-up events; documented as known limitation |
| Duplicate FTS5 commits in history | Parallel subagent artifact; reconciliation commit resolves; no code duplication in final state |

## Known Follow-ups

1. **Real secondary STT provider** — replace `EchoProvider` with OpenAI Realtime / AssemblyAI / Groq Whisper.
2. **System audio STT event consumption** — the parallel STT instance runs but its `TranscriptEvent`s aren't yet consumed by the session manager or forwarded to the overlay.
3. **PTT press/release** — requires either a lower-level key hook or a different plugin.
4. **Real auto-update endpoint + pubkey** — needs a release server (GitHub Releases or custom).
5. **Deepgram key validation in onboarding** — currently saves without verifying.
6. **Multi-language UI** — English only today.
7. **Speaker auto-color assignment** — user must provide color manually.
8. **Full settings page UI** — currently a Placeholder; Tauri commands are ready.
9. **Local whisper.cpp provider** — tier 3 in the fallback chain plan.

## Review Checklist (for reviewer)

- [ ] Files match the scope described above
- [ ] No unrelated changes included
- [ ] Tests cover the new modules (router, echo, secrets, search, speakers, settings)
- [ ] Code style matches CLAUDE.md rules
- [ ] No TODOs without linked task IDs
- [ ] No secrets in code (API keys go through keyring, never logged)
- [ ] Overlay stealth flags correctly applied on both platforms
- [ ] FTS5 triggers maintain index consistency
- [ ] Tauri commands return masked keys, not raw values

# Phase 3 Round 5 — Handoff for Codex Review

**Branch**: `feat/phase-3-round-5`
**Base**: `feat/phase-3-round-4` tip (`559b21b`)
**Authors**: kiro (4 parallel subagents), uno (user — reconciliation oversight)

## Scope

Round 5 of Phase 3. Ships all P0+P1 user-facing features for the terminal-distributed alpha: native overlay rendering with stealth, STT fallback router, secure API key storage, settings/onboarding UX, FTS5 search, session export, speaker mapping, global hotkeys, system tray, and auto-update infrastructure.

### Commits (16 ahead of R4)

```
8ad44fe feat(dashboard): auto-update support via tauri-plugin-updater [P3.R5]
863b44b feat(dashboard): system tray with listening + dashboard + overlay controls [P3.R5]
4f1501d feat(dashboard): global hotkeys for listening/PTT/overlay-toggle [P3.R5]
617ed16 fix(p3r5): reconcile parallel-subagent cherry-picks (deps + secrets module + commands wiring) [P3.R5]
deb48fa feat(dashboard): first-run onboarding flow [P3.R5]
e52767f feat(daemon): SttRouter with failover + EchoProvider stub [P3.R5]
12991de feat(daemon): route system audio through parallel STT [P3.R5]
1759ceb feat(overlay): stealth flags (hide-from-screenshare, hide-from-dock/alt-tab) [P3.R5]
4143ce8 feat(overlay): render transcript content in macOS + Windows overlays [P3.R5]
c4dc846 feat(overlay): macOS native overlay binary with NSWindow + protocol ABI [P3.R5]
bb4c5f8 feat(dashboard): speaker name mapping with persistence [P3.R5]
3b118c0 feat(dashboard): export sessions to markdown/text/json + clipboard/file [P3.R5]
361c38e feat(daemon): FTS5 transcript search with backfill [P3.R5]
3e3dada feat(dashboard): settings panel with STT provider + audio device + language [P3.R5]
f832d3a feat(daemon): FTS5 transcript search with backfill [P3.R5]
cbcdc08 feat(daemon): secure API key storage via OS keychain [P3.R5]
```

## Verification — ALL GREEN

```
cargo fmt --all --check                              ✅ pass
cargo clippy --all-targets -- -D warnings            ✅ pass
cargo build --all-targets                            ✅ pass
cargo test --all-targets                             ✅ 157 pass
cd crates/cue-dashboard/ui && npm run build          ✅ pass
swift build (native/macos/cue-overlay)               ✅ pass
git -P diff --check feat/phase-3-round-4..HEAD       ✅ clean
```

### Test count delta

| Tier | Round 3 (main) | Round 4 | Round 5 | Δ (R5 vs R4) |
|------|----------------|---------|---------|---------------|
| cue-core lib | 45 | 45 | 45 | — |
| cue-daemon lib | 78 | 81 | 98 | +17 |
| Integration tests | 7 | 10 | 11 | +1 |
| Dashboard commands | 0 | 0 | 3 | +3 |
| Ignored (hardware/keychain) | 1 | 1 | 2 | +1 |
| **Total running** | **130** | **136** | **157** | **+21** |

## Architecture Diagram — Round 5 Additions

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              cue-daemon                                      │
│                                                                             │
│  Mic ──▶ Framer ──▶ TwoStageVad ──▶ ┌─────────────────────────────────┐    │
│                                      │ SttRouter (BLUEY_STT_ROUTER=1)  │    │
│                                      │   providers[0]: Deepgram        │    │
│                                      │   providers[1]: EchoProvider    │    │
│                                      │   failover on Auth/Quota        │    │
│                                      └──────────────┬──────────────────┘    │
│                                                     │                       │
│                                                     ▼                       │
│                                          TranscriptEvent → SessionManager   │
│                                                     │                       │
│  SystemAudioCapture ──▶ Framer ──▶ ┌────────────────┴──────────────────┐   │
│    (native helper)                 │ Parallel STT (system audio)        │   │
│    16kHz mono i16 LE               │   (events not yet consumed)        │   │
│                                    └───────────────────────────────────┘   │
│                                                                             │
│  ┌─── Secrets ───┐    ┌─── DB Layer ──────────────────────────────────┐    │
│  │ keyring crate │    │ app_settings (KV)                             │    │
│  │ store/load/   │    │ transcripts + transcript_fts (FTS5)           │    │
│  │ delete API key│    │ speakers (session_id, speaker_id, name, color)│    │
│  └───────────────┘    └───────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────────┐
│                           cue-dashboard (Tauri)                              │
│                                                                             │
│  ┌─── Global Hotkeys ────────────────────────────────────────────────────┐  │
│  │ Cmd/Ctrl+Shift+D → toggle dashboard                                   │  │
│  │ Cmd/Ctrl+Shift+L → toggle listening                                    │  │
│  │ Cmd/Ctrl+Shift+P → push-to-talk (toggle)                              │  │
│  │ Cmd/Ctrl+Shift+H → toggle overlay                                     │  │
│  └────────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
│  ┌─── System Tray ───────────────────────────────────────────────────────┐  │
│  │ Toggle Listening | Show Dashboard | Toggle Overlay | Settings |        │  │
│  │ Check for Updates | Quit                                               │  │
│  └────────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
│  ┌─── Auto-Update ──┐  ┌─── Onboarding ──────────────────────────────┐    │
│  │ tauri-plugin-     │  │ welcome → apikey → mic test → sysaudio →   │    │
│  │ updater (30s      │  │ done (persists onboarding_complete=true)    │    │
│  │ silent check)     │  └────────────────────────────────────────────┘    │
│  └───────────────────┘                                                     │
│                                                                             │
│  Window close → hide to tray (not quit)                                     │
└─────────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────────┐
│                         Native Overlays                                      │
│                                                                             │
│  ┌─── macOS (Swift) ─────────────────────────────────────────────────────┐  │
│  │ NSWindow (borderless, floating, click-through)                         │  │
│  │ sharingType = .none → invisible in screen capture                      │  │
│  │ activationPolicy = .accessory → no Dock/Cmd+Tab                        │  │
│  │ NDJSON stdin: transcript_partial/final, session_switched, ping         │  │
│  │ NDJSON stdout: pong, request_sync                                      │  │
│  │ Renders: final text (white) + partial text (italic dim) + banner       │  │
│  └────────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
│  ┌─── Windows (C/Direct2D) ──────────────────────────────────────────────┐  │
│  │ WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST                      │  │
│  │ WDA_EXCLUDEFROMCAPTURE → invisible in screen capture                   │  │
│  │ Same NDJSON IPC protocol                                               │  │
│  │ Direct2D transcript rendering with GDI fallback                        │  │
│  └────────────────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Per-Feature-Area Review Checklist

### Native Overlays (macOS + Windows)

- [ ] macOS overlay: `NSWindow` borderless + floating + click-through + `sharingType = .none`
- [ ] macOS overlay: `.accessory` activation policy (no Dock, no Cmd+Tab)
- [ ] macOS overlay: NDJSON stdin parsing handles all `OverlayMessage` variants gracefully
- [ ] macOS overlay: stdin EOF → clean exit (daemon closed pipe)
- [ ] macOS overlay: transcript rendering (final = white medium, partial = italic dim)
- [ ] macOS overlay: session banner with 3s auto-dismiss
- [ ] Windows overlay: `WDA_EXCLUDEFROMCAPTURE` applied
- [ ] Windows overlay: transcript rendering in Direct2D path
- [ ] Both: no secrets logged, no PII in stdout

### System Audio → STT Routing

- [ ] `app.rs` wiring spawns parallel STT instance for system audio chunks
- [ ] System audio STT events are produced (even if not yet consumed downstream)
- [ ] No regression to existing mic → STT pipeline

### SttRouter + EchoProvider

- [ ] `SttRouter::new()` asserts non-empty provider list
- [ ] `next_event()` checks `should_failover()` and advances `active` index
- [ ] Network/Protocol errors do NOT trigger failover
- [ ] Auth/Quota errors DO trigger failover immediately
- [ ] `close()` closes ALL providers, not just active
- [ ] `EchoProvider` emits indexed `TranscriptEvent::Final` per chunk
- [ ] `EchoProvider` rejects `send_audio` after `close()`
- [ ] Router gated behind `BLUEY_STT_ROUTER=1` env var
- [ ] 4 unit tests cover happy path, auth failover, network no-failover, exhaustion

### Secrets / Keyring

- [ ] `store_api_key` / `load_api_key` / `delete_api_key` use `keyring::Entry`
- [ ] Service name `cue-daemon`, username `stt_{provider}`
- [ ] `delete_api_key` on nonexistent entry returns `Ok(())` (not error)
- [ ] Dashboard `load_stt_api_key` command returns masked value (`****<last4>`), never raw key
- [ ] Tests use separate `TEST_SERVICE` to avoid polluting real keychain

### Settings Panel

- [ ] `save_settings` / `load_settings` Tauri commands wired
- [ ] `app_settings` table created by migration 005
- [ ] Settings route exists in App.tsx (currently renders Placeholder)
- [ ] `list_audio_devices` uses `cpal` to enumerate input devices

### Onboarding

- [ ] 4-step flow: welcome → apikey → mic → sysaudio → done
- [ ] API key saved via `save_stt_api_key` command (skippable)
- [ ] Mic test calls `list_audio_devices` and shows success if ≥1 device
- [ ] System audio opt-in persisted as `system_audio_continuous` setting
- [ ] `onboarding_complete=true` persisted on finish

### FTS5 Search + Export + Speakers

- [ ] `transcript_fts` is standalone FTS5 (not content-synced)
- [ ] Triggers `trg_transcripts_ai` / `trg_transcripts_ad` maintain FTS index
- [ ] `search_transcripts` uses `snippet()` with Unicode markers for highlighting
- [ ] `export_session_markdown` / `_text` / `_json` produce correct output
- [ ] Export respects `ExportOptions` (include_partials, include_timestamps, include_speakers)
- [ ] `set_speaker_name` uses UPSERT (ON CONFLICT DO UPDATE)
- [ ] Search UI debounces input (200ms) and renders highlighted snippets
- [ ] Export commands support both clipboard (return string) and file (write to path)

### Hotkeys / Tray / Auto-Update

- [ ] 4 global shortcuts registered with platform-appropriate modifiers
- [ ] Shortcuts emit Tauri events (not direct function calls) — UI subscribes
- [ ] System tray menu has 6 items with separators
- [ ] Tray "Quit" calls `app.exit(0)` (not window close)
- [ ] Window close intercepted → `window.hide()` (stays in tray)
- [ ] Auto-update: silent check 30s after launch, emits `update_available` event
- [ ] `check_for_updates` command available for manual trigger
- [ ] `UpdateToast` component renders on `update_available` event with dismiss button
- [ ] Updater config in `tauri.conf.json` has placeholder endpoint + pubkey

## Known Artifact: Duplicate FTS5 Commits

Commits `f832d3a` and `361c38e` both have the message `feat(daemon): FTS5 transcript search with backfill [P3.R5]`. This is an artifact of 4 parallel subagents cherry-picking into the same branch. Two subagents independently implemented FTS5 search. The reconciliation commit `617ed16` resolved conflicts and deduplicated the code. The final state is correct — this is NOT a bug or a revert/re-apply situation.

## Explicit Deferrals (NOT in Round 5)

1. **Real secondary STT provider** — only `EchoProvider` stub exists; OpenAI/AssemblyAI/Groq implementation is a future round.
2. **System audio STT event consumption** — parallel STT runs but events aren't consumed by session manager or forwarded to overlay.
3. **PTT press/release** — toggle only; `tauri-plugin-global-shortcut` doesn't expose key-up events.
4. **Real auto-update endpoint + pubkey** — placeholders in `tauri.conf.json`.
5. **Deepgram key validation** — onboarding saves key without verifying it works.
6. **Multi-language UI** — English only.
7. **Speaker auto-color** — user-provided today.
8. **App signing** — terminal-distributed alpha; no Apple Developer or Authenticode.

## Verdict Request

Codex: review the 16 commits + new modules + integration tests + IMPL doc. Write `docs/work/REVIEW-PHASE-3-ROUND-5.md` with verdict.

- 🟢 **ACCEPT** → merge to main, start Round 6 (real secondary STT provider + system audio event consumption + AI features)
- 🟡 **ACCEPT WITH NITS** → fold nits into Round 6
- 🔴 **REQUEST CHANGES** → kiro writes `docs/work/FIX-PHASE-3-ROUND-5.md` and re-hands

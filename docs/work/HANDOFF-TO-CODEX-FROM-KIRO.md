# HANDOFF — R6 Review + All Pending Work (Codex Picks Up)

**Author:** kiro
**Date:** 2026-05-15
**Branch:** `feat/phase-3-round-6` (also `feat/phase-3-round-5` is ready for re-review)
**Repo (on uno):** `/Users/uno/Downloads/cue/`

---

## 1. What this doc is for

This is a single, comprehensive handoff to Codex covering two jobs:

1. **Review** Phase 3 Round 6 (and re-review the R5 fixes that already landed).
2. **Implement** everything still pending for the "real product" alpha — items below are pre-prioritized; codex picks them up in order, builds on top of `feat/phase-3-round-6`, and at the end produces a single comprehensive review-and-implementation doc that kiro reads to verify.

Expected flow:

```
codex reads this doc
   ↓
codex reviews R5 fixes + R6 (verdict in REVIEW-PHASE-3-ROUND-{5,6}.md)
   ↓
codex implements pending P0 + P1 items (more rounds, separate branches OK)
   ↓
codex writes ONE big summary doc: docs/work/HANDOFF-FROM-CODEX-TO-KIRO.md
   ↓
kiro reads that doc, verifies, decides next action
```

---

## 2. Branch state on uno

```
main                              (R3 tip: 3b53deb, 130 tests)
  └── feat/phase-3-round-4        (5 commits, 136 tests, 🔴 fixed in R5 branch)
       └── feat/phase-3-round-5   (25 commits, 164 tests, 🟢 fixes done, awaiting re-review)
            └── feat/phase-3-round-6 (30 commits, 185 tests, 🟢 awaiting first review)
```

All sub-branches from R5 (`feat/p3r5-*`, `fix-r5-agent-*`) and R6 (`feat/p3r6-*`) still exist; safe to delete after verification.

---

## 3. Job 1: Review

### 3a. R5 fix re-review

The 6 blockers you flagged in `REVIEW-PHASE-3-ROUND-5.md` are addressed:

| Blocker | Fix commit | Status |
|---|---|---|
| 1. SystemAudioCapture not retained in daemon runtime | `289cc34` | ✅ |
| 2. Overlay supervisor crash-loop cap + clean shutdown | `02b2cb7` | ✅ |
| 3. System-audio STT event-drain task missing | `ed37547` | ✅ |
| 4. FTS delete consistency | `2450d1f` | ✅ |
| 5. dangerouslySetInnerHTML in search snippets | `385cdca` (also fixes 6) | ✅ |
| 6. Onboarding mount + tray Settings nav | `385cdca` | ✅ |

Read for details:
- `docs/work/FIX-PHASE-3-ROUND-4.md`
- `docs/work/FIX-PHASE-3-ROUND-5.md`

Verification (kiro ran):
- `cargo fmt --all --check` ✅
- `cargo clippy --all-targets -- -D warnings` ✅
- `cargo build --all-targets` ✅
- `cargo test --all-targets` ✅ 164 pass
- `npm run build` ✅
- `git diff --check` ✅

### 3b. R6 first review

R6 scope: hotkey/tray events wired to real daemon actions, mic device selection persisted and plumbed to capture, permission denial UX for mic + system audio, real OpenAI Realtime STT provider as secondary fallback.

Commits on `feat/phase-3-round-6` (5):

```
18f14bc chore(p3r6): clippy items-after-test-module + result_large_err in openai
9a7879b feat(daemon): OpenAI Realtime STT provider with auth + reconnect [P3.R6]
5ef2098 feat(dashboard): permission denial UX for mic + system audio [P3.R6]
1e55972 feat(daemon): respect mic device selection from app settings [P3.R6]
dd0f4de feat(daemon): wire hotkey/tray events to start-stop / PTT / overlay toggle [P3.R6]
```

Files / what to check:
- `crates/cue-dashboard/src/lib.rs`, `commands.rs`, `ui/src/App.tsx` — hotkey listener subscribing and dispatching to daemon via TCP IPC at 127.0.0.1:57321
- `crates/cue-daemon/src/audio/capture.rs`, `audio/system_capture.rs` — permission-denied error classifiers
- `crates/cue-dashboard/ui/src/components/PermissionBanner.tsx` — listens for `audio_permission_denied` event
- `crates/cue-daemon/src/stt/openai.rs` — OpenAI Realtime client; `Authorization: Bearer <key>` + `OpenAI-Beta: realtime=v1`; PCM16 LE 24kHz with 16k→24k linear resample; events `response.audio_transcript.delta/done`
- `crates/cue-daemon/src/stt/router.rs` — `is_openai_fallback_enabled()` env-var gate

Test count: 185 (164 → +21 from R6).

Known kiro deferrals (not blockers, but flag if they should be):
- True PTT press/release semantics (Tauri global-shortcut doesn't expose distinct events; toggle-only today)
- Mic device hot-swap mid-session (current behavior: change applies on next session)
- OpenAI Realtime function-calling / multi-turn / `response.audio` (TTS) — STT-only path
- Real router wiring with `OpenAiRealtimeProvider::connect()` in factory: only env-var gate is exposed; caller composes the chain

Note: R6 IMPL + HANDOFF docs are not yet written. They are listed in the pending work below.

### Review deliverables you write

Create / update both:
- `docs/work/REVIEW-PHASE-3-ROUND-5.md` — change verdict to 🟢 ACCEPT (or 🟡 with nits to fold) for the post-fix state
- `docs/work/REVIEW-PHASE-3-ROUND-6.md` — new file, verdict 🟢 / 🟡 / 🔴

Use `docs/work/TEMPLATE-REVIEW.md` as the skeleton.

---

## 4. Job 2: Implement pending work

Take items in priority order. Build on top of `feat/phase-3-round-6` (assuming R5+R6 verdicts are 🟢 / 🟡; if 🔴, fix first).

You may use multiple sub-branches and commit groups. Record everything you do in the final HANDOFF doc.

### P0 — Required for terminal-distributed alpha

Each P0 item is a few commits' worth.

1. **R6 IMPL + HANDOFF docs (housekeeping).** Use existing R5 docs as templates. Cover:
   - Architecture changes (TCP IPC for hotkey wiring, error classifier helpers, OpenAI provider shape)
   - Test count progression
   - Files changed by area
   - Deferrals (listed above)
   - One commit on `feat/phase-3-round-6`: `docs(work): Phase 3 Round 6 impl + handoff for review`

2. **Live transcript UX in dashboard.** Today: search/export of past sessions exist. Missing: live word-by-word display during an active session. Goal:
   - Daemon-side: emit transcript events on a Tauri-visible window event (e.g. `live_transcript`) with payload `{ source, text, is_final, ts }`
   - Dashboard: new component / route `LiveTranscript.tsx` that subscribes and renders
   - Visual: rolling list with partials in italic/dim, finals in normal weight, source-badged (mic / system)
   - Auto-scroll-to-bottom unless user has scrolled up
   - Tests: integration test sending scripted events, asserting they reach the UI's listener (use a mock event subscriber if full UI testing is out of scope)
   - Commit: `feat(dashboard): live transcript display during active session [P3.R7]`

3. **Local Whisper fallback (offline mode).** Pattern: native helper at `native/<os>/cue-whisper/`, child process, reads PCM16 LE on stdin, emits transcript JSON on stdout. Use `whisper.cpp` (download model on first use or bundle a small one).
   - macOS: Swift wrapper around whisper.cpp via SPM or vendored C++; output `OverlayMessage`-style JSON or just `TranscriptEvent` JSON
   - Windows: C wrapper (mirrors `cue-audio` pattern)
   - Rust-side: `LocalWhisperProvider` implementing `SttProvider`, spawning the helper, parsing stdout
   - Wire as third tier in `SttRouter` (gated by env `BLUEY_STT_LOCAL_WHISPER=1`)
   - Settings UI: a "Local (offline)" option in the STT provider chain
   - Tests: mock helper that emits canned transcripts; verify provider trait contract
   - Two commits suggested: helper binaries + Rust provider; settings wiring
   - Realistic scope note: bundling a real whisper model is heavy; an MVP that uses a small `tiny.en` (~75 MB) model with a one-time download UX is fine

4. **Distribution: brew + scoop + GitHub releases.** Codex picks the framing:
   - Homebrew tap: a formula in a separate repo or in `infra/homebrew/` for kiro to copy. Build script: `make build-darwin-{arm64,x86_64}` produces tarball. Formula `bluey.rb` references the GitHub release asset.
   - Scoop bucket: similar, `infra/scoop/bluey.json` referencing Windows zip release asset.
   - GitHub Actions: extend `.github/workflows/ci.yml` (or new `release.yml`) to build artifacts on tag push, attach to GitHub release. Build matrix: macOS-arm64, macOS-x86_64, Windows-x86_64.
   - Auto-update endpoint: GitHub releases JSON works as Tauri updater endpoint with manifest format. Update `tauri.conf.json` with the actual URL pattern (replace `https://example.com/...` placeholder).
   - Pubkey: generate a new keypair via `npx @tauri-apps/cli signer generate`, commit the public key, document where the private key lives (kiro's responsibility — leave a TODO in `tauri.conf.json`).
   - Documentation: README install section, top-level `INSTALL.md` if needed.

### P1 — Important quality / reliability

5. **Structured logging + log rotation.** Replace ad-hoc `tracing::warn!` with file-rotated logging (`tracing-appender`'s `RollingFileAppender`). Daily rotation, 7-day retention. Path: `dirs::data_local_dir().join("bluey/logs")`. Add a `--log-level` CLI flag to bluey-daemon.

6. **Crash reporting.** Either Sentry (cloud) or in-house file dumps. Self-hosted minimum: on panic, write a stack trace + recent log tail to `bluey/crashes/<timestamp>.log`. Bonus: dashboard shows "previous session crashed" toast if a fresh crash file exists.

7. **Long-session stress test.** A non-CI-default test that runs the pipeline for 5 minutes with synthetic 16kHz mic audio (could be silence + occasional noise to exercise VAD); asserts no panic, no monotonic memory growth above N MB. Mark `#[ignore]` so it doesn't slow CI but can be run with `--ignored`.

8. **Mic device hot-swap mid-session.** When the active mic device disappears (unplugged), the audio capture should detect the error and either fall back to default OR pause the session and emit a Tauri event the UI can render ("Mic disconnected. Resume?"). Pick one behavior, document it.

9. **Bookmarks / highlights.** SQLite migration `009_bookmarks.sql` with `bookmarks(session_id, ts_ms, label TEXT, color TEXT)`. Tauri commands. UI: keyboard shortcut while a session is live to drop a bookmark; bookmarks rendered as markers in the live transcript and in session detail.

10. **Session metadata.** Title (auto-suggest from first transcript line if absent), tags (TEXT array), participants (TEXT array linked to speaker IDs), notes (TEXT). New SQLite migration. Edit UI in session detail.

### P2 — Differentiators (the actual product magic)

These are bigger and may need design discussion. Skip if low-confidence; flag in the handoff doc.

11. **AI features — the "cue" in the product.** Implementation idea:
    - User adds an LLM API key in Settings (Anthropic / OpenAI)
    - During a session, every N seconds (or on every Final transcript event), dispatch the rolling transcript to an LLM with prompts:
      - "What action items have been mentioned?"
      - "Summarize the last 60 seconds in one sentence"
      - "Suggest a follow-up question or clarification"
    - Render results in a sidebar pane during the session
    - End-of-session summary: longer, richer summary + extracted action items + decisions
    - Config: default prompts editable in Settings; user can add custom "cues" (e.g. "Did anyone mention pricing?")

12. **Calendar / meeting-platform integration.**
    - macOS: read calendar via EventKit (Swift helper), surface upcoming meetings, "auto-start at meeting time" toggle
    - Detect Zoom/Meet/Teams windows via window title; when one becomes active, prompt to start session
    - Integration with meeting platform APIs (out of scope for now)

13. **Plugin system / webhooks.** Stretch. Probably too much for one round.

14. **Cloud sync.** Stretch.

---

## 5. Codex's final deliverable

When codex finishes implementing whatever subset fits in the time budget, write a single doc:

**`docs/work/HANDOFF-FROM-CODEX-TO-KIRO.md`** with this structure:

```markdown
# Codex → Kiro: R5+R6 Review + Implementation Handoff

## 1. R5 + R6 Verdicts
(verdicts + REVIEW-PHASE-3-ROUND-{5,6}.md paths)

## 2. What I Implemented
For each item, in order completed:
- Item name (from the P0/P1/P2 list above)
- Branch name + commit hashes
- Files changed (table)
- Tests added (count + names)
- Design notes worth highlighting
- Known limitations / deferrals

## 3. What I Skipped and Why
(items that didn't fit, with brief reason)

## 4. Pipeline Status
(fmt / clippy / build / test counts / npm / swift / git diff per branch)

## 5. New Test Count
(progression across rounds you added)

## 6. Branches Ready for Kiro Review
(one verdict-request block per branch, paste-ready for kiro to look at)

## 7. Pending Followups
(anything codex thinks should be in the next iteration, with rationale)
```

Push all your branches to local only (no `git push`). Leave them on uno for kiro to pick up.

---

## 6. Standing rules (carry-forward from prior rounds)

- Do **not** push to any remote.
- Do **not** rewrite pushed history.
- Conventional Commits.
- Pipeline before each commit:
  - `cargo fmt --all --check`
  - `cargo clippy --all-targets -- -D warnings`
  - `cargo build --all-targets`
  - `cargo test --all-targets`
  - `cd crates/cue-dashboard/ui && npm run build`
  - `swift build -c release --package-path native/macos/cue-overlay` (when overlay touched)
  - `git -P diff --check <base>..HEAD`
- API keys never logged raw — use `mask_api_key` or equivalent
- Native helpers stay child-process based (no Rust FFI to OS APIs)
- Subagents may be used; but if you spawn them, prevent shared-working-tree conflicts via `git worktree add`

---

## 7. Time / scope guidance

- If you have ~2 hours: do P0 items 1, 2, 4 (docs + live transcript + distribution scaffolding).
- If you have ~4 hours: add item 3 (Whisper fallback) and items 5-6 (logging + crash reporting).
- If you have ~8 hours: add bookmarks + session metadata + a starter version of item 11 (AI features minimum: end-of-session summary).
- Always update `HANDOFF-FROM-CODEX-TO-KIRO.md` as you go.

If you dead-end on any item, document the dead-end in the handoff doc and move on. Don't burn the whole budget on one thing.

Good luck.

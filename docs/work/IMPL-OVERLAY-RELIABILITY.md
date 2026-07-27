# IMPL: OVERLAY-RELIABILITY — End-to-End Meeting Overlay Reliability

## Scope

**Does:**

- Stop macOS audio helpers from repeatedly relaunching after a permission denial
  or non-retryable setup failure. Keep the denied source explicit in the UI and
  allow an intentional retry after the user fixes the relevant system setting,
  without restarting an unaffected source.
- Preserve stable helper identities and signatures across development installs
  and release packaging, including the hardened-runtime audio-input entitlement
  required for macOS to authorize the microphone helper.
- Provision and prewarm the local Parakeet model before capture, while sharing
  one model load across system-audio and microphone decoders.
- Make the collapsed overlay pill draggable, suppress click-after-drag, repair
  its source and Ask controls, preserve source-specific permission failures
  across collapse/expand, and keep the expanded window on-screen.
- Make meeting-prep banner delivery capability-safe and idempotent, keep normal
  transcript traffic out of its webview, preserve capture exclusion, and place
  it within the cursor display's work area. Queue simultaneous offers and scope
  dismissal/approval to the exact calendar occurrence.
- Keep system-audio and microphone state authoritative across daemon IPC and the
  React overlay, including microphone-only sessions and source-specific stops.
- Reconcile CLI audio diagnostics with those authoritative continuous source
  handles without overwriting the separate chunk/runtime pipeline.
- Keep source capture/task generations paired during shutdown, prevent the
  system idle watchdog from archiving an active microphone session, and fail a
  source start if its STT provider cannot initialize.
- Harden Google and Microsoft native PKCE onboarding, keychain token storage,
  incremental calendar sync, serialized connect/disconnect, persistent account
  settings, connection-health reporting, and public webhook validation.
- Make the development reinstall stop stale daemon/overlay processes before
  replacing binaries.
- Remove a macOS `current_dir` call from the visible dev-overlay startup
  critical path so daemon IPC can bind reliably.
- Serialize overlay resize intents and final-sign the fully staged macOS release
  application.

**Does NOT:**

- Register the external Google or Microsoft OAuth applications.
- Create or renew remote calendar webhook subscriptions. Incremental polling is
  the active calendar-sync transport.
- Bypass or auto-approve macOS privacy prompts.

## Files Created / Modified

| File | Action | Purpose |
|------|--------|---------|
| `crates/cue-daemon/src/app.rs` | Modified | Coordinate audio sessions, lifecycle state, source controls, and startup prewarm. |
| `crates/cue-daemon/src/audio/` | Modified | Report helper outcomes and expose capture liveness. |
| `crates/cue-daemon/src/stt/` | Modified | Provision, prewarm, and single-flight shared Parakeet weights. |
| `crates/cue-transcribe/` | Modified | Add reusable engine warm-up support and coverage. |
| `crates/cue-core/src/overlay*.rs` | Modified | Carry authoritative per-source state over overlay IPC. |
| `crates/cue-core/src/calendar.rs` and `ipc.rs` | Modified | Carry calendar identity, connection health, and exact prep responses. |
| `crates/cue-meeting-overlay/src/` | Modified | Queue private prep banners and bridge source-aware native events. |
| `crates/cue-meeting-overlay/ui/src/` | Modified | Repair dragging, controls, source-state restoration, onboarding, and screen clamping. |
| `crates/cue-meeting-overlay/capabilities/` and generated schemas | Modified | Grant the banner only its required event capability. |
| `crates/cue-meeting-overlay/tauri.conf.json` | Modified | Restore default capture protection. |
| `native/macos/cue-audio/` | Modified | Classify permission outcomes and produce a stable, entitled signed helper bundle. |
| `native/macos/cue-shot/build.sh` | Modified | Preserve a stable screenshot-helper signing identity. |
| `crates/cue-calendar-cloud/` | Modified | Harden PKCE, keychain storage, pagination, delta merge, and provider lifecycle. |
| `server/src/api/calendar.rs` | Modified | Validate webhook identity tokens and bound payloads. |
| `Makefile`, `Cargo.lock`, scripts, and `.github/workflows/release.yml` | Modified | Build required features, package signed helpers, and stop stale processes safely. |
| `ops/bluey-api.env.example` | Modified | Document optional calendar webhook ingress configuration. |
| `docs/work/FIX-*.md` | Added | Record root cause, fix, tests, and limitations for each defect. |

## Final validation

Final branch-tip verification completed on 2026-07-26:

- [x] `cargo fmt --all -- --check` — passed for the full workspace.
- [x] `cargo test -p cue-daemon --features parakeet-stt,local-memory,cloud-calendar` —
      423 passed, 0 failed, 18 intentionally ignored.
- [x] `cargo clippy -p cue-daemon --all-targets --features parakeet-stt,local-memory,cloud-calendar -- -D warnings` —
      passed.
- [x] Combined `cue-core`, `cue-calendar-cloud`, `cue-meeting-overlay`, and
      `cue-transcribe` test gate — 242 passed, 0 failed: core 163, calendar 68,
      overlay 4, and transcribe 7.
- [x] `cargo clippy -p cue-meeting-overlay --all-targets -- -D warnings` —
      passed.
- [x] `(cd server && cargo test calendar)` — 8 passed, 0 failed, 130 filtered;
      server all-target Clippy also passed with warnings denied.
- [x] `npx --yes prettier@3.6.2 --check <18 changed UI files>` — all 18 files
      passed.
- [x] `(cd crates/cue-meeting-overlay/ui && npm run build)` — passed with 78
      modules transformed; three pre-existing mixed-import advisories remained
      non-fatal.
- [x] `bash native/macos/cue-audio/bundle-app.sh` — produced a signed arm64
      `BlueyAudio.app` with stable bundle ID `sh.bluey.audio` and Team ID
      `FS65MX3B6M`.
- [x] Strict `native/macos/cue-audio/verify-app.sh` verification — signature,
      designated requirement, required audio-input entitlement, arm64
      architecture, direct no-capture probe, and LaunchServices no-capture
      probe all passed.
- [x] `bash native/macos/cue-shot/build.sh` — signed arm64 helper build and safe
      no-capture executable smoke passed.
- [x] Native plist lint, helper architecture checks, and modified shell-script
      syntax checks — passed.
- [x] `git diff --check` — passed for staged and unstaged changes.
- [x] Clean-install visible-mode pill drag and source-toggle smoke — passed.
      The pill moved from `{720,60}` to `{382,182}` without expanding.
- [x] Real macOS microphone deny/settings/retry smoke — passed on 2026-07-27.
      TCC first exposed the missing audio-input entitlement, the corrected
      helper produced one proper `BlueyAudio` prompt, the user-approved grant
      reached `authorized`, and one helper PID remained stable while the
      warning cleared.
- [x] Continuous audio status regression tests — 9 passed, covering starting,
      running microphone-only, chunk-runtime isolation, source-switch gaps,
      terminal cleanup, same-session telemetry, new-session resets, inactive
      permission denial, and handle/session publication races.

## Deviations from Plan

| Deviation | Rationale |
|-----------|-----------|
| Webhook ingress is not a delivery path | The public routes authenticate and acknowledge provider doorbells only. Native incremental polling remains authoritative until subscription lifecycle, ownership routing, and an authenticated device nudge exist. |
| Real OAuth consent is pending | Public client IDs must come from registered Google Desktop and Microsoft mobile/desktop applications and are intentionally not embedded as secrets. |

## Known Follow-ups

- Configure `BLUEY_GOOGLE_CLIENT_ID` and `BLUEY_MICROSOFT_CLIENT_ID` from the
  registered public-client applications before exercising real consent.
- Configure the webhook secrets and public URL only when server-side
  subscription creation, renewal, and device relay are implemented.
- Add the release signing certificate secrets documented by the release
  workflow before publishing a distributable macOS archive.
- Run one interactive Google and Microsoft consent/refresh scenario after test
  public-client registrations are available.

## Review Checklist (for reviewer)

- [ ] Files match the scope described above
- [ ] No unrelated changes included
- [ ] Tests cover acceptance criteria from plan
- [ ] Code style matches `AGENTS.md` rules
- [ ] No TODOs without linked task IDs

# Round 505 — Bluey Live in one click

Date: 2026-07-12
Status: Complete
Final verification base: `325093b24541c2655b029c3b8d92c2fd36d80ab6`
Research handoff: [Round 504](ROUND-504-BLUEY-DMG-CROSS-PRODUCT-AUDIT-AND-LIVE-CAPTURE-HANDOFF.md)

## Objective

Make the dashboard's primary Listen action start and stop Bluey's existing system-plus-microphone audio pipeline, expose the daemon's real capture state, and preserve transcript catch-up without introducing a second audio stack.

## Implementation boundary

- Rust dashboard command and fake-daemon protocol tests.
- Live Transcript Start/Stop control, truthful pending/active/error state, and pure UI-state tests.
- No competitor code, assets, prompts, binaries, endpoints, or dependencies.
- No changes to Jobs submission, retry, receipt, or browser-isolation invariants.

## Evidence for the change

Before this round, `daemon_toggle_listening` sent meeting lifecycle commands without starting audio, while the adjacent PTT flow already used `AudioStatus`, `AudioStart`, and `AudioStop`. Round 504 records the exact baseline references and clean-room decision.

## Changes

### Typed capture control

- Added a typed listening-status command over the existing daemon IPC.
- Replaced meeting-only toggle behavior with `AudioStatus` followed by dual-source `AudioStart` or cancelable `AudioStop`.
- Reuses the saved microphone choice and fail-closes if both requested sources are not active; a partial start is immediately stopped.
- Publishes content-safe status/error events so direct buttons, tray actions, and the global shortcut converge.

Primary implementation: `crates/cue-dashboard/src/commands.rs:732-1019`; command registration: `crates/cue-dashboard/src/lib.rs:81-111`.

### Truthful lifecycle and recap

- The daemon now publishes `Starting` as soon as a start generation is claimed. A Stop request can cancel that generation before capture becomes active.
- Meeting end is serialized against new starts, stops/cancels audio, waits only the remaining bounded STT tail window, and only then archives/deletes and starts recap/sync.
- Added a separate End Session action. Stop Listening pauses capture while keeping the session open; End Session saves/ends the meeting and preserves the completed transcript on screen.

Daemon lifecycle implementation: `crates/cue-daemon/src/app.rs:2034-2092,3410-3821,4075-4083,7750-7795`.

### Live Transcript UX

- Added always-visible Start/Stop/Retry and End Session controls with pending, active, degraded, permission/sign-in, timeout, and saved-session state.
- Status comes from the daemon pipeline, not meeting or transcript presence.
- Active copy reports actual system/microphone coverage. Missing or failed sources cannot be presented as healthy dual capture.
- Existing transcript catch-up, session/index de-duplication, the 200-segment bound, and after-stop visibility are preserved.
- Polling is coalesced and bounded; timestamp plus local transition generations prevent delayed status responses from overwriting newer actions/events.

UI implementation: `crates/cue-dashboard/ui/src/routes/LiveTranscript.tsx:22-368`; pure state contract: `crates/cue-dashboard/ui/src/routes/liveTranscriptState.ts:1-264`.

### Boundary hardening

- Local daemon IPC now has a bounded operation timeout.
- Renderer-facing audio errors are classified into fixed public copy; raw chained errors, URLs, paths, and token-shaped values stay out of events/status fields.
- The listening shortcut is loaded once from persisted settings, registered from that managed startup value, and displayed from the same value. A newly saved binding takes effect after restart.

Timeout/error boundary: `crates/cue-dashboard/src/commands.rs:63-159,742-879`; shortcut authority: `crates/cue-dashboard/src/commands.rs:1986-2080` and `crates/cue-dashboard/src/lib.rs:137-145,320-337`.

## Verification

- `cargo test -p cue-daemon --lib` — 341 passed, 5 hardware/Keychain tests ignored, 0 failed.
- `cargo test -p cue-dashboard` — 24 passed, 0 failed.
- `npm test` in `crates/cue-dashboard/ui` — 25 passed, 0 failed.
- `npm run build` — TypeScript and Vite production build passed.
- `cargo check -p cue-daemon -p cue-dashboard` — passed.
- `cargo fmt --check -p cue-daemon -p cue-dashboard` — passed.
- `cargo clippy -p cue-daemon -p cue-dashboard --all-targets -- -D warnings` — passed.
- Documentation/source whitespace, link, hash, and secret-shape checks — passed.

The Rust protocol tests cover dual-source start with saved microphone, active and starting cancellation, partial-source cleanup, half-open timeout, content-free End Session, status-refresh failure, public-error redaction, and shortcut consistency. Daemon tests cover starting publication, generation cancellation, bounded tail settlement, end/start ordering, newly-created-empty cleanup, and protection for newer/preexisting/contentful meetings. UI tests cover inactive/starting/capturing/stopped/failed/degraded states, source coverage, public errors, stale status rejection, and shortcut labels.

## Remaining work

- Authenticated real-hardware validation still requires a signed desktop build with explicit Microphone and Screen Recording permission; no production credentials or permissions were used in this round.
- A component-level Tauri harness can add DOM assertions for invoke/listen cleanup beyond the pure state and protocol tests in this patch.
- Interview presets, consent-first dashboard screenshots, and the Jobs-to-live context handoff remain P1 work from Round 504.

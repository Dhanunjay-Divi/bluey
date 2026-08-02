# Round 504 — Bluey DMG cross-product audit and live-capture handoff

Date: 2026-07-12

Bluey coordination baseline: `eb923a89f24b69664c73479607bf665725789658`

Full evidence index: [DMG audit index](../research/dmg-audit/INDEX.md)

## Executive summary

Five signed/notarized desktop interview or contextual-assistant DMGs were statically audited and, where useful, launched only in bounded unauthenticated outbound-denied harnesses. None contains evidence of an end-to-end job discovery/application system comparable to Bluey. Bluey is stronger in discovery, applicant identity isolation, ATS execution, challenge handoff, durable retry/idempotency, duplicate-submit prevention and submission evidence. The competitors are strongest in polished one-click live capture, interview modes, screenshot assistance, permission onboarding and visible recovery state.

The immediate Bluey defect is concrete and local: `daemon_toggle_listening` checks meeting state and sends only `MeetingStart`/`MeetingEnd` (`crates/cue-dashboard/src/commands.rs:689-713`). `MeetingStart` creates a meeting record/card but never starts audio (`crates/cue-daemon/src/app.rs:1971-2000`). The existing PTT path already demonstrates `AudioStatus` → `AudioStart`/`AudioStop` (`crates/cue-dashboard/src/commands.rs:716-752`), and `start_audio_capture` already ensures an active meeting (`crates/cue-daemon/src/app.rs:3459-3487`). The smallest clean-room P0 is therefore to connect Bluey's own button/hotkey to Bluey's own audio pipeline and expose its real status.

## Evidence-backed findings

1. **Bluey's Jobs foundation is the lead.** Provider-first execution and interventions are implemented in `jobs/automation/src/execute.ts:37-97`; identity-scoped browser profiles in `jobs/browser/src/profile.ts:4-33`; durable workflow/irreversible boundaries in `jobs/workflows/src/workflows.ts:10-119` and `jobs/browser/src/irreversible-submit.ts:44-175`; receipts in `jobs/automation/src/receipts.ts:100-201`. Every DMG lacks an observed comparable job pipeline; see each [gap map](../research/dmg-audit/INDEX.md#cross-product-bluey-status-matrix).
2. **Bluey's live foundations exist but the primary control is disconnected.** The current toggle does not send audio commands, while PTT and the daemon audio path already provide the necessary state and capture primitives (`crates/cue-dashboard/src/commands.rs:689-752`; `crates/cue-daemon/src/app.rs:2253-2301,3459-3487`).
3. **The UI amplifies the defect.** Live Transcript has no start/stop control and only says “Start one with Cmd+Shift+L” (`crates/cue-dashboard/ui/src/routes/LiveTranscript.tsx:72-86`), while the registered shortcut is `Ctrl+Alt+L` (`crates/cue-dashboard/src/lib.rs:293-319`) and stored/default settings advertise `CmdOrCtrl+Shift+L` (`crates/cue-dashboard/src/commands.rs:1264-1281`).
4. **Competitor patterns worth adapting are product contracts, not code.** Named modes, explicit permission cards, visible audio health, bounded VAD/reconnect, structured coding-answer panels and meeting-end hints recur across the [five feature maps](../research/dmg-audit/INDEX.md#artifact-register).
5. **Competitor security shortcuts should be rejected.** The packs evidence generic renderer IPC, token-bearing deep links, broad preload surfaces, renderer-readable cookies, URL query JWTs, content-bearing telemetry, global Origin rewriting, plaintext fallbacks and—in LockedIn—a path from WebRTC messages to OS input. Details and provenance are linked from the [artifact register](../research/dmg-audit/INDEX.md#artifact-register).

## Cross-application feature matrix

| Capability | Cluely | Littlebird | LockedIn | ParakeetAI | Final Round | Bluey decision |
|---|---|---|---|---|---|---|
| Jobs discovery/browser/ATS/receipts | Not observed | Not observed | Not observed | Not observed | Not observed | Preserve Bluey's stronger architecture. |
| Live mic+system capture | Observed | Observed | Observed | Observed | Observed | Wire Bluey's existing audio path to the primary control. |
| Interview presets/modes | Observed | General workspace | Observed | Observed setup | Observed | Add Bluey-authored scoped presets after P0. |
| Screenshot assistance | Observed | Observed context | Observed | Observed | Observed | Productize Bluey's existing consent-first capture path. |
| Meeting detection/lifecycle | Calendar-linked | Observed | Observed | Native mic activity | Native process lifecycle | Add advisory, privacy-minimized lifecycle hints. |
| Durable application queue/idempotency | Not observed | Not observed | Not observed | Not observed | Not observed | Keep Bluey's lease/side-effect-unknown invariants. |
| Email/calendar outcomes | Meeting calendar | Gmail/calendar assistant | Not observed | Not observed | Not observed | Bluey remains Partial until one provider is production-active. |
| High-risk renderer boundary | Generic IPC/origin rewrite | Generic IPC/tokens | Generic IPC/remote input | Generic IPC/cookies | Broad identical preloads | Retain typed least-privilege Bluey boundaries; tighten Tauri capabilities. |

The normalized comparative status table is in the [INDEX](../research/dmg-audit/INDEX.md#cross-product-bluey-status-matrix).

## P0 — make Listen actually listen

Implement one cohesive Bluey-owned slice:

1. Add a typed `daemon_listening_status` command returning `AudioPipelineStatus` (`crates/cue-core/src/audio.rs:363-378`).
2. Change `daemon_toggle_listening` to query `AudioStatus`; when inactive send `AudioStart { enable_system: true, enable_microphone: true, mic_device_id }`, reusing the saved microphone selection at `crates/cue-dashboard/src/commands.rs:787-794`; when active send `AudioStop`, then preserve the existing explicit end/recap behavior or expose a separate “End session” action.
3. Add a prominent Start/Stop control to Live Transcript with pending, listening, paused and failed states; poll once on mount and reconcile after events/errors. Never display “listening” from meeting state alone.
4. Make one shortcut source authoritative. Register the stored/default accelerator instead of hard-coded `Ctrl+Alt+L`, and render that same value in the empty state (`crates/cue-dashboard/src/lib.rs:293-339`; `crates/cue-dashboard/src/commands.rs:1264-1299`).
5. Keep system+mic capture fail-closed: permission/sign-in denial must surface as an error/degraded state; it must not silently fall back while claiming both sources.

Acceptance tests:

- Fake-daemon test: inactive status produces exactly one dual-source `AudioStart` with the saved mic ID.
- Fake-daemon test: active/starting status cannot produce a second start; stop is idempotent.
- Failure tests: daemon unavailable, permission denial, missing system helper, missing mic and partial-source resolution produce truthful UI state.
- UI tests: Start → pending → listening, Stop/End, failure/retry, remount reconciliation and shortcut label.
- Regression test: `MeetingStart` alone must never satisfy a “capture active” assertion.

## P1

1. Add scoped, Bluey-authored interview presets and priority questions on top of the existing live answer/RAG pipeline (`crates/cue-daemon/src/llm/answer.rs:6-34`; `crates/cue-daemon/src/rag_indexer.rs:148-229`).
2. Productize the consent-first screenshot path in the dashboard, with preview, region/window choice, byte/dimension limits, destination disclosure and no implicit persistence (`crates/cue-cli/src/app.rs:2104-2124,2145-2165,2212-2220,2275-2334`; `crates/cue-dashboard/ui/src/App.tsx:119-127`).
3. Add a typed Jobs-to-live handoff from the existing sanitized interview-prep packet, preserving receipt/resume/application consistency and excluding contact, demographics, inbox content and attendees (`jobs/portal/src/lib/interview-prep.ts:28-100`; `jobs/portal/src/components/InterviewPrepDialog.tsx:20-112`).
4. Make long-lived Bluey account tokens OS-secure-store-backed by default. Current provider API keys use keychain, but account tokens default to a private local profile (`crates/cue-daemon/src/secrets/mod.rs:1-29`; `crates/cue-cloud-client/src/tokens.rs:1-6,72-123`).
5. Complete one least-scope read-only inbox provider before claiming email/calendar parity; the current UI explicitly marks authorization beta/not active (`jobs/portal/src/views/SettingsView.tsx:213-233,284-291`).

## P2

- Add advisory meeting-ended detection with debounce and user confirmation; do not auto-end from browser/frontmost heuristics.
- Benchmark an owned/audited AEC or optional neural VAD only against measured latency, double-talk and error rates; do not reuse bundled competitor models/binaries.
- Add structured Approach/Code/Explanation/Complexity/Tests and system-design presentation over existing Bluey answer artifacts.
- Finish a signed dashboard updater release path: Bluey currently has updater wiring but placeholder endpoint/public key (`crates/cue-dashboard/src/lib.rs:49-51,250-273`; `crates/cue-dashboard/tauri.conf.json:42-47`).

## Reuse and provenance

No DMG code, source-map body, prompt, UI text, model, asset, native addon, endpoint contract or embedded configuration may be copied into Bluey. The artifacts are proprietary/minified and do not establish redistribution rights or complete dependency provenance. Adapt only independently specified behavior using Bluey's code and public platform APIs after ownership, license, SBOM and dependency review. Reject generic IPC, broad renderer authority, content telemetry, token-in-URL patterns, silent plaintext fallback, remote OS input and “undetectable” claims.

## Security and privacy

- Add CI tests proving tokens, OTPs, transcripts, answers, resumes, form answers, email bodies and screenshot bytes cannot enter logs/telemetry. Littlebird and Final Round show concrete content-leak failure modes in their [security packs](../research/dmg-audit/INDEX.md#artifact-register).
- Reduce the dashboard's broad default Tauri permissions (`crates/cue-dashboard/capabilities/default.json:1-15`) and expose privileged actions through narrow typed commands.
- Preserve Bluey's HTTPS/WSS and private-target guard (`jobs/browser/src/browser-network.ts:18-53`), encrypted profile envelope (`jobs/runner/src/crypto-envelope.ts:43-167`), challenge approval (`jobs/automation/src/challenge-handling.ts:55-140`) and irreversible-submit journal (`jobs/browser/src/irreversible-submit.ts:44-175`).
- Capture state, permission state, upload destination, retention and deletion must be visible and revocable; meeting state is not proof of active capture.

## Unknowns requiring runtime/authenticated validation

- Competitor authenticated UI, server authorization, retention/deletion, billing enforcement, update rejection, real audio latency/accuracy and authenticated recovery.
- Bluey production installer/updater signing, live provider configuration, inbox/calendar workers, full local/cloud runner canary and no-content telemetry behavior.
- Comparative speed requires same-hardware/network/input measurements: first-audio, first-transcript, first-answer, reconnect recovery, CPU and missed/duplicate segments.

## Concrete implementation handoff

Start only with the P0 slice in `crates/cue-dashboard/src/commands.rs`, `crates/cue-dashboard/src/lib.rs`, and `crates/cue-dashboard/ui/src/routes/LiveTranscript.tsx`. Reuse `AudioStatus`, `AudioStart`, `AudioStop`, saved mic selection and `AudioPipelineStatus`; do not add a second audio stack. Keep daemon protocol payloads typed, add fake-daemon Rust tests plus UI state tests, run dashboard Rust tests/checks and UI test/build, and document results in Round 505 or the next unclaimed round. Do not change Jobs submit/retry/evidence invariants and do not import competitor material.

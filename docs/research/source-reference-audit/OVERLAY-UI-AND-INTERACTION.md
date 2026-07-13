# Overlay, UI, and interaction comparison

## Executive result

Bluey's native overlay is already the strongest implementation in this corpus for its
actual product: it has typed daemon messages, per-spawn capability tokens, field and
state validation, incremental answers, supersession/recovery, route state, attachment
chips, a split workbench, coding/system-design artifacts, click-through controls, and
native macOS/Windows surfaces. The source references supply small UX refinements, not
a replacement frontend. The only clear net-new interaction opportunities are a native
Windows region/multi-monitor selector, user-customizable bounded quick actions, and an
optional `attach to next answer` versus `analyze now` screenshot staging choice.

Current scale is not itself proof of quality, but it explains why a wholesale swap is
irrational: the macOS native overlay is 15,926 lines and the Windows overlay is 3,816
lines. Their relevant behavior is evidenced below, not inferred from line count.

## What Bluey already does better

### Typed, capability-bound overlay IPC

The protocol defines field limits, typed daemon messages, typed renderer commands, a
per-spawn token envelope, allowed UI states, and length validation
(`crates/cue-core/src/overlay_ipc.rs:11-223`). The overlay supervisor validates the
token and has security integration tests
(`crates/cue-daemon/src/overlay.rs:206-229,335-473`;
`crates/cue-daemon/tests/overlay_security_integration.rs:37-205`).

This is stronger than OpenCluely/Vysper's preload surfaces. OpenCluely exposes many
window, session, credential, settings, capture, clipboard, and quit operations and
then adds an unrestricted generic receive method
(`_refs/OpenCluely-main/preload.js:5-100`). Vysper exposes a similar bridge and even
tries to call `require('electron').app.quit()` from a context-isolated renderer
(`_refs/Vysper-main/preload.js:1-87`), which conflicts with its own
`nodeIntegration: false` configuration. Neither pattern belongs in Bluey.

### Streaming, recovery, and structured workbench

Bluey's generation fence drops stale updates and visibly supersedes the prior card
(`crates/cue-daemon/src/app.rs:819-854,1027-1143`). The macOS overlay maps coding and
system-design artifacts into structured canvas modes
(`native/macos/cue-overlay/Sources/cue-overlay/main.swift:2848-2875,12820-12855`),
shows stream/recovery actions (`main.swift:3526-3575,3929-3947,4389`), and includes a
route/status badge and Auto/Quick/Thorough control
(`main.swift:5213-5947,8465-8490,10327-10355,10777-10795`). The Windows overlay has
parallel recovery and answer-detail behavior
(`native/windows/cue-overlay/main.c:81-89,212-237,612-617,1877-1909`).

Final Round's coding/system-design windows and SolveWatch's cheap raw-token rendering
remain useful regression references, but Bluey does not lack those product concepts.
Future work should improve correctness and replay rather than add another renderer.

### Context and evidence presentation

Bluey already presents document/screen context chips, session history, transcript
state, and an answer/canvas split. Its daemon enforces screenshot count/byte limits
(`crates/cue-daemon/src/app.rs:12079-12155,12876-12889`) and its prompt layer supports
coding, system-design, meeting, behavioral, and screen-analysis contracts
(`crates/cue-daemon/src/app.rs:11954-12031,12062,13182-13188`).

Littlebird's exact source-map recovery remains the best workspace/context design
reference, but its workspace shell should inform Bluey's hierarchy and empty/error
states rather than be copied over the native overlay. The recovery evidence is
summarized at
`/Users/uno/Downloads/dmg_backtrack_code/reformed/SECOND-PASS-RECOVERY.md:65-79`.

## Source UI patterns worth adapting

### Pluely: permission remediation

**Observed source.** Pluely presents explicit `checking`, `granted`, `requesting`, and
`denied` states, polls for a bounded 20 seconds, provides a manual recheck, and shows
step-by-step settings instructions
(`_refs/pluely-master/src/pages/app/components/speech/PermissionFlow.tsx:17-84,86-189`).
Its status component clearly prioritizes error, generation, transcription, and
listening (`speech/StatusIndicator.tsx:11-49`). Manual recording exposes `Discard`
and `Stop & Send`, a maximum-duration progress bar, and keyboard hints
(`speech/RecordingPanel.tsx:16-136`).

**Bluey comparison.** Bluey already has a stronger audio-readiness model, explicit
native overlay states, and actionable permission/setup text
(`crates/cue-daemon/src/app.rs:3207,3823,3998-4006,4758-4780,4935-4952`). Treat
Pluely's card as a usability reference only. Add a guided native remediation card
only if task testing shows the existing state/error presentation is insufficient.

### Pluely: bounded user-controlled capture modes

Pluely labels Auto-detect/VAD and Manual modes clearly
(`speech/ModeSwitcher.tsx:10-62`), but its browser VAD immediately sends a whole WAV
after `onSpeechEnd` and lacks generation/cancellation/backpressure controls
(`completion/AutoSpeechVad.tsx:32-92`). Use the labels and explicit user choice, not
that pipeline.

Pluely's multi-monitor area selector captures monitors, creates one selection overlay
per display, crops the selected region, and clears retained captures
(`_refs/pluely-master/src-tauri/src/capture.rs:42-96,120-188,200-257`). This is a useful
interaction reference for a future bounded screen-review flow. Bluey must retain its
own byte/pixel/deadline limits, consent state, temp-file rules, and native helper
trust boundary.

### Aura: screenshot staging

Aura uses a four-item screenshot queue and reuses the display-capture track
(`_refs/Aura-AI-master/web/js/screenshot-service.js:4-14,312-425`). Add a maximum-four
Bluey staging strip with states `captured`, `queued`, `analyzing`, `ready`, and
`failed`; allow remove/retry before model submission. Do not use Aura's base64
WebSocket transport, which lacks server-side byte/pixel bounds
(`web/js/websocket-handler.js:278-320`; `services/vision_service.py:78-95`).

### OpenCluely/Vysper: window interaction vocabulary

The related projects use compact actions for show/hide, interaction mode, linked
window movement, session clear, speech availability, and capture. Their main windows
use context isolation and disable Node integration
(`_refs/OpenCluely-main/src/managers/window.manager.js:195-222`), while their preload
methods and channel validation reveal the intended interaction vocabulary
(`_refs/OpenCluely-main/preload.js:5-132`;
`_refs/Vysper-main/preload.js:5-119`).

Bluey already has click-through and global keyboard control. Keep its typed protocol;
borrow only concise labels and progressive disclosure. Do not reproduce the multiple
overlapping APIs, generic receive, renderer listener leaks, or renderer-controlled
credentials/settings.

Vysper's only material addition over the shared family is OCR and a broader prompt
pack. Its OCR serializes work with an `isProcessing` flag, deletes temporary files,
and returns metadata (`_refs/Vysper-main/src/services/ocr.service.js:8-45,47-105,108-153`).
It also logs temp paths and writes screenshots to a configurable directory without
visible size/deadline controls. Treat it as a flow reference, not production OCR.

### Natively and Littlebird: context hierarchy

Natively's session tracker distinguishes final/interim transcript, current coding
question, recent assistant responses, epoch summaries, and temporal intent. Littlebird
supplies the broadest exact workspace/onboarding/integration renderer source. Together
they suggest one hierarchy for Bluey:

1. current ask and live source state;
2. explicit attached evidence;
3. current session/workspace memory;
4. derived long-horizon summaries;
5. optional integration context.

Bluey already has all five data concepts across native overlay, daemon, RAG, and Jobs.
The improvement is to make provenance and staleness visible: show source, timestamp or
revision, scope, and whether an item is raw or derived.

### Natively: frame-paced stream presentation

Natively collects answer tokens and commits display updates on
`requestAnimationFrame`, then synchronously flushes the remaining text at completion
(`_refs/natively-cluely-ai-assistant-main/src/components/NativelyInterface.tsx:884`).
Bluey currently emits every daemon delta immediately
(`crates/cue-daemon/src/app.rs:856-878`) and the macOS overlay relayouts every
`UpdateCard` (`native/macos/cue-overlay/Sources/cue-overlay/main.swift:11165`).

Adapt the presentation optimization, not Natively's renderer boundary: batch only
display deltas into 16--33 millisecond frames or a bounded byte threshold, preserve
generation/session fencing and exact final text, and synchronously flush or discard
on final, error, cancellation, and supersession.

### Pluely: bounded quick actions

Pluely lets users show, hide, add, and remove compact prompt chips
(`_refs/pluely-master/src/pages/app/components/speech/QuickActions.tsx:14-140`) and
ships defaults for `What should I say?`, follow-ups, fact-check, and recap
(`_refs/pluely-master/src/config/constants.ts:35-40`). Bluey can reimplement this as
an optional native-overlay feature using its existing settings/session persistence.
Enforce count/length limits and store only prompt text, never provider keys, cURL, or
executable content.

## Smallest production-quality UI additions

### P0-linked UI

These UI states must ship with the underlying P0 transport work, not before it:

- explicit `backpressure` or `audio overloaded` health when bounded queues drop
  frames; never pretend the pipeline is healthy;
- `reconnecting`, `recovered`, and `stopped after repeated crashes` helper states;
- a stable terminal state for every answer stream using request/generation/sequence;
- a privacy-safe audit setting that explains whether final conversation content is
  synced, without exposing operational audit internals.

### P1 interaction pass

1. Native Windows region/multi-monitor selector. Current Windows CLI capture uses
   PowerShell and only `PrimaryScreen.Bounds`
   (`crates/cue-cli/src/app.rs:2482-2509`); recreate Pluely's geometry/lifecycle in
   Bluey's native Windows path with cancellation, pixel/byte bounds, mixed-DPI tests,
   and no global base64 event.
2. Stream reconnection banner backed by monotonic sequence and snapshot replay, then
   disappear after a verified current snapshot.
3. Show source/scope/revision for raw and derived context where the existing context
   chip does not already make it clear.
4. Frame-paced answer presentation with mandatory final/error/supersede flush and an
   exact-final reconstruction test at burst rates.

### P2 polish

- Optional bounded quick-action chips implemented natively.
- Optional screenshot staging choice with preview/remove/retry before analysis.
- Guided permission remediation only if usability tests show the current readiness
  states are insufficient.
- Task-level usability tests for time-to-first-answer, mistaken mode, screenshot
  review, recovery comprehension, and keyboard-only use.
- Mixed-DPI/multi-monitor area selection only after privacy, bounds, and real-platform
  canaries.
- Calendar/email context badge only after a production connector has scope, cursor,
  deletion, and retention guarantees.

## Security and maintenance rejects

- Pluely grants both windows Keychain operations, SQL, PostHog, shell-open, updater,
  and unrestricted HTTP/HTTPS destinations
  (`_refs/pluely-master/src-tauri/capabilities/default.json:1-32` and
  `cross-platform.json:1-31`). Bluey should not adopt this broad renderer authority
  and should not introduce Keychain integration.
- OpenCluely/Vysper expose large overlapping preload APIs and generic event receive;
  Bluey's typed request variants and token/state checks remain authoritative.
- OpenCluely enables DevTools in its window base configuration
  (`_refs/OpenCluely-main/src/managers/window.manager.js:201-216`) and forwards selected
  renderer console output into main logs (`window.manager.js:174-179`). Do not copy
  content-bearing renderer diagnostics.
- OpenCluely disables TLS certificate validation and grants microphone, camera, and
  display capture without an origin-specific policy
  (`_refs/OpenCluely-main/main.js:149-180`). It also logs settings that can contain
  credentials (`main.js:1086`) and implements timeout with `Promise.race` without
  cancelling the underlying request (`src/services/llm.service.js:730`). Reject all
  four patterns.
- Vysper writes screenshot temp files and logs their paths. Bluey should prefer an
  in-memory bounded path or owner-only randomly named files with guaranteed cleanup
  and content-free logging.
- Vysper loads remote renderer dependencies and assigns rendered Markdown to
  `innerHTML` (`_refs/Vysper-main/llm-response.html:1,621`). Do not ship remote UI
  dependencies or unsanitized HTML.
- Recovered products' broad identical preloads, renderer-readable secrets, token URLs,
  remote input, and plaintext fallbacks remain rejected
  (`/Users/uno/Downloads/exe_backtrack_code/recovered/INDEX.md:27-30`).

## Definition of “better” for the next UI round

Do not claim superiority from feature count. Retain comparative evidence for:

- first visible stable transcript and first answer token p50/p95/p99;
- bounded memory during a two-hour and eight-hour session;
- permission-to-listening completion rate;
- recovery success after helper, network, and overlay restarts;
- screenshot queue completion and mistaken-send rate;
- keyboard-only and screen-reader completion of core actions;
- zero raw user content in operational telemetry.

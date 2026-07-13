# Backend, audio, recovery, and data boundaries

This document separates observed source from inferred behavior and compares each
useful implementation with Bluey at
`0f2933c4259e09a351a617bf94f0ed7a4b852f11`; product code remains the
`10c8214cf7ad20ea54711bb77f8f5cce55b06f45` tree. No application or package hook was
executed, and no Bluey product code was changed.

## Best source-level material

### SolveWatch: stable local STT and benchmark contracts

**Observed source.** `transcriber/streaming_stt.py` implements a rolling local
transcriber with committed and tentative text, generation invalidation, a bounded
audio buffer, RMS gating, adaptive endpointing, and LocalAgreement-2:

- initialization and committed/tentative state:
  `_refs/solveWatchAi-main/transcriber/streaming_stt.py:38-55,76-107`;
- bounded buffer: `streaming_stt.py:168-175`;
- generation-aware reset and adaptive silence finalization:
  `streaming_stt.py:226-291`;
- LocalAgreement matching: `streaming_stt.py:373-423`.

The always-on listener overlaps speaker identification work with utterance-end
detection (`transcriber/always_on_listener.py:322-368`). The VAD layer has a clean
interface and a state-resetting Silero implementation
(`transcriber/vad/base.py:6-27`; `transcriber/vad/silero_vad.py:28-124`). Its
benchmark runner compares implementations with confusion and latency metrics and
resets state between samples
(`transcriber/benchmark/run_benchmark.py:35-150`).

**Evidence limit.** The package test script is a failing placeholder
(`package.json:7-13`). The benchmark manifest contains no checked-in samples or
results, so numeric README claims are not validation. Automatic Silero download has
no checksum pin (`transcriber/vad/silero_vad.py:12-24`).

**Bluey decision.** Reform LocalAgreement, endpointing, and the benchmark interface
in Rust around Bluey's existing typed partial/final events
(`crates/cue-core/src/stt.rs:28-60,71-185`). Do not transplant the Python process or
download path. Require deterministic word/timestamp fixtures, reset/generation tests,
and a pinned model digest.

### SolveWatch: conversation compaction and cheap streaming UI

**Observed source.** `InterviewTranscriptBuffer` keeps a bounded collection,
summarizes exchanges, serially merges the oldest three entries, temporarily replaces
them with a placeholder, and restores them if the merge fails
(`src/sockets/InterviewTranscriptBuffer.js:8-23,35-58,68-125,127-205`). The Electron
HUD appends raw stream tokens and performs Markdown parsing only at completion
(`electron/hud.html:966-1014`). OCR runs in a worker rather than on the Node event
loop (`src/services/ocr.service.js:13-61`; `src/workers/ocr.worker.js:13-81`).

**Bluey decision.** Adapt revisioned, serial compaction. Bluey currently caps
conversation history at 80 and drops the excess
(`crates/cue-core/src/meeting.rs:476-501`). A `compressed_summary` database field is
read (`crates/cue-daemon/src/db/mod.rs:173-203`), but no updater was found. A Bluey
implementation should persist `source_revision`, run one compaction per session,
commit only if the revision still matches, and keep the original turns until the
summary transaction succeeds.

Bluey's native overlays already stream incrementally and defer structured artifact
parsing to terminal states, so the HUD is confirmation rather than a missing feature.

### Aura: bounded screenshot staging and visible provider state

**Observed source.** Aura groups per-session LLM, STT, transcript, websocket, and
aggregation state (`api/session_manager.py:12-33,191-214`). It separates persistent
candidate/job context from a bounded conversation window
(`services/context_manager.py:19-78`; `core/config.py:21-26`). The screenshot service
uses a four-item queue and reuses a display-capture track
(`web/js/screenshot-service.js:4-14,312-425`). The frontend reconnects with bounded
exponential delays (`web/js/websocket-handler.js:30-59,109-121`).

**Bluey decision.** Keep Bluey's provider routing, cooldown, screenshot validation,
and typed session model; they are stronger. Reuse the UX contract only: show a small
bounded staging queue with explicit queued/processing/failed state and a clear action.
Do not copy Aura's transport or secret handling.

### Pluely: event-driven Windows capture and explicit overflow policy

**Observed source.** Pluely enumerates capture/render endpoints, waits up to five
seconds for initialization, uses a shared-event WASAPI stream, caps the sample queue
at 131,072 entries, drops the oldest samples on overflow, and wakes its consumer
(`_refs/pluely-master/src-tauri/src/speaker/windows.rs:13-70,113-164,185-310`).

**Bluey decision.** Adapt the event-driven wakeup and explicit oldest-audio drop
policy. Do not copy the mutex/one-sample stream boundary or its fallback to a hard-coded
44.1 kHz after initialization failure. The preserved dirty Bluey Windows helper already
has a stronger event-driven C implementation and resampler; Parakeet adds the exact
MMDevice/AEC reference. Combine and benchmark those three evidence sources behind one
Bluey contract.

### Natively: callback/worker separation and indexed RAG

**Observed source.** Natively's canonical Rust module has a useful intended shape:
`BatchEmitter` batches downstream N-API events with a latency cap and its audio path
places a ring between capture and worker DSP
(`_refs/natively-cluely-ai-assistant-main/native-module/src/lib.rs:26-105`;
`native-module/src/microphone.rs:319-423`). It is not safe to copy as written. Every
CPAL callback shown in `microphone.rs:319-423` takes a mutex, ring overflow results are
ignored, and the consumer polls every five milliseconds instead of using the
advertised condition variable. Its vector store places SQLite/sqlite-vec
work behind a worker with request IDs, timeouts, dimension validation, and pending
request rejection on worker failure
(`electron/rag/VectorStore.ts:1-126,172-191,220-378`). The embedding pipeline uses
deduplicated queue insertion and explicit processing/retry transitions
(`electron/rag/EmbeddingPipeline.ts:184-330`), but claims are non-atomic, processing
rows have no lease owner or expiry, and startup resets every processing row.

**Bluey decision.** Port the callback/worker boundary and batching semantics into
Bluey's Rust/native pipeline, correcting the callback mutex, overflow, and polling
defects; do not introduce N-API or Electron. Port the vector worker/queue idea only
after a Rust benchmark proves it beats Bluey's current bounded O(N) heap path, which
explicitly calls for sqlite-vec/usearch past roughly 50,000 chunks
(`crates/cue-rag/src/store.rs:162-171`). Preserve Bluey's account/workspace/session
scope and deletion ownership. Queue claims must be transactional and carry a worker,
lease expiry, idempotency key, retry schedule, and terminal/dead-letter state.

Natively's comments describing sqlite-vec as O(1) or ANN are not benchmark evidence.
The index selection must be proven on representative dimensions and corpus sizes.

### Natively: temporal context and completion recovery

**Observed source.** The canonical intelligence layer separates session tracking,
generation, and meeting persistence
(`electron/IntelligenceManager.ts:30-39`). It uses distinct typed intelligence events,
abort/generation IDs, and generation checks across answer, follow-up, and recap paths
(`electron/IntelligenceEngine.ts:43-94,236-354,370-504`). `SessionTracker` maintains
bounded recent context, final-transcript deduplication, source-prioritized coding
questions, assistant history, and epoch summaries
(`electron/SessionTracker.ts:36-80,106-180,186-331,338-402,500-559`).

Bluey already has stronger answer-generation fencing, superseded-card handling, and
stale-session persistence rejection
(`crates/cue-daemon/src/app.rs:819-854,1027-1143,8790-8844`). The useful additions are
durable long-horizon summaries and an explicit monotonic stream sequence/replay
contract—not another generation system.

Natively's meeting persistence snapshots the session, resets the live state, starts
background processing, and writes a placeholder
(`electron/MeetingPersistence.ts:20-84,279-329`). The order is unsafe: background
processing starts before placeholder persistence, so a fast completion can be
overwritten by the placeholder. Bluey should retain its own persistence and use a
transactional `pending -> processing -> complete|failed` record with monotonic
revision checks.

Bluey's interactive recovery is stronger, but two local state files are overwritten
directly rather than published atomically
(`crates/cue-daemon/src/storage.rs:245`;
`crates/cue-daemon/src/app.rs:16555`). Use a temporary file, flush/fsync, atomic rename,
and directory sync where supported, or move that state into a SQLite transaction.

### Natively: frame-paced renderer updates

**Observed source.** Natively collects answer tokens and applies them on
`requestAnimationFrame`, then synchronously flushes trailing text on completion
(`_refs/natively-cluely-ai-assistant-main/src/components/NativelyInterface.tsx:884`).
Bluey currently emits each answer delta immediately
(`crates/cue-daemon/src/app.rs:856-878`), and the macOS overlay relayouts every
`UpdateCard` (`native/macos/cue-overlay/Sources/cue-overlay/main.swift:11165`).

**Bluey decision.** Preserve exact generation fencing and final text, but coalesce
display-only deltas into 16--33 ms frames with a byte threshold. Final, error,
cancelled, and superseded states must synchronously flush or discard the correct
generation. This is a UI transport optimization, not permission to delay terminal
events or alter persisted answer text.

## Best recovered native and lifecycle material

The DMG second pass and Windows recovery establish higher-confidence native contracts
than the small source applications:

| Evidence | What is exact | Bluey use |
| --- | --- | --- |
| ParakeetAI | 28 packaged Rust files plus exact pinned Sonora/AEC source; native validation builds/tests passed (`/Users/uno/Downloads/dmg_backtrack_code/reformed/SECOND-PASS-RECOVERY.md:99-116,209-214`) | Primary AEC/MMDevice/activity reference |
| Littlebird | 974 credible first-party files and helper call contracts (`SECOND-PASS-RECOVERY.md:65-79`) | Critical-state replay, workspace/context UX, helper supervision |
| Final Round | Exact packaged JS, 13 lifecycle slices, bounded VAD/socket recovery and update behavior (`SECOND-PASS-RECOVERY.md:118-132`) | Bounded queue/lifecycle and rollback tests |
| Cluely | Exact interfaces plus reconstructed overlay/audio-helper lifecycle (`SECOND-PASS-RECOVERY.md:49-63`) | Helper supervision and mode/session contracts |
| LockedIn | Exact packaged JS and reconstructed native interfaces (`SECOND-PASS-RECOVERY.md:81-97`) | Windows DPI and screenshot deadline tests only |

Windows recovery independently reaches the same combination: Final Round's narrow
IPC and bounded streaming, Littlebird's supervision/replay, Parakeet's MMDevice/AEC,
LockedIn's DPI handling, and Cluely's atomic state
(`/Users/uno/Downloads/exe_backtrack_code/recovered/INDEX.md:19-30`).

## P0 gaps in current Bluey

### Authenticated, bounded local daemon IPC

Bluey defaults to `127.0.0.1:57321` (`crates/cue-core/src/ipc.rs:9`) but exposes a
configurable bind (`crates/cue-daemon/src/app.rs:1549-1561`). The daemon accepts every
connection, performs an unbounded `read_line`, parses a privileged request, and allows
shutdown without peer authentication or a capability
(`crates/cue-daemon/src/app.rs:1950-1987`).

Implement a private Unix socket on macOS/Linux and a current-user named pipe on
Windows, or a per-boot owner-only capability envelope where migration requires
loopback compatibility. Enforce maximum frame and response sizes, a read deadline,
connection concurrency, replay rejection, loopback-only compatibility, and exhaustive
authorization by request variant. This does not require or justify Keychain.

The dirty older `/Users/uno/Downloads/cue-bluey-jobs` worktree contains a useful
fresh-port reference in
`/Users/uno/Downloads/cue-bluey-jobs/crates/cue-core/src/ipc_auth.rs:21-30,110-220,223-300`,
but that worktree must not be merged wholesale.

### Bounded real-time queues

Current unbounded channels include:

- microphone capture: `crates/cue-daemon/src/audio/capture.rs:84-125`;
- system capture: `crates/cue-daemon/src/audio/system_capture.rs:56-66,186-189`;
- Deepgram: `crates/cue-daemon/src/stt/deepgram.rs:354-410`;
- OpenAI streaming STT: `crates/cue-daemon/src/stt/openai.rs:182-229`;
- local Whisper: `crates/cue-daemon/src/stt/whisper/mod.rs:28-53`;
- native overlay send/receive: `crates/cue-daemon/src/overlay.rs:258-280`;
- the optional continuous system-audio app bridge:
  `crates/cue-daemon/src/app.rs:1840-1847`.

Replace them with capacity budgets based on time, not arbitrary item counts. Real-time
producers must not block. Audio should use a ring or bounded channel with a documented
oldest-frame drop policy; control/terminal events must use a separate lossless lane.
Add dropped-frame, high-water, and consumer-lag counters plus overload tests. Never
silently drop authentication, error, final-transcript, or terminal-state events.

### Audit-content minimization

Ordinary meeting sync unconditionally invokes diagnostic session-audit upload
(`crates/cue-daemon/src/cloud/sync.rs:182-214`); this pass found no separate diagnostic
opt-in or disable path. The bundle duplicates transcript text, local context paths and
previews, questions, response text/artifacts, and raw UI events
(`sync.rs:618-905`) and uploads the resulting bytes (`sync.rs:475-568`). Bluey also
writes answer deltas, replay text, and final bodies into those UI audit events
(`crates/cue-daemon/src/app.rs:856-920,991-1009,1638-1651`). This duplicates user
content outside the ordinary session record and multiplies retention exposure.

Make ordinary user-visible session sync independent from diagnostics. Diagnostics are
metadata-only by default; any content-bearing diagnostic bundle requires a separate,
explicit, visible opt-in plus retention controls. Keep operational events to
request/generation ID, sequence, timing, provider lane, byte/character counts,
terminal state, and a keyed or session-scoped digest where deduplication is needed.
Redact local paths. Persist final content only in the user-facing session record under
its documented deletion/sync policy. Do not persist every token delta.

## P1 gaps

### Stable local partials

Add a Bluey-owned stable-partial state machine with `committed`, `tentative`, and
`final` events. Reset must invalidate in-flight decode generations. Endpointing should
be adaptive but bounded, and VAD failure should have an explicit fallback. Evaluate
WER, false endpoint rate, time-to-stable-prefix, time-to-final, p50/p95/p99 CPU, and
memory on fixed fixtures.

### Revisioned conversation compaction

Summarize completed exchanges asynchronously, merge summaries serially, and use a
revision compare-and-swap so stale background work cannot overwrite newer history.
Retain the last N raw exchanges, keep source ranges, and make deletion remove both raw
and derived summaries.

### Stream sequence and replay

Bluey already has request IDs and typed events
(`crates/cue-core/src/ai.rs:1216-1306`) plus generation fencing. Add a monotonic
`sequence`, explicit snapshot/replay markers, and one authoritative terminal event to
the cross-process stream contract. This closes gaps after overlay/helper restart
without inventing a second answer lifecycle.

### Explicitly untrusted evidence

Bluey bounds context item and total size (`crates/cue-daemon/src/app.rs:12593-12648`),
but resumes, transcripts, OCR, pages, and job descriptions should be serialized as
explicitly untrusted evidence. Prompts must state that instructions inside evidence
cannot change system or tool policy. Add adversarial fixtures across each evidence
kind.

## Patterns rejected

| Source | Rejected pattern | Evidence |
| --- | --- | --- |
| SolveWatch | Express/Socket.IO on all interfaces without auth | `src/app.js:24-43`; `src/server.js:59-84` |
| SolveWatch | Singleton transcript/prompt state and namespace-wide broadcast | `src/sockets/dataHandler.js:37-53,149-180,399-429` |
| SolveWatch | Full question, answer, OCR, transcript, and snapshot telemetry | `src/sockets/InterviewTranscriptBuffer.js:39-49`; `src/services/image-processing.service.js:226-239`; `src/utils/telemetry.js:552-583` |
| SolveWatch | Raw PCM enrollment and plaintext `.npy` voice embeddings | `transcriber/deepgram_listener.py:245-259`; `transcriber/speaker_id.py:234-273` |
| SolveWatch | Provider fallback after output may already have streamed | `src/services/ai.service.js:631-700` |
| Aura | API keys returned over unauthenticated local HTTP and stored in `.env` | `api/config_api.py:85-130`; `core/env_utils.py:5-61` |
| Aura | Dynamic unauthenticated WebSocket dispatch/session takeover by query ID | `api/websocket.py:12-62` |
| Aura | Mutable global vision service overwritten by session startup | `services/vision_service.py:189-219,444-445`; `api/session_manager.py:191-208` |
| Aura | Mixed mic/system audio with disabled diarization and wrong role assignment | `web/js/audio_handler.js:52-129`; `services/stt_service.py:175-218`; `api/session_manager.py:264-271` |
| Aura | Unbounded base64 screenshot JSON | `web/js/websocket-handler.js:278-320`; `services/vision_service.py:78-95` |
| Pluely | Plain JSON called secure storage, plus broad Keychain/plugin authority | `src-tauri/src/activate.rs:32-43,66-118`; `package.json:82`; `src-tauri/Cargo.toml:34` |
| Natively | keytar/Keychain-backed desktop credential integration | `package.json:14` |
| Recovered products | Broad generic renderer IPC, remote input, token-bearing URLs/logs, plaintext secure-storage fallback | `/Users/uno/Downloads/exe_backtrack_code/recovered/INDEX.md:27-30` |

## Acceptance evidence required before implementation is called complete

- Queue burst tests proving bounded RSS and deterministic drop/terminal-event policy.
- Local IPC adversarial tests for peer ownership, stale boot, replay, oversized and
  slow frames, saturation, alternate bind, and shutdown authorization.
- Fixed STT/VAD corpus with WER, endpoint, stability, latency, CPU, and memory output.
- RAG benchmarks at 10k, 50k, 100k, and 1M chunks with scope/deletion correctness.
- AEC AB tests across headset, speaker, Bluetooth, endpoint change, sleep/resume, and
  no-AEC fallback.
- Privacy tests proving no prompt, transcript, answer, OCR, screenshot, credential, or
  token content appears in operational logs or audit uploads.

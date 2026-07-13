# Concrete Bluey implementation handoff

Status: ready for a fresh implementation round; no product code was changed by this
audit

Audit baseline: `0f2933c4259e09a351a617bf94f0ed7a4b852f11`; product code is unchanged from
`10c8214cf7ad20ea54711bb77f8f5cce55b06f45`

## Architectural decision

Bluey remains the product spine:

```text
CLI / native overlay
        |
authenticated, bounded local transport
        |
cue-daemon session + generation authority
        |-------------------------|
bounded audio/STT pipeline        scoped local RAG + durable context
        |                         |
Bluey provider router ---------- managed server
                                  |
accounts, billing, usage, sync, Jobs, receipts, deletion
```

Do not create a second Electron/Tauri desktop stack or replace the server with any
reference backend. All additions terminate at existing Bluey types, ownership scopes,
and deletion paths.

## P0 implementation sequence

### P0.1 Authenticated, bounded daemon IPC

Goal: no unauthenticated privileged command, unbounded frame, slowloris connection,
replay, or arbitrary non-loopback bind.

Primary files:

- add a Bluey-owned transport/auth module under `crates/cue-core/src/`;
- migrate `crates/cue-daemon/src/app.rs:1549-1561,1950-1987`;
- migrate CLI and dashboard clients in `crates/cue-cli/src/app.rs` and
  `crates/cue-dashboard/src/commands.rs`;
- keep `crates/cue-core/src/ipc.rs` as the typed request/response model.

Contract:

1. macOS/Linux use an owner-only Unix socket in the Bluey runtime directory.
2. Windows uses a local current-user named pipe with remote clients rejected.
3. A per-boot 256-bit capability authenticates protected reads, mutations, and
   shutdown; compare it in constant time and never log it.
4. Classify every `DaemonRequest` exhaustively as public, protected read, mutation,
   or shutdown. Adding a new request must fail compilation until classified.
5. Enforce request/response limits before allocation or write, a read deadline, a
   connection semaphore, and a bounded replay cache keyed by boot/request ID.
6. Permit only numeric loopback during a compatibility window; do not permit arbitrary
   `--addr` exposure.
7. Publish the capability through an atomic owner-only runtime file with reparse and
   ownership checks. No Keychain, keytar, or `tauri-plugin-keychain`.

Fresh-port reference only:
`/Users/uno/Downloads/cue-bluey-jobs/crates/cue-core/src/ipc_auth.rs:21-30,32-220,223-300`.
That worktree is older and very dirty; port reviewed hunks and tests, never merge it.

Required tests:

- valid public/protected/mutation/shutdown cases;
- wrong bearer, stale boot, duplicate request, non-owner peer, other session, remote
  pipe, alternate bind;
- exactly-at-limit and one-byte-over request/response;
- half-open/slow frame, connection saturation, cancellation, crash cleanup, concurrent
  daemon starts, and atomic capability replacement.

### P0.2 Bounded real-time lanes and helper supervision

Goal: bounded memory and deterministic behavior under stalled consumers, helper
failure, provider delay, and overlay restart.

Primary files:

- `crates/cue-daemon/src/audio/capture.rs`;
- `crates/cue-daemon/src/audio/system_capture.rs`;
- `crates/cue-daemon/src/stt/{deepgram,openai,whisper/mod}.rs`;
- `crates/cue-daemon/src/overlay.rs`;
- `crates/cue-daemon/src/app.rs`.

Contract:

1. Split lossy real-time audio/data from lossless control/final/error events.
2. Size audio buffers by milliseconds; producers use nonblocking writes and drop the
   oldest audio when the budget is exceeded.
3. Never drop auth, provider error, permission error, final transcript, shutdown,
   cancellation, or terminal answer events.
4. Add high-water, dropped-frame, dropped-partial, consumer-lag, restart, and terminal
   counters without content fields.
5. Spawn helpers with a minimal allowlisted environment, piped bounded diagnostics,
   a readiness deadline, `kill_on_drop`, a stop deadline, and forced abort fallback.
6. Use a bounded exponential restart budget and replay only explicit critical state
   after a verified ready handshake.

Evidence to combine:

- Pluely event-driven capture and oldest-drop queue:
  `_refs/pluely-master/src-tauri/src/speaker/windows.rs:185-310`;
- Littlebird crash supervision/state replay:
  `/Users/uno/Downloads/exe_backtrack_code/recovered/INDEX.md:14,21-25`;
- Final Round bounded streaming/lifecycle:
  `/Users/uno/Downloads/dmg_backtrack_code/reformed/SECOND-PASS-RECOVERY.md:118-132`;
- preserved Bluey supervisor work:
  `/Users/uno/Downloads/cue-bluey-jobs/crates/cue-daemon/src/audio/system_capture.rs`.

Required tests:

- burst producer with blocked consumer and bounded RSS;
- exact drop policy and no loss of final/control events;
- helper ready timeout, malformed/oversized diagnostics, crash loop, clean stop,
  timed-out stop, and state replay;
- provider reconnect while audio continues, overlay restart during stream, and daemon
  shutdown at each lifecycle state.

### P0.3 Content-minimized operational audit

Goal: user content exists once under the user-visible session/history retention
policy, and ordinary cloud sync never silently doubles as content-bearing diagnostic
upload.

Primary files:

- `crates/cue-daemon/src/app.rs:856-920,991-1009,1638-1651`;
- `crates/cue-daemon/src/cloud/sync.rs:182-214,475-568,618-905`;
- session audit schema and deletion/export tests.

Decouple normal session sync from diagnostic bundle upload. Default diagnostics to
metadata only. Content-bearing diagnostics require a separate explicit opt-in,
visible scope/retention controls, and local-path redaction. Replace raw
delta/replay/final fields with request ID, generation, monotonic sequence, provider
lane, timing, character/byte count, terminal state, failure category, and a
session-scoped digest only where deduplication requires it. Keep final answer content
in the existing conversation record. Confirm that account/session deletion removes
both primary and derived records.

Required tests grep structured logs, JSONL, crash output, and upload fixtures for
known prompt/transcript/answer/OCR/screenshot/token canaries.

## P1 implementation sequence

### P1.1 Stable local transcription and VAD benchmark

Port the behavior of SolveWatch's bounded LocalAgreement-2 implementation, not the
Python runtime:

- `_refs/solveWatchAi-main/transcriber/streaming_stt.py:38-55,76-107,168-175,226-291,373-423`;
- `_refs/solveWatchAi-main/transcriber/vad/base.py:6-27`;
- `_refs/solveWatchAi-main/transcriber/benchmark/run_benchmark.py:35-150`.

Bluey types should distinguish `tentative`, `committed`, and `final`, carry segment
and generation IDs, and invalidate decode work on reset/device/session change. Pin
model artifacts by digest. The benchmark must include fixed audio/transcript labels
and report WER, stable-prefix delay, endpoint false-positive/false-negative rates,
p50/p95/p99, CPU, and RSS.

### P1.2 Windows audio quality and AEC

Fresh-port the event-driven Windows 10 helper and polyphase resampler from the dirty
Bluey worktree, then integrate exact Parakeet evidence behind an optional AEC stage:

- current weak path: `native/windows/cue-audio/main.c:3,165-185,261-270,295-307`;
- preserved Bluey path:
  `/Users/uno/Downloads/cue-bluey-jobs/native/windows/cue-audio/main.c:203-246,317-371` and
  `resampler.{c,h}`;
- exact AEC/MMDevice source index:
  `/Users/uno/Downloads/dmg_backtrack_code/reformed/parakeetai-3.6.21/exact-native-source/README.md`;
- exact pinned Sonora evidence:
  `/Users/uno/Downloads/dmg_backtrack_code/reformed/parakeetai-3.6.21/exact-dependency-source/sonora/README.md`;
- Natively callback/worker and batching reference (design only; its callback mutex,
  ignored overflow, and five-millisecond polling must be corrected):
  `_refs/natively-cluely-ai-assistant-main/native-module/src/microphone.rs:319-423` and
  `native-module/src/lib.rs:26-105`.

The real-time callback may only validate and push to a preallocated ring. Resampling,
AEC, VAD, batching, and serialization run off the callback thread. AEC remains
optional until AB measurements prove better intelligibility/latency across speaker,
headset, Bluetooth, endpoint switch, sleep/resume, and raw fallback.

### P1.3 Revisioned conversation compaction

Add a durable per-session summary state with source revision, raw range, derived
summary, model/version, state, attempt count, and timestamps. Keep one compactor per
session and commit only if the source revision is still current. Retain recent raw
turns and remove both raw and derived data on deletion.

Source references:

- SolveWatch serial merge/restore:
  `_refs/solveWatchAi-main/src/sockets/InterviewTranscriptBuffer.js:35-58,68-125`;
- Natively bounded epochs:
  `_refs/natively-cluely-ai-assistant-main/electron/SessionTracker.ts:36-80,500-559`.

Do not copy either implementation's content logging or fire-and-forget race behavior.
Also make Bluey's active meeting and daemon-state publication crash-safe: current
direct overwrites at `crates/cue-daemon/src/storage.rs:245` and
`crates/cue-daemon/src/app.rs:16555` need temporary-file + flush/fsync + atomic rename
and directory sync where supported, or a SQLite transaction.

### P1.4 Indexed, queued local RAG

Replace the O(N) scan only after benchmark parity and scope correctness:

- current path and documented threshold: `crates/cue-rag/src/store.rs:162-171`;
- current 10k baseline: `crates/cue-rag/tests/rag_scaling.rs:20-64`;
- Natively worker/index reference:
  `_refs/natively-cluely-ai-assistant-main/electron/rag/VectorStore.ts:1-126,172-191,220-378`;
- Natively deduplicated embedding queue and retry-state reference (its claims are
  non-atomic and have no lease owner/expiry):
  `electron/rag/EmbeddingPipeline.ts:184-330`.

Implement in Rust with account/workspace/session scope in every row and query. Queue
records need idempotency key, lease/owner, attempt count, next attempt, source
revision, model/dimension, and terminal state. A model/dimension change creates a new
index generation; never compare vectors of different dimensions. Deletion must cover
source chunks, vectors, queue entries, summaries, and caches transactionally or by a
durable outbox.

Benchmark brute-force and candidate indexes at 10k/50k/100k/1M chunks for p50/p95/p99,
recall@k, index time, update time, delete time, disk, and RSS.

### P1.5 Stream sequence/replay and evidence trust

Extend existing answer request IDs and generation fencing with monotonic sequence,
snapshot/replay markers, and one terminal state. Overlay restart requests a current
snapshot then resumes only at `last_sequence + 1`.

Serialize resume, job description, transcript, OCR, page, screenshot-derived text,
and documents as typed untrusted evidence. The system contract must state that
instructions in evidence cannot alter tool/system policy. Add cross-kind prompt
injection fixtures.

### P1.6 Focused native-overlay UX

Retain the native overlay. The one clear P1 addition is:

- a native Windows region/multi-monitor selector to replace the current primary-screen
  PowerShell path, with cancellation, strict coordinate/pixel/byte/deadline bounds,
  consent, and mixed-DPI tests;
- verified reconnect/replay status and missing provenance labels where current UI
  testing identifies a gap.

Keep bounded customizable quick actions, screenshot staging, and any expanded
permission-remediation card as P2 usability improvements. Bluey's current native
readiness, generation, recovery, accessibility, and overlay IPC model is already
stronger than the three small source references.

References and rejects are in
[OVERLAY-UI-AND-INTERACTION.md](OVERLAY-UI-AND-INTERACTION.md).

### P1.7 Frame-paced answer presentation

Keep the daemon's generation/session checks and exact persisted final text, but batch
display-only answer deltas into 16--33 millisecond frames with a byte threshold.
Final, error, cancelled, and superseded states must synchronously flush or discard the
correct generation.

Evidence:

- current immediate delta emission: `crates/cue-daemon/src/app.rs:856-878`;
- current per-update macOS relayout:
  `native/macos/cue-overlay/Sources/cue-overlay/main.swift:11165`;
- Natively frame batching and final flush:
  `_refs/natively-cluely-ai-assistant-main/src/components/NativelyInterface.tsx:884`.

Acceptance requires 1,000 synthetic deltas per second without unbounded memory or
layout work, exact final reconstruction, stale-generation rejection, and mandatory
flush behavior for every terminal path.

## P2 candidates

- consent-first native window/area selection with mixed-DPI and multi-monitor tests;
- OCR worker with strict input/output/deadline/temp-file limits and no path/content
  logs;
- terminal-visible advisory meeting hints after real Windows/macOS validation;
- one email/calendar outcome connector after cursor, scope, deletion, retention, and
  content-free telemetry contracts;
- native Windows ARM64 only after helper/dependency/test parity;
- optional native quick-action chips and `attach next`/`analyze now` screenshot
  staging after task-level usability testing;
- stronger binary hardening as cost-raising defense, never as a claim that shipped
  client code is unrecoverable.

## Direct reuse, adaptation, and rejection register

| Material | Decision | Reason |
| --- | --- | --- |
| Parakeet exact Rust MMDevice/AEC and pinned dependency source | Selective direct reuse is eligible | Exact source with hashes/build evidence; isolate behind Bluey interfaces and retain per-file provenance |
| Littlebird exact source-map UI/store files | Selective reuse is eligible for a named UI slice | Exact first-party source; avoid preload/helper/backend and revalidate dependencies |
| SolveWatch LocalAgreement/VAD/benchmark | Adapt | Strong readable algorithm but different runtime, incomplete fixtures/tests, unpinned model download |
| Natively RAG, context, batching, and callback/worker code | Adapt after correcting defects | Useful architecture, but callback mutex/overflow/polling defects, non-atomic queue claims, Electron/N-API boundary, sparse tests, and race/content-log issues |
| Pluely permission/capture/audio code | Adapt | Useful contracts; broad capabilities, plaintext storage, Keychain dependency, and Tauri boundary are rejected |
| Cluely/LockedIn/Final Round minified or reconstructed bundles | Behavior/test reference | Maintainer cost and missing original types/tests; exact slices remain available for contract verification |
| Aura server/session/provider code | Reject | Weaker than Bluey and contains unauthenticated control, plaintext secrets, shared mutable state, and unbounded payloads |
| Any remote-input, proctor-evasion, generic preload, token-URL, plaintext fallback, content telemetry | Reject | Violates Bluey's security/privacy/maintenance boundary |

## Release gate

An implementation round is not complete when code compiles. It is complete when:

1. scoped tests and the relevant Rust/native/UI suites pass;
2. `cargo fmt --all -- --check`, warnings-denied Clippy, and `git diff --check` pass;
3. queue/IPC/privacy adversarial tests pass;
4. real Windows tests close named-pipe ACL and WASAPI/AEC questions where those paths
   changed;
5. p50/p95/p99 and bounded-memory evidence is recorded from fixed workloads;
6. no raw user content or secret appears in operational logs/artifacts;
7. exact imported files, dependency versions, and source hashes are recorded;
8. rollback is defined before deployment.

The current mainline must not be described as already containing the dirty alternate
worktree's IPC/Windows hardening. No deployment should be based on that assumption.

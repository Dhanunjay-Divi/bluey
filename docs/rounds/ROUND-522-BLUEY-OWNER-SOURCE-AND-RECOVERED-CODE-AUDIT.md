# Round 522 - Bluey owner-source and recovered-code audit

Date: 2026-07-13

Status: final evidence pass complete; implementation handoff ready; no product code,
deployment, package, or credential state changed

Audit baseline: `0f2933c4259e09a351a617bf94f0ed7a4b852f11`

Product-code baseline: `10c8214cf7ad20ea54711bb77f8f5cce55b06f45`; the
newer commit adds only Round 521 documentation.

Evidence index: [source-reference audit](../research/source-reference-audit/INDEX.md)

Full-system map:
[backend, frontend, native, cloud, and Jobs](../research/source-reference-audit/FULL-SYSTEM-MAP.md)

## Executive summary

The supplied `_refs` directory, the five recovered macOS products, the five recovered
Windows products, and current Bluey are now one evidence-indexed comparison corpus.
The final pass changes the implementation priority, not Bluey's core architecture.

Bluey's backend and product spine are already stronger: authenticated/rate-limited
account APIs, provider routing/health, atomic request idempotency, billing/usage,
deletion/sync, the Jobs tenant/runner/intervention/lease/receipt foundation, typed
overlay IPC, structured native workbench, and stale-answer generation protection.
Replacing those components with any supplied Express, FastAPI, Electron, or Tauri
backend would be a regression.

That does not make every Jobs feature production-complete. Its provider surfaces are
beta/uncertified, ranking is heuristic, resume extraction/tailoring is basic, Jobs
plan assignment is administrative, and Gmail/Outlook/calendar synchronization is not
active. The exact frontend/backend status is separated in the
[full-system map](../research/source-reference-audit/FULL-SYSTEM-MAP.md#complete-jobs-frontendbackend-feature-matrix).

The best ready material is narrower and more valuable:

- ParakeetAI's exact Rust MMDevice/AEC source;
- SolveWatch's LocalAgreement-2 transcription and VAD benchmark contract;
- Natively's callback/worker separation and batching ideas, durable embedding queue,
  vector worker, renderer coalescing, and long-horizon context ideas, after correcting
  its mutex, overflow, lease, and lifecycle defects;
- Littlebird's exact workspace/context UI and helper critical-state replay;
- Pluely's bounded event-driven Windows audio and permission/capture UX;
- Final Round's bounded lifecycle, rollback, and stream recovery;
- Cluely's helper/window lifecycle and atomic local state;
- LockedIn's Windows DPI/deadline details;
- Aura's four-item screenshot staging UX only.

Three P0 issues are present in current Bluey main: unauthenticated/unbounded daemon
IPC, unbounded real-time queues, and ordinary cloud sync unconditionally invoking a
content-bearing diagnostic/audit upload. Older documents describe some solutions that exist only as
uncommitted work in an older dirty worktree; they are not deployed or merged.

## Evidence-backed findings

### Bluey backend remains authoritative

- Public/auth/admin/worker boundaries, rate limits, body limits, account, billing,
  sync, STT, RAG, export, and deletion routes are visible at
  `server/src/api/mod.rs:50-183,185-364`.
- Atomic account/request reservation, cached completion, terminal failure, and
  transient release exist for SQLite and Postgres at
  `server/src/db/idempotency.rs:1-190`.
- Classification and the forced local-only policy are separated at
  `crates/cue-router/src/auto.rs:1-84`.
- ATS adapters, leases, and receipts are implemented at
  `jobs/automation/src/standard-adapters.ts:52`,
  `jobs/runner/src/execution-lease.ts:165`, and
  `jobs/automation/src/receipts.ts:100`.

No supplied desktop artifact or source repository contains a stronger complete
account/billing/Jobs backend. Opaque server absence is recorded as unknown rather than
proof that a remote service lacks a private feature.

### Bluey's overlay and generation model remain authoritative

Bluey already validates overlay capabilities, field sizes, and UI state
(`crates/cue-core/src/overlay_ipc.rs:11-223`) and rejects stale answer updates while
marking superseded cards (`crates/cue-daemon/src/app.rs:819-854,1027-1143`). The
native overlay already supports route state, Auto/Quick/Thorough, streaming recovery,
attachments, and coding/system-design workbench artifacts. References should supply
permission, provenance, staging, and replay refinements—not another renderer.

### The real P0s are local and measurable

1. The daemon defaults to loopback TCP but accepts a configurable address, reads an
   unbounded line, spawns a task per connection, and permits privileged commands and
   shutdown without a capability
   (`crates/cue-core/src/ipc.rs:9`;
   `crates/cue-daemon/src/app.rs:1549-1561,1950-1987`).
2. Microphone, system audio, Deepgram, OpenAI STT, local Whisper, overlay, and app
   bridges use unbounded channels. Exact locations are listed in the
   [backend audit](../research/source-reference-audit/BACKEND-AUDIO-AND-RECOVERY.md#bounded-real-time-queues).
3. Ordinary session sync unconditionally invokes audit-bundle synchronization. The
   bundle includes transcript text, local context paths/previews, questions, responses,
   artifacts, and raw UI stream events, including answer deltas/finals
   (`crates/cue-daemon/src/cloud/sync.rs:182-214,475-568,618-905`;
   `crates/cue-daemon/src/app.rs:856-920,991-1009,1638-1651`).

### Current Windows audio is not the preserved hardened version

Current main targets Windows 7, uses averaging downsampling, initializes a one-second
WASAPI buffer, and polls every five milliseconds
(`native/windows/cue-audio/main.c:3,165-185,261-270,295-307`). The dirty older
`/Users/uno/Downloads/cue-bluey-jobs` worktree contains event-driven Windows 10 audio,
a polyphase resampler, structured readiness/errors, bounded helper supervision, and
authenticated IPC experiments. It is not mergeable or deployed.

The next implementation must fresh-port reviewed pieces onto current main, then add
the exact Parakeet AEC/MMDevice layer only behind benchmark and fallback gates.

## Cross-application feature matrix

| Product | Highest-value evidence | Bluey comparison | Locked disposition |
| --- | --- | --- | --- |
| Cluely | Helper/window lifecycle, modes/calendar, local VAD, atomic state | Partial for helper robustness; stronger overlay boundary | Adapt contracts; reject generic IPC/bundles |
| Littlebird | 974 credible first-party files, workspace/context UI, crash replay | Partial for workspace cohesion/replay; stronger backend | Selective exact UI reuse or adapt |
| LockedIn | Presets, restoration, screenshot/document context, DPI/deadlines | Bluey stronger overall | Adapt DPI/deadline tests; reject remote input |
| ParakeetAI | Exact Rust MMDevice/activity/AEC and pinned Sonora | Bluey missing AEC/endpoint parity | Selective direct reuse eligible with provenance |
| Final Round | Bounded VAD, lifecycle rollback, sender validation, structured panels | Equivalent panels; partial lifecycle/backpressure | Adapt lifecycle/tests; reject addon/updater transplant |
| SolveWatch | LocalAgreement-2, adaptive finalization, VAD benchmark, OCR worker | Partial local STT stability | Port algorithms; reject global state/content telemetry |
| Aura | Four-item screenshot staging, provider/preflight visibility | Bluey stronger except staging presentation | Adapt UI only; reject transport/backend |
| OpenCluely | Compact window/capture interaction and prompt modes | Bluey stronger | Interaction reference only |
| Vysper | OCR lifecycle and expanded prompt taxonomy | Partial OCR/window selection | Adapt bounded OCR later; reject preload bridge |
| Pluely | Event-driven bounded Windows audio, area selection, permission states | Partial capture/preflight | Adapt contracts; reject plaintext storage/Keychain/broad capabilities |
| Natively | Audio callback/worker separation, UI coalescing, RAG queue/worker, context compaction | Partial audio/RAG/compaction; Bluey stronger generation/IPC | Correct defects and port focused contracts; reject Electron boundary/content logs |

Exact recovered counts, source classifications, and hard boundaries are anchored by
`/Users/uno/Downloads/dmg_backtrack_code/reformed/INDEX.md:20-89`,
`/Users/uno/Downloads/dmg_backtrack_code/reformed/SECOND-PASS-RECOVERY.md:47-175`, and
`/Users/uno/Downloads/exe_backtrack_code/recovered/INDEX.md:1-35`.

## Recommended P0 changes

### P0.1 Owner-authenticated local transport

Use owner-only Unix sockets on macOS/Linux and a current-user local named pipe on
Windows. Add a per-boot capability, constant-time comparison, exhaustive request
classification, replay cache, maximum request/response frames, read deadline,
connection cap, and atomic private capability publication. No Keychain integration.

Fresh-port reference:
`/Users/uno/Downloads/cue-bluey-jobs/crates/cue-core/src/ipc_auth.rs`; implementation
and adversarial tests are specified in the
[handoff](../research/source-reference-audit/BLUEY-HANDOFF.md#p01-authenticated-bounded-daemon-ipc).

### P0.2 Bounded real-time lanes and supervised helpers

Separate lossy audio/partial data from lossless control/final/error events. Size audio
queues in milliseconds, use nonblocking oldest-drop behavior, expose content-free
lag/drop counters, minimize helper environments, require a ready handshake, bound
stderr, force abort after stop timeout, and replay only verified critical state.

### P0.3 Content-minimized audit

Decouple ordinary session sync from diagnostic upload. Operational diagnostics are
metadata-only by default and retain IDs, sequence, timing, sizes, provider, terminal
state, failure category, and optional scoped digest. Any content-bearing diagnostics
require separate explicit opt-in and retention controls. Final answer text remains
only in the user-facing conversation record under its deletion/sync policy. Redact
local paths and do not persist each token delta.

## Recommended P1 changes

1. Port LocalAgreement-2/stable partials and build a fixed STT/VAD corpus reporting
   WER, endpoint errors, stable-prefix delay, p50/p95/p99, CPU, and RSS.
2. Fresh-port event-driven Windows 10 audio/resampling; add Parakeet AEC/MMDevice and
   a corrected Natively-style callback/worker boundary behind measured
   optional/fallback gates.
3. Add revisioned durable conversation compaction with stale-result compare-and-swap.
4. Add a lease/idempotency/retry embedding queue and benchmark-gated vector index,
   preserving account/workspace/session and deletion scope.
5. Extend streams with monotonic sequence, snapshot/replay markers, and one terminal
   event; current generation fencing remains authoritative.
6. Mark resume, job, transcript, OCR, page, and document material as typed untrusted
   evidence and add prompt-injection fixtures.
7. Add a native Windows region/multi-monitor selector plus verified reconnect/replay
   status. Keep quick actions, screenshot staging, and expanded permission UX as P2
   unless usability evidence moves them forward.
8. Coalesce display-only answer deltas into 16--33 ms frames with a size threshold
   and mandatory final/error/supersede flush; preserve exact generation fencing and
   persisted final text.
9. Publish active meeting and daemon state atomically or transactionally; current
   direct overwrites at `crates/cue-daemon/src/storage.rs:245` and
   `crates/cue-daemon/src/app.rs:16555` are crash-sensitive.

## Recommended P2 changes

- Native bounded window/area capture and OCR after mixed-DPI/privacy canaries.
- Optional bounded native quick actions and `attach next`/`analyze now` screenshot
  staging after usability tests.
- Terminal-visible meeting hints after real platform validation.
- One email/calendar outcome connector only after scope, cursor, deletion, retention,
  and content-free telemetry guarantees.
- Windows ARM64 only after native dependency/test parity.
- Binary hardening as defense in depth; never claim metadata can prevent analysis of
  code shipped to a user's machine.

## Reuse and provenance notes

The user asserted ownership and authorized reuse of every supplied source repository,
DMG, EXE, and recovered tree. The six `_refs` trees contain 1,464 files and
336,221,399 exact file bytes; tree digests and prior recovery anchors are in
[hashes.txt](../research/source-reference-audit/hashes.txt).

Direct reuse is eligible only for an isolated named slice with its source hash,
dependency inventory, attribution/distribution decision, tests, and Bluey ownership
boundary recorded. The strongest eligible direct source is Parakeet's exact Rust
native layer and selected Littlebird exact source-map UI/store files. SolveWatch,
Natively, Pluely, and recovered/minified products are better adapted into Bluey-owned
Rust/native contracts.

On-disk license metadata conflicts are provenance facts, not a challenge to the
owner's authorization. They are recorded in the
[INDEX](../research/source-reference-audit/INDEX.md#authorization-and-provenance).

## Security and privacy findings

Retain or add:

- owner/session peer verification, per-boot capability, replay and size bounds;
- narrow typed capability surfaces and explicit terminal states;
- content-free helper/queue/provider diagnostics;
- user-visible capture consent and evidence provenance;
- ordinary session sync separated from explicit content-bearing diagnostics;
- server-authoritative secrets and local-only policy enforcement;
- deletion covering primary, derived, queued, indexed, cached, and audit records.

Reject:

- Keychain/keytar/`tauri-plugin-keychain` additions;
- generic renderer IPC/preload receive, broad HTTP/plugin capabilities, or renderer
  credential mutation;
- unauthenticated local HTTP/WebSocket sessions or fixed temp command files;
- plaintext secrets, URL tokens, automatic API-key rotation, or unverified downloads;
- raw prompt/transcript/answer/OCR/screenshot/credential telemetry;
- arbitrary remote mouse/keyboard control and proctor-evasion behavior.

## Unknowns requiring runtime validation

Static/source evidence cannot settle:

- real speaker/headset/Bluetooth AEC quality and latency;
- endpoint loss/change, sleep/resume, exclusive-mode, and long-session capture;
- Windows named-pipe ACL, peer/session, reparse, and saturation behavior;
- p50/p95/p99 STT-to-stable-prefix and audio-to-answer latency;
- indexed-RAG recall/latency/disk/RSS at 10k through 1M chunks;
- overlay restart replay, accessibility, permission completion, and mistaken
  screenshot-send rates;
- updater publisher and rollback behavior on real Windows;
- opaque third-party server authorization, retention, deletion, or billing behavior.

Runtime inspection can validate behavior. It cannot recreate private server source,
deleted names/comments/types/tests, Git history, or native source never packaged.

## Concrete next-agent handoff

The next agent should implement only P0 on current main first:

1. Record/freeze the exact starting tree and preserve unrelated concurrent changes.
2. Fresh-port authenticated IPC as a small reviewable slice; migrate daemon, CLI, and
   dashboard together and pass the adversarial matrix.
3. Convert real-time channels by lane with explicit capacity/drop policy and helper
   supervision; add overload and lifecycle tests.
4. Decouple diagnostic upload from normal sync, default it to metadata-only, remove
   raw answer/path/transcript fields, and prove content canaries do not appear in
   local/synced diagnostics without explicit test opt-in.
5. Run full relevant Rust/native/UI tests, formatting, warnings-denied Clippy,
   `git diff --check`, and real Windows canaries for platform code.
6. Only then take P1 in separate benchmarked slices: stable STT, Windows/AEC, context
   compaction, indexed RAG, stream replay, evidence trust, and UI refinements.

Do not merge `/Users/uno/Downloads/cue-bluey-jobs`, copy an entire reference
application, claim dirty-worktree changes are deployed, or weaken bounds to make a
test pass. The file-level implementation and acceptance checklist is in
[BLUEY-HANDOFF.md](../research/source-reference-audit/BLUEY-HANDOFF.md).

## Verification for this audit round

Completed on 2026-07-13:

- `git diff --check` passed; each untracked document also passed
  `git diff --no-index --check /dev/null <file>`.
- All relative Markdown links and anchors across the six audit Markdown files
  resolved.
- 118 explicit repository/absolute citation occurrences were checked for path
  existence and cited line bounds.
- All six `_refs` tree digests/counts/byte totals and all seven prior recovery-file
  SHA-256 values reproduced exactly from [hashes.txt](../research/source-reference-audit/hashes.txt).
- `git status --short` shows only the new `docs/research/` audit and this Round 522
  document. No tracked Bluey product file was changed, staged, committed, pushed, or
  deployed.

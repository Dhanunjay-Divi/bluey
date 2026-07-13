# Bluey source-reference and recovered-artifact audit

Date: 2026-07-13

Status: evidence corpus inventoried; architecture and implementation handoff locked; no Bluey product code changed

Bluey audit baseline: `0f2933c4259e09a351a617bf94f0ed7a4b852f11`

Product-code baseline: `10c8214cf7ad20ea54711bb77f8f5cce55b06f45`; the one
newer commit adds only the Round 521 reconciliation document.

## Outcome

The final pass was worthwhile, but it does not justify replacing Bluey's backend or
native overlay. Bluey already has the stronger account, billing, routing, provider
health, idempotency, Jobs, answer-generation fencing, typed overlay protocol, and
session-ownership foundation. The best owner-supplied material narrows to six
production improvements:

1. bounded real-time audio, STT, overlay, and helper queues;
2. authenticated and size-bounded local daemon IPC;
3. separate ordinary session sync from content-bearing diagnostic/audit upload;
4. LocalAgreement-style stable local transcription and a real VAD/STT benchmark;
5. durable conversation compaction and indexed local RAG;
6. a measured Windows audio path combining exact Parakeet AEC/MMDevice evidence,
   Natively's callback/worker separation after correcting its defects, and the
   already-preserved Bluey Windows work.

The source and recovered artifacts are an implementation evidence base. They are not
one coherent codebase and must not be merged wholesale.

## Authorization and provenance

The user stated on 2026-07-13 that every supplied source repository, DMG, EXE, and
recovered tree is owned by the user and authorized for reuse in Bluey and the source
products. This audit records that owner attestation without turning inconsistent
on-disk metadata into a blocker.

On-disk provenance still matters:

- OpenCluely and Vysper carry Apache-2.0 `LICENSE` files while their package manifests
  say ISC (`_refs/OpenCluely-main/package.json:31` and
  `_refs/Vysper-main/package.json:23`).
- SolveWatch carries an MIT `LICENSE` while its manifest says ISC
  (`_refs/solveWatchAi-main/package.json:33`).
- Natively carries AGPL-3.0 text while its manifest says ISC
  (`_refs/natively-cluely-ai-assistant-main/package.json:143`).
- Pluely consistently declares GPL-3.0
  (`_refs/pluely-master/package.json:11` and
  `_refs/pluely-master/src-tauri/Cargo.toml:6`).
- Aura contains no local license file.

Owner authorization permits the requested work. A direct import must still retain a
per-file source locator, dependency review, and a decision about attribution and
distribution so future maintainers know where the code came from.

## Corpus register

The six `_refs` trees contain 1,464 files and 336,221,399 exact file bytes. Tree
digests cover every regular file and its relative path; the exact formula and anchors
are in [hashes.txt](hashes.txt).

| Source tree | Architecture | Files / exact bytes | Tree SHA-256 | Canonical audit scope |
| --- | --- | ---: | --- | --- |
| Aura AI | Python, FastAPI, WebSocket, pywebview, Deepgram, browser JS | 61 / 44,097,523 | `777d3ba3...1ff5` | `api/`, `core/`, `services/`, `web/`, `main.py` |
| OpenCluely | Electron 29, plain renderer JS, Gemini/Azure integrations | 40 / 1,495,748 | `20243857...55b` | Main, preload, renderer, prompts; compared as a Vysper relative |
| Vysper | Electron 29, plain renderer JS, OCR and prompt packs | 44 / 1,595,954 | `61c95d2d...a49` | Main, preload, renderer, OCR, prompts; shared code deduplicated |
| Natively | Electron 33, React, TypeScript, Rust N-API, SQLite/sqlite-vec | 856 / 269,037,662 | `f0557d9d...e76` | Canonical `electron/`, `src/`, `native-module/src/`; not `temp*`, `.orig`, agents, or bundled installer |
| Pluely | Tauri 2, Rust, React 19, SQLite, native permissions | 224 / 8,081,647 | `67ae2396...5ad` | `src/`, `src-tauri/src/`, capabilities and configuration |
| SolveWatch | Node/Express/Socket.IO, Electron HUD, Python STT/VAD | 239 / 11,912,865 | `64d1c4e3...3c8` | `src/`, `electron/`, `transcriber/`, benchmark contracts |

The prior artifact corpus remains separately anchored:

- DMG byte recovery: `/Users/uno/Downloads/dmg_backtrack_code/recovered/INDEX.md:9-47`.
- DMG second-pass reconstruction:
  `/Users/uno/Downloads/dmg_backtrack_code/reformed/INDEX.md:20-89` and
  `/Users/uno/Downloads/dmg_backtrack_code/reformed/SECOND-PASS-RECOVERY.md:47-175`.
- Windows recovery:
  `/Users/uno/Downloads/exe_backtrack_code/recovered/INDEX.md:1-35`.
- Windows clean-room contracts:
  `/Users/uno/Downloads/exe_backtrack_code/reformed-windows/INDEX.md:1-30`.

## Evidence classification and deduplication

Every conclusion uses one of these labels:

- **Observed source**: readable source text in `_refs`.
- **Exact recovered source**: source text packaged in an artifact or recovered from a
  verified source map.
- **Exact packaged slice**: production bundle bytes with verified offsets/hashes, but
  not necessarily original authoring source.
- **Mechanical reconstruction**: formatting, module splitting, demangling, or
  decompilation that preserves useful behavior without claiming lost source.
- **Inference**: a behavior or boundary supported by call sites, validators, strings,
  or consumers but not a visible implementation body.

Natively's `temp/`, `temp_code_expansion/`, `.orig` files, bundled Python installer,
agent material, generated output, and copied dependencies are not independent product
implementations. OpenCluely and Vysper share substantial ancestry and are not counted
twice merely because the same pattern appears in both. DMG and EXE variants are also
treated as versions of one product unless a platform-specific implementation differs.

## Cross-product disposition

| Product/evidence | Best material | Current Bluey status | Decision |
| --- | --- | --- | --- |
| Cluely DMG/EXE | Overlay/audio-helper lifecycle, local VAD orchestration, modes, calendar context, atomic state | Equivalent/Partial | Adapt lifecycle invariants; reject generic IPC, global Origin rewriting, and token extraction |
| Littlebird DMG/EXE | Exact workspace/context UI, helper supervision, crash replay, typed renderer contracts | Partial | Adapt workspace and critical-state replay; do not replace Bluey's native overlay or backend |
| LockedIn DMG/EXE | DPI handling, presets, screenshot/document context, session restoration | Equivalent | Adapt DPI tests only; reject broad remote input and generic preload bridges |
| ParakeetAI DMG/EXE | Exact Rust MMDevice/activity/AEC source and meeting detection | Partial | Highest-confidence native input; benchmark and port behind Bluey-owned Rust/C contracts |
| Final Round DMG/EXE | Bounded VAD, narrow sender-validated IPC, lifecycle rollback, update gate, structured coding panels | Equivalent/Partial | Adapt bounded lifecycle/update tests; Bluey already has structured artifacts and recovery UI |
| SolveWatch source | LocalAgreement-2, adaptive endpointing, VAD benchmark design, worker OCR | Partial | Reform algorithms with deterministic fixtures; reject global state, content telemetry, unauthenticated Socket.IO |
| Aura source | Four-item screenshot staging and explicit provider/preflight UI | Bluey stronger/Partial | Adapt only bounded staging and status presentation; reject its local API, session, secret, and audio mixing model |
| OpenCluely source | Compact overlay/window controls and capture interaction | Equivalent | Use as interaction reference, not a replacement stack |
| Vysper source | OCR/prompt extensions over the OpenCluely family | Equivalent/Partial | Adapt bounded OCR worker contracts only if they beat Bluey's existing capture path |
| Pluely source | Event-driven bounded Windows audio, multi-monitor selection, permission/status UX | Partial | Adapt audio overflow and capture/preflight contracts; reject plaintext "secure" JSON, Keychain, and broad plugin authority |
| Natively source | Callback/worker separation and batching ideas, temporal context, compaction, SQLite vector worker/queue | Partial | Adapt the design after correcting its callback mutex, ignored overflow, polling, queue-claim, and lifecycle defects; reject its Electron boundary |

Recovered counts and hard boundaries are evidenced at
`/Users/uno/Downloads/dmg_backtrack_code/reformed/INDEX.md:22-28` and
`/Users/uno/Downloads/exe_backtrack_code/recovered/INDEX.md:11-30`. Private server
bodies, deleted pre-minification information, missing native source, tests never
packaged, and Git history remain unavailable; runtime inspection cannot recreate
bytes that were never shipped
(`/Users/uno/Downloads/dmg_backtrack_code/reformed/SECOND-PASS-RECOVERY.md:159-175`).

## Current Bluey comparison

| Capability | Status | Current evidence and result |
| --- | --- | --- |
| Accounts, billing, quotas, deletion, sync, worker auth | Bluey stronger | `server/src/api/mod.rs:50-183,185-364` |
| Provider routing, deadlines, local-only policy, health | Bluey stronger | `crates/cue-router/src/auto.rs:1-84`; `server/src/provider_health.rs:1-166` |
| Request idempotency and duplicate charging protection | Bluey stronger | `server/src/db/idempotency.rs:1-190` |
| Jobs product foundation: onboarding, local/cloud runners, isolation, interventions, leases, receipts | Bluey stronger in source | Current `jobs/` and `server/src/api/jobs.rs`; no comparable complete pipeline was observed in the supplied desktop products |
| Jobs ranking, resume intelligence, provider certification, plan activation, email/calendar runtime | Partial | Exact code gaps and beta/deployment gates are in [the full-system map](FULL-SYSTEM-MAP.md#complete-jobs-frontendbackend-feature-matrix) |
| Answer generation fencing and stale persistence rejection | Bluey stronger | `crates/cue-daemon/src/app.rs:819-854,1027-1143,8790-8844` |
| Native overlay protocol and input validation | Bluey stronger | `crates/cue-core/src/overlay_ipc.rs:11-223`; `crates/cue-daemon/tests/overlay_security_integration.rs:37-205` |
| Overlay streaming, route/status controls, structured answers, recovery | Equivalent/Bluey stronger | macOS overlay `main.swift:2350-2717,3526-3947,5213-5947,8465-8490`; Windows overlay `main.c:81-237,612-617,1877-1909` |
| Local daemon IPC authentication and request bounds | Missing | Loopback TCP default at `crates/cue-core/src/ipc.rs:9`; configurable bind and unbounded `read_line` at `crates/cue-daemon/src/app.rs:1549-1561,1950-1987` |
| Real-time queue backpressure | Missing | Unbounded channels in audio, STT, Whisper, overlay, and app paths; see [backend audit](BACKEND-AUDIO-AND-RECOVERY.md#p0-gaps-in-current-bluey) |
| Stable local partial transcription | Partial | Typed partial/final events exist, but no LocalAgreement-equivalent stabilizer was found in `crates/cue-daemon/src/stt/whisper/` |
| Long-horizon conversation compaction | Partial | Conversation drops turns beyond 80 at `crates/cue-core/src/meeting.rs:476-501`; a `compressed_summary` field is read but no updater was found |
| Local vector retrieval at large scale | Partial | Bounded top-k but O(N), explicitly documented at `crates/cue-rag/src/store.rs:162-171` |
| Windows event-driven capture/resampling/AEC | Partial | Current main polls WASAPI and averages samples; exact improvements exist in recovered Parakeet evidence and the dirty alternate Bluey worktree, but not in current main |
| Content-minimized diagnostic synchronization | Missing | Ordinary sync unconditionally invokes diagnostic bundle upload at `crates/cue-daemon/src/cloud/sync.rs:182-214`; the bundle duplicates transcript, paths/previews, questions, responses/artifacts, and raw UI events at `sync.rs:618-905` |

## Current-main versus preserved experimental work

The canonical `/Users/uno/Downloads/cue` checkout does not contain the authenticated
IPC and Windows capture hardening described by older audit documents. The relevant
implementation remains uncommitted in the very dirty, older
`/Users/uno/Downloads/cue-bluey-jobs` worktree. For example:

- `/Users/uno/Downloads/cue-bluey-jobs/crates/cue-core/src/ipc_auth.rs:21-30,110-220,223-300`
  defines
  bounded authenticated envelopes and private capability publication.
- `/Users/uno/Downloads/cue-bluey-jobs/native/windows/cue-audio/main.c:203-246,317-371`
  adds structured readiness,
  recoverable errors, event-driven WASAPI, and a shorter buffer.
- `/Users/uno/Downloads/cue-bluey-jobs/native/windows/cue-audio/resampler.c` and its
  tests contain the preserved
  polyphase-resampler work.

Those files are implementation references, not a mergeable branch. They overlap newer
mainline code and must be fresh-ported hunk by hunk with their tests and real-Windows
canaries.

## Locked decisions

- Preserve Bluey's server, router, Jobs, typed overlay, account ownership, and
  answer-generation model.
- Do not add Keychain/keytar/`tauri-plugin-keychain`; owner-only runtime files,
  OS ACLs, short-lived capabilities, and server-authoritative credentials are enough
  for the recommended local IPC work.
- Do not copy whole Electron/Tauri applications, preloads, provider clients, or
  backend servers into Bluey.
- Do not retain raw prompt, answer, transcript, screenshot, OCR, token, or credential
  content in operational telemetry.
- Do not add arbitrary remote mouse/keyboard input, proctor-evasion behavior,
  unauthenticated local HTTP/WebSocket control, automatic API-key rotation, or
  unverified model downloads.
- Prefer direct source reuse only where the implementation is isolated, testable,
  and provenance-addressed; otherwise port the behavior into Bluey's Rust/native
  architecture.

## Audit documents

- [Backend, audio, recovery, and data boundaries](BACKEND-AUDIO-AND-RECOVERY.md)
- [Overlay, UI, and interaction comparison](OVERLAY-UI-AND-INTERACTION.md)
- [Complete backend, frontend, native, cloud, and Jobs map](FULL-SYSTEM-MAP.md)
- [Concrete Bluey implementation handoff](BLUEY-HANDOFF.md)
- [Integrity anchors](hashes.txt)
- [Round 522 decision record](../../rounds/ROUND-522-BLUEY-OWNER-SOURCE-AND-RECOVERED-CODE-AUDIT.md)

## Remaining runtime unknowns

This pass did not launch the source applications because source inspection answered
the architecture questions and product implementation was explicitly deferred. It
also did not execute recovered binaries. Runtime work remains useful only for measured
questions: real device AEC quality, endpoint changes, p50/p95/p99 latency, long-session
memory/queue behavior, Windows ACL and named-pipe behavior, updater publisher checks,
and task-level overlay usability. Those tests cannot recover missing source and must
not be used to weaken the locked security boundaries.

# Full-system Bluey map: current code, recovered code, and implementation target

Date: 2026-07-13

Status: complete backend/frontend/native/Jobs evidence map; product implementation
has not started in this audit round

Audit baseline: `0f2933c4259e09a351a617bf94f0ed7a4b852f11`

Product-code baseline: `10c8214cf7ad20ea54711bb77f8f5cce55b06f45`

## What “all code” means here

This map includes every unique implementation surface supplied in this task:

- current Bluey terminal CLI, daemon, native macOS and Windows overlays/helpers,
  optional dashboard, cloud client, managed server, storage, billing, sync, updater,
  observability, release scripts, and tests;
- the complete Bluey Jobs portal, API, database model, automation library, local
  browser, cloud runner, Temporal workflows, discovery worker, receipts, and tests;
- all five macOS recovered/reformed products and all five Windows
  recovered/reformed products;
- all six owner-supplied `_refs` source trees;
- useful but unmerged Bluey experiments in
  `/Users/uno/Downloads/cue-bluey-jobs`, treated only as a dirty fresh-port reference.

Generated output, dependency mirrors, duplicate forks, repeated macOS/Windows
versions, Natively `temp*` expansions, and minified copies of already-recovered exact
source are deduplicated. No unique product behavior is excluded. “Included” means
indexed, compared, and routed to an implementation decision; it does not mean every
file should be copied into Bluey.

## End-to-end product spine

```text
terminal CLI ------------------------------ native overlay (macOS / Windows)
     |                                                ^
     | typed local command                            | typed card/status stream
     v                                                |
cue-daemon: session + generation authority + recovery + local persistence
     |                 |                    |
     |                 |                    +-- context / screen / documents / RAG
     |                 +-- provider router ---- local or managed completion/STT
     +-- audio helpers -> VAD -> STT -> question -> answer stream
                              |
                              +-- optional account sync / deletion / diagnostics
                                                   |
managed server: auth + devices + routing + usage + billing + sync + account control
                                                   |
Bluey Jobs: React portal -> Jobs API -> discovery / local browser / cloud Temporal
                                      -> interventions -> immutable receipt/evidence
```

Bluey keeps one authority per concern. The daemon owns live assistant state; the
managed server owns account, billing, provider, and cloud data; Jobs owns application
automation state. None of the supplied Electron, Tauri, Express, FastAPI, or Python
applications should become a parallel product spine.

## Assistant frontend and backend map

| Layer | Current Bluey evidence | Strongest supplied evidence | Status | Locked action |
| --- | --- | --- | --- | --- |
| Terminal lifecycle and commands | `crates/cue-cli/src/app.rs:80-195` defines on/off, login, account, settings, usage, update, export/delete, status, overlay, meeting, listen, ask, recap, actions, and context commands | Littlebird onboarding/workspace recovery; Final Round lifecycle | Bluey stronger | Keep terminal-first control; improve state/error copy only with measured usability evidence |
| Native overlay/frontend | Typed capability and field limits at `crates/cue-core/src/overlay_ipc.rs:11-223`; generation/recovery at `crates/cue-daemon/src/app.rs:819-854,1027-1143`; macOS workbench at `native/macos/cue-overlay/Sources/cue-overlay/main.swift:2848-2875,12820-12855`; Windows recovery at `native/windows/cue-overlay/main.c:1877-1909` | Littlebird hierarchy; Final Round panels; Pluely selection/quick actions; Aura staging | Bluey stronger | Retain native UI; add only stream coalescing/replay, Windows region selection, and later bounded quick actions/staging |
| Optional dashboard | Tauri commands currently use the same daemon and cloud authorities at `crates/cue-dashboard/src/commands.rs:62-150,175-260` | No reference has a safer privileged renderer boundary | Partial | Do not make it a second authority; migrate it with authenticated local IPC if retained |
| Local daemon IPC | Loopback default at `crates/cue-core/src/ipc.rs:9`; configurable listener and unbounded per-connection line at `crates/cue-daemon/src/app.rs:1549-1561,1950-1987` | Older Bluey `ipc_auth.rs`; Final Round sender validation | Missing, P0 | Owner-only Unix socket/current-user Windows named pipe, bounded frames/deadlines/concurrency, per-launch capability, replay protection; no Keychain |
| Session and answer authority | Generation checks, supersession, and stale-persistence rejection at `crates/cue-daemon/src/app.rs:819-854,1027-1143,8790-8844` | Natively generation IDs; Final Round state machine | Bluey stronger | Preserve exactly; all new queues, replay, and coalescing carry generation/session IDs |
| Microphone and system capture | Unbounded microphone and helper lanes at `crates/cue-daemon/src/audio/capture.rs:84-125` and `audio/system_capture.rs:56-79,177-189`; current Windows poll/resample path at `native/windows/cue-audio/main.c:165-185,261-307` | Parakeet exact MMDevice/AEC; Natively ring/worker design; Pluely event-driven overflow; dirty Bluey resampler | Partial, P0/P1 | First bound lanes and remove callback allocation/locking; then benchmark fresh-ported event-driven Windows audio and optional AEC |
| VAD and transcription | Typed partial/final contract at `crates/cue-core/src/stt.rs:28-185`; unbounded Deepgram/OpenAI/Whisper lanes at `crates/cue-daemon/src/stt/deepgram.rs:354-410`, `stt/openai.rs:182-229`, and `stt/whisper/mod.rs:28-53` | SolveWatch LocalAgreement-2 and VAD benchmark; Final Round bounded VAD | Partial | Bound transport in P0; port stable partials/adaptive endpointing in P1 behind a fixed WER/latency corpus |
| Answer routing and providers | Local policy/classification at `crates/cue-router/src/auto.rs:1-84`; managed completion/embed/transcribe routes at `server/src/api/mod.rs:232-280`; provider health in `server/src/provider_health.rs:1-166` | No supplied backend is stronger; recovered streaming/reconnect logic is narrower | Bluey stronger | Keep router/server; add sequence/snapshot/terminal stream semantics without duplicating provider clients |
| Streaming presentation | Every daemon delta is sent immediately at `crates/cue-daemon/src/app.rs:856-878`; macOS relayouts each update at `native/macos/cue-overlay/Sources/cue-overlay/main.swift:11165` | Natively `requestAnimationFrame` batching and final flush at `_refs/natively-cluely-ai-assistant-main/src/components/NativelyInterface.tsx:884`; SolveWatch parse-on-final HUD | Partial | Coalesce display deltas at 16--33 ms/size threshold and always flush final/error/supersede for the correct generation |
| Context, screenshots, and evidence | Prompt/context bounds at `crates/cue-daemon/src/app.rs:12079-12155,12593-12648,12876-12889`; current Windows capture is primary-screen PowerShell at `crates/cue-cli/src/app.rs:2482-2509` | Littlebird workspace; Pluely multi-monitor region selector; Aura four-item staging; Vysper bounded OCR idea | Bluey stronger overall; Windows partial | Preserve typed untrusted evidence and byte/pixel limits; add a native Windows selector; staging/OCR remain measured P2 work |
| Conversation memory | History is capped and oldest turns are drained at `crates/cue-core/src/meeting.rs:476-501`; `compressed_summary` is read at `crates/cue-daemon/src/db/mod.rs:173-203` but no writer was found | SolveWatch serial merge/restore; Natively epoch summaries | Partial | Add durable revisioned compaction with compare-and-swap and deletion of raw plus derived state |
| Local RAG | Scoped/dimension-checked bounded retrieval at `crates/cue-rag/src/store.rs:125-171`; only a 10k scale test at `crates/cue-rag/tests/rag_scaling.rs:20-64`; indexing is in-memory/fire-and-forget at `crates/cue-daemon/src/rag_indexer.rs:178-248` | Natively sqlite-vec worker and persistent queue | Partial | Add transactional lease/idempotency queue first; choose sqlite-vec/usearch only after 10k--1M recall/latency/RSS tests |
| Local persistence and crash recovery | Native Continue/Retry and partial recovery at `crates/cue-daemon/src/app.rs:8567`; active meeting/state files overwrite directly at `crates/cue-daemon/src/storage.rs:245` and `crates/cue-daemon/src/app.rs:16555` | Littlebird critical-state replay; Cluely atomic state; Natively placeholder race as a reject | Partial | Make state publication atomic or transactional; never copy Natively's processing-before-placeholder ordering |
| Cloud sync and deletion | Session/artifact sync, list/get/delete, RAG, export, and account delete routes at `server/src/api/mod.rs:281-318`; account export includes Jobs at `server/src/db/account_data.rs:24-38,74-82`; deletion removes scoped objects and cascades rows at `server/src/api/account.rs:802-924` | No supplied source has a stronger complete ownership/deletion model | Bluey stronger, with one P0 privacy defect | Keep ownership/deletion; decouple normal sync from diagnostic upload and default diagnostics to metadata only |
| Diagnostic/privacy boundary | Normal sync invokes audit upload at `crates/cue-daemon/src/cloud/sync.rs:182-214`; bundles include transcript, path/text previews, questions, answers/artifacts, and UI events at `sync.rs:618-905` | Supplied apps mostly demonstrate patterns to reject: raw content logs, token URLs, plaintext secrets | Missing, P0 | Separate explicit content-diagnostic consent from ordinary sync; redact paths; do not persist token deltas; add content canary tests |
| Authentication and devices | Rate-limited signup/login/refresh/device flows at `server/src/api/mod.rs:57-135`; account/device controls at `server/src/api/mod.rs:203-225`; CLI device login at `crates/cue-cli/src/app.rs:1491-1755` | Final Round PKCE is a useful protocol check, not a fuller account system | Bluey stronger | Keep server-authoritative identity; default desktop storage remains the private account profile; add no Keychain dependency |
| Billing, plans, usage, and overage | Pricing, checkout/portal, usage, account billing, Stripe/Square hooks at `server/src/api/mod.rs:130-135,221-225,299-314`; auto-reload bounds at `server/src/api/account.rs:305-421`; atomic request charging at `server/src/db/idempotency.rs:1-190` | None of the supplied desktop backends exposes equivalent source | Bluey stronger | Preserve; verify live processor/webhook/capacity canaries before deployment claims |
| Updater and release trust | CLI fetches with timeout, verifies an embedded-key manifest signature, pins artifact/installer SHA-256, and blocks unverified production installs at `crates/cue-cli/src/update.rs:216-340,405-459` | Final Round update gate/rollback; Cluely helper lifecycle | Bluey stronger/equivalent | Keep terminal updater. Signing/notarization is not a requirement for the user's terminal-only app scope; manifest/hash trust and rollback still are |
| Observability and support | CLI exposes redacted doctor/support at `crates/cue-cli/src/app.rs:127-157`; redaction patterns/tests at `crates/cue-cli/src/logs.rs:7-94,205-350` | Littlebird/Final Round failure replay; many references' content logging is rejected | Partial | Keep content-free health/drop/retry metrics; fix audit-content duplication before calling privacy complete |

## Complete Jobs frontend/backend feature matrix

Bluey Jobs is real code, but release endpoints are dark unless
`BLUEY_JOBS_BETA_ENABLED=1` (`server/src/api/jobs.rs:229-245`), and the repository
explicitly records remaining external production gates
(`jobs/ARCHITECTURE.md:78-96`). “Implemented” below therefore does not mean deployed
or live-provider certified.

| Required product capability | Bluey code evidence | Status versus corpus | Completion decision |
| --- | --- | --- | --- |
| Onboarding and authentication | Six-step profile/resume/goals/defaults flow at `jobs/portal/src/components/Onboarding.tsx:36-126,157-168`; shared Bluey bearer identity at `jobs/portal/src/App.tsx:54-83`; server auth/device routes at `server/src/api/mod.rs:57-135` | Bluey stronger | Keep; add production task analytics only if content-free and consent-compatible |
| Local and cloud browser execution | One API binds the packet/identity/profile and chooses a scoped local ticket or cloud workflow at `server/src/api/jobs.rs:799-1092`; local Electron uses sandbox/context isolation at `jobs/browser/src/main.ts:100-126`; cloud Temporal flow is at `jobs/workflows/src/workflows.ts:27-119` | Bluey stronger in source; deployment unverified | Certify real providers, production browser takeover, object storage, and cloud infrastructure before GA |
| Browser-profile and multi-account isolation | Per-account/per-application-identity directories at `jobs/browser/src/profile.ts:4-33`; encrypted scoped cloud envelopes at `jobs/runner/src/crypto-envelope.ts:17-88`; every Jobs API operation derives the authenticated account at `server/src/api/jobs.rs:247-277` | Bluey stronger | Retain account + verified identity scope; test account switching, aliases, cleanup, and cross-tenant denial |
| Job discovery and ranking | Scheduled discovery loop at `jobs/workflows/src/discovery.ts:277-716`; server lists matches in score order and computes profile/preference score at `server/src/db/jobs.rs:1712-1726,1804-1824,3869-3918`; portal filters/ranks at `jobs/portal/src/views/MatchesView.tsx:55-171` | Partial | Greenhouse/Lever scheduled sources are implemented; certify remaining public ATS/licensed sources and replace heuristic score claims only after a labeled ranking eval |
| Resume import, tailoring, visible diffs, and export | PDF/DOCX/TXT import and PDF/DOCX export at `jobs/portal/src/lib/documents.ts:5-100`; per-job version/diff UI at `jobs/portal/src/views/ApplicationsView.tsx:112-123,214-235,310-340`; server creates one job-specific version at `server/src/db/jobs.rs:4593-4768` | Partial | Keep provenance/claim IDs and exact per-job versions, but replace basic contact extraction and deterministic skill/summary tailoring (`jobs/portal/src/lib/documents.ts:39-55`; `server/src/db/jobs.rs:4850-4887`) with an evaluated semantic engine and render golden tests |
| Answer memory and user interventions | Scoped account/track/company selection at `jobs/automation/src/answer-memory.ts:1-94`; API CRUD at `server/src/api/jobs.rs:1182-1618`; UI pauses instead of guessing and lets the user scope a remembered answer at `jobs/portal/src/views/ApplicationsView.tsx:138-156,166-177,224-238` | Bluey stronger | Retain precedence and confirmation rules; add deletion/export and prompt-injection fixtures for every answer source |
| ATS-specific adapters and generic fallback | Dedicated Greenhouse/Lever state machines at `jobs/automation/src/providers/greenhouse.ts:193-625` and `providers/lever.ts:212-762`; Workday/Ashby/SmartRecruiters/semantic definitions at `jobs/automation/src/standard-adapters.ts:52-101,157-340` | Partial | Keep generic fallback review-first; policy currently marks known ATSs beta and no surface certified (`jobs/automation/src/policy.ts:15-53`), so live-certify each provider/tenant variant before background submission |
| CAPTCHA, 2FA, assessments, and browser takeover | Challenge plans preserve the page and require takeover at `jobs/automation/src/challenge-handling.ts:61-83,133-139`; standard adapters detect the same classes at `jobs/automation/src/standard-adapters.ts:112-140`; Temporal waits and resumes at `jobs/workflows/src/workflows.ts:41-67` | Partial | Logic exists; production takeover gateway and Gmail/Outlook authorization remain release gates; raw OTP stays ephemeral |
| Queueing, retries, crash recovery, and duplicate prevention | Temporal retries only reversible work and never blindly retries submit at `jobs/workflows/src/workflows.ts:12-25,35-91`; fenced execution lease/heartbeats at `jobs/runner/src/execution-lease.ts:67-163,165-298`; durable irreversible marker at `jobs/browser/src/irreversible-submit.ts:44-153` | Bluey stronger in code | Retain `side_effect_unknown` reconciliation; pass crash-at-every-boundary and duplicate-submit canaries in local and cloud runners |
| Submission receipts, screenshots, and evidence | Receipt count/byte/type limits at `server/src/api/jobs.rs:29-44,3037-3231`; deterministic immutable bundle at `jobs/automation/src/receipts.ts:100-181`; portal receipt timeline at `jobs/portal/src/views/ApplicationsView.tsx:202-245` | Bluey stronger | Retain exact packet/fingerprint and account-scoped object keys; validate real object-store deletion and partial-upload recovery |
| Email/calendar integrations and outcome tracking | Mailbox/integration endpoints at `server/src/api/jobs.rs:1623-1844`; linked provider evidence contract at `jobs/automation/src/receipts.ts:77-178`; UI explicitly says provider sync is beta/inactive at `jobs/portal/src/views/SettingsView.tsx:213-222,284-291` | Partial | Implement provider OAuth, push/change subscriptions, cursor/idempotency, renewal, deletion, and status normalization; do not claim live sync yet |
| Billing, plans, usage limits, and overages | Jobs plan limits and runner policy at `server/src/db/jobs.rs:920-960`; idempotent monthly packet/overage metering at `server/src/db/jobs.rs:5233-5429`; managed account billing at `server/src/api/mod.rs:221-225,299-314` | Partial | Keep one Bluey balance and once-per-job charge; add a self-service Jobs plan activation path because plan assignment is currently administrative (`server/src/api/jobs.rs:212-225,1853-1868`), then run processor/refund/concurrency tests |
| Settings | Career tracks, identities, search/daily/salary rules, modes, thresholds, challenge policy, memory, integrations, and plan state at `jobs/portal/src/views/SettingsView.tsx:90-233` | Bluey stronger | Keep server-enforced invariants and make beta/unavailable settings visibly non-operative |
| Export and deletion | Export includes the Jobs workspace and run state at `server/src/db/jobs.rs:9364-9400`; account export includes Jobs at `server/src/db/account_data.rs:24-38,74-82`; object-aware hard delete at `server/src/api/account.rs:802-924` | Bluey stronger | Retain object ownership checks and cascade coverage; add production object-store and interrupted-delete canaries |
| Jobs storage privacy | Structured Jobs payload encryption is at `server/src/db/jobs.rs:1027-1085`; cloud profile snapshots are context-bound and encrypted at `jobs/runner/src/profile-store.ts:29-74`; repository privacy gate is at `jobs/scripts/privacy-gate.mjs:58-85,168-253` | Bluey stronger | Retain tenant scope/encryption and verify production key rotation, backup, and deletion behavior |
| Whole-product privacy and telemetry | Admin support output excludes content at `server/src/api/admin.rs:345-395`, but ordinary meeting sync still creates the content-bearing audit described above | Missing, P0 | Fix diagnostic upload before calling the whole product private; prove prompt/transcript/answer/path canaries never enter default operational telemetry |
| Updates | Terminal updater verifies manifest signature and artifact hashes at `crates/cue-cli/src/update.rs:216-459` | Bluey stronger | Keep the terminal updater and validate hosted signature, rollback, and platform artifacts at release time |

## Recovered and owner-source coverage

| Product/source | macOS evidence | Windows evidence | Readable owner source | Unique material retained for Bluey |
| --- | --- | --- | --- | --- |
| Cluely | Exact ASAR plus reconstructed slices under `/Users/uno/Downloads/dmg_backtrack_code/recovered` and `reformed` | Exact/reformed executable evidence under `/Users/uno/Downloads/exe_backtrack_code` | None separately supplied | Helper/overlay lifecycle, modes/calendar, VAD orchestration, atomic state |
| Littlebird | Main/preload plus 974 credible first-party source-map files | Helper supervision/replay evidence | None separately supplied | Workspace/onboarding/context hierarchy and critical-state replay |
| LockedIn | Main/preload and complete minified renderer | Native/DPI interface evidence | None separately supplied | DPI, screenshot/document context, presets, restoration deadlines |
| ParakeetAI | Packaged JS plus 28 exact Rust files and pinned Sonora source | Exact MMDevice/AEC/activity implementation evidence | Native source is exact packaged source | Highest-confidence Windows capture, AEC, activity, meeting detection |
| Final Round | Main/preload and nine renderer windows plus 13 lifecycle slices | Bounded streaming/update/recovery evidence | None separately supplied | Narrow IPC, bounded VAD/socket lifecycle, rollback, panels, PKCE |
| Aura | Not a DMG/EXE recovery target | Not a DMG/EXE recovery target | `_refs/Aura-AI-master` | Only bounded screenshot staging/provider state; backend/security patterns rejected |
| OpenCluely | Not independently counted against the recovered Cluely product | Not independently counted | `_refs/OpenCluely-main` | Compact interaction vocabulary; shared family is deduplicated |
| Vysper | Not independently counted as a recovered product | Not independently counted | `_refs/Vysper-main` | OCR flow and prompt taxonomy; shared OpenCluely ancestry deduplicated |
| Pluely | Source tree supplies Tauri/native implementation | Windows WASAPI and selection source | `_refs/pluely-master` | Event-driven overflow policy, multi-monitor selection, bounded quick actions |
| Natively | Source tree supplies Electron/React/Rust implementation | Cross-platform Rust/CPAL evidence | `_refs/natively-cluely-ai-assistant-main` | Corrected callback/worker boundary, stream coalescing, context, queued/indexed RAG ideas |
| SolveWatch | Source tree supplies Node/Electron/Python implementation | No unique packaged Windows native layer | `_refs/solveWatchAi-main` | LocalAgreement-2, adaptive endpointing, VAD benchmark, serial compaction, OCR worker |

Exact recovery counts and hard limits are preserved at
`/Users/uno/Downloads/dmg_backtrack_code/reformed/INDEX.md:20-89`,
`/Users/uno/Downloads/dmg_backtrack_code/reformed/SECOND-PASS-RECOVERY.md:47-175`,
`/Users/uno/Downloads/exe_backtrack_code/recovered/INDEX.md:1-35`, and
`/Users/uno/Downloads/exe_backtrack_code/reformed-windows/INDEX.md:1-30`.
The six `_refs` trees and every regular-file digest are anchored in
[hashes.txt](hashes.txt).

## Complete implementation order

The smallest production-quality end-to-end implementation is deliberately phased so
performance work cannot hide security or privacy regressions.

### P0: protect and bound the current product

1. Replace unauthenticated daemon TCP with owner-scoped authenticated local
   transport and bounded frames, deadlines, replay cache, and connection limits.
2. Convert real-time audio/STT/overlay paths into bounded lossy-data and lossless
   control lanes; add helper readiness, forced-stop, reconnect, and drop/lag metrics.
3. Make normal sync independent from diagnostics; metadata-only diagnostics by
   default, explicit content opt-in, redacted paths, retention/deletion, no delta log.
4. Prove the changes with slowloris, unauthorized shutdown, slow-consumer,
   1,000-delta/sec, content-canary, and long-session bounded-RSS tests.

### P1: make Bluey faster and more resilient

1. LocalAgreement-style stable partials plus a deterministic STT/VAD benchmark.
2. Event-driven Windows audio, proper resampling, exact Parakeet AEC/MMDevice behind
   measured fallback gates, and per-channel health/reconnect status.
3. 16--33 ms answer display coalescing with exact final reconstruction and mandatory
   final/error/supersede flush.
4. Atomic meeting/state publication, revisioned conversation compaction, and durable
   stream sequence/snapshot/replay.
5. Transactional leased RAG indexing queue, idempotent content keys, then a
   benchmark-gated scalable vector index.
6. Native Windows region/multi-monitor capture and typed untrusted-evidence prompts.

### P2: finish breadth after the foundations are measured

1. Bounded native quick actions and optional `attach next`/`analyze now` staging.
2. OCR worker only with input/output/deadline/temp/privacy bounds.
3. Gmail/Outlook and calendar provider runtime with push/change cursors, deletion,
   renewal, plan enforcement, and content-free telemetry.
4. Licensed discovery providers, remaining ATS certification, production browser
   takeover, and real local/cloud crash/receipt canaries.
5. Binary hardening only as cost-raising defense in depth; never claim client bytes
   shipped to a customer are unrecoverable.

## Conditions before “complete” or “deployable”

The source/code comparison is complete. The product changes are not implemented by
this audit and current main must not be described as having them. A later
implementation round is complete only after:

- P0 adversarial IPC, queue, privacy, and recovery tests pass;
- exact final stream reconstruction and bounded memory are measured;
- Rust/native/Jobs/UI tests, formatting, warnings-denied lint, privacy gates, and
  `git diff --check` pass;
- real Windows named-pipe, WASAPI, AEC, mixed-DPI, updater, and browser canaries pass
  for any changed Windows path;
- Jobs provider/OAuth/object-store/takeover dependencies are configured and live
  sandbox-certified before those features are called deployed;
- rollback and provenance for every directly reused source slice are recorded.

This is the complete backend/frontend/native handoff. The file-level work plan and
acceptance tests continue in [BLUEY-HANDOFF.md](BLUEY-HANDOFF.md).

# Final Round versus Bluey gap map

Bluey baseline: repository commit `eb923a89f24b69664c73479607bf665725789658`. Final Round baseline: owner-supplied 2.4.0 arm64 DMG, SHA-256 recorded in [hashes.txt](hashes.txt). Status means Bluey's current source position relative to the observed Final Round desktop capability, not a claim that either product is deployed or operationally proven.

## Comparison matrix

| Capability | Final Round evidence | Bluey status | Exact Bluey evidence and decision |
|---|---|---|---|
| Job discovery, freshness, filtering, ranking | Absent from complete desktop route/service/API inventory. [Features](FEATURES.md#application-automation-absence-map) | Bluey stronger | Public ATS discovery deduplicates, freshness-filters, and normalizes jobs (`jobs/automation/src/public-ats.ts:45-127`); Matches filters/scores by track/workplace (`jobs/portal/src/views/MatchesView.tsx:35-119`). Keep and production-validate. |
| ATS adapters and generic fallback | Absent | Bluey stronger | Greenhouse, Lever, Ashby, SmartRecruiters, Workday, and semantic definitions plus challenge detection (`jobs/automation/src/standard-adapters.ts:52-140`, `157-355`); provider-first registry (`jobs/automation/src/execute.ts:37-98`). Adapt only through Bluey's own tests. |
| Local and cloud application execution | Absent | Bluey stronger | Local persistent Playwright browser and cloud runner paths (`jobs/browser/src/main.ts:123-170`, `405-430`; `jobs/runner/src/server.ts:91-220`). Final Round has no reusable job-browser implementation. |
| Browser profile/multi-account isolation | No job browser; shared Electron default session | Bluey stronger | Account+application-identity profile keys/directories (`jobs/browser/src/profile.ts:4-34`); encrypted cloud snapshots (`jobs/runner/src/profile-store.ts:17-75`). Preserve identity isolation and verify cleanup. |
| Resume import, job tailoring, visible diff, export | Context upload only; no tailoring/diff/export | Bluey stronger | Deterministic PDF materialization (`jobs/automation/src/documents.ts:58-125`) and portal import, visible diff, PDF/DOCX export (`jobs/portal/src/views/ResumeView.tsx:57-98`, `109-142`; `jobs/portal/src/views/ApplicationsView.tsx:310-340`). Keep provenance/claim guards. |
| Reusable answer memory | Only ephemeral interview messages; no application memory | Bluey stronger | Confirmed account/track/company precedence (`jobs/automation/src/answer-memory.ts:26-103`; `jobs/automation/src/form-intelligence.ts:120-153`) and edit/delete UI (`jobs/portal/src/views/SettingsView.tsx:191-208`, `300-353`). |
| Sensitive/unknown application questions | No application workflow | Bluey stronger | Sensitive patterns, verified-fact checks, and intervention planning (`jobs/automation/src/form-intelligence.ts:78-141`). Maintain fail-closed Auto-submit behavior. |
| CAPTCHA, 2FA, assessments, takeover | No application workflow | Bluey stronger | CAPTCHA/assessment/browser takeover plus ephemeral approved email OTP (`jobs/automation/src/challenge-handling.ts:55-140`); portal takeover/approval UI (`jobs/portal/src/views/BrowserView.tsx:47-66`). |
| Queueing, retries, crash/duplicate prevention | Live session reconnect and in-memory XState only; no durable job queue/lease/idempotency. [Architecture](ARCHITECTURE.md#session-lifecycle-and-failure-semantics) | Bluey stronger | Temporal retry policy and no automatic retry around irreversible work (`jobs/workflows/src/workflows.ts:12-25`), idempotent workflow ID (`jobs/workflows/src/gateway.ts:22-50`), execution leases (`jobs/runner/src/execution-lease.ts:66-162`), and durable exclusive submit marker (`jobs/browser/src/irreversible-submit.ts:44-175`). |
| Submission receipts and screenshots | Interview screenshots/reports, not application receipts | Bluey stronger | Exact packet, documents, checksums, events, result, URL, and screenshots (`jobs/automation/src/receipts.ts:28-173`); completeness guards require confirmation+screenshot (`jobs/automation/src/packet-guards.ts:21-54`); local runner captures/delivers evidence (`jobs/browser/src/main.ts:259-328`). |
| Duplicate-company/daily safety | No application safety model | Bluey stronger | Atomic policy checks block an active/submitted company duplicate and daily-limit excess (`server/src/db/jobs.rs:3535-3629`). |
| Email/calendar outcome tracking | Absent; support contact only | Partial | Evidence types and provider models exist (`jobs/automation/src/receipts.ts:74-98`, `176-190`), and portal presents inbox/calendar state, but provider authorization is explicitly beta/not active (`jobs/portal/src/views/SettingsView.tsx:213-233`, `284-291`). Smallest next step: one read-only Gmail provider, scoped OAuth, durable cursor, dedupe, provenance, deletion, then calendar. |
| Jobs plans, limits, overages | Plans/trial/subscription; no explicit desktop overage UI | Bluey stronger | Free/Pro/Cloud limits and runner entitlements (`server/src/db/jobs.rs:930-988`), idempotent per-job metering/overage ledger (`server/src/db/jobs.rs:5375-5584`), and user-facing overage terms (`jobs/portal/src/views/SettingsView.tsx:226-233`). |
| Account export and deletion | No self-service desktop deletion control; server behavior unknown | Bluey stronger | Structured/ZIP export and confirmed hard delete of scoped objects, diagnostic logs, and cascading data (`server/src/api/account.rs:723-751`, `754-890`). Surface these controls in Jobs settings. |
| OAuth/onboarding/permission mission | Polished PKCE OAuth plus permission/payment onboarding | Partial | Bluey has account auth and Jobs onboarding, but the audited source anchors do not show equally integrated macOS audio/screen permission diagnosis. Smallest implementation: preflight-only permission panel with explicit purpose, no automatic prompt, deep link to settings, and post-grant test. |
| Mic and system-audio capture | Core Audio taps (14.2+), ScreenCaptureKit fallback, AEC symbols, 16 kHz STT/48 kHz recording | Partial | Bluey has native ScreenCaptureKit audio (`native/macos/cue-audio/Sources/cue-audio/main.swift:153-210`) and supervised 16 kHz framing/restarts (`crates/cue-daemon/src/audio/system_capture.rs:1-59`, `163-303`), but its factory documents split streaming versus chunked mic paths (`crates/cue-daemon/src/stt/factory.rs:1-30`). Unify paths before adding Core Audio tap/AEC. |
| Local VAD | Bundled Silero ONNX with bounded drop-oldest queue | Equivalent | Bluey uses adaptive RMS plus WebRTC VAD and hangover (`crates/cue-core/src/vad.rs:1-48`; `crates/cue-daemon/src/audio/vad.rs:87-217`). Benchmark accuracy/latency before changing models; do not copy the ONNX asset. |
| STT provider resilience/local fallback | Cloud socket ASR with reconnect; no observed offline STT | Bluey stronger | Deepgram, optional OpenAI Realtime, and local Whisper chain (`crates/cue-daemon/src/stt/factory.rs:41-135`), Deepgram reconnect (`crates/cue-daemon/src/stt/deepgram.rs:479-558`), native local Whisper helper (`native/macos/cue-whisper/Sources/CueWhisper/main.swift:1-107`). Productionize one unified streaming path. |
| Live transcript/answer overlay | Multiple Electron widgets, streaming transcript/answers, shortcuts | Equivalent | Bluey has durable transcript/conversation models (`crates/cue-core/src/meeting.rs:33-52`, `227-293`, `356-453`) and native streaming overlay (`native/macos/cue-overlay/Sources/cue-overlay/main.swift:1-21`). Bluey's native boundary avoids Final Round's broad renderer IPC pattern. |
| Screenshot/page context and coding assistance | Screenshot upload with structured code/system-design streams | Partial | Bluey captures screen/page context and attaches artifacts (`crates/cue-daemon/src/app.rs:3151-3187`, `13909-14184`, `14462-14562`) and generates code/system-design artifacts, but lacks an equally explicit interview-mode pattern/idea/tests workflow. Add a presentation layer over existing answer/artifact contracts, not a new transport. |
| Phone/external-audio interview mode | Explicit external/phone workflow and audio endpoint | Missing | No equivalent production path found in Bluey's audio/session inventory. Smallest safe implementation: opt-in remote ingest session with ephemeral pairing code, visible recording indicator, hard expiry, consent, and no PSTN recording by default. |
| Video career coach/mock interview | Daily/Pluot coach and mock-interview resources | Missing | Bluey has evidence-grounded interview-prep packets (`jobs/automation/src/interview-prep.ts:71-138`, `175-197`, `247-289`) but no live video coach. Start with audio/overlay mock mode using that packet; video is unnecessary for MVP. |
| Reports, transcript history, recap | Cloud reports/doc/transcript/regenerate | Equivalent | Bluey persists meeting transcript/context/conversation/summary (`crates/cue-core/src/meeting.rs:273-374`), uses `MeetingStore` (`crates/cue-daemon/src/storage.rs:9-66`), and generates recap in daemon (`crates/cue-daemon/src/app.rs:2013-2039`). Validate export/UI polish rather than replace the model. |
| Meeting-app detection/auto lifecycle | Polls native process list and closes session when all tracked apps exit; browser meetings unsupported | Partial | Bluey polls frontmost macOS bundle IDs every two seconds and emits detection (`crates/cue-daemon/src/cloud/meeting_detect.rs:1-78`) but does not show equivalent ended-app debounce/auto-stop. Add an advisory stop prompt first; never silently end on browser heuristics. |
| Content protection/stealth overlay | Every window content-protected by default, reapplied every 500 ms | Equivalent | Bluey's native overlay floats across spaces and sets `sharingType = .none` in production (`native/macos/cue-overlay/Sources/cue-overlay/main.swift:1793-1805`). Keep user-visible disclosure; reject process masquerading/anti-debug as a product differentiator. |
| Local secret/profile encryption | safeStorage normally; silent plaintext fallback | Partial | Jobs uses contextual AES-256-GCM/HKDF profiles (`jobs/runner/src/crypto-envelope.ts:43-167`) and daemon API keys use OS keychain (`crates/cue-daemon/src/secrets/mod.rs:1-54`), but Bluey account tokens default to a private local profile and OS secure storage is opt-in (`crates/cue-cloud-client/src/tokens.rs:1-6,72-123`). Make long-lived account-token storage fail-closed and secure-by-default. |
| Renderer/local IPC least privilege | Broad identical preloads; sender path check only | Bluey stronger | Native overlay events carry a random per-session token and are validated before typed decode (`crates/cue-daemon/src/app.rs:15416-15532`, `15710-15925`). Audit daemon TCP callers too, but do not adopt Final Round's shared preload surface. |
| Signed in-app updates | HTTPS generic electron-updater, signed/notarized current bundle | Partial | Bluey's dashboard registers the Tauri updater and checks in the background (`crates/cue-dashboard/src/lib.rs:49-51,250-273`; `crates/cue-dashboard/src/commands.rs:766-784`), but its release endpoint and public key remain explicit placeholders (`crates/cue-dashboard/tauri.conf.json:42-47`). Replace them and add Team ID/publisher pinning, monotonic versions, staged rollout, rollback, and wrong-signer tests before release. |
| Telemetry/privacy controls | Rich Sentry/PostHog/Amplitude; transcript/answer content reaches log calls | Bluey stronger | Bluey's observability defines redaction-safe stable fields (`crates/cue-core/src/observability.rs:19-171`) and account hard delete exists. Still add an automated no-content telemetry test before release. Reject Final Round's content logging. |

## Highest-value conclusions

Bluey is already substantially stronger on the user's primary objective: discovering jobs, tailoring packets, isolating browser identities, handling application forms/challenges, guarding irreversible submission, and producing durable evidence. Final Round contributes useful evidence about polished interview UX and low-latency desktop audio, not a superior job-automation architecture. [Feature evidence](evidence/features-static.txt) [Bluey code map](evidence/bluey-code-map.txt)

The smallest path to “faster and more awesome” is therefore to harden and ship Bluey's existing Jobs path while selectively closing three interview gaps: unified low-latency audio, clearer structured coding/system-design presentation, and a safe signed updater. Copying proprietary bundles or recreating Final Round's broad Electron IPC/telemetry design would move Bluey backward.

## Recommended changes

### P0 — production trust and latency

1. Unify Bluey's mic and system-audio pipelines behind the streaming STT router. The current factory explicitly says chunked mic/real-audio bypasses provider fallback (`crates/cue-daemon/src/stt/factory.rs:15-30`). Use the existing 20 ms PCM/VAD path, bounded queues, reconnect metrics, and local Whisper fallback. This is the smallest change likely to improve perceived speed.
2. Add content-free telemetry invariants across desktop and Jobs. Tests should fail if transcript, answer, resume, application answer, cookies, tokens, or screenshot bytes reach logs/metrics. Final Round's static Sentry path demonstrates the concrete failure mode (`main:12104-12115`, `main:12251-12255`).
3. Ship a signed updater with signer/Team ID pinning, monotonic version checks, staged rollout, rollback, and failure recovery. Bluey should not rely on users manually replacing a high-permission desktop binary.
4. Run one production-like end-to-end Jobs canary that proves: idempotent queue request, encrypted profile restore/seal, final-review authorization, durable submit marker, lease loss behavior, receipt/evidence upload, and no double charge. The components already exist; the missing evidence is integrated deployment, not architecture.

### P1 — interview advantage without architectural sprawl

1. Add an interview mode that renders existing Bluey answer artifacts into compact `Approach`, `Code`, `Explanation`, `Complexity`, `Tests`, and system-design panels. Reuse `MeetingRecord`, screenshot contexts, and current artifact contracts; do not create a second assistant transport.
2. After unified streaming is stable, benchmark ScreenCaptureKit against a macOS 14.2 Core Audio tap backend and optional WebRTC AEC. Keep ScreenCaptureKit fallback and require explicit permission UI. Adopt the platform pattern, not Final Round's native binary.
3. Extend meeting detection to lifecycle hints with debounce and a visible “meeting appears to have ended” prompt. Never auto-end solely from browser/frontmost-process heuristics.
4. Complete one read-only inbox connector with minimal OAuth scopes, encrypted tokens, provider cursor/replay protection, provenance, deduplication, user disconnect, and deletion. Calendar follows only after the inbox evidence model is proven.
5. Surface existing Bluey account export/delete and Jobs overage rules directly in Jobs settings.

### P2 — optional product expansion

1. Build audio-first mock interview coaching from the existing evidence-grounded prep packet. Add video only if user research shows material value.
2. Consider phone/external-audio pairing only with explicit consent, ephemeral codes, session expiry, visible status, and a no-recording default.
3. Add updater channels and diagnostic support bundles only after redaction/retention controls are independently tested.

## Reuse and provenance decisions

| Observed Final Round element | Decision | Reason |
|---|---|---|
| PKCE/state flow, VAD backpressure, capture compression, reconnect cadence, meeting-end debounce | Adapt concept | Standard patterns; independently implement against public APIs and Bluey's architecture. |
| Core Audio tap/AEC approach | Research then adapt | Platform technique is valuable, but addon source/license/performance provenance is unavailable. |
| Electron/React widgets and UI assets | Reject direct reuse | Proprietary/minified, no source provenance or license grant for copying. |
| Native addons and Silero model bytes | Reject direct reuse | No bundled source/SBOM/license proof sufficient for Bluey redistribution review. |
| API routes, backend contracts, public ingestion identifiers | Reject | Proprietary service boundary; identifiers were intentionally redacted. |
| Broad identical preload, plaintext secret fallback, warn-only schemas, content telemetry | Reject | Security/privacy regressions. |
| Default content protection | Adapt with disclosure | Useful privacy feature, but users must understand what is hidden from recordings. |
| Process masquerading/anti-debug as “stealth” | Reject | Damages transparency, supportability, and user trust; not required for capture exclusion. |

## Unknowns requiring future validation

- Final Round authenticated UI, audio latency/accuracy, video coach, reports, permission flows, and server enforcement.
- Final Round backend authorization, storage encryption, retention/deletion, queueing, telemetry scrubbing, and update rejection.
- Bluey's deployment readiness, installer/updater signing, live-provider configuration, full end-to-end runner canary, and production telemetry behavior.
- Comparative performance must be measured on identical hardware/network/input; static bundle size or framework choice is not a speed benchmark.

## Concrete handoff for the implementation agent

1. Do not import anything from the DMG.
2. Start in `crates/cue-daemon/src/stt/factory.rs` and the real-audio loop: design one `SttProvider`-based streaming path for mic+system sources, preserving `crates/cue-daemon/src/audio/vad.rs` and bounded reconnect semantics.
3. Add integration tests for source mixing, provider failover, disconnect/reconnect, queue bounds, permission denial, and teardown; measure first-transcript and first-answer latency.
4. Add a repository-wide telemetry guard test that feeds synthetic secrets/PII/transcripts/resume answers through log paths and asserts no sink payload contains them.
5. Specify the signed updater trust model before implementation: platform, manifest signing, signer pin, rollout, rollback, and atomic replacement.
6. Only after those P0 gates pass, implement interview panel presentation over existing `CueCardArtifact`/screen context, then inbox sync.

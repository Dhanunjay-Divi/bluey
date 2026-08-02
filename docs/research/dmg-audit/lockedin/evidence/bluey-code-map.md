# Bluey code-reference map

All paths are relative to `/Users/uno/Downloads/cue-bluey-jobs`; line ranges were revalidated against comparison HEAD `eb923a89f24b69664c73479607bf665725789658` during final coordination.

## Product boundaries and guarantees

- `jobs/README.md:3-16` identifies the Jobs packages; `jobs/README.md:41-46` distinguishes cloud execution and intervention handoff; `jobs/README.md:61-98` states safety and durability guarantees.
- `jobs/ARCHITECTURE.md:15-23` maps services, `jobs/ARCHITECTURE.md:25-42` storage/encryption, `jobs/ARCHITECTURE.md:44-62` invariants, and `jobs/ARCHITECTURE.md:85-96` release gates.

## Browser and identity isolation

- `jobs/browser/src/profile.ts:4-33` derives per-account/per-identity profile directories from hashes.
- `jobs/browser/src/main.ts:94-120` creates a sandboxed controller and denies window opens; `jobs/browser/src/main.ts:123-169` validates identity/run state and profile ownership; `jobs/browser/src/main.ts:405-430` launches a persistent Playwright context and installs a network guard.
- `jobs/browser/src/browser-network.ts:18-53` validates every request/WebSocket destination, requires HTTPS/WSS, and rejects non-public targets.
- `jobs/browser/src/protocol.ts:1-20` restricts the browser protocol payload to a run ID and 64-hex ticket.
- `jobs/runner/src/profile-store.ts:17-70` scopes encrypted cloud profiles by tenant and identity.
- `jobs/runner/src/crypto-envelope.ts:7-21,43-71,80-167` defines the versioned AES-256-GCM envelope, AAD contexts, fail-closed decryption, validation, and HKDF-derived keys.

## ATS execution, handoff, and answer memory

- `jobs/automation/src/contracts.ts:1-21,36-68,99-161,164-199` defines ATS types, state/runners, browser contract, interventions, receipts/adapters, and discovery contracts.
- `jobs/automation/src/execute.ts:37-97` uses provider-first adapters and converts blocking conditions into interventions.
- `jobs/automation/src/standard-adapters.ts:52-100,112-140,197-354` selects Greenhouse, Lever, Ashby, SmartRecruiters, Workday, or semantic adapters; detects CAPTCHA/2FA/assessments; bounds form execution to 12 steps; detects providers by host.
- `jobs/automation/src/policy.ts:6-53` requires handoff for LinkedIn/Indeed and semantic fallbacks and blocks invalid/private targets.
- `jobs/automation/src/challenge-handling.ts:6-90,92-192` defines takeover/email-code flows, approval requirements, domain checks, and code masking.
- `jobs/automation/src/answer-memory.ts:1-14,35-103` records confirmed/source-scoped answers and resolves company, track, then account precedence.

## Discovery, ranking, queueing, and duplicate prevention

- `jobs/workflows/src/discovery.ts:7-63,70-203,213-222,320-418` defines providers, policy/proof, canonical state, telemetry, a 15-minute default cadence, bounded retries, pause behavior, and replay-safe discovery.
- `server/src/db/jobs.rs:3370-3633` enforces freshness, exclusions, compensation/location/sponsorship policies, one-company and daily-limit rules, match thresholds, and certification.
- `server/src/db/jobs.rs:3869-3918` computes match scoring; `server/src/db/jobs.rs:4688-4811` persists tailored resumes, answer memory, queue/review decisions, receipt metadata, and answer precedence.
- `jobs/workflows/src/workflows.ts:10-91,94-119` separates retryable work from irreversible submission, uses a 24-hour intervention pause, keys resume requests uniquely, preserves side-effect-unknown, and persists receipts before release.
- `jobs/runner/src/execution-lease.ts:27-77,105-159` claims and heartbeats leases, gates final submit, and finishes leases.
- `jobs/browser/src/irreversible-submit.ts:6-73,87-175` gives one process exclusive final-submit authority with O_EXCL/fsync-backed records.
- `jobs/browser/src/local-failure.ts:6-38,60-95` classifies safe failures and prevents resubmission after side effects become unknown.

## Resumes, receipts, and evidence

- `jobs/automation/src/documents.ts:44-125,280-334,371-413` materializes and generates normalized PDF documents with budgets and contact/section handling.
- `jobs/portal/src/views/ResumeView.tsx:27-69,87-143` imports and extracts resumes, saves facts/versions, presents diffs, and exports PDF/DOCX.
- `jobs/portal/src/lib/documents.ts:10-55,57-100` imports PDF/DOCX/TXT and exports DOCX/PDF.
- `server/src/db/jobs.rs:3921-3949` stores structural before/after resume diffs and fact IDs.
- `jobs/automation/src/receipts.ts:4-53,74-201` defines document/evidence bundles, linked provider evidence, deterministic receipts, confirmations, screenshots, and email/calendar fingerprints.
- `jobs/automation/src/packet-guards.ts:4-54` validates packet identity and requires document hashes, confirmation, and screenshot in receipts.
- `jobs/browser/src/main.ts:214-340` executes a run, captures evidence, handles interventions, builds receipts, and delivers them.

## Live meeting/interview support

- `crates/cue-daemon/src/audio/capture.rs:1-10,24-47,72-125` captures microphone audio through CPAL.
- `crates/cue-daemon/src/audio/system_capture.rs:1-8,21-64,163-225,290-302` supervises the system-audio helper with retries and backoff.
- `crates/cue-daemon/src/llm/answer.rs:6-34,168-240` defines a live meeting/work-copilot answer contract and streaming responses.
- `crates/cue-daemon/src/llm/recap.rs:6-42,46-83` creates structured meeting recaps and streams them.
- `crates/cue-daemon/src/overlay.rs:17-38` bounds overlay restart attempts and authenticates overlay IPC with a 64-hex session token.
- `jobs/automation/src/interview-prep.ts:20-77,88-172,247-372` creates evidence-grounded interview preparation, resists fabricated claims, and sanitizes PII.

## Auth, settings, deletion, billing, and integrations

- `crates/cue-dashboard/ui/src/pages/Onboarding.tsx:6-16,38-69,93-135,167-206` handles browser-deep-link sign-in and permission/disclosure onboarding.
- `crates/cue-dashboard/ui/src/pages/Settings.tsx:5-14,67-125,135-264,266-334` exposes account, billing, deletion, disguise, and visibility settings.
- `crates/cue-core/src/config.rs:10-24,48-88,99-143` defines account/audio/sync/retention/disguise settings and writes private configuration files with mode 0600.
- `crates/cue-core/src/app_paths.rs:20-73` centralizes app paths and creates private directories with mode 0700.
- `crates/cue-cloud-client/src/tokens.rs:1-6,72-115` documents and implements private-file token storage by default, with Keychain opt-in; the raw-token-in-0600-file default remains a Bluey hardening opportunity.
- `server/src/auth/jwt.rs:1-17,70-92` specifies 15-minute access tokens, 30-day refresh tokens, and verification behavior.
- `server/src/api/auth_routes.rs:24-80,138-149,170-237` defines auth response shapes, CAPTCHA configuration, and hashed OTP generation.
- `jobs/portal/src/views/SettingsView.tsx:213-233,284-292,425-450` exposes plan/inbox limits and explicitly marks Gmail/Outlook and calendar authorization as beta/not active for the account.
- `jobs/portal/src/views/SettingsView.tsx:229-233` shows Free/Pro/Cloud included applications and overage pricing, with retries/handoffs excluded from repeat charges.

## Update and release boundaries

- `crates/cue-cli/src/update.rs:20-23,81-129,166-225,228-371,395-460` implements Bluey's CLI update flow, verifies an Ed25519-signed manifest, requires installer/artifact hashes unless an explicit unsigned override is enabled, and offers a five-second cancellation window; compare that boundary with LockedIn's S3/electron-updater auto-download behavior.
- `jobs/ARCHITECTURE.md:85-96` records production release gates, including external OAuth credential requirements.

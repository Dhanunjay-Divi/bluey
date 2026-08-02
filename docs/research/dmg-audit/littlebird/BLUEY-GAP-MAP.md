# Littlebird → Bluey gap map

Bluey source references were revalidated against repository commit `eb923a89f24b69664c73479607bf665725789658`.

Statuses describe Bluey relative to the observed Littlebird implementation:

- `Bluey stronger`: Bluey has a more complete or safer implementation for this product need.
- `Equivalent`: comparable client-visible capability is implemented.
- `Partial`: Bluey covers part of the capability or uses a narrower model.
- `Missing`: the Littlebird capability is not present in Bluey.

## Cross-product matrix

| Capability | Status | Evidence-based comparison | Reuse decision | Smallest production-quality Bluey action |
| --- | --- | --- | --- | --- |
| Job discovery/ranking/provenance | **Bluey stronger** | Littlebird has no static job pipeline; Bluey normalizes sources, persists provenance, handles duplicates, and applies retry policy. E-LB-FEAT-007; E-LB-BLUEY-007 | Reject code reuse | Keep Bluey's source contracts; add source-quality observability only if gaps emerge. |
| Local/cloud browser execution | **Bluey stronger** | Bluey has local Playwright execution and a durable cloud runner boundary; Littlebird context observation is not a job runner. E-LB-ARCH-008; E-LB-BLUEY-001, -002, -007 | Reject | Preserve both execution modes and explicit application identity scope. |
| Browser-profile/multi-account isolation | **Bluey stronger** | Bluey hashes account+application identity and encrypts cloud snapshots with externally supplied AES-256-GCM keys/AAD. Littlebird supports multiple integrations but no per-job browser profile model was found. E-LB-FEAT-004; E-LB-BLUEY-001, -002 | Reject | Keep external key management; add lifecycle tests for profile cleanup/rotation. |
| ATS-specific adapters + fallback | **Bluey stronger** | Bluey resolves Greenhouse/Lever first, then standard/semantic fallbacks; Littlebird's generic parser fallback observes context but does not submit ATS forms. E-LB-ARCH-008; E-LB-BLUEY-003 | Reject | Expand Bluey adapters provider-by-provider behind fixture/state-machine tests. |
| Resume tailoring/diffs/export | **Bluey stronger** | Bluey materializes tailored resume and cover-letter PDFs, hashes/uploads them, and binds exact resume to receipt. No equivalent Littlebird workflow was found. E-LB-FEAT-007; E-LB-BLUEY-005, -006 | Reject | Keep deterministic document hashes; add user-facing diff if not already surfaced. |
| Job answer memory | **Bluey stronger** | Bluey scopes confirmed answers by account/track/company and normalizes/private-hashes lookup keys. Littlebird has general memory, not job-question memory. E-LB-FEAT-002, -007; E-LB-BLUEY-004, -005 | Reject | Preserve confirmation and scope precedence; never store authentication factors. |
| CAPTCHA/2FA/assessment/takeover | **Bluey stronger** | Bluey models challenges and browser takeover, treats email codes ephemerally, and rejects secrets from persisted interventions. Littlebird only shows account-login OTP. E-LB-FEAT-009; E-LB-BLUEY-004 | Reject | Keep explicit expiry/provider-message binding and secret-storage rejection. |
| Queue/retry/crash recovery/idempotency | **Bluey stronger** | Littlebird supervises helpers/WSS but pending queues are memory-only. Bluey has workflow retry and durable activity/result boundaries plus final-submit marker/evidence guards. E-LB-FEAT-008; E-LB-BLUEY-001, -007 | Reject | Maintain durable idempotency at submit boundary; test unknown-submit recovery. |
| Submission receipts/screenshots | **Bluey stronger** | Bluey records confirmation, exact resume, fingerprints, and screenshots and prevents `submitted` without evidence. No Littlebird job receipt exists. E-LB-FEAT-007; E-LB-BLUEY-006 | Reject | Keep evidence gate and immutable receipt schema/version. |
| Email/calendar integrations | **Partial** | Littlebird has multi-account Gmail/Calendar paths, email draft operations, and broad app parsers. Bluey has job interventions/outcome primitives but not the same assistant-grade integration surface. E-LB-FEAT-004; E-LB-BLUEY-004, -008 | Adapt concept only | Add narrowly scoped Gmail/Outlook outcome ingestion with explicit consent, minimal scopes, and application matching. |
| Outcome tracking | **Bluey stronger** | Littlebird tracks meetings/activity but no application outcome model was found; Bluey has application state/evidence APIs. E-LB-FEAT-007; E-LB-BLUEY-006, -008 | Reject | Add provider-email correlation atop Bluey's canonical application identity, not general inbox capture. |
| Billing/plans/limits/overages | **Partial** | Littlebird exposes richer feature credits/pools/refill UX. Bluey has protected billing routes and jobs entitlement/metering, but the inspected code comparison does not show equivalent general assistant credit tooling. E-LB-FEAT-005; E-LB-BLUEY-008, -009 | Adapt product model | Implement only job-specific transparent limits, receipts, cap controls, and idempotent metering. |
| Account deletion/export/privacy | **Equivalent** | Both expose account deletion/export controls; Littlebird also has time-range context deletion/exclusions because it captures OS context. Bluey implements a server hard-delete workflow. E-LB-FEAT-006; E-LB-BLUEY-008 | Reject code reuse | Keep Bluey's hard-delete audit/tests; add connector-data deletion when integrations ship. |
| Electron renderer security | **Bluey stronger** | Bluey explicitly enables Chromium sandbox and restricts navigation; Littlebird has context isolation/Node disabled but generic IPC, no explicit sandbox/CSP, and renderer-reachable tokens. E-LB-SEC-003, -004; E-LB-BLUEY-001 | Reject | Add/retain typed channel allowlists and sender validation everywhere; keep tokens out of renderer. |
| Native cross-app context | **Partial** | Bluey already has explicit user-approved periodic screenshots, active-browser page text on macOS/Windows, screenshot fallback, context artifacts, overlay context chips, and RAG. Littlebird is broader: continuous native observation and 46 application-specific accessibility parsers plus app/domain/category exclusions. E-LB-ARCH-008; E-LB-BLUEY-010 | Reject direct reuse | Keep Bluey's consent-first, artifact-scoped model; add a clean-room per-app adapter only for a proven meeting/job use case and pair it with exclusion tests. |
| Meeting capture/transcription | **Equivalent** | Bluey has meeting-app detection, native system+microphone capture, managed and fallback STT, partial/final overlay/dashboard streaming, transcript deduplication, persistence, action/decision extraction, recaps/RAG, and live answers. Littlebird has a comparable native meeting helper plus calendar-aware flows; authenticated UX depth remains untested. E-LB-FEAT-003; E-LB-BLUEY-010 | Reject code reuse | Improve Bluey's existing pipeline—permission UX, auto-detection coverage, retention, and calendar correlation—rather than building a parallel meeting stack. |
| General assistant workspace/MCP | **Partial** | Bluey already provides a native overlay, saved sessions, streamed context-aware answers, transcript/file/page/screenshot context, and current/global RAG. Littlebird additionally exposes chats, projects, journals, routines, world model, arenas, and MCP grants as a larger workspace. E-LB-FEAT-002; E-LB-BLUEY-010 | Reject direct reuse | Extend Bluey's existing session/context model only where product demand is clear; MCP and journal/project parity are not prerequisites for Jobs. |
| Local-tool consent policy | **Partial** | Littlebird's Axon uses allow/ask/deny and read/update/destroy risk classes. Bluey has job-specific review/interventions and submit approval, but no general tool policy layer. E-LB-FEAT-008; E-LB-BLUEY-003, -004 | Adapt clean-room concept | Generalize only if Bluey gains non-job tools; retain operation-specific approvals. |

## Exact Bluey code anchors

- Electron sandbox/navigation and local execution: `jobs/browser/src/main.ts:94-170`; profile identity: `jobs/browser/src/profile.ts:4-33` (E-LB-BLUEY-001).
- Encrypted profile snapshots: `jobs/runner/src/profile-store.ts:17-70`; AES-256-GCM/HKDF/AAD/durable replacement: `jobs/runner/src/crypto-envelope.ts:7-130` (E-LB-BLUEY-002).
- Provider-first adapter selection and execution: `jobs/automation/src/execute.ts:19-97`; Lever strict submit/challenge code: `jobs/automation/src/providers/lever.ts:14-34,93-99,166-209` (E-LB-BLUEY-003).
- Job challenge and takeover: `jobs/automation/src/challenge-handling.ts:6-141`; persisted secret rejection: `server/src/db/jobs.rs:5657-5740` (E-LB-BLUEY-004).
- Answer memory and tailored docs: `jobs/automation/src/answer-memory.ts:1-103`, `jobs/automation/src/documents.ts:58-82`, `server/src/db/jobs.rs:4771-4813,5791-5952` (E-LB-BLUEY-005).
- Receipts/evidence gate: `jobs/automation/src/receipts.ts:100-168,198-204`, `server/src/db/jobs.rs:5103-5135` (E-LB-BLUEY-006).
- Discovery/retries/durable activity: `jobs/workflows/src/discovery.ts:7-222`, `jobs/workflows/src/activities.ts:10-98` (E-LB-BLUEY-007).
- API/account deletion: `server/src/api/jobs.rs:50-194`, `server/src/api/mod.rs:165-189,257-277`, `server/src/api/account.rs:769-888` (E-LB-BLUEY-008).
- Application preparation/eligibility/metering: `server/src/db/jobs.rs:4593-4768` (E-LB-BLUEY-009).
- Desktop meeting/audio/STT: `crates/cue-daemon/src/cloud/meeting_detect.rs:3-97`, `crates/cue-daemon/src/app.rs:3459-3646,7608-7718`, `crates/cue-daemon/src/stt/factory.rs:1-135` (E-LB-BLUEY-010).
- User-approved screen/browser context: `crates/cue-daemon/src/app.rs:3151-3184,14049-14151,14234-14452` (E-LB-BLUEY-010).
- Overlay answers and RAG memory: `crates/cue-core/src/overlay.rs:26-128`, `crates/cue-daemon/src/app.rs:7947-8265`, `crates/cue-daemon/src/rag_indexer.rs:148-228,259-307`, `crates/cue-daemon/src/db/rag.rs:35-93` (E-LB-BLUEY-010).

## Priority recommendations

### P0

1. Do not copy Littlebird code, parsers, UI assets, or source-map content. Require ownership/license/dependency/provenance review before considering any reuse.
2. Preserve Bluey's stronger Electron sandbox, external key boundary, ephemeral authentication-factor handling, and receipt/evidence gate.
3. Add a regression rule/test across Bluey telemetry: authentication codes, credentials, resume contents, form answers, and email bodies must never enter analytics. The Littlebird OTP path demonstrates why this must be explicit.

### P1

1. Add narrowly scoped Gmail/Outlook application-outcome ingestion: minimal OAuth scopes, explicit opt-in, deterministic application matching, provenance, deletion, and no generalized inbox capture.
2. Ensure every Bluey preload API is typed/allowlisted and every sensitive handler validates sender and schema; keep refresh/access tokens out of renderer-accessible storage.
3. Expand provider adapters behind fixtures and durable idempotency tests, retaining provider-specific final-review/confirmation logic.
4. Surface transparent job usage caps/receipts and an opt-in overage ceiling if commercial needs justify it.

### P2

1. Extend Bluey's existing user-approved screen/page context with narrow per-app adapters only where a concrete workflow needs them; do not add continuous OS-wide capture for parity.
2. Add a generalized risk-class consent policy only if Bluey expands beyond job-specific operations.
3. Treat MCP, journals, projects, and a general world model as product-scope decisions; Bluey's meeting/transcription stack already exists and should be evolved in place.

## Clean-room handoff

The next implementation agent should begin from Bluey's contracts, not Littlebird artifacts:

1. Specify an `ApplicationOutcomeConnector` interface keyed by Bluey account, application identity, canonical job, provider message ID, and observed timestamp.
2. Implement Gmail/Outlook adapters with read-only/minimal scopes, server-side token vaulting, deduplicated event ingestion, explicit disconnect/delete, and no email-body telemetry.
3. Correlate outcomes to existing applications through deterministic sender/domain/job/company evidence and require user review for ambiguous matches.
4. Emit immutable provenance into Bluey's existing receipt/evidence model and test retry/idempotency, token revocation, deletion, and duplicate email delivery.
5. Run a separate security review for OAuth storage, IPC, telemetry schemas, and data-retention documentation.

This is the smallest Littlebird-inspired addition that strengthens Bluey's actual product without importing the high-risk, unrelated OS-wide context architecture.

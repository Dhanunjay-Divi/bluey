# Cluely → Bluey gap map

## Rating method

- `Bluey stronger`: implemented Bluey behavior materially exceeds the statically evidenced Cluely behavior.
- `Equivalent`: both implement the essential capability, with different tradeoffs.
- `Partial`: Bluey has relevant foundations but lacks a distinct Cluely workflow or polish.
- `Missing`: no corresponding Bluey meeting-product implementation was found.

Cluely evidence comes from [FEATURES.md](FEATURES.md) and [static-bundle-map.txt](evidence/static-bundle-map.txt). Bluey references are pinned and summarized in [bluey-reference-map.txt](evidence/bluey-reference-map.txt). “Not observed” in Cluely is not evidence about its opaque server.

## Cross-product matrix

| Capability | Rating | Bluey evidence | Comparison / smallest production change |
|---|---|---|---|
| Live system-audio capture | `Bluey stronger` | `crates/cue-daemon/src/audio/system_capture.rs:22-58,163-225` | Both use a native helper. Bluey has an explicit supervisor/restart design; keep it and add only user-visible health if absent. |
| Voice activity detection | `Bluey stronger` | `crates/cue-daemon/src/audio/vad.rs:16-74,87-177` | Cluely ships Silero/ONNX; Bluey uses adaptive RMS + WebRTC and avoids a large model/runtime. Benchmark accuracy before considering an optional neural gate. |
| Speech-to-text resilience/locality | `Bluey stronger` | `crates/cue-daemon/src/stt/router.rs:15-117` | Cluely's observed path is cloud transcription with bounded concurrency; Bluey has ordered failover and can include local Whisper. |
| Live answer copilot | `Bluey stronger` | `crates/cue-daemon/src/llm/answer.rs:6-34`; `crates/cue-daemon/src/rag_indexer.rs:114-229` | Both answer from live context. Bluey adds local transcript/artifact RAG and current/global query. Preserve; improve compact overlay UX independently. |
| Screen context | `Bluey stronger` | `crates/cue-daemon/src/app.rs:13909-14093,14164-14220` | Cluely captures a full display screenshot. Bluey prefers semantic page/accessibility text and falls back to screenshots on macOS/Windows, reducing privacy/cost. |
| Overlay capture exclusion | `Equivalent` | `crates/cue-dashboard/src/macos.rs:41-71` | Cluely uses Electron content protection; Bluey uses `NSWindowSharingNone`. Keep capture exclusion, avoid “undetectable” guarantees, add a compatibility test matrix. |
| Transcript history/search | `Bluey stronger` | `crates/cue-dashboard/ui/src/routes/Search.tsx:47-102`; `crates/cue-daemon/src/rag_indexer.rs:191-229` | Cluely has cloud session list/search/delete/resume; Bluey has local indexed search, deletion and current/global retrieval. |
| Structured recaps | `Bluey stronger` | `crates/cue-daemon/src/llm/recap.rs:6-77` | Cluely has session post-processing/summaries; Bluey's structured recap implementation is explicit and local-pipeline integrated. |
| Named modes and preset library | `Partial` | `crates/cue-daemon/src/rag_indexer.rs:148-229`; `crates/cue-daemon/src/llm/answer.rs:6-34`; `jobs/automation/src/interview-prep.ts:247-356` | Bluey can attach context and generate job-specific prep, but lacks Cluely's named reusable mode/prompt/file library. Add a typed `copilot_mode` model, versioned prompt, attachments, ordering and active selection. |
| Google Calendar meeting start/attendee briefs | `Partial` | `server/src/db/jobs.rs:7319-7324`; `jobs/automation/src/receipts.ts:176-201` | Bluey integrates calendar evidence for job outcomes but no equivalent desktop upcoming-meeting/people-brief workflow was found. Add least-scope calendar read, explicit meeting selection and revocation; reuse existing provider identity/event model. |
| Onboarding and OS permissions | `Partial` | `crates/cue-dashboard/ui/src/pages/Onboarding.tsx:33-183` | Bluey has browser sign-in onboarding; Cluely has a more explicit mic/screen/accessibility teaching sequence. Add capability-by-capability checks, why-needed copy, skip/degraded states and recheck. |
| Authentication token boundary | `Partial` | `crates/cue-daemon/src/secrets/mod.rs:1-26`; `crates/cue-cloud-client/src/tokens.rs:1-6,72-123`; onboarding refs above | Bluey uses OS keychain for provider secrets and avoids Cluely's page-global token extraction, but Bluey account tokens default to a private local profile while OS secure storage is opt-in. Move long-lived account tokens to OS secure storage by default; never copy `_globalGetToken`. |
| Settings/account deletion | `Bluey stronger` | `crates/cue-dashboard/ui/src/pages/Settings.tsx:116-124,135-223`; `crates/cue-daemon/src/cloud/sync.rs:1128-1190` | Bluey has confirmed deletion UI and enumerates scope; no Cluely app-specific delete control was evidenced. |
| Billing/usage controls | `Bluey stronger` | `server/src/api/mod.rs:185-186,258-271`; `server/src/billing/policy.rs:4-46`; `server/src/billing/topup.rs:103-174` | Cluely has clear free/Pro/Pro Plus UI; Bluey adds provider policy, billing-risk restriction and controlled auto-reload. Borrow only clearer plan presentation. |
| Telemetry privacy | `Bluey stronger` | `crates/cue-daemon/src/cloud/sync.rs:477-620,1128-1190`; Settings deletion refs | Cluely statically identifies users and can capture console exceptions. Bluey has explicit audit/retention primitives. Add a user-facing diagnostic/telemetry disclosure if absent; do not adopt broad console capture. |
| Local/cloud browser execution | `Bluey stronger` | `jobs/browser/src/browser-network.ts:18-53`; `jobs/browser/src/profile.ts:4-33` | No Cluely automation browser was observed. Bluey has guarded Playwright contexts and identity-scoped keys. |
| Browser profile/multi-account isolation | `Bluey stronger` | `jobs/browser/src/profile.ts:4-33`; `jobs/runner/src/profile-store.ts:17-75` | No Cluely equivalent was observed. Bluey encrypts sealed identity-scoped profiles and removes active plaintext. |
| Job discovery/ranking | `Bluey stronger` | `jobs/workflows/src/discovery-runtime.ts:112-320`; `server/src/db/jobs.rs:3262-3430` | No Cluely job-discovery feature was observed. Keep Bluey's eligibility and ranking; no Cluely work needed. |
| Resume tailoring/diff/export | `Bluey stronger` | `jobs/automation/src/documents.ts:43-84,410-466`; `jobs/automation/src/interview-prep.ts:88-174` | Cluely uploads generic context files but has no evidenced resume pipeline. Bluey builds deterministic, hashed job documents. |
| Reusable answer memory | `Bluey stronger` | `jobs/automation/src/answer-memory.ts:35-73,89-102`; `jobs/automation/src/form-intelligence.ts:78-152` | No confirmed/scope-ranked Cluely answer memory was observed. Keep Bluey's approval and company/track/account precedence. |
| ATS adapters and fallback | `Bluey stronger` | `jobs/automation/src/execute.ts:37-97` | No Cluely ATS path was observed. Bluey has provider-first adapters and standard/semantic fallback. |
| CAPTCHA/2FA/assessment intervention | `Bluey stronger` | `jobs/automation/src/challenge-handling.ts:55-140` | No Cluely job takeover flow was observed. Bluey preserves the page, correlates owned inbox codes and resumes after user action. |
| Durable queue/retry/crash safety | `Bluey stronger` | `jobs/runner/src/leased-run.ts:19-90`; `jobs/workflows/src/workflows.ts:10-119`; `server/src/db/jobs.rs:2199-2550,4087-4230` | Cluely has chat reconnect/timers but no evidenced durable job lease. Bluey separates reversible retries and irreversible single attempts, with `side_effect_unknown`. |
| Duplicate prevention | `Bluey stronger` | `server/src/db/jobs.rs:4087-4230`; `jobs/automation/src/receipts.ts:100-173` | No Cluely application fingerprint/reservation was observed. Bluey reserves attempts and binds evidence to deterministic run/application IDs. |
| Submission receipts/screenshots/evidence | `Bluey stronger` | `jobs/automation/src/receipts.ts:100-201` | Cluely screenshots are chat context, not submission proof. Bluey requires real confirmation evidence and stores document/provider evidence. |
| Email/calendar outcome tracking | `Partial` | `jobs/automation/src/receipts.ts:93,176-201`; `jobs/portal/src/views/SettingsView.tsx:213-233,284-291`; `server/src/db/jobs.rs:7319-7324` | Cluely has a functioning client-side meeting calendar surface. Bluey models provider evidence and application binding, but the Jobs portal explicitly marks provider authorization beta/not active. Finish one least-scope provider end to end before claiming parity. |
| IPC/origin hardening | `Bluey stronger` | `jobs/browser/src/browser-network.ts:18-53`; Tauri command boundaries throughout dashboard/daemon | Cluely's generic preload and global Origin rewrite are avoidable risk. Keep typed commands and destination policy; do not adapt those patterns. |

## Recommended implementation priorities

### P0 — preserve Bluey's safety lead

- Add regression tests ensuring renderer-facing commands remain typed/capability-specific and no global Origin rewrite is introduced. Base the destination rules on `jobs/browser/src/browser-network.ts:18-53`.
- Make meeting-capture upload boundaries user-visible: audio, screenshot/page text, transcript, attachments and diagnostics should each have explicit state and revocation. Tie deletion to the existing Settings/cloud retention path (`crates/cue-dashboard/ui/src/pages/Settings.tsx:135-223`; `crates/cue-daemon/src/cloud/sync.rs:1128-1190`).
- Keep job submit invariants unchanged: irreversible work remains single-attempt, `side_effect_unknown` blocks blind retry, and submitted status requires evidence (`jobs/workflows/src/workflows.ts:10-119`; `jobs/automation/src/receipts.ts:100-201`).

### P1 — highest product leverage

- Build reusable **Copilot Modes**: account-scoped name, versioned system instruction, ordered attachment IDs, optional job/application linkage, active/default flag and immutable audit metadata. Reuse Bluey's artifact/RAG ingestion, not Cluely code (`crates/cue-daemon/src/rag_indexer.rs:148-229`; `jobs/automation/src/interview-prep.ts:247-356`). Ship interview, recruiting, sales, lecture and generic meeting presets as Bluey-authored prompts.
- Add a least-privilege **meeting calendar surface**: provider connection, next meetings, explicit “start with this meeting,” attendee/context brief, disconnect and data deletion. Reuse the existing Google/Outlook provider-event model (`server/src/db/jobs.rs:7319-7324`) and never silently join/open a link.
- Expand onboarding into capability cards for microphone, system audio, screen/accessibility context, overlay exclusion and local/cloud processing. Each card needs check/retry/skip plus a precise privacy consequence.

### P2 — polish after instrumentation

- Add an audio-device test, live input meter, selected-language preview and supervised-helper health to Bluey's settings.
- Add a compact recent-people/recent-meetings view generated from user-owned local meeting records, with retention controls.
- Benchmark optional neural VAD only if measured missed-speech/false-positive data beats the existing adaptive RMS + WebRTC pipeline enough to justify model/runtime size.
- Publish an accurate capture-exclusion compatibility matrix; never promise universal invisibility.

## Reuse and provenance decision

**Do not copy any extracted code, prompt, asset, model packaging, UI text, or template.** The DMG is proprietary, minified, and no source license or dependency provenance was supplied. The audit retains only hashes, metadata, behavior summaries and narrow identifiers needed for interoperability comparison.

- **Adapt clean-room concepts:** named modes, clear permission onboarding, compact meeting/session navigation, audio test UX, explicit plan comparison, bounded concurrency, and capture-exclusion testing.
- **Reuse Bluey foundations:** RAG/artifacts, keychain, supervised audio, provider failover, calendar provider events, billing policy, deletion/retention, job leases and evidence bundles.
- **Reject:** generic channel IPC, global Origin rewriting, page-global auth token extraction, broad helper entitlements, automatic move/login-item enablement, opaque console telemetry, and “undetectable” product guarantees.

## Concrete handoff

The next implementation agent should begin with one design-only slice: define `CopilotMode { id, account_id, name, instruction, attachment_ids, preset_key?, active, version, created_at, updated_at }`, authorize every mutation by account, ingest attachments through the existing artifact/RAG path, and add create/edit/reorder/select/delete UI with a migration-free empty state. Tests must cover tenant isolation, attachment deletion, prompt version audit, size/type limits, offline behavior, and current-session pinning. A second isolated slice can add calendar meeting selection using existing provider events. Neither slice should alter job-submit retry/evidence invariants.

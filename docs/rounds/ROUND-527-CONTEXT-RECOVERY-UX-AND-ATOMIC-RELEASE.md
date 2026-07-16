# Round 527 - Context, Recovery, UX, And Atomic Release

Date: 2026-07-16

Branch: `codex/bluey-interrupted-asks-round519-20260712`

Status: implementation complete; final release and production evidence pending

## Executive Summary

This round consolidates the owner-authorized macOS DMG, Windows EXE, and source
reference research into production Bluey behavior. The result is not a clone of
one product: Bluey combines a terminal-first native copilot, explicit
Littlebird-style foreground work context, meeting suggestions, low-latency
interview coaching, durable local/cloud memory, and a separate evidence-driven
Jobs workflow.

The most important safety rule is preserved throughout: detection and context
availability do not equal permission to record, upload, or submit.

## Evidence-Backed Findings

| Reference strength | Bluey implementation |
|---|---|
| Littlebird foreground context and meeting suggestion | Explicit Context Watch, supported-browser semantic capture, exclusions, bounded local retention, and Start/Ignore/Settings meeting banner |
| Cluely overlay/audio supervision | Native overlay helper supervision, full state rehydration, dual-source audio, VAD, bounded retries, and cancellation |
| LockedIn session continuity | Persisted session memory, answer snapshots, transcript high-water marks, and restart-safe overlay state |
| ParakeetAI activity/audio recovery | Native macOS/Windows audio helpers, local activity/VAD logic, typed STT routing, and honest capability failure |
| Final Round low-latency lifecycle | Immediate thinking cards, streaming updates, final deduplication, and bounded failure/recovery states |
| Jobs references | Factual resume tailoring, answer memory, review gates, ATS adapters, handoff-only policy, evidence receipts, and crash recovery |

Observed source evidence is indexed under
`docs/research/source-reference-audit/`. Current implementation evidence lives
in the files cited below. Runtime-only claims are listed separately and are not
promoted from marketing text or static inference.

## Cross-Application Feature Matrix

| Capability | Bluey status | Evidence |
|---|---|---|
| Foreground work learning | Implemented, explicit | `crates/cue-daemon/src/app.rs`, `crates/cue-core/src/config.rs`, `crates/cue-dashboard/ui/src/pages/Context.tsx` |
| Meeting detection without auto-record | Implemented | `crates/cue-core/src/meeting.rs`, `crates/cue-daemon/src/cloud/meeting_detect.rs` |
| Overlay restart/state recovery | Implemented | `crates/cue-daemon/src/overlay.rs`, `crates/cue-daemon/src/app.rs` |
| Dual-source audio and VAD | Implemented | `native/macos/cue-audio/`, `native/windows/cue-audio/`, `crates/cue-daemon/src/audio/` |
| Local macOS Whisper | Implemented; model required | `native/macos/cue-whisper/`, `crates/cue-daemon/src/stt/whisper/` |
| Local Windows Whisper | Not claimed | `native/windows/cue-whisper/main.c`, release package guards |
| Answer memory and local/cloud RAG | Implemented with bounded queues | `crates/cue-daemon/src/db/rag_queue.rs`, `crates/cue-rag/`, `server/src/db/sync.rs` |
| Jobs discovery/ranking/tailoring | Implemented | `jobs/automation/`, `server/src/api/jobs.rs` |
| ATS adapters and safe handoff | Implemented for supported policies | `jobs/automation/src/providers/`, `jobs/automation/src/policy.ts` |
| Jobs duplicate/receipt controls | Implemented | `jobs/runner/`, `server/src/db/jobs.rs` |
| Jobs crash recovery | Implemented; no auto-resubmit | `jobs/automation/src/recovery.ts`, `jobs/browser/src/local-checkpoint-store.ts`, `jobs/runner/src/run-checkpoint-store.ts` |
| Email/calendar outcome automation | Request-only beta | The portal explicitly says no authorization, reading, sync, or billing begins; production OAuth lifecycle is not claimed |
| Local Bluey Browser distribution | Release-gated | Plan entitlement is masked unless `BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED=1`; this terminal release does not claim a downloadable Jobs browser |
| Billing, deletion, export | Implemented | `server/src/api/billing.rs`, account routes, dashboard/terminal settings |

## Bluey Code References

The ranges below are the exact source locations used for the status calls in
this round:

| Boundary | Current Bluey evidence |
|---|---|
| Context consent and terminal controls | `crates/cue-cli/src/app.rs:429-489,810-838,2140-2185`; `crates/cue-core/src/config.rs:75-205` |
| Context settings and visible history | `crates/cue-dashboard/ui/src/pages/Settings.tsx:404-538,605-627`; `crates/cue-dashboard/ui/src/pages/Context.tsx:1-350` |
| Semantic-first watch, private commit, exclusions, retention, and first-ready card | `crates/cue-daemon/src/app.rs:4309-4370,16719-17032,17129-17157,17389-17655` |
| Meeting evidence and no-auto-start workflow | `crates/cue-daemon/src/cloud/meeting_detect.rs:1-49,98-280,334-463`; `crates/cue-daemon/src/app.rs:3630-3717,4111-4188` |
| Native meeting suggestion UI | `native/macos/cue-overlay/Sources/cue-overlay/main.swift:2021-2132,2168-2235,15398-15657`; `native/windows/cue-overlay/main.c:1182-1355,1939-2050,2122-2210` |
| Generation-fenced overlay recovery and rehydration | `crates/cue-daemon/src/app.rs:3440-3491,3512-3627,3788-3921,19782-19808`; `crates/cue-daemon/src/overlay.rs:67-103,136-179,212-235` |
| Encrypted local Jobs checkpoint and reconciliation | `jobs/browser/src/local-checkpoint-store.ts:26-166,193-412`; `jobs/browser/src/main.ts:209-225,312-321,404-414,626-678,764-879` |
| Encrypted cloud Jobs checkpoint and reconciliation | `jobs/runner/src/run-checkpoint-store.ts:19-188,196-228,242-326`; `jobs/runner/src/server.ts:90-204,329-445,486-506,615-642` |
| Browser distribution entitlement and execution gates | `server/src/api/jobs.rs:248-273,857-866,1205-1214,4079-4091`; `jobs/portal/src/lib/runner-access.ts:1-44`; `jobs/portal/src/views/BrowserView.tsx:142-145` |
| Request-only mailbox beta | `server/src/api/jobs.rs:1876-1894`; `jobs/portal/src/views/SettingsView.tsx:223-245,295-304,443-470` |
| Immutable signed release ordering and client verification | `scripts/publish-bluey-release.sh:45-49,58-107,123-197,208-311`; `crates/cue-cli/src/update.rs:218-356,405-421,459-523` |
| Private account-file default and explicit Keychain opt-in | `crates/cue-cloud-client/src/tokens.rs:1-6,44-179,182-298`; `crates/cue-core/src/app_paths.rs:114-200` |

## P0/P1/P2 Result

### P0 completed

- Persisted dual consent at every cloud processing boundary.
- Owner/account isolation for local and cloud session/context data.
- No automatic recording from meeting detection.
- No fake Windows transcription in a release archive.
- Durable Jobs submission fences, idempotency, and ambiguous-side-effect
  reconciliation.
- Signed immutable release manifest and installer publication order.

### P1 completed

- Semantic-first Context Watch with privacy exclusions and explicit fallback.
- Generation-fenced overlay and audio/STT recovery.
- Encrypted Jobs restart checkpoints and safe user resume.
- Terminal parity for context and meeting privacy controls.
- Accessible modal/focus/error behavior across Dashboard and Jobs.
- macOS Intel/universal release coverage.

### P2 completed in this round

- Bounded local RAG ingestion queue and revocation checks.
- Source/sequence-aware transcript deduplication.
- Deterministic package member and placeholder-transcript guards.
- Versioned installer checksum verification and deterministic publisher fixture.

## Pre-Release Verification

- Root warnings-denied Clippy and the full all-target workspace suite passed.
- Server warnings-denied Clippy passed; `414` unit tests and `72` end-to-end
  integration tests passed.
- The daemon/account-store focused rerun passed `31` cloud-client tests,
  `500` daemon tests, and all overlay, audio, RAG, streaming, and Whisper
  integration suites. Hardware- or interactive-only tests stayed explicitly
  ignored.
- The Windows GNU CLI/daemon cross-target warnings-denied Clippy gate passed.
- Dashboard tests passed (`33`), and its TypeScript and production build passed.
- All five Jobs packages passed their tests (`244` total), typechecks, and
  production builds. Schema parity, provenance/license, CI-guard, and privacy
  gates passed.
- macOS audio, overlay, and Whisper production builds passed. Audio argument,
  resampler, overlay-protocol, capture-contract, and Windows MinGW audio
  cross-build gates passed.
- Dashboard and Jobs production dependency audits reported zero
  vulnerabilities.
- Release shell syntax, artifact-scanner self-test, release hygiene, signed
  deterministic publisher fixture, workflow YAML, and all `24` immutable
  action pins passed.
- Rust formatting and `git diff --check` passed.

## Deliberately Gated Follow-On Work

- Context Watch provides explicit foreground supported-browser context, a
  visible learning state, recent observations, and a first-context-ready card.
  It is not advertised as autonomous cross-application surveillance and does
  not yet provide Littlebird-style Projects, Routines, or periodic generated
  work summaries.
- Cross-session semantic memory uses managed embeddings only after cloud
  processing consent. Current-session page context works locally; a bundled
  local embedding model and project/routine scopes remain separate measured
  work.
- Bluey's audio queueing, VAD, retries, and finalization are hardened, but a
  production acoustic echo canceller and Bluetooth/hot-swap matrix are not
  claimed.
- Inbox/calendar integration is request-only until OAuth tokens, revocation,
  ingestion workers, deletion, and live provider canaries exist.
- The local Jobs browser is not exposed by plan entitlement until its own
  versioned package, updater, and physical macOS/Windows canaries exist.

## Reuse And Provenance

Bluey uses behavioral and architectural comparison from the owner-provided
material. Shipped code remains Bluey-maintained clean-room implementation.
Packaged reference binaries, minified bundles, credentials, profiles, and user
data are not included in the repository or release. Any future direct source
reuse still requires file-level provenance and dependency review even when
ownership is established.

## Security And Privacy Findings

- Context capture is explicit, scoped, bounded, and locally private.
- Screenshot fallback remains off until enabled.
- Symlink, hard-link, reparse-point, ownership, and permission checks protect
  page/screenshot staging.
- Cloud consent is rechecked when queued work executes, not only when queued.
- Jobs checkpoints use scope-bound authenticated encryption and never persist
  root/worker credentials.
- Irreversible Jobs ambiguity is terminal and visible, never silently retried.
- OS Keychain access is not part of normal account-token operation.

## Unknowns Requiring Runtime Validation

- Physical Windows launch, overlay, dual-audio, UI Automation context, DPAPI
  recovery, updater, and managed-caption canaries.
- Clean Intel Mac and universal-archive install/update canaries.
- Supported-browser semantic capture under restrictive enterprise policies.
- Live ATS schema changes, CAPTCHA/2FA/assessment handoffs, and provider
  certification accounts.
- Mail/calendar OAuth and production outcome-sync credentials.
- Acoustic echo cancellation, Bluetooth, device hot-swap, and sleep/wake audio
  behavior.
- Production load/latency under the intended tenant and Jobs worker scale.

These are explicit hardware/provider gates; static analysis cannot honestly
convert them into completed runtime evidence.

## Implementation Handoff

The next release operator should:

1. Run the full Rust, server, Dashboard, Jobs, native-helper, release-policy,
   deterministic-package, and secret/provenance gates.
2. Build all four native archives from the exact committed source with a fixed
   `SOURCE_DATE_EPOCH` and embedded Ed25519 public key.
3. Rebuild once and compare hashes.
4. Back up PostgreSQL and the current binaries/static/release tree.
5. Deploy the exact source commit, static assets before HTML, and immutable
   release assets before the signed manifest.
6. Verify both APIs report the exact source commit and all public artifact
   hashes/signatures match.
7. Append exact commits, artifact hashes, backup/rollback locations, service
   status, and live-canary results to this document.

No implementation agent should treat the hardware/provider unknowns above as
already validated.

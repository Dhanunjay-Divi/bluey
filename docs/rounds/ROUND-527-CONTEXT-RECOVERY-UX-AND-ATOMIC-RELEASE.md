# Round 527 - Context, Recovery, UX, And Atomic Release

Date: 2026-07-16

Implementation branch: `codex/bluey-interrupted-asks-round519-20260712`

Deployed source implementation commit:
`7b816343d2bffdca80f7ab21ebf6f00de952b521`

Status: complete; exact source promoted, signed release published, and
production acceptance passed

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
| macOS installer executable and nested-bundle verification | `ops/install/install.sh:557-580` |
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
- Fail-closed ad-hoc signature verification for installed macOS executables and
  the nested file-picker application bundle.

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
- An isolated production-installer smoke downloaded the immutable artifact,
  verified its checksum, installed it without Keychain access, and strictly
  verified all top-level Mach-O signatures plus the nested file-picker bundle.
- Rust formatting and `git diff --check` passed.

## Production Release Evidence

### Exact source and native packages

- Production was built from
  `7b816343d2bffdca80f7ab21ebf6f00de952b521`.
- The local source archive used for upload is
  `/tmp/bluey-0.1.102-7b816343d2bf-source.tar.gz`, with SHA-256
  `e6e970059bd4be593c5c1534f0db6da2ed53636adf89c70f05dc060b263aa353`.
  Its production extraction is
  `/opt/bluey-releases/round527-7b816343d2bf`.
- Native packages were built twice from detached exact source with
  `SOURCE_DATE_EPOCH=1784245775`; the second build was byte-identical.

| Platform | Bytes | SHA-256 |
|---|---:|---|
| macOS arm64 | 21,902,791 | `230e2479985f2bf69b70ffaf6d8e65299a5899922122505189f17485b95983c9` |
| macOS Intel | 23,274,384 | `48ef56c58c44499cd16b18c28bc07fd84783ba7fa2536e825fad96f605b34055` |
| macOS universal | 45,184,619 | `3cecaee5f2ecac635e57dedde86e405b3752c86912507a416e252d18b69b4093` |
| Windows x86-64 | 21,245,777 | `2646cf31870a5e28a92b474c7089f931ac6f7b01c690c42193be6691a7c7a108` |

The signed-manifest sizes matched the exact local publication files. The live
verifier then passed independently for all four immutable platform URLs and
SHA-256 values. The arm64 archive was also unpacked on compatible hardware;
the CLI, daemon, and alternate daemon identities reported `0.1.102`, and the
four overlay/audio helper aliases checked by the verifier were present. The
other three packages remain subject to the physical-platform canaries listed
below.

The immutable installers are:

| Installer | Bytes | SHA-256 |
|---|---:|---|
| `install.sh` | 23,326 | `63e7ed0d4e8af63016a46f6be622d1f93905fa744bddf7a4f69cc46a8517ca36` |
| `install.ps1` | 20,778 | `74de690c5eebf6a03cac6aafb2e00ab96b410fab9db919ecfc55ef1e111481b3` |

Both immutable installer paths and root aliases returned the expected MIME
types. A separate byte comparison confirmed that each root alias exactly
matched its immutable installer; the immutable byte counts and hashes agreed
with the signed manifest and `SHA256SUMS.txt`.
`latest.json` has SHA-256
`b3082b8abcfd9158cd8f7a8df362f32dae3753038938bffa2b2019ce8cc9d8c3`;
its base64 signature file has SHA-256
`8a09cce7674d9990200053b69755c876947a082b19594309be1e490a3208fb73`.
Ed25519 verification passed with the public key. Versioned artifacts,
checksums, installers, and the signature were published before the signed
manifest and root aliases. The owner-authorized release used a direct signing
key file, not Keychain, and did not depend on a GitHub Actions deployment.

### Recovery and atomic deployment

- Fresh database backup:
  `/var/backups/bluey-api/hourly/bluey-postgres-20260716T233544Z.pgdump`
  (`24,037,357` bytes, SHA-256
  `20f704e7e7ca4c91d92ea33165a4d4fa7346a4cb84cbf1fd65a230824e86d50d`).
  Its checksum passed, `pg_restore --list` returned `357` lines containing
  `342` non-comment entries, and the offsite R2 upload completed.
- Rollback snapshot:
  `/var/backups/bluey-api/releases/20260716T233632Z-before-round527-d215db35a0f8`
  (`83` files, `50,950,693` bytes), containing both prior APIs, the Jobs
  environment, non-release web tree, installers, and signed manifest.
- The health-gated atomic swap ran from `2026-07-17T00:01:08Z` through
  `2026-07-17T00:01:11Z`.
- The running main API SHA-256 is
  `4d44ae41f08093223732620f2d768a62574395c45c9d5b306f36d59f4135cea1`;
  the running Jobs API SHA-256 is
  `27f6d7fb581e57163db18a4693867f6d99db541a7519d79484a41f1a545f435a`.
  Both loopback health endpoints report the exact deployed source commit.
- `BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED=0` occurs exactly once in
  the production Jobs environment and is also `0` in the live process. This
  release therefore does not advertise an unavailable browser package.

### Live acceptance

- The exact deployed cloud preflight passed with zero warnings: Postgres and
  pgvector, Redis/Valkey strict mode, object and log storage, offsite backup,
  provider pools, production billing configuration and Bluey branding,
  Turnstile, public health, and signed update files all passed.
- Browser, `bluey-cloud-client/0.1.102`, and `bluey-cli/0.1.102` health
  requests returned `200`, `status=ok`, and the exact deployed commit.
  Turnstile configuration returned `200` with a public site key.
  `/account/me` and `/api/jobs/workspace` returned `401`; the private
  discovery lease route returned `404`; and `/llms.txt` returned `410`.
- All `27` current-build Jobs assets (`3,820,525` bytes) were byte-identical
  to their production counterparts. Older hashed assets remain available for
  already-open tabs. The entry JavaScript SHA-256 is
  `05130d4cdbffc3a615cc2a916e95b82b7821cc3efc91ca1e1e7605dd36b564bc`;
  the entry CSS SHA-256 is
  `c3f87f965f45534181f53158b67cd4703dd89c20d2261ad9a11942c06f863cec`.
  Hashed assets are immutable cached, source maps return `404`, and the
  bundles contain no `sourceMappingURL`. After removing only Cloudflare's
  managed challenge injection, the live Jobs HTML is byte-identical to
  `web/jobs/index.html`.
- All `10` shared public site assets (`826,665` bytes) matched the repository.
  The home, product-explanation, context, overlay, and Jobs application routes
  returned `200`.
- The edge verifier passed: crawler policy, Jobs `noindex`/`no-cache`,
  immutable asset caching, redirect behavior, and protected-route status all
  matched policy. Googlebot received `200`, GPTBot received `403`, direct
  HTTPS origin bypass was blocked, and direct HTTP was redirect-only.
- `bluey-api`, `bluey-jobs-api`, and Caddy were active and enabled with
  `NRestarts=0`. APIs listened only on `127.0.0.1:8080` and
  `127.0.0.1:8081`; no API listener was exposed on a non-loopback address.
  Twenty-three consecutive samples from `2026-07-17T00:06:16Z` through
  `00:17:20Z` kept both APIs on the exact deployed commit with all services
  active and zero restarts. The final `00:17:49Z` scan, more than sixteen
  minutes after deployment began, still found zero warning-or-higher records
  and zero textual warning, error, panic, fatal, failed, or critical matches.
  The host had `27,070,754,816` bytes free (`56%` used).

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

## Release Closure And Residual Handoff

The implementation, deterministic rebuild, backup, atomic deployment, signed
publication, and public acceptance steps for `0.1.102` are complete. Future
work should start from the current fetched `origin/main`, treat the deployed
source commit cited above as the immutable runtime baseline, and preserve the
consent, irreversible-side-effect, and release-ordering boundaries established
in this round.

The next measured work is limited to the explicit hardware/provider gates:
physical Windows and Intel Mac canaries, live ATS/CAPTCHA/2FA certification,
mail/calendar OAuth and ingestion, a separately signed Bluey Browser
distribution, acoustic echo cancellation/device-change coverage, and full
Projects/Routines/generated-summary/local-embedding product work. No
implementation agent should relabel those items complete without the
corresponding runtime evidence.

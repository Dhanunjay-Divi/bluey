# IMPL: Phase 624 - Diagnostic Spine, Account Fences, And Test Isolation

> **Codex preflight:** Load `$bluey-ops` before implementation and verify its
> memory against the current repository state and task-specific docs.

## Scope

**Does:**

- Adds a closed, metadata-only end-to-end diagnostic spine for overlay input,
  daemon dispatch, audio/STT readiness, RAG, model connection/first text,
  persistence, and native first/final render.
- Keeps the answer and audio hot paths non-blocking through bounded `try_send`
  queues, background batching, explicit terminal priority, and dropped-event
  summaries.
- Adds account-scoped, append-only support-diagnostic consent and authenticated
  upload/delete contracts with server schema validation, consent epochs,
  tombstones, bounded retention, and durable cleanup retries.
- Introduces exact credential authority across owner, generation, API origin,
  device, and token pair so delayed Account A work cannot clear, publish, or
  hydrate Account B or refreshed Account A2 state.
- Fences account/session deletion across desktop state, cloud sync, RAG,
  object storage, Jobs data, refresh tokens, and response-loss reconciliation.
- Removes STT transcript, provider-body, token, endpoint, vocabulary, and path
  leakage from errors, `Debug`, and production trace fields.
- Makes local Rust test/build validation disposable, authority-free, and
  self-cleaning so repeated agents do not recreate 7 to 20 GiB per hour.
- Preserves the HuddleMate research evidence for the owner's later product
  review while keeping Bluey's current customer copy truthful: capture
  exclusion is best effort and the product does not promise invisibility or
  zero detection risk.

**Does NOT:**

- Upload transcript, question, answer, prompt, document, screenshot, path, URL,
  cookie, token, or provider response content through support diagnostics.
- Enable remote support uploads without both explicit persisted account-scoped
  user consent and matching active server consent.
- Enable OS Keychain storage or introduce new Keychain prompts.
- Copy or execute HuddleMate code, broaden the Electron surface, or adopt its
  undetectability claims.
- Complete Phase 625's overlay visual redesign, answer-mode UX, or live latency
  optimization.
- Fix the remaining P1 exact abandoned-device-link cleanup contract.
- Merge to `main`, deploy a server, publish a desktop release, or claim physical
  macOS/Windows certification.

## Files Created / Modified

| Area | Files | Purpose |
|------|-------|---------|
| Diagnostics | Daemon diagnostics, app, sync | Closed bus, writer, bundles, purge |
| Correlation | Core IPC/auth/observability; native overlays | Validated IDs and render events |
| Credentials | Cloud tokens/client/types; core config | Exact snapshot, redaction, clear/CAS |
| Runtime | Daemon app/balance/DB/RAG; dashboard | Owner checks and consent/delete UI |
| Server | Support schema; sync/account/object DB/API | Consent, ingest, tombstones, receipts |
| Deletion | Jobs/resume/object/token code; migrations 013, 019-022 | Fence account descendants |
| STT | Core STT; OpenAI, Deepgram, factory, Whisper | Safe errors, traces, and debug |
| Tests | Launcher, smoke scripts, Makefile, CI, runbooks | Disposable workspace and cleanup |
| Pre-commit | Observability pre-commit hook | Isolated opt-in Clippy |
| Web | `web/index.html`, `web/assets/bluey-site.js` | Diagnostic and privacy disclosure |
| Records | FIX-588 through FIX-590; Phase/Round docs; changelog | Scope, evidence, and handoff |

## Build & Test

Confirmed on the current Phase 624 source diff:

```text
Isolated launcher self-test: passed success, failure, SIGTERM, and orphan cleanup
Full root Rust workspace suite through the disposable launcher: passed
Full server Rust suite: passed (839 unit + 80 HTTP integration + auxiliary tests)
Root Rust 1.98 strict Clippy for all targets: passed
Server Rust 1.98 strict Clippy for all targets: passed
Cue Core focused STT suite: passed (13)
Cue Daemon focused STT suite: passed (86)
Dashboard Vitest suite: passed (6 files / 35 tests)
Dashboard TypeScript and production build: passed
Jobs tests: passed (automation 198, browser 100, runner 50, workflows 54, portal 87)
Jobs typechecks and production builds across all five workspaces: passed
macOS overlay arm64 and x86_64 release builds: passed
macOS audio, whisper arm64/x86_64, picker, and embedded behavior gates: passed
Windows overlay protocol/capture/audio tests and MinGW cross-builds: passed
Rust formatting and git diff --check after the documentation draft: passed
Disposable Rust workspace cleanup: passed after every completed run
Hermetic Bluey product smoke: passed
Strengthened observability rerun: passed (DAEMON_PID exited zero; workspace cleaned)
Root workspace `cargo +1.98 build --workspace --release`: passed
Server `cargo +1.98 build --manifest-path server/Cargo.toml --bins --release`: passed
Cue daemon post-fix `cargo +1.98 check -p cue-daemon --release`: passed without warnings
```

The hermetic product smoke proved daemon and owned overlay-protocol startup,
transcript, instructions, context, memory, audio status, routing/cloud
scaffolds, the signed-out provider fence, action items, recap, archive, and
confirmed authenticated shutdown. The strengthened observability rerun proved
trace/request UUID round-trip and minting, server lifecycle logs,
authenticated daemon trace propagation, Phase 3 regressions, explicit
`DAEMON_PID` termination and zero exit status, and cleanup of
`/private/tmp/bluey-tests.j1T1G7`.

The first full workspace release build passed with one local `dead_code`
warning because the marked test-workspace helper itself was still compiled in
release while its caller was debug-only. Gating that helper with
`cfg(any(debug_assertions, test))` removed it from the release build. The exact
debug override ownership test passed again, and the release cue-daemon check
was warning-free. Release-build workspaces
`/private/tmp/bluey-tests.ZKLxrm` and
`/private/tmp/bluey-tests.xpYbGC` were cleaned.

Pending at the time this implementation record was drafted:

```text
Final static release/policy scan after the launcher poison-value correction
Final whole-diff independent review
Packaged/signed exact-artifact smoke and physical macOS/Windows certification
```

These pending gates must be updated with command evidence before the review can
be marked ready to merge or release.

## Deviations from Plan

| Deviation | Rationale |
|-----------|-----------|
| Diagnostics use a closed schema | Key filtering cannot safely govern drifting payload names. |
| Remote sharing needs client and server consent | Environment state alone is not user authority. |
| Terminal events have a bounded queue | Preserve terminal evidence without blocking hot paths. |
| Cargo leaf remains `target` | Preserve development updater/installer protections. |
| Product smoke uses a protocol stub | Native helpers keep separate platform certification. |
| Debug overlay allowance needs a marked root | Keep test discovery possible without weakening release verification. |
| Deletion uses an opaque recovery capability | Reconcile lost responses without restoring identity. |

## Known Follow-ups

- Add an exact, idempotent device-link-attempt abandon endpoint. It must bind a
  random attempt/session identifier and revoke only the abandoned credential;
  never replace this with broad device logout.
- Complete Phase 625 overlay/UI and latency work using diagnostic milestones
  from accepted input through first usable text and native paint. Keep recap,
  storage, and embedding work off the first-text critical path.
- Run the static and final review gates and update this record before merge.
- Keep the launcher authority scrub and opt-in pre-commit Clippy isolation in
  sync when a new provider, API alias, helper, or external service is added.
- Build once, smoke the exact release artifact, and certify physical macOS and
  Windows packages before any customer deployment.
- Validate consent, object cleanup, account deletion, and response-loss
  reconciliation against the production PostgreSQL/object-store topology in an
  approved preproduction environment.

## Review Checklist (for reviewer)

- [x] Diagnostic events have no free-form content field
- [x] Hot-path producers use bounded non-blocking queues
- [x] Server rejects unknown schema fields, labels, and forbidden content keys
- [x] Upload requires current explicit account-scoped client and server consent
- [x] Revocation/deletion fences reserved and in-flight uploads
- [x] Account, API origin, device, credential generation, and token pair are one snapshot
- [x] Delayed clears and refreshes compare the exact captured snapshot
- [x] Account/session/child tombstones prevent stale resurrection
- [x] STT errors, debug values, and traces exclude transcript/provider bodies and secrets
- [x] Test launcher clears ambient external authority and owns cleanup
- [x] Test launcher success/failure/signal/orphan cases pass
- [x] Launcher strips managed tokens, API aliases, FFmpeg, and context-picker overrides
- [x] Debug overlay allowance requires the exact marked owned workspace
- [x] Hermetic product smoke passes and confirms daemon shutdown
- [x] Strengthened observability rerun confirms zero daemon child exit status
- [x] Root workspace and server binary release builds pass in isolated workspaces
- [x] Release cue-daemon check is warning-free after debug/test-only helper gating
- [x] No Keychain behavior or prompt was enabled
- [x] HuddleMate evidence remains preserved without copying or impossible customer claims
- [ ] Final static policy/release scan passes
- [ ] Final whole-diff independent review is complete
- [ ] Exact packaged macOS and Windows artifacts are certified before release

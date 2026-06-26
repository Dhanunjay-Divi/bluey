# Round 055 - Production Readiness Sweep - 2026-06-17

## Goal

Do a broad risk pass before live testing. The focus was not feature polish; it was
checking the places that can hurt a production alpha:

- money path and Square environment isolation
- streaming billing/idempotency behavior
- signed updater trust boundary
- account token storage/profile behavior
- stale daemon cleanup
- RAG/session deletion races
- web account rendering and route behavior
- overlay/native build health
- observability smoke coverage

## Changes Made

Documentation-only correction:

- `docs/SECURITY-HARDENING.md`
- `docs/rounds/ADMIN-USER-FLOW-2026-06-04.md`
- `docs/rounds/END-TO-END-FLOW-AND-LIVE-TEST-GUARD-2026-06-04.md`
- `docs/rounds/END-TO-END-AGENT-CONTEXT-2026-05-25.md`

These docs still described OS keychain token storage as the default. Current
Bluey stores account tokens in the private local account profile by default and
uses OS keychain storage only as a legacy fallback when
`BLUEY_LEGACY_KEYRING_FALLBACK=1`. The docs now match the code and web privacy
copy.

No production code was changed in this sweep.

## Code Paths Rechecked

### Streaming Billing And Idempotency

- OpenAI streaming now requires terminal `[DONE]` and final usage before yielding
  `Done`; truncated streams become upstream errors and are not billed/cached as
  success.
- Anthropic streaming now requires `message_stop` and final usage before yielding
  `Done`.
- Managed desktop SSE path rejects EOF without a billing/final event.
- First-token stall fallback is in place for provider stalls while preserving
  billing/idempotency semantics.

### Square And Credits

- Square webhook verification prefers environment-specific keys.
- Sandbox-signed events are rejected when the active billing ledger is production.
- Usage/billing tests cover production-path cleanup and environment mismatch.

### Account Token/Profile Behavior

- Dashboard cloud clients read the saved account `api_url`, so staging/local
  accounts do not silently hit production.
- Dashboard sign-out clears account-file tokens and also clears the legacy
  keyring store when fallback mode is enabled.
- CLI logout clears tokens while preserving account metadata such as saved API
  URL/workspace/device id.

### Local Daemon Safety

- Stale daemon cleanup validates the recorded PID/path instead of killing an
  arbitrary reused PID or scanning every daemon.

### RAG And Session Deletion

- Live transcript indexing, attachment indexing, session rebuild, and session
  delete all serialize through `rag_index_lock`.
- Deleting the active session stops audio capture and continuous screen capture
  before clearing the meeting, preventing immediate session recreation from late
  capture events.
- Continuous system-audio capture uses the session-start-allowed transcript path,
  so opt-in system audio can still create/update a meeting.

### Web

- Reload pricing CTA points at `/reload`, not `/login`.
- Account usage labels are rendered with text nodes, not raw `innerHTML`.
- Privacy copy correctly says desktop tokens live in the local Bluey account
  profile by default, with legacy keychain fallback.
- Static JS parses with `node --check`.

### Updater

- Update metadata is trusted only after Ed25519 signature verification with the
  embedded public key.
- Missing/tampered signatures reject the update.
- Unsigned installs require the explicit dev escape hatch
  `BLUEY_UPDATE_ALLOW_UNSIGNED=1`.
- Release runbook documents byte-identical static serving for
  `latest.json`/`latest.json.sig` and the current key-rotation limitation.

## Verification

All checks below ran locally on the Mac or local build tree; no GitHub Actions
were used.

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cd server && cargo test
cd crates/cue-dashboard/ui && npm test -- --run
cd crates/cue-dashboard/ui && npm run build
node --check web/assets/bluey-site.js
swift build -c release --package-path native/macos/cue-overlay
swift build -c release --package-path native/macos/cue-picker
swift build -c release --package-path native/macos/cue-whisper
bash scripts/observability-acceptance-smoke.sh
git diff --check
```

Results:

- Workspace Rust tests: 536 passed.
- Server tests: 130 lib tests plus integration suites passed.
- Dashboard Vitest: 15 passed.
- Dashboard production build: passed.
- Native Swift overlay/picker/whisper release builds: passed.
- Observability acceptance smoke: 8/8 assertions passed.
- `bluey-dev.db` remains untracked and untouched.

## Remaining Live Gates

These are not code blockers found in this sweep; they require live services or
manual UX validation.

- Real funded-provider smoke for OpenAI/Anthropic model routing and latency.
- Real Deepgram live mic/system STT verification on macOS with permissions.
- Real Square sandbox/prod checkout webhook smoke against the deployed endpoint.
- Manual visible overlay QA for clickability, pass-through mode, resize/fullscreen
  mode, transcript scroll, and screen-capture exclusion.
- Deploy the latest site/server/release artifacts from local/cloud machines, not
  GitHub Actions, until signed production automation is intentionally enabled.
- Validate backup restore and monitoring/log export on the droplet.

## Verdict

No new production code blockers were found in this sweep. The one mismatch found
was documentation drift around account-token storage, and it was corrected here.

# Round 478 - Reliability, Trust, and Usage Hardening

Date: 2026-07-12
Branch: `codex/bluey-web-ui-parallel-20260704`
Backup task id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The competitive and engineering audit in Round 477 identified stop-ship risks
in session ownership, final transcript delivery, Windows streaming, local RAG
isolation, provider-spend settlement, Auto Reload, and object-storage controls.
This round implements those fixes as one coordinated Mac, Windows, API, billing,
storage, and trust pass before the next signed release.

## Product Decision

Bluey keeps best-effort screen-share privacy as an explicit user control. It is
not positioned as a covert or deceptive interview tool. The durable product
direction is a consent-first live context copilot for engineering conversations
and technical work: screen, audio, documents, current conversation, saved
context, source-backed research, and versioned workbench artifacts.

Provider/model names and internal route labels remain diagnostics. Normal users
see `Auto`, `Quick`, or `Thorough`, evidence/status labels, sources, recovery,
and saved history.

## Runtime And Transcript Correctness

- Audio and transcript work now carry both the immutable meeting/session ID and
  the audio-session ID captured when Listen starts.
- Late STT events from a previous meeting are dropped and logged instead of
  being written into whichever meeting is currently active.
- Mac and Windows send an explicit `answer_current_transcript` intent for live
  transcript answers; typed questions do not accidentally trigger transcript
  draining.
- Answer submission snapshots a transcript high-water mark. A successful answer
  consumes only that prefix, so speech arriving while the provider is answering
  remains available for the next turn.
- Account sign-out, session switch, new/open/delete session, and account change
  invalidate active answer generations and stop active audio work.
- Answer and error persistence is owner/session checked before durable writes.

## Local RAG Isolation

- Local vector chunks have immutable `account_id` and optional `workspace_id`
  ownership.
- Search, insert, deletion, and cleanup paths require a `RagScope`; queries can
  no longer scan another account's rows on a shared computer.
- Legacy unowned rows are quarantined until explicitly claimed by the owning
  migration path.
- Account/workspace switch and scaling tests cover the isolation contract.

## Managed Usage Safety

- LLM/trial usage is reserved atomically in the database before provider
  dispatch.
- Concurrent requests cannot all pass a stale balance/trial check and overspend
  the same allowance.
- Provider execution and settlement continue after a client stream disconnects.
- Actual usage settles independently; unused reservations are refunded and stale
  reservations are reconciled by TTL.
- Provider cooldown and fallback remain bounded so a 429 does not loop through
  providers or repeatedly charge the user.

## Stripe Auto Reload

- Auto Reload creates an unconfirmed PaymentIntent, persists its durable attempt
  and idempotency keys, rechecks the account/attempt, and only then confirms it.
- `payment_intent.succeeded`, terminal failure, cancellation, refund, and dispute
  paths reconcile against the same attempt.
- Success credits exactly once. Reversal revokes exactly once. A reversal before
  success suppresses a late credit.
- Identity mismatches fail closed, and terminal payment failures disable future
  automatic reload until the owner repairs the payment method.

## Object Storage And Audit Controls

- Artifact and session-audit uploads use stable account-scoped object keys.
- Database reservations enforce default total, daily-byte, and object-count
  quotas before an upload URL is issued.
- An object-storage outbox records upload/delete work and supports retry,
  abandonment, and stale-pending cleanup without blocking live requests.
- Session deletion enqueues owned object deletion and prevents tombstoned
  sessions from being resurrected by a later sync.
- The database stores object metadata/indexes, not diagnostic bodies.

## Windows Parity

- The native Windows overlay replaces fixed-size line/JSON/answer buffers with a
  bounded dynamic NDJSON stream (8 MiB maximum event size).
- Fragmented and coalesced messages, Unicode, long answers, multiple cards,
  session payloads, artifacts, and nested fields have portable protocol tests.
- Listen/transcript asks emit the same explicit transcript intent as macOS.
- The full Windows overlay cross-links successfully with the production helper
  source, and the portable protocol tests pass under AddressSanitizer and
  UndefinedBehaviorSanitizer.

## Trust And Settings

- Automatic identity disguise UI and copy were removed from the normal product
  path. Stable helper aliases remain an installer/runtime implementation detail.
- Screen-share privacy is described as best effort rather than guaranteed
  invisibility.
- New local settings begin with cloud sync off. Successful signed-in desktop
  linking enables saved-session sync and records the persisted preference;
  Settings can turn it off later.
- Existing signed-in installs that already had sync enabled preserve that state
  when the new consent field is absent.
- Public privacy/terms copy matches current behavior: temporary raw audio is
  deleted after transcription by default, and submitted content is not used for
  model training.

## Safety Review

- No provider keys, payment secrets, release private key, or customer content
  were added to source or release-facing docs.
- Account deletion, sign-out, and device relinking stop paid compute before the
  local UI can continue listening or answering.
- Billing reservations, balance updates, Stripe credits/reversals, and upload
  quotas are database-backed and idempotent across multiple API processes.
- RAG and object keys are account scoped. Account switches do not migrate local
  history automatically.
- Capture-visible development flags remain forbidden by the release artifact
  scanner.

## Verification

Completed locally on the consolidated worktree:

- `cargo test -p cue-core -p cue-cli`: 166 passed
- `cargo test -p cue-daemon`: 357 passed, 5 intentionally ignored, all daemon
  integration suites passed
- `cargo test -p cue-rag`: 13 passed
- `cargo test --manifest-path server/Cargo.toml --lib`: 317 passed
- `cargo test --manifest-path server/Cargo.toml --test integration_e2e`: 57 passed
- strict Clippy passed for the root and server workspaces
- dashboard unit tests: 15 passed; production UI build passed
- macOS production overlay build passed
- Windows portable protocol tests passed with ASan/UBSan
- Windows overlay cross-link passed with MinGW
- JavaScript syntax, Rust formatting, secret scan, and `git diff --check` passed

## Release Gate

No deployment is claimed in this round. The next round must version the exact
verified commit, build one immutable Mac and real Windows artifact, run cloud
preflight/backup/billing reconciliation, publish a signed manifest without
GitHub Actions, and record live install/API/web smoke evidence.

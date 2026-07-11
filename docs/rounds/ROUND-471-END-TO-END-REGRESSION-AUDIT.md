# Round 471 - End-to-End Regression Audit

Date: 2026-07-10

Backup task id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Goal

Review the complete shared Bluey worktree with fresh eyes after the recent
overlay, STT, session sync, auth, trial, routing, provider, web, billing, and
installer changes. Fix confirmed regressions, add coverage for the failure
shapes, and leave an honest record of what was and was not exercised.

This round was local-only. It did not deploy, publish a release, run GitHub
Actions, commit, or push.

## Confirmed Defects Fixed

### Trial creation could silently lose ownership

`attach_grant_account` previously treated a zero-row update as success. A
missing, denied, or already-owned trial grant could therefore leave a newly
created account detached from the abuse ledger.

- The update is now idempotent only for the same account.
- A missing, denied, or differently owned grant fails closed.
- New-account setup cleanup releases the reservation and removes the incomplete
  account if grant attachment or legal-acceptance recording fails.
- Regression coverage proves same-account replay and cross-account rejection.

### Pre-output provider failures did not reliably continue fallback

Streaming route selection now treats pre-output provider errors and empty
terminal streams as candidate failures. Bluey continues through healthy
provider/model/key candidates before exposing a capacity or upstream error.
Provider cooldown and retry metadata remain preserved for support diagnostics.

### A provider could stall forever after starting an answer

Bluey already bounded route connection and first useful output, but later
stream chunks had no idle deadline. A provider that emitted one delta and then
stopped could leave the overlay spinning indefinitely.

Lane-aware idle deadlines now guard every post-prefetch stream event:

| Lane | Default idle deadline |
| --- | ---: |
| instant | 8 seconds |
| balanced | 15 seconds |
| vision | 25 seconds |
| deep/thinking | 40 seconds |

All deadlines have environment overrides. Timeout records provider, model,
lane, request/session references, partial character count, and whether any
delta reached the user. The SSE stream closes with a specific retryable reason
instead of hanging.

### STT account checks were both too frequent and incomplete

The STT websocket queried the account database for every client audio frame and
every provider frame. That multiplied database traffic with audio frame rate,
while a quiet open socket could avoid the check entirely.

- Account liveness now runs on a two-second periodic monitor.
- Restricted and expired accounts close promptly.
- Missing/deleted accounts are distinguished from existing inactive accounts.
- Restricted/expired accounts settle actual audible usage and release unused
  reservations; deleted accounts stop without attempting to write to a row that
  no longer exists.
- Transient liveness-check errors are logged separately instead of being
  mislabeled as account deletion.

### Abandoned STT reservations could remain forever

If the desktop reserved trial time or credits and died before claiming or
settling the relay, the hold had no reconciliation path.

- Expired unclaimed relays are now settled at zero and fully released.
- Claimed relays become stale only after their maximum runtime plus a 60-second
  grace period.
- Reconciliation is idempotent under concurrent account refresh/session start.
- New STT reservations reconcile stale holds first.
- `/account/me` also reconciles stale holds so normal overlay/dashboard polling
  repairs the displayed balance or trial time without another Listen attempt.
- With no trustworthy post-crash audio duration, Bluey refunds rather than
  inventing a charge.

### Transcript lifecycle could resend or misalign text

- Removing a late duplicate/partial transcript now moves the consumed cursor
  with the underlying segment list.
- Reopening historical conversations marks restored transcript context as
  consumed so Enter does not resend old speech.
- Transcript clear and final-tail flush paths now preserve the last received
  audio before send while preventing already-sent text from remaining live.
- The audio idle countdown no longer resets its own state before the final
  transcript-settle decision.

### Failed visible answers were not durable enough for diagnosis

Provider errors, dropped streams, and partial visible output are now persisted
with stable request/session references before sync. A support audit can compare
what the UI showed with routing, billing, context, transcript, and provider
events instead of relying on transient overlay state.

### Cloud session deletion could become unretryable

Signed-in deletion previously removed the local session first. If the cloud
tombstone failed, the UI asked the user to retry even though the local retry
handle was already gone.

- Signed-in deletion confirms the account-scoped cloud tombstone first.
- Local files, RAG state, and session records are removed only after cloud
  confirmation.
- A failed cloud request leaves the local session intact for retry.
- Signed-out local-only deletion remains local.

### Cloud tombstones could affect unowned legacy local sessions

Cloud deletion now applies only when the local session owner exactly matches
the current signed-in account. Unowned legacy cache rows are not silently
claimed or deleted by whichever account signs in next.

### Desktop login could announce a half-linked account

The desktop previously persisted cloud tokens and reported success even when
device registration failed. That produced a signed-in overlay whose computer
never appeared in My Computers and could be signed out by the next status poll.

- Tokens are staged in memory first.
- A stable device id is required.
- Device registration and an active-status read must succeed.
- Only then are account/tokens persisted and success shown.
- A failed link clears staged tokens and leaves the prior local account file
  unchanged.

### Legacy companion symlink cleanup missed relative targets

Both macOS installer paths now normalize relative `Terminal` symlink targets
before deciding ownership. They remove only links resolving inside Bluey's
install roots and do not delete a real system/Pinky Terminal binary.

## Validation

Passed locally:

- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings`
- server library tests excluding sandbox-bound mail mocks: 280 passed
- STT accounting tests: 8 passed, including stale paid reservation recovery
- daemon library: 335 passed before 12 loopback-only websocket mocks were
  denied by the managed sandbox; five hardware/Keychain tests remained ignored
- daemon app tests: 150 passed in the focused run
- daemon cloud sync tests: 12 passed in the focused run
- core/CLI/RAG/router/stealth package tests passed in the broad run
- dashboard Vitest suite: 15 passed
- dashboard TypeScript/Vite production build passed
- macOS overlay `swiftc -parse` passed
- Windows overlay MinGW C syntax check passed
- all changed shell scripts passed `bash -n`
- `node --check web/assets/bluey-site.js`
- PostCSS parsed the full site stylesheet
- all 191 HTML ids are unique
- every direct JavaScript DOM id lookup exists
- every extension-bearing local HTML asset reference exists
- release hygiene scan passed (dev-only references were confined to explicit
  local smoke/developer docs)
- `git diff --check`

The daemon websocket failures were all `PermissionDenied` while binding a local
mock listener. Parser, protocol, auth/quota mapping, VAD, transcript, router,
and production code compiled and their non-bind tests passed. The server mail
mock tests have the same sandbox limitation.

## Known Boundaries

1. No live provider canary, signed artifact, browser production smoke, or
   deployment was run because this round explicitly forbids deployment.
2. PowerShell is not installed in this macOS workspace, so Windows `.ps1`
   scripts were reviewed and Windows C compiled, but the PowerShell parser did
   not run locally.
3. The Windows native offline Whisper helper remains the previously documented
   stub. Production Windows transcription uses managed streaming STT; do not
   claim a real offline Windows Whisper fallback yet.
4. `cargo-audit` is not installed in this environment. Strict compilation,
   tests, release hygiene, and secret scans passed, but the release pipeline
   should still run its dependency advisory scan before a signed promotion.
5. `cue-daemon/src/app.rs` and `server/src/api/router.rs` remain large ownership
   surfaces. They are covered heavily, but future refactors should split by
   transcript lifecycle, answer orchestration, artifact persistence, and
   provider stream state in isolated rounds rather than mixing that churn into
   a release fix.

## Files Directly Repaired In This Audit

- `crates/cue-daemon/src/app.rs`
- `crates/cue-daemon/src/cloud/sync.rs`
- `ops/install/install.sh`
- `scripts/install.sh`
- `server/src/api/account.rs`
- `server/src/api/auth_routes.rs`
- `server/src/api/router.rs`
- `server/src/api/stt.rs`
- `server/src/db/stt_accounting.rs`
- `server/src/db/trial_abuse.rs`
- `server/tests/integration_e2e.rs`

## Release Gate

Before the owner requests a signed deployment:

1. Run the loopback websocket/mail suites in normal CI or an unrestricted
   preproduction host.
2. Run macOS and Windows install/update/login/listen/answer/session-sync smoke.
3. Exercise one abandoned STT reservation and verify account polling restores
   the exact balance/trial time.
4. Exercise one deliberately stalled provider stream and verify the partial
   answer, audit event, retry reason, and idempotency state.
5. Verify account switch/delete stops audio and hides prior-account history.
6. Promote one immutable signed artifact only after those gates pass.

# Round 524 - Bluey Mainline Convergence And Signed Release

Date: 2026-07-16

Branch: `codex/bluey-interrupted-asks-round519-20260712`

Backup task: `019e133e-d92a-7830-8df0-3a050a4e22f6`

Status: implementation and pre-release verification complete; deployment
evidence is appended only after exact-commit promotion

## Trigger

The owner asked Bluey to finish every reviewed non-Sashreek branch and
interrupted ask, merge the useful work into the canonical mainline, and perform
one signed manual deployment without GitHub Actions or Keychain prompts.

## Mainline Reconciliation

The release branch is a strict descendant of `origin/main`; main has no unique
commit that is absent from the branch. Rounds 520-523 audited the remaining
remote branches and preserved worktrees by patch identity and behavior.

The following Sashreek-owned refs remain explicitly excluded:

- `origin/agent/agent-bridge`
- `origin/agent/agent-bridge-fixes`
- `origin/agent/meeting-frontend`
- `origin/agent/parakeet-stt`
- `origin/meeting-main`

No remaining non-Sashreek branch is safe or useful to merge wholesale:

- `codex/bluey-branch-reconciliation-20260712` and
  `codex/bluey-jobs-20260710` have no patch-unique launch work.
- `codex/bluey-stream-attachments-20260704` contains historical behavior that
  is represented by newer implementations; merging it would restore obsolete
  trial, provider, device, and release code.
- The preserved Jobs/Coach worktree contains incomplete desktop handoff,
  workspace, IPC, and Windows dependencies. Reviewed Jobs behavior is already
  on main, while the incomplete dependency chain remains deliberately hidden.

## Fixed In This Release

### Account And Credential Isolation

- Account-token updates use generation-aware compare-and-swap writes.
- A delayed refresh cannot replace a newer login or restore credentials after
  logout.
- Local profile and token identity must agree; malformed profiles fail closed.
- OS Keychain integration remains explicit opt-in only.

### Local And Cloud Session Ownership

- Local sessions carry `owner_account_id` and all dashboard history, active
  session, archive, title, and delete operations are owner-scoped.
- Account change or revocation clears the previous account's visible session
  and stops active paid work.
- SQLite and PostgreSQL sync reject duplicate IDs in a batch, cross-account
  reparenting, missing parents, and parent mismatches.
- Stable IDs and immediate transactions keep retries idempotent.

### Audio And Transcript Finalization

- Meeting end invalidates racing audio-start generations.
- Stop waits for the bounded STT tail before persisting and settling the
  session, preventing the final spoken words from remaining unsent.
- Deepgram session credentials are carried in a request header, not a URL query
  parameter.

### Answer Safety And Continuity

- Trusted internal envelopes are validated structurally. Untrusted user text is
  never trusted merely because it starts with `Question:`.
- Adversarial multi-paragraph disclosure requests are blocked across server and
  daemon answer paths.
- Safe partial output survives provider timeout, upstream disconnect, and
  terminal-settlement failures instead of disappearing.
- Coding prompts retain approach, complete code, explanation, complexity, edge
  cases, and full in-place follow-up replacement behavior.
- System-design follow-ups continue the existing artifact rather than losing
  the prior design.

### Signed-Out Recovery

- macOS signed-out status and balance badges are real click targets that open
  the existing sign-in flow.
- The click actions are disabled immediately after authentication so signed-in
  header dragging remains unchanged.

### Windows Release Integrity

- The native overlay protocol fixture now writes UTF-8 test bytes without
  implementation-defined signed-character casts, so strict MSVC `/WX` builds
  remain portable.
- The top-level Windows packager no longer suppresses native helper output or
  ignores failed child-process exit codes.
- Packaging now verifies every required overlay, audio, speech, daemon, and
  stable process-identity artifact before creating the release archive.
- The GitHub release workflow also fails closed when a required Windows helper
  is absent instead of publishing a partial package.

## Verification Before Commit

- Full root workspace tests passed.
- Full server all-target suite passed, including 71 integration tests.
- Root and server warnings-denied Clippy passed.
- Dashboard unit tests passed: 24 tests across 2 files.
- Dashboard TypeScript and production Vite build passed.
- macOS overlay parsed successfully with `swiftc`.
- Release hygiene scan passed all clean and rejection self-tests.
- Shell syntax, Rust formatting, whitespace, and `git diff --check` passed.
- A clean Windows builder completed the full `scripts/build-windows.ps1` path:
  Rust release binaries, overlay protocol tests, native overlay, audio driver,
  local speech helper, alias verification, and ZIP packaging.
- The release secret/dev-flag scan is rerun against the staged artifacts before
  publish.

## Deliberate Boundaries

- Raw mic/system audio bytes are not retained after transcription by default.
  The audit bundle records technical/session evidence but does not provide
  source-audio QA replay.
- Jobs-to-Coach handoff, local Workspaces, unattended workers, mailbox/calendar
  OAuth, and ATS certification remain dependency-gated product work.
- PostgreSQL/pgvector is the cloud source of truth, but scalable
  tenant-filtered database KNN remains a separate measured rollout gate.
- The signed Windows package receives static/package checks in this round. A
  real Windows launch, audio, update, and overlay canary is still required
  before broad public rollout.

## Deployment Evidence

This section is intentionally incomplete until the exact tested commit is
fast-forwarded to `main`, built once, signed, promoted manually, and verified
live. Do not infer deployment from this document's existence.

- Source commit: pending
- Mainline commit: pending
- Server release ID: pending
- Native version: `0.1.101`
- macOS artifact SHA-256: pending
- Windows artifact SHA-256: pending
- Production API SHA-256: pending
- Backup and rollback identity: pending
- Live acceptance: pending

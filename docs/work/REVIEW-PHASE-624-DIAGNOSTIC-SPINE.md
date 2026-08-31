# REVIEW: Phase 624 - Diagnostic Spine, Account Fences, And Test Isolation

> **Codex preflight:** Load `$bluey-ops` before review and verify its memory
> against the current repository state and commit range.

**Commit range:** `83f15263..WORKTREE`
**Reviewer:** Codex self-review; independent STT/privacy review complete; final review pending
**Date:** 2026-08-31

## Per-Task Review

### FIX-588 - Account-bound diagnostics and deletion fences

| Field | Value |
|-------|-------|
| Files | Cloud client, daemon, dashboard, server consent/sync/account/object code, migrations |
| Verdict | 🟢 accept for completed source review |

**Findings:**

- Credential authority includes owner, generation, normalized API origin,
  device, and the exact token pair. Custom `Debug` output redacts every secret
  and direct identity field.
- Background 401, balance, listen, sync, audio/STT, answer, and hydration paths
  revalidate the captured snapshot. A late A1 or Account A response cannot
  clear or publish Account A2/Account B state.
- Support diagnostics use schema version 2 closed events and labels. There is
  no transcript/question/answer/prompt/audio/screenshot/path/URL/token payload
  field.
- Ordinary and terminal queues are bounded and use `try_send`; writer I/O is
  off the answer/audio producer path. Queue loss is explicitly summarized.
- Local sharing state is account-scoped, the server retains append-only consent
  receipts, and upload requires both current local and server consent. A later
  grant cannot revive an object fenced by an earlier consent epoch.
- Session and account deletion publish tombstones/fences before cleanup,
  prevent reserved-object finalization, and retain opaque response-loss
  reconciliation authority after customer rows are removed.
- Independent review reported no P0 in the completed account/privacy source.
- One P1 remains: a remotely completed but locally rejected browser/device
  sign-in needs exact attempt-scoped abandonment. It is a follow-up, not a
  reason to add broad device logout.

### FIX-589 - Isolated test workspaces

| Field | Value |
|-------|-------|
| Files | Test launcher, smoke scripts, Makefile, CI, runbooks, templates, CLI/daemon test gates |
| Verdict | 🟢 accept for completed launcher review |

**Findings:**

- Each local Rust invocation owns one marked `/tmp/bluey-tests.*` workspace and
  binds Cargo, SQLite, app data, config, runtime, logs, and temporary files to
  it.
- The launcher removes ambient PostgreSQL and external-service authority,
  disables credential stores and UI side effects, and keeps release/package
  artifacts explicitly outside its cleanup scope.
- The explicit scrub covers `BLUEY_ACCESS_TOKEN`, API base/URL/host aliases,
  `FFMPEG_PATH`, `BLUEY_FFMPEG_PATH`, and `BLUEY_CONTEXT_PICKER_APP`. Poison
  assertions specifically cover the managed token, API base and host, generic
  FFmpeg path, and context-picker override.
- Cleanup validates basename plus ownership marker and runs after normal exit,
  command failure, `SIGHUP`, `SIGINT`, and `SIGTERM`.
- The child process group receives bounded termination before the workspace is
  removed. The self-test proves an ignoring descendant does not remain alive.
- The short default path fixes macOS Unix-socket failures observed with longer
  temporary roots.
- Product smoke no longer writes native build output or kills globally named
  Bluey processes.
- The initial owned overlay peer exposed a debug discovery/verifier mismatch.
  The final debug-only allowance requires the exact canonical marked
  `bluey-tests.*` root, rejects missing-marker and outside helpers, and is not
  compiled into release behavior. Its focused ownership test passes.
- Opt-in pre-commit Clippy enters the same disposable launcher instead of
  writing a shared repository target.

### FIX-590 - STT provider log privacy

| Field | Value |
|-------|-------|
| Files | Cue Core STT; daemon OpenAI, Deepgram, factory, and local Whisper modules |
| Verdict | 🟢 accept |

**Findings:**

- Errors retain typed internal variants but `Display`, `Debug`, and trace fields
  expose only closed categories.
- Transcript, word, agreement, vocabulary, provider config, endpoint, frame,
  provider-body, and helper-path content is absent from diagnostic formatting.
- OpenAI and Deepgram provider errors no longer retain raw remote messages or
  payloads, and masked key suffix logging was removed entirely.
- Sentinel tests cover transcript text, keys, tokens, URLs, paths, vocabulary,
  malformed JSON, provider errors, and helper events.
- Independent STT privacy review found no remaining P0 or P1 disclosure path in
  the five reviewed files.

### Product truth and research boundary

| Field | Value |
|-------|-------|
| Files | Public web privacy copy and retained local research evidence |
| Verdict | 🟢 accept |

**Findings:**

- HuddleMate research evidence is preserved for the owner's final product pass;
  no competitor application code was copied or executed by this batch.
- Bluey's current public language remains truthful. Capture exclusion is a
  best-effort privacy boundary and does not promise zero detection risk,
  invisibility, or untraceability.
- Support diagnostics are metadata-only. Cloud session transcript sync remains
  a distinct, user-controlled product path rather than being relabeled as
  diagnostics.

## Cross-Task Findings

- The same exact account/credential authority now fences diagnostics, answer
  work, audio/STT, balance, sync, RAG, deletion, and UI publication. This is
  stronger than independently checking only an account email or access token.
- Diagnostics are suitable for latency attribution without adding synchronous
  filesystem or network writes before first text.
- Revocation and deletion are treated as distributed state transitions, not
  best-effort cleanup after acknowledging success.
- The disposable test launcher addresses both disk growth and accidental test
  use of live external authority.
- No merge, deployment, release publication, production migration, or physical
  package certification is represented by this source review.

## Build & Test Verification

```text
Launcher success/failure/SIGTERM/orphan self-test: passed
Full root Rust suite in isolated workspace: passed
Full server Rust suite: passed (839 unit + 80 HTTP integration + auxiliary)
Root and server Rust 1.98 strict Clippy for all targets: passed
Cue Core STT focused tests: passed (13)
Cue Daemon STT focused tests: passed (86)
Dashboard tests: passed (6 files / 35 tests)
Dashboard TypeScript and production build: passed
Jobs tests: passed (489 across automation/browser/runner/workflows/portal)
Jobs typechecks and production builds: passed across all five workspaces
macOS arm64/x86_64 overlay and native helper source/build gates: passed
Windows overlay protocol/capture/audio and MinGW source/build gates: passed
Disposable workspace cleanup: passed for completed launcher runs
Hermetic product smoke: passed through confirmed daemon shutdown
Strengthened observability rerun: passed UUID/trace/Phase 3 checks
Observability daemon child: DAEMON_PID terminated and returned exit status zero
Observability launcher workspace: /private/tmp/bluey-tests.j1T1G7 cleaned
Root workspace release build: passed
Server binary release build: passed
Post-fix cue-daemon release check: passed without the prior local warning
Release workspace /private/tmp/bluey-tests.ZKLxrm: cleaned
Release workspace /private/tmp/bluey-tests.xpYbGC: cleaned

Pending at draft time:
- final static release/policy rerun
- final independent whole-diff verdict
- packaged/signed exact-artifact smoke and physical macOS/Windows certification
```

The first root release build passed but emitted one local `dead_code` warning
for the marked test-workspace helper. Its production caller was already absent;
the helper definition is now also gated by
`cfg(any(debug_assertions, test))`. The exact debug-override ownership test
passed again, and `cargo +1.98 check -p cue-daemon --release` confirmed the
warning is absent.

## Overall Verdict

⏳ **PENDING FINAL GATES** - The completed source and focused privacy reviews
have no known P0 blocker, and the remaining exact sign-in-attempt cleanup is a
documented P1 follow-up. Merge readiness is intentionally withheld until the
static-policy and independent whole-diff gates above complete.
Deployment and release remain separately blocked on exact-artifact and physical
platform certification.

## Follow-ups for Next Batch

- Implement exact capability-only abandonment for a remotely completed but
  locally rejected device-link attempt; never use broad device logout.
- Finish the static/review gates and replace this
  provisional verdict with the evidence-backed final verdict.
- Use the new milestones in Phase 625 to measure accepted-input to model
  connection, first event, first usable text, and native paint. Keep persistence,
  recap, and embedding outside the first-text critical path.
- Run production-topology consent/deletion/object cleanup canaries before a
  customer deploy.
- Build once, certify physical macOS/Windows behavior, and promote only the
  exact tested release artifacts.

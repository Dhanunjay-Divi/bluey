# FIX-589: Isolated Test Workspaces And Deterministic Cleanup

> **Codex preflight:** Load `$bluey-ops` before diagnosis or implementation and
> verify its memory against the current repository state.

## Issue

Repeated Bluey test and smoke runs were recreating approximately 7 to 20 GiB
per hour. Cargo targets, temporary databases, application state, logs, and
native smoke build output could remain in the repository or Downloads after a
pass, failure, interruption, or agent handoff.

Besides wasting disk, a test process could inherit live database URLs,
provider credentials, update settings, helper overrides, or OS credential-store
settings from the operator's shell.

## Root Cause

- Local commands wrote to shared root and server `target` directories instead
  of one task-owned disposable target.
- Tests and smoke scripts did not consistently bind SQLite, application data,
  config, runtime, logs, and general temporary files to one cleanup boundary.
- The first disposable smoke could select a debug overlay helper under its
  owned temporary root, but overlay verification still compared that helper
  with the checkout or installed directory. Discovery rejected the valid test
  peer before the protocol handshake.
- Shell exit traps did not own the complete Cargo/compiler/test process group,
  so an interrupted launcher could remove files while descendants were active
  or leave ignoring/orphaned children behind.
- Existing smoke scripts compiled native helpers into repository-local build
  trees and could target a shared daemon port or globally kill another Bluey
  process.
- Runbooks, CI, Make targets, and review templates still suggested direct local
  `cargo test` invocations.

## Fix Summary

- Added `scripts/run-bluey-tests.sh` as the mandatory local Rust-test entry
  point for the root workspace, server workspace, focused commands, and
  multi-command smoke harnesses.
- Create one short `/tmp/bluey-tests.*` root per invocation with an ownership
  marker and isolated Cargo target, SQLite file, Bluey/Cue data, config,
  runtime, logs, cache, and general temporary directories.
- Remove inherited production/test PostgreSQL URLs, cloud/provider secrets,
  billing, object-store, mail, Redis, Jobs, test-service, updater, installer,
  and native-helper authority before the child process starts.
- Explicitly scrub `BLUEY_ACCESS_TOKEN`, every Bluey API base/URL/host alias,
  `FFMPEG_PATH`, `BLUEY_FFMPEG_PATH`, and `BLUEY_CONTEXT_PICKER_APP` in addition
  to the broader provider/helper families. The launcher self-test specifically
  poisons and verifies the managed token, API base and host, generic FFmpeg
  path, and context-picker override.
- Disable OS Keychain, secure-store, and legacy-keyring access for automated
  tests; suppress browser sign-in, updater, and permission UI.
- Start the command in its own process group. On success, failure, `SIGHUP`,
  `SIGINT`, or `SIGTERM`, stop Cargo/compiler/test descendants, escalate to
  `SIGKILL` only after a bounded grace period, then remove only the marked
  owned workspace and verify no residue remains.
- Keep the Cargo leaf named `target` so CLI development-build protections still
  apply. Reject long custom temporary parents before macOS Unix-socket limits
  can make otherwise healthy tests fail.
- Add a lightweight `--self-test` that proves success cleanup, exit-status 37
  propagation, `SIGTERM` cleanup, and termination of an ignoring orphan.
- Route the product smoke, observability acceptance smoke, staging Rust checks,
  Make test targets, active runbooks, onboarding/handoff docs, templates, PR
  checklist, and CI launcher test through the isolated entry point.
- Replace smoke-time native compilation with a task-owned protocol stub. Native
  helpers retain their separate platform build/protocol certification gates.
- Allow that protocol peer only in debug builds and only when its canonical
  path is inside the exact configured `bluey-tests.*` root with the ownership
  marker. Release builds never use the allowance. The focused
  `debug_overlay_allowance_requires_marked_owned_test_workspace` test covers a
  missing marker, an accepted owned helper, and an outside helper.
- Use a unique loopback daemon address and stop only the process started by the
  current smoke; no global `pkill` remains.
- Route opt-in pre-commit Clippy through the same isolated launcher so a local
  commit gate cannot recreate a repository target or inherit live authority.

## Files Modified

| File | Change |
|------|--------|
| `scripts/run-bluey-tests.sh` | Owned workspace, authority scrub, cleanup, modes, self-test |
| `docs/TESTING-RUNBOOK.md` | Canonical local test, CI, integration, and release boundary |
| `scripts/smoke-test.sh` | Disposable Rust smoke with exact built daemon and task-owned overlay peer |
| `scripts/observability-acceptance-smoke.sh` | Disposable target, unique daemon port, owned-process cleanup |
| `scripts/bluey-e2e-staging-smoke.sh` | Isolated local Rust policy checks |
| `scripts/pre-commit-observability.sh` | Isolated opt-in pre-commit Clippy |
| `scripts/check-bluey-ops-docs.sh` | Guard all active test entry points and runbooks |
| `Makefile` | `test-rust*` targets call the launcher |
| `.github/workflows/ci.yml` | Run the launcher self-test on CI |
| `.github/PULL_REQUEST_TEMPLATE.md` | Require launcher-based full Rust verification |
| `AGENTS.md` | Permanent no-residue local test rule |
| `AGENT-ONBOARDING.md` | Isolated full-gate command |
| `AGENT-HANDOFF.md` | Isolated handoff gate |
| `docs/HANDOFF.md` | Isolated ongoing verification command |
| `docs/DELIVERY-LIFECYCLE.md` | Disposable pre-release test stage |
| `docs/MODEL-ROUTING.md` | Isolated routing-focused tests |
| `docs/RELEASE-RUNBOOK.md` | Isolated source test gate; release artifacts remain canonical |
| `docs/SECURITY-HARDENING.md` | Isolated security-focused tests |
| `docs/work/TEMPLATE-IMPL.md` | Future implementation records name the launcher |
| `docs/work/TEMPLATE-REVIEW.md` | Future reviews require cleanup evidence |
| `crates/cue-cli/src/bluey_cmds.rs` | Test-only browser-sign-in suppression is honored |
| `crates/cue-daemon/src/app.rs` | Marker-verified debug helper allowance for disposable smoke/test roots |

## Edge Cases Handled

- Successful command cleanup.
- Failing command cleanup while preserving its exact exit status.
- `SIGTERM`, `SIGINT`, and `SIGHUP` during a compiler or test process.
- A child and grandchild that ignore termination signals.
- Missing or forged ownership marker during cleanup.
- A custom temporary parent whose resolved path is too long for macOS test
  sockets.
- Ambient `BLUEY_DATABASE_URL` or `BLUEY_TEST_POSTGRES_URL` values.
- Ambient provider keys, R2/AWS, billing, SMTP, Redis, Jobs, updater, install,
  and native-helper overrides.
- Tests running on a machine where OS Keychain access is enabled for the real
  product.
- Product smoke running beside another Bluey daemon.
- Debug overlay override inside an unmarked root or outside the configured
  marked test root.
- Release helper discovery when a test-only overlay override is present.
- Test overlay verification outside the production install root: accepted only
  in debug builds, under a canonical marked `bluey-tests.*` root, and never in
  release builds.

## How to Test

```bash
bash scripts/run-bluey-tests.sh --self-test
BLUEY_RUST_TOOLCHAIN=1.98 bash scripts/run-bluey-tests.sh -- \
  cargo +1.98 test -p cue-daemon \
  debug_overlay_allowance_requires_marked_owned_test_workspace -- --exact
BLUEY_RUST_TOOLCHAIN=1.98 bash scripts/run-bluey-tests.sh all
BLUEY_RUST_TOOLCHAIN=1.98 bash scripts/run-bluey-tests.sh -- \
  cargo +1.98 build --workspace --release
BLUEY_RUST_TOOLCHAIN=1.98 bash scripts/run-bluey-tests.sh -- \
  cargo +1.98 build --manifest-path server/Cargo.toml --bins --release
BLUEY_RUST_TOOLCHAIN=1.98 bash scripts/run-bluey-tests.sh -- \
  cargo +1.98 check -p cue-daemon --release
bash scripts/smoke-test.sh
bash scripts/observability-acceptance-smoke.sh
bash scripts/check-bluey-ops-docs.sh
find /private/tmp /tmp -maxdepth 1 -type d -name 'bluey-tests.*' -print
git diff --check
```

The launcher self-test passed its success, failure, `SIGTERM`, and orphan
cleanup cases. The complete root and server Rust suites and strict Clippy ran
inside disposable workspaces and each reported that its exact workspace was
cleaned. The focused debug-overlay ownership test also passed. Repository-local
root and daemon targets were removed before the final runs.

The hermetic product smoke passed its exact task-built daemon/CLI and owned
overlay protocol peer, transcript/context/memory/routing/cloud scaffolds,
signed-out provider fence, action-items/recap/archive, and confirmed shutdown.
The strengthened observability rerun passed UUID correlation, authenticated
daemon trace, Phase 3 regressions, explicit `DAEMON_PID` termination and exit
status zero, and cleanup. Its exact launcher workspace
`/private/tmp/bluey-tests.j1T1G7` was removed successfully.

The isolated root workspace release build and isolated server binary release
build passed. The first workspace build exposed one local `dead_code` warning:
the marked test-helper verifier was compiled in release even though its caller
was debug-only. The helper is now compiled only under
`cfg(any(debug_assertions, test))`; its focused ownership test passed again and
`cargo +1.98 check -p cue-daemon --release` passed without that warning. The
exact workspaces `/private/tmp/bluey-tests.ZKLxrm` and
`/private/tmp/bluey-tests.xpYbGC` were removed.

## Known Limitations

- Release/package targets, signed artifacts, shared caches, and another task's
  directories are intentionally outside this cleanup mechanism. Release builds
  must retain the one exact artifact that is tested and promoted.
- Tests requiring live PostgreSQL, Redis, object storage, funded providers,
  physical audio, or real platform helpers need an explicitly provisioned
  integration harness. The isolated launcher refuses to infer that authority
  from ambient shell variables.
- Ephemeral CI runners may keep their workflow-owned Cargo cache because the
  whole runner is discarded; the no-residue rule applies to local agent runs.

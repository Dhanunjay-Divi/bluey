# Bluey Test Workspace Runbook

> **Codex preflight:** Load `$bluey-ops` before running Bluey tests and
> reconcile it with the current branch and repository instructions.

## Local Rust Test Rule

All local Rust tests run through `scripts/run-bluey-tests.sh`. Do not run a
local `cargo test` directly unless an outer harness already provides an owned,
throwaway `CARGO_TARGET_DIR` and isolated Bluey data/database paths.

The launcher creates one private temporary workspace per invocation and binds:

- `CARGO_TARGET_DIR` to a fresh Cargo target;
- `BLUEY_DB_PATH` to a fresh SQLite file directory;
- Bluey/Cue data, config, runtime, and log paths to fresh directories;
- `TMPDIR`, `TMP`, and `TEMP` to the same owned workspace so test-created
  temporary databases and files stay inside the cleanup boundary;
- the server backend to SQLite with inherited `BLUEY_DATABASE_URL` and
  `BLUEY_TEST_POSTGRES_URL` removed; and
- OS Keychain and legacy-keyring access off for the test process.

Automated runs also suppress browser sign-in, desktop permission prompts, and
self-update/install paths. The Cargo leaf remains named `target` inside the
owned workspace so the CLI keeps its normal development-build safety posture.
Inherited cloud/provider, billing, object-store, mail, Redis, Jobs-worker, and
native-helper credentials or endpoints are removed. Tests needing a live
provider, PostgreSQL, Redis, object store, or physical audio device must use a
separate explicitly provisioned integration harness; ambient shell authority
is never accepted.

The workspace is removed after success, failure, `SIGINT`, `SIGTERM`, or
`SIGHUP`. The launcher refuses to remove a path without both its generated
name and ownership marker. It never uses or cleans release/package targets,
distribution staging, or signed artifacts.

The default workspace parent is the short system `/tmp` path because macOS
Unix-domain sockets have a strict path-length limit. If
`BLUEY_TEST_TEMP_PARENT` is set, the launcher rejects a resolved workspace path
that would leave too little room for test socket names.

The local product smoke, observability acceptance smoke, and staging routing
evals enter the same launcher automatically. Multiple Cargo commands inside
one smoke share its single temporary target and clean it once at the end.

## Commands

Run both Rust workspaces:

```bash
bash scripts/run-bluey-tests.sh all
```

Run only the desktop workspace or only the server workspace:

```bash
bash scripts/run-bluey-tests.sh workspace
bash scripts/run-bluey-tests.sh server
```

Run a focused test while preserving the same isolation and cleanup:

```bash
bash scripts/run-bluey-tests.sh -- cargo test -p cue-daemon test_name -- --nocapture
```

Select the repository's pinned test toolchain when needed:

```bash
BLUEY_RUST_TOOLCHAIN=1.98 bash scripts/run-bluey-tests.sh workspace
```

Run the deterministic lightweight launcher test. It uses fake success and
failure commands and does not compile Rust:

```bash
bash scripts/run-bluey-tests.sh --self-test
```

## CI And Release Boundaries

Ephemeral CI runners may keep workflow-owned Cargo caches because their entire
machine is discarded after the job. Release builds and package jobs retain
their canonical targets so the exact tested artifact can be promoted without a
rebuild. This runbook changes local Rust test storage only; it does not clean,
move, or overwrite build/release artifacts.

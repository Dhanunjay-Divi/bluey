# Round 500: Bluey Jobs CI and Privacy Gate

## Required check

The isolated workflow is `.github/workflows/jobs-ci.yml`. Branch protection
should require the `Jobs CI and privacy gate` job. The workflow runs for every
pull request, merge queue check, relevant push, and manual dispatch.

The workflow uses only the established official `actions/checkout@v4` and
`actions/setup-node@v4` action patterns. Rust components are installed with the
runner's existing `rustup`; the lane adds no third-party action dependency.

## Gate coverage

The lane runs, in order:

1. Deterministic self-tests for every custom guard.
2. A scan of Git-tracked paths and blobs for candidate/user data, credential
   files, raw secrets, browser profiles, receipts/screenshots, and archives.
3. Static SQLite/Postgres parity checks for the discovery, execution-lease,
   and local-run resume-action tables, their indexes, and partial-index
   predicates, plus the Postgres migration include/ledger wiring.
4. A package-lock license inventory and commit-pinned source provenance check.
5. `npm ci --prefix jobs`, then all Jobs tests, typechecks, and builds through
   the root Jobs workspace scripts.
6. server-only Rust formatting, the `bluey-jobs-api` check, all library tests
   selected through the `jobs` module path, and Jobs integration tests.

The parity set remains strict for every `jobs_discovery_*` table and also
requires `jobs_execution_leases` and `jobs_local_run_resume_actions`. Lease
parity includes the binding index and both unique active indexes, including
their `prepared`/`click_started` partial predicates. Local-resume parity covers
the complete approval/consumption record, its application-history index, and
the unique active-run index with the exact `status = 'approved'` predicate.
The passing inventory is five tables and eight indexes.

The Rust formatter is intentionally scoped with
`cargo fmt --manifest-path server/Cargo.toml -- --check`. The root workspace's
unrelated `cue-cli` rustfmt drift remains visible in its existing CI lane; this
Jobs gate neither reformats nor suppresses it.

## Privacy policy

`jobs/scripts/privacy-gate.mjs` reads the Git index rather than untracked local
files, so the result is stable and corresponds to content that can be
committed. Findings print only a path, line number, and category; secret values
are never echoed into CI logs.

Tracked text blobs larger than 5 MiB fail closed instead of being skipped. This
prevents padding from bypassing content inspection while keeping the check
independent of generated or ignored local files.

High-confidence provider token formats and opaque credential assignments fail.
Documented environment substitutions, standard example tokens, obvious dummy
sequences, and synthetic test values pass. Credential-shaped fixture file
names still fail even when their contents claim to be dummy, because those
paths are too easy to replace accidentally with live exports. Documentation
images and source files such as `receipts.ts` and `profile.ts` are not treated
as runtime artifacts.

The guard has no broad allowlist. Synthetic evidence fixtures must live under a
test/example path and include `dummy`, `example`, `fixture`, `sample`,
`synthetic`, or `test` in the filename.

## Provenance inventory

`jobs/scripts/check-provenance-licenses.mjs` checks workspace manifests against
`jobs/package-lock.json`, requires npm registry URLs and SHA-512 integrity for
external packages, inventories license expressions, validates 12-character
reviewed commits, and verifies notices for adapted MIT sources.

`unionfs@4.6.0` is the sole audited metadata override. Its package omits a
`license` field but ships the Unlicense text in `LICENSE`; the override is
version-specific so a dependency update requires a fresh review.

## Local commands

```sh
node jobs/scripts/ci-guards-self-test.mjs
node jobs/scripts/privacy-gate.mjs
node jobs/scripts/check-jobs-schema-parity.mjs
node jobs/scripts/check-provenance-licenses.mjs
npm ci --prefix jobs --no-audit --no-fund
npm test --prefix jobs
npm run typecheck --prefix jobs
npm run build --prefix jobs
cargo fmt --manifest-path server/Cargo.toml -- --check
cargo check --manifest-path server/Cargo.toml --bin bluey-jobs-api
cargo test --manifest-path server/Cargo.toml --lib jobs
cargo test --manifest-path server/Cargo.toml --test integration_e2e jobs_
```

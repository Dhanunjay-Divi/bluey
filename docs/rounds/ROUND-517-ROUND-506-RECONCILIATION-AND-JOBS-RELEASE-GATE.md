# Round 517 - Round 506 Reconciliation And Jobs Release Gate

Date: 2026-07-12

Backup Codex task: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Scope

Reconcile the separately developed Bluey Jobs and Round 506 edge-hardening
work with the signed Bluey `0.1.99` runtime release without rolling back the
newer production web/API deployment or losing the billing, storage, legal,
and account-safety work already on `main`.

## Integration

- Merged Round 506 commit `0368822987a7b636821447371f848e00653cfa2f`
  into a branch based on `main`.
- Preserved both SQLite schema families:
  - legal acceptance, object upload controls, managed-usage reservations, and
    durable Auto Reload attempts;
  - Bluey Jobs profiles, applications, evidence, local capabilities,
    discovery, and execution leases.
- Preserved all Postgres migrations, including pgvector/runtime compatibility,
  Jobs, usage reservations, object controls, and durable Auto Reload.
- Kept the current privacy contract: submitted content is not training data,
  raw audio is not retained after transcription by default, cloud sync is
  explicit, and Jobs data/sharing/deletion behavior is disclosed.
- Kept `/llms.txt` retired with HTTP `410` as part of the intentional
  anti-scraping policy.
- Renumbered the eight newly merged Jobs documents that collided with existing
  Bluey round numbers to Rounds 509-516.

## Release-Blocking Findings Closed

1. Private Jobs workers now sign every internal request with the exact
   `bluey-jobs-worker-v1` HMAC contract. The signing key is server-side only;
   the runner token remains scoped to runner-service authentication.
2. The local Bluey Browser now consumes operation-scoped `result` and `resume`
   capabilities returned by `/claim`. The one-time root ticket is not reused.
3. Non-strict Redis replay protection falls back locally for both connection
   and command failures. Strict production mode still fails closed.
4. Jobs HTML is served with `no-cache`; hashed Jobs assets remain immutable and
   missing assets return a real `404` instead of SPA HTML.
5. JavaScript and Rust share one fixed HMAC test vector to detect future
   signing-contract drift.

## Verification Before Deployment

- `cargo test -p cue-cli --lib --quiet`: 65 passed.
- `cargo test --manifest-path server/Cargo.toml --lib --quiet`: 370 passed.
- `cargo test --manifest-path server/Cargo.toml --test integration_e2e --quiet`:
  71 passed.
- `cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings`:
  passed.
- Bluey Jobs tests: 15 automation files / 122 tests, 6 browser files / 30
  tests, 6 runner files / 35 tests, 4 workflow files / 23 tests, and 5 portal
  files / 15 tests all passed.
- Bluey Jobs typecheck and production build passed.
- Edge policy and Jobs client/server boundary checks passed.
- Managed production Turnstile, Postgres/pgvector, Valkey, R2 object/log/backup
  storage, Square branding, provider routing, SMTP, health, and signed release
  manifest passed strict production preflight with zero warnings.

## Deployment Discipline

This round uses manual signed deployment and does not start GitHub Actions.
The exact integrated commit, live artifact hashes, service restart evidence,
post-deploy smoke results, and rollback reference must be appended before the
round is considered complete.

## Reproducible Server Build Gate

- The first clean droplet build correctly rejected `--locked` because
  `server/Cargo.lock` existed only as an ignored local file.
- Bluey Server is an application, so its resolved dependency graph is now
  committed and the explicit ignore rule was removed.
- The lockfile uses Cargo format 3 so both the developer toolchain and the
  isolated production builder (Cargo/Rust 1.75) can verify the same graph.
- `cargo metadata --manifest-path server/Cargo.toml --locked --no-deps` passes.
- Production candidates must continue to build with `cargo build --locked`;
  silently resolving newer dependencies during a release is not allowed.

## Live Edge Findings

- Cloudflare Free-plan Bot Fight Mode was rejected for Bluey because it placed
  a browser JavaScript challenge in front of native API clients. It was
  disabled while Turnstile, product rate limits, explicit crawler denial,
  Browser Integrity Check, strict TLS, and the direct-origin firewall stayed
  enabled.
- The public edge matrix caught an ambiguous Caddy `redir` form: inside a
  `handle`, `redir /jobs 308` was interpreted as a path matcher plus redirect
  target. Both legacy Jobs routes now use `redir * /jobs 308`, and the static
  edge test asserts the explicit form.

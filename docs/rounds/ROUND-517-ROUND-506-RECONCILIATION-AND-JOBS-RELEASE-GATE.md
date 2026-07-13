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
- The lockfile uses Cargo format 3 for broad tooling compatibility. The final
  isolated production builder uses Cargo/Rust 1.95 because resolved
  dependencies require Edition 2024 support; the host's older system Rust was
  not modified.
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
- Direct HTTPS to the historical origin is blocked by the host firewall. Port
  80 remains redirect-only so Caddy can renew the Let's Encrypt origin
  certificate: direct requests must return exactly `308` to the canonical
  `https://bluey.sh` URL and must never expose API or static content. The live
  verifier now treats either a blocked connection or that exact redirect as
  safe, while failing any content-bearing response or off-domain redirect.

## Production Release

- Deployed code commit:
  `16098a0014c2278c3ac38727fe2240b0d860234f`
- Signed release ID: `round517-16098a0014c2`
- Built at: `2026-07-13T00:41:27Z`
- Builder: Cargo/Rust `1.95.0`, locked release profile
- Release directory:
  `/opt/bluey-releases/round517-16098a0014c2`
- The Ed25519 manifest signature verifies both locally and from the deployed
  bundle.

| Artifact | SHA-256 |
| --- | --- |
| Source archive | `a6dca3871c9431dcc20a0796d70da3d7927d3b1f658ed323b16972a39c53c588` |
| Main API | `cfd483339258f214f59add688a343f7a351ea05c9f7ec2bdec0ab3dd490bb358` |
| Jobs API | `7201dd4f8b9b674c946ab5c301d5a4a15efbfaadc8bd8e3e4644b5cab2b86b84` |
| Web archive | `e78e82454937d8aa78a5de0b02e5591eed7db8ad4d694b5e4a891f31c9e9ae8c` |
| Caddyfile | `91bfba4d2266825d3d31a81ae2c125ee279c1393370afca9ccefd2419ddeae70` |

The signed native release remained `0.1.99` and was not rebuilt or
overwritten:

| Platform | SHA-256 |
| --- | --- |
| macOS arm64 | `a0bceff0868013a0a738a6a026bd4f631fa2c29f203bf88c904832f05a1bbef8` |
| Windows x86_64 | `dc4a6aa8def8b3d9447ec722e447ca992b2421f65db212608f5d6bef7f6f3b6d` |

## Backup And Rollback

- Fresh PostgreSQL backup:
  `/var/backups/bluey-api/hourly/bluey-postgres-20260712T234050Z.pgdump`
- Backup SHA-256:
  `b5441827e2b96f5b1482ea0a5384c5981042ee836c8a2512b65c34f0ffb97daa`
- Backup size: `24,016,449` bytes
- Final pre-release rollback snapshot:
  `/var/backups/bluey-api/releases/20260713T004312Z-before-round517-16098a0014c2`

## Live Acceptance

- Main and Jobs `/health` report the exact deployed commit.
- `bluey-api`, `bluey-jobs-api`, and `caddy` are active with `NRestarts=0`.
- The post-restart log scan found no panic, fatal, error, `429`, provider
  capacity, or dropped-connection entries.
- `bluey-cloud-client/0.1.99`, `bluey-cli/0.1.99`, and a browser User-Agent all
  receive `200` from `/health` through Cloudflare.
- Turnstile config is public and valid; unsigned account access remains `401`;
  private Jobs discovery remains `404`.
- GPTBot, OAI-SearchBot, ChatGPT-User, ClaudeBot, and PerplexityBot receive
  `403`; conventional Googlebot receives `200`.
- `/llms.txt` returns `410`, legacy `/JobApply` returns canonical `308`, missing
  Jobs assets return `404`, Jobs HTML is `no-cache`, and hashed assets are
  immutable.
- Direct origin HTTPS times out; direct HTTP exposes only the exact canonical
  `308` redirect required for certificate renewal.
- Public `latest.json` signature verification passed. Installer MIME types and
  both native artifact hashes match the signed manifest.
- Disk usage after deployment is 48 percent (`30G` free of `58G`).
- Round 518 contains the Cloudflare state, firewall evidence, crawler matrix,
  and live desktop/mobile Jobs screenshots.

## Storage And Diagnostic Evidence

- R2 log storage is reachable and the newest object under the configured
  archive prefix was written at `2026-07-13T00:17:07.915Z`.
- The production `diagnostic_log_chunks` table currently contains zero
  client-uploaded chunks. Server archive health is proven; a real desktop
  support/session audit upload still needs an authenticated production smoke
  before client diagnostic retrieval can be called live-verified.
- Raw mic/system audio is not retained by the current product after
  transcription by default. The `audio/audio.jsonl` audit entry is a manifest
  placeholder, not replayable source audio. Historical planning docs that
  proposed raw-audio QA/training retention are not proof of shipped behavior or
  consent.

## Remaining Operational Gates

- The Jobs API and customer portal are live, but Temporal/discovery/browser
  workers are not installed as unattended production services. Jobs remains a
  review-first staged beta until worker rollout, ATS certification, mailbox and
  calendar OAuth, and authenticated synthetics are complete.
- This server/edge round reused the already signed Windows artifact; it did not
  have a physical Windows machine for a fresh runtime smoke.
- Add one authenticated desktop diagnostic-bundle upload/retrieval smoke before
  claiming end-to-end production support-bundle archival.
- Any future raw-audio retention must be bounded, account/session scoped,
  encrypted, lifecycle-deleted, separately consented, and reflected accurately
  in Terms and Privacy before implementation.

## Result

The Round 506 reconciliation, signed server deployment, production web/Caddy
promotion, Turnstile enforcement, and Cloudflare/origin edge gate are complete.
No GitHub Actions workflow was started. This result does not promote the staged
Jobs worker plane or unverified raw-audio retention into production claims.

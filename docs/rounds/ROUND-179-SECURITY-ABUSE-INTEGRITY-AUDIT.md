# Round 179 - Security Abuse Integrity Audit

## Trigger

Owner asked for a security standpoint pass: prevent abusive usage, make sure endpoints are secured, make sure release/download code is hash-verified, and check backend/frontend/UI state for anything still missing.

Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`  
Workspace: `/Users/uno/Downloads/cue`  
Round completed: 2026-06-25 16:05 EDT

## Audit Scope

- Backend route/auth surface:
  - public auth, pricing, health, Stripe/Square webhooks
  - protected account, router, STT, sync/RAG, billing, device, admin routes
  - JWT auth, admin middleware, refresh-token storage, device flow, password reset, signup OTP, trial abuse ledger
- Abuse controls:
  - auth brute-force limits
  - trusted proxy handling
  - trial abuse hashed signals
  - provider/account capacity buckets
  - router idempotency and billing hard-stop behavior
- Integrity/distribution:
  - macOS and Windows installers
  - release hygiene scan
  - checksum verification behavior
  - existing release profile symbol stripping and LTO
- Frontend/dashboard dependency security:
  - dashboard npm audit
  - dashboard test/build
  - public web JS syntax

## Fixes

- Hardened trusted client IP attribution:
  - added `rate_limit::trusted_client_ip_from_headers(...)`
  - forwarded IP headers are now accepted only when the immediate peer is in `BLUEY_TRUSTED_PROXIES`
  - signup trial-abuse signals and Turnstile `remoteip` now use the same trusted-proxy rule as rate limiting
  - direct callers can no longer spoof `X-Forwarded-For`/`CF-Connecting-IP` to change trial-abuse IP hashes
- Hardened the public macOS web installer:
  - `ops/install/install.sh` now fails closed when neither `BLUEY_ARTIFACT_SHA256` nor `SHA256SUMS.txt` can verify the artifact
  - missing `shasum` now stops install unless `BLUEY_SKIP_CHECKSUM=1` is explicitly set for local testing
  - this matches the Windows installer's existing fail-closed checksum behavior
- Removed dashboard npm audit findings:
  - upgraded React Router to a non-vulnerable range
  - upgraded Vite/esbuild/Vitest toolchain
  - upgraded Babel transitive packages through `npm audit fix`
  - final `npm audit` reports `0 vulnerabilities`

## Security State Confirmed

- Protected backend routes are behind `require_auth`; admin routes add `require_admin`.
- Webhooks require Stripe/Square HMAC signature validation before processing.
- Refresh tokens are stored SHA256-hashed and consumed atomically.
- Signup OTPs are HMAC-hashed and attempt-limited.
- Trial-abuse email, domain, IP, device, and user-agent signals are stored as hashes.
- Object storage keys are account-scoped and cross-account object keys are rejected.
- AI router uses idempotency reservations, provider capacity checks, balance/trial guards, and sanitized upstream errors.
- Existing release profile already strips symbols and uses thin LTO:
  - `strip = "symbols"`
  - `lto = "thin"`
  - `codegen-units = 1`
- Release/download integrity is checksum-gated by default; signed update metadata remains the stronger update path.

## Verification

Passed:

- `cargo fmt --all`
- `cargo fmt --check --all`
- `cd server && cargo clippy --all-targets -- -D warnings`
- `cd server && cargo test --all-targets`
- `cd server && cargo test auth_routes::tests::signup_signals_do_not_trust_forwarded_headers_without_peer_info`
- `cd server && cargo test rate_limit::tests::xff_ignored_when_no_trusted_proxy_set`
- `cd server && cargo test rate_limit::tests::xff_honored_when_peer_is_trusted_proxy`
- `bash -n ops/install/install.sh scripts/install.sh`
- `node --check web/assets/bluey-site.js`
- `cd crates/cue-dashboard/ui && npm audit`
- `cd crates/cue-dashboard/ui && npm test -- --run`
- `cd crates/cue-dashboard/ui && npm run build`
- `git diff --check`
- `scripts/release-hygiene-scan.sh`
- targeted secret regex scan over server, UI, ops, scripts, web, and non-round docs

Results:

- Server tests: `158` unit tests, `1` ConnectInfo real-serve test, `2` GDPR cleanup tests, and `41` integration tests passed.
- Dashboard UI: `15` Vitest tests passed, production build passed.
- `npm audit`: `0 vulnerabilities`.
- Secret scan hits were placeholders, docs examples, generated random test env, or test fixture secrets.
- Release hygiene passed with expected dev-only warning mentions in local visual smoke scripts/docs.

## Environment Gaps

- `scripts/bluey-cloud-preflight.sh` still fails in this local shell because production env/secrets are not loaded:
  - missing `BLUEY_PUBLIC_URL`
  - missing `BLUEY_JWT_SECRET`
  - missing `BLUEY_BILLING_PROVIDER`
  - missing provider key pools
  - missing `OFFSITE_DESTINATION`
  - Turnstile, SMTP, Redis, and object storage warnings
- `cargo-audit` is not installed locally, so Rust advisory DB scanning could not run.
- Neither `pwsh` nor `powershell` is installed locally, so Windows PowerShell parse/build checks could not run.
- Homebrew Cask still has `sha256 :no_check` until concrete release artifacts are published; the safer one-line install/update paths verify checksums/signatures.

## Files Touched

- `server/src/rate_limit.rs`
- `server/src/api/auth_routes.rs`
- `ops/install/install.sh`
- `crates/cue-dashboard/ui/package.json`
- `crates/cue-dashboard/ui/package-lock.json`
- `docs/rounds/ROUND-179-SECURITY-ABUSE-INTEGRITY-AUDIT.md`
- `docs/rounds/BLUEY-COMPACTION-HANDOFF-2026-06-25.md`

## Mac Windows Parity

- Mac installer behavior was hardened in `ops/install/install.sh`.
- Windows installer already fails closed on missing checksum by requiring `latest.json`/`SHA256SUMS.txt`/`BLUEY_ARTIFACT_SHA256`; no Windows code change was needed.
- Windows PowerShell parse could not be run locally because PowerShell is not installed.

## Current State

- No dashboard npm advisories remain; final audit is clean.
- Backend auth, webhook, billing, streaming, sync, object storage, rate limit, and trial-abuse tests pass.
- Distribution integrity is safer by default because the macOS web installer no longer falls back to HTTPS-only downloads.
- Production readiness still requires real deployment env/secrets and a machine with PowerShell/cargo-audit for those two external gates.

# Security Hardening + Code Protection Handoff For Kiro

> Date: 2026-05-21  
> Author: Codex  
> Scope: client code-protection posture, local data permissions, and security doc refresh.

## 1. Why This Round Exists

User asked for hard protection so customers cannot backtrack into code, read what
is inside, or attack the product.

The important engineering call is that **installed desktop code can never be made
impossible to inspect**. We should not build the product on that false premise.
The correct production model is:

1. Treat the desktop client as untrusted.
2. Keep provider keys, wallet state, pricing, routing policy, and metering on
   `bluey-server`.
3. Use short-lived server-issued tokens for relay flows.
4. Lock down local files to reduce accidental or casual exposure.
5. Add obfuscation, anti-debug, and self-checks only as friction.

## 2. Code Changes

### Local directory permissions

File: `crates/cue-core/src/app_paths.rs`

- `AppPaths::ensure()` now creates Bluey data/config/runtime directories via
  `create_private_dir()`.
- On Unix/macOS those directories are normalized to `0700`.
- Added regression test:
  `ensure_uses_private_directory_permissions`.

### Meeting JSON permissions

File: `crates/cue-daemon/src/storage.rs`

- Meeting archive directory now uses `create_private_dir()`.
- `active-meeting.json` and archived meeting JSON files are written via a
  private writer that normalizes Unix/macOS permissions to `0600`.
- Rename path now also preserves private permissions.
- Added regression test:
  `meeting_store_writes_private_files`.

### SQLite database permissions

File: `crates/cue-daemon/src/db/mod.rs`

- On Unix/macOS file-backed DB open now creates the SQLite file with mode
  `0600` before `rusqlite` opens it.
- Existing DB files are normalized to `0600`.
- WAL/SHM sidecar files are normalized to `0600` when present.
- Parent directory uses `create_private_dir()`.
- Added regression test:
  `file_database_is_created_private`.
- Added guard test:
  `relative_db_parent_does_not_target_current_directory`.

### Release profile hardening

File: `Cargo.toml`

- Release profile now strips symbol tables with `strip = "symbols"`.
- Release profile uses thin LTO and single codegen unit for tighter release
  binaries.
- This is explicitly documented as reverse-engineering friction, not a trust
  boundary.

### Overlay helper SHA sidecar verification

File: `crates/cue-daemon/src/overlay.rs`

- `verify_overlay_binary()` still rejects relative paths and helpers outside
  the install directory.
- If a sibling SHA-256 sidecar exists (`overlay.sha256` or
  `overlay.exe.sha256` style), the daemon now hashes the helper and rejects a
  mismatch before spawning it.
- Missing sidecar remains allowed for local/dev artifacts. Signed release
  manifests are still the stronger production control.
- Added regression tests for matching and mismatched sidecars.

### Stripe multi-signature proof

File: `server/src/api/billing.rs`

- Bluey already iterated all `v1=` entries in `Stripe-Signature`, enforced a
  5-minute timestamp tolerance, and compared signatures in constant time.
- Added a regression test proving a header with one bad `v1` and one good `v1`
  is accepted. This mirrors the Pinky follow-up checklist item.

### Sensitive diagnostics redaction

Files:

- `crates/cue-cloud-client/src/client.rs`
- `crates/cue-cloud-client/src/auth.rs`
- `crates/cue-dashboard/src/lib.rs`
- `server/src/api/auth_routes.rs`
- `server/src/api/billing.rs`

The Pinky leak-review note applies directly to Bluey. Passwords/provider keys
were not intentionally exposed, but normal diagnostics could still become
sensitive.

Changes:

- Cloud-client non-2xx response bodies are now JSON-redacted and truncated
  before logging. Token/code/secret/password/auth/url fields are replaced with
  `<redacted>`.
- Device-flow polling uses the same safe body logger.
- Stripe checkout/portal upstream error logs redact URL/client-secret/token and
  payment-method style fields.
- Malformed deep-link parse logs no longer include the raw `bluey://...` URL,
  because it may contain a one-time login code.
- Email verification/password reset URLs are suppressed from logs by default
  when SMTP is not configured. Local developers can opt in with
  `BLUEY_DEV_LOG_AUTH_LINKS=1`; production should use SMTP and leave this unset.

This does not make logs public-safe. Logs can still contain account ids, emails,
IP-level metadata, timing, device names, and operational diagnostics. Treat logs,
review docs, preprod hostnames, and any session/share links as private.

## 3. Documentation Changes

### Updated canonical security doc

File: `docs/SECURITY-HARDENING.md`

The old doc described the older local-first/BYOK v0.1 model. It now reflects the
current managed product shape:

- desktop client is untrusted;
- server is the security boundary;
- provider keys stay server-side;
- `/stt/session` and `/stt/relay` protect STT credentials;
- cloud sync/RAG is account-scoped;
- overlay IPC token/state-machine protections are documented;
- local private permissions are documented;
- missing P0/P1 work is explicit.

## 4. Honest Limitations

This round does **not** make desktop code unreadable. Nothing can, if the code is
installed and executed on the user's own machine.

It also does not yet implement:

- signed release manifests;
- encrypted local SQLite;
- certificate pinning;
- updater signature verification;
- binary self-integrity checks;
- dependency audit CI.

Those are now clearly listed as P0/P1 hardening work in
`docs/SECURITY-HARDENING.md`.

## 5. Reviewer Checklist

Please review:

- `crates/cue-core/src/app_paths.rs`
- `Cargo.toml`
- `crates/cue-daemon/src/storage.rs`
- `crates/cue-daemon/src/db/mod.rs`
- `crates/cue-daemon/src/overlay.rs`
- `crates/cue-cloud-client/src/client.rs`
- `crates/cue-cloud-client/src/auth.rs`
- `crates/cue-dashboard/src/lib.rs`
- `server/src/api/auth_routes.rs`
- `server/src/api/billing.rs`
- `docs/SECURITY-HARDENING.md`

Questions to answer:

1. Are `0700` directories and `0600` files acceptable as the default on
   macOS/Linux?
2. Do we need Windows ACL hardening in the next round, or is Windows still gated
   behind the separate Windows parity brief?
3. Should local SQLite encryption be the next security implementation, or should
   signed release manifests come first?
4. Should we add `cargo deny`/`cargo audit` before any wider alpha?

## 6. Verification Commands

```bash
cargo fmt --all --check
cargo test -p cue-core app_paths::tests::ensure_uses_private_directory_permissions
cargo test -p cue-daemon storage::security_tests::meeting_store_writes_private_files
cargo test -p cue-daemon db::tests
cargo test -p cue-daemon overlay::tests::verify_overlay_binary_accepts_matching_sha256_sidecar
cargo test -p cue-daemon overlay::tests::verify_overlay_binary_rejects_mismatched_sha256_sidecar
cd server && cargo test stripe_log_body_redacts_sensitive_fields
cd server && cargo test signature_verifies_when_any_v1_signature_matches
cargo test -p cue-cloud-client log_safe_response_body_redacts_tokens_and_urls
cargo clippy -p cue-core -p cue-daemon --all-targets -- -D warnings
cargo build -p cue-cli -p cue-daemon --release
git -P diff --check main..HEAD
```

## 7. Recommended Next Security Round

Recommended order:

1. Signed release manifest with embedded ed25519 public key.
2. `bluey check-update` / update installer verification against that manifest.
3. Local DB encryption using OS keyring-derived key.
4. Windows ACL parity for data/config/runtime directories and local DB files.
5. Dependency audit CI.

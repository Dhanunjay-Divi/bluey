# Review: Security Hardening + Pinky Leak-Review Parity

**Branch:** `feat/phase-3-round-12`
**Tip:** `520bc20` (uncommitted batch in working tree)
**Reviewer:** Kiro
**Date:** 2026-05-21

---

## 1. Verdict

🟡 **ACCEPT WITH ONE BLOCKER, ONE NIT**

The hardening is the right shape: realistic threat model in `docs/SECURITY-HARDENING.md`, file-permission tightening across the local data plane, Stripe webhook multi-`v1` with constant-time comparison, overlay helper SHA-256 sidecar verification, and broad diagnostic redaction across cloud-client / Stripe / auth / deep-link paths. The user's product-claim rule is correctly reflected — no "unbacktraceable" wording.

One blocker (B-1, same one as Stage 25 review): the `0700` directory permission tightening regresses on system-owned parent paths. Fix is one line.

---

## 2. What I Reviewed

### Code paths read
- `crates/cue-daemon/src/overlay.rs` — `verify_overlay_binary`, `verify_optional_sha256_sidecar`, `sha256_sidecar_for`, `read_sha256_sidecar`, `sha256_file_hex`, `OverlayVerifyError`, all 5 unit tests
- `server/src/api/billing.rs` — `verify_stripe_signature`, `signature_verifies_when_any_v1_signature_matches`, `stripe_log_body_redacts_sensitive_fields`, `log_safe_stripe_body`
- `crates/cue-core/src/app_paths.rs` — `AppPaths::ensure`, `create_private_dir`, `set_private_dir_permissions`, `ensure_uses_private_directory_permissions` test
- `crates/cue-daemon/src/db/mod.rs` — `ensure_private_sqlite_file`, `secure_sqlite_file_family`, `should_harden_db_parent`, `file_database_is_created_private` test
- `crates/cue-daemon/src/storage.rs` — meeting JSON 0o600 writer, `meeting_store_writes_private_files` test
- `Cargo.toml` — `[profile.release]` section with `strip = "symbols"`, `lto = "thin"`, `codegen-units = 1`
- `crates/cue-cloud-client/src/client.rs` — `log_safe_response_body`, `redact_json_value`, `is_sensitive_log_key`, `log_safe_response_body_redacts_tokens_and_urls` test
- `crates/cue-cloud-client/src/auth.rs` — device-flow polling using `log_safe_response_body`
- `server/src/api/auth_routes.rs` — `allow_dev_auth_link_logs`, verification/reset URL log gates with negative-case warn
- `crates/cue-dashboard/src/lib.rs` — `install_deep_link_handler`, raw URL suppression on parse failure

### Handoff docs read
- `docs/rounds/SECURITY-HARDENING-CODE-PROTECTION-FOR-KIRO.md`
- `docs/SECURITY-HARDENING.md` (the user-facing doc)

---

## 3. What's Right

### 3.1 Overlay helper SHA-256 sidecar — 🟢 strong

`crates/cue-daemon/src/overlay.rs` `verify_overlay_binary`:

1. Rejects relative paths (`OverlayVerifyError::NotAbsolute`).
2. Canonicalizes path AND install dir, requires path is inside install dir (`OverlayVerifyError::OutsideInstallDir`).
3. Calls `verify_optional_sha256_sidecar`, which:
   - Resolves both `overlay.sha256` (extension swap) AND `<file_name>.sha256` (Windows `overlay.exe.sha256` suffix). First existing sidecar wins.
   - Parses sidecar via `read_sha256_sidecar` — accepts standard `shasum` output format (whitespace-separated, finds the first 64-char hex token, lowercases).
   - Hashes the binary via `sha256_file_hex` → SHA-256 of `std::fs::read(path)`.
   - Compares with `eq_ignore_ascii_case` — short-circuit comparison. Not constant-time, but the secret is the binary's hash which an attacker who can vary the binary contents can compute trivially. Acceptable.
4. Missing sidecar = OK (preserves dev/local builds without sidecar packaging).

The 5 unit tests cover: relative path reject, inside install dir accept, matching sidecar accept, mismatched sidecar reject, outside install dir reject. Coverage is complete for the contract.

**Concerns and why they're acceptable:**

- TOCTOU between `verify_overlay_binary` Ok and actual `Command::spawn`: an attacker with local user-level write access to the install dir could swap the binary between check and spawn. **Acceptable**: this attacker already has account-level write, which is well beyond what hash-verification can defend against.
- Whole-file `std::fs::read` allocation: fine for a few-MB Swift overlay. Streaming hash via chunk-buffered `Sha256::update` would scale better for hypothetical larger helpers.
- Sidecar absence is permissive: by design. Production release packaging needs to include the sidecar — this is an operator gate, not a code gate.

### 3.2 Stripe webhook multi-`v1` + constant-time match — 🟢 strong

`verify_stripe_signature` in `server/src/api/billing.rs`:

- Parses `Stripe-Signature` into `t=<ts>` + zero-or-more `v1=<sig>`.
- Rejects empty timestamp + empty `v1` list.
- 5-minute (300s) tolerance on `event_ts vs now()`.
- HMAC-SHA256(`secret`, `<ts>.<body>`) → hex-encoded.
- Iterates ALL `v1=` candidates. For each: length check first (cheap), then `subtle::ConstantTimeEq::ct_eq` for byte-equal comparison. Sets `any_match = true` on success **but does not break** — keeps iterating to keep timing constant relative to the number of signatures.
- Final check: `if !any_match { Err(...) } else { Ok(()) }`.

The regression test `signature_verifies_when_any_v1_signature_matches` constructs `t=...,v1=<bad>,v1=<good>` and asserts Ok. Direct proof of multi-signature rotation safety.

**One subtle thing:** the `any_match = true` assignment is conditional on `ct_eq` returning Choice(1), which compiles to a branch. This means timing reveals **how many signatures matched** — a non-secret since `any_match` is the only output. No real leak.

### 3.3 Local file/directory permissions — 🟢 strong (with B-1)

- `AppPaths::ensure()` creates data, config, runtime dirs with `0o700`.
- `MeetingStore` archives directory uses `create_private_dir`. Active and archived meeting JSON written via private writer with `0o600`. Rename preserves perms.
- SQLite `ensure_private_sqlite_file`: uses `OpenOptions::new().mode(0o600).create_new(true)` for new files (atomic — file is created with 0o600, no chmod-after-create race). Falls back to `set_permissions(0o600)` on existing files. WAL/SHM sidecars normalized via `secure_sqlite_file_family`.
- `should_harden_db_parent` guards against `.` and empty paths.

Tests:
- `ensure_uses_private_directory_permissions` asserts 0o700 on all three dirs.
- `meeting_store_writes_private_files` asserts 0o600 on active.json + archived.json.
- `file_database_is_created_private` asserts 0o600 on `sessions.db`.
- `relative_db_parent_does_not_target_current_directory` is a guard test against accidentally hardening `.`.

**The blocker (B-1) is in this section** — see §4.

### 3.4 Release profile hardening — 🟢 acceptable

`Cargo.toml`:
```toml
[profile.release]
strip = "symbols"
lto = "thin"
codegen-units = 1
```

The doc comment correctly labels this as friction, not a trust boundary. `strip = "symbols"` removes function names from binary symbol tables; `lto = "thin"` enables cross-crate inlining (smaller, slightly faster binaries); `codegen-units = 1` is a slight build-time hit for tighter optimization. None of these prevent disassembly. They make casual `nm` / `strings` less useful.

If you want to go further:
- `panic = "abort"` removes unwinder code — smaller binary, but loses panic-handler hooks. Skip unless you confirm no test rig depends on unwind.
- `debug = false` (already default in release).
- Symbol stripping at link time via `RUSTFLAGS="-C link-arg=-Wl,-x"` for Mach-O.

Not necessary for v0.2 alpha.

### 3.5 Cloud-client log redaction — 🟢 strong

(Detailed in Stage 25 review §3.5.) JSON-aware recursive redaction on:

- `token` / `secret` / `password` / `authorization` (substring)
- `code` exact + `_code` suffix
- `url` exact + `_url` suffix

Truncated at 256 bytes. Falls back to raw string for non-JSON. Used by both `client.rs` request paths and `auth.rs` device-flow polling.

### 3.6 Auth verification/reset URL gate — 🟢 correct

`server/src/api/auth_routes.rs::allow_dev_auth_link_logs()` gated behind `BLUEY_DEV_LOG_AUTH_LINKS=1|true|yes|on`. Used at:

- Email verification when SMTP returns `MailDelivery::NotConfigured`
- Password reset when SMTP returns `MailDelivery::NotConfigured`

**Both have the negative-case path covered:**

```rust
} else {
    tracing::warn!(
        account_id = %account.id,
        email = %account.email,
        "email verification link created but SMTP is unconfigured; link suppressed from logs"
    );
}
```

So operators see "we made a link, SMTP isn't set up" — they don't get the link itself unless they explicitly opt in. This is the right tradeoff.

**Minor concern:** account ID + email are still logged. Both are needed for diagnostics. Acceptable.

### 3.7 Deep-link parse failure log — 🟢 correct

`crates/cue-dashboard/src/lib.rs::install_deep_link_handler`:

```rust
tracing::warn!(
    url_len = url.len(),
    error = %e,
    "deep link parse failed; raw URL suppressed"
);
```

The raw `bluey://link?code=...` URL is NOT logged because it may contain a one-time login code. Length and error are emitted for diagnostics. Correct.

### 3.8 Stripe upstream error redaction — 🟢 correct

`log_safe_stripe_body` redacts `url`, `client_secret`, `payment_method`-shaped fields from Stripe upstream error response bodies before logging. The regression test `stripe_log_body_redacts_sensitive_fields` proves it.

### 3.9 Pinky parity table in `docs/SECURITY-HARDENING.md` — 🟢 correct

The Pinky controls table maps cleanly. Notable:

- Bluey is **better** on provider-key isolation: Pinky is BYOK-ish; Bluey is server-managed by design.
- Bluey is **at parity** on auth (bcrypt cost 12, JWT split, hashed refresh tokens), parameterized SQL, capture-excluded overlay, Stripe multi-`v1`, install checksum, helper sidecar verification.
- Bluey is **deferred** on signed release manifest, local DB encryption, certificate pinning, binary self-integrity, dependency audit CI.

The user's product wording rule is correctly reflected:

> Low-profile native overlay, capture-excluded in normal OS capture paths, server-managed provider access, account-scoped cloud memory, hardened auth, and audited release controls.

NOT "unbacktraceable" / "undetectable" / "impossible to reverse engineer." ✅

---

## 4. Blocker

### B-1 🔴 `0700` permissions regress on system-owned parent paths

**Same blocker reported in `docs/reviews/REVIEW-STAGE-25.md` §4.**

Root cause is shared between Stage 25 and Security Hardening because the `create_private_dir` helper in `cue-core` is called by both `AppPaths::ensure` (security hardening) and `Database::open` parent setup (security hardening's SQLite branch).

**Symptom:** `cargo test --all-targets` → 1 failure:
```
test load_mic_device_setting_round_trips ... FAILED
called `Result::unwrap()` on an `Err` value:
  failed to set private permissions on /tmp
Caused by: Operation not permitted (os error 1)
```

**Fix:** make `set_private_dir_permissions` best-effort in `create_private_dir`. The chmod is defense-in-depth; degraded permission on a non-owned dir is exactly the kind of thing where `tracing::debug!` is more useful than failing the call. ~5-line change.

---

## 5. Nit

### N-1 🟡 SHA sidecar comparison is not constant-time

`verify_optional_sha256_sidecar`:

```rust
if !actual.eq_ignore_ascii_case(&expected) {
    return Err(OverlayVerifyError::HashMismatch { ... });
}
```

`eq_ignore_ascii_case` short-circuits on the first mismatch, leaking timing about which byte differs. Since the "secret" being compared is a hex-encoded SHA-256 of the binary (which the attacker can compute trivially given the binary), this is not a meaningful timing leak.

**Strict fix (low priority):** use `subtle::ConstantTimeEq::ct_eq` after lowercasing both. Not blocking.

---

## 6. User's Question: Signed Release Manifest vs Local DB Encryption — Which Next P0?

**Answer: signed release manifest first, local DB encryption second.**

### Why signed release manifest goes first

1. **Larger blast radius** — without it, the entire `install.sh` + `bluey.rb` cask path is vulnerable to compromised release hosting. An attacker who takes over `bluey.dev/releases/...` ships malicious code to every customer that runs install or auto-updates. This is a one-host-pwn → all-customers-pwn pattern.

2. **Pairs with the existing distribution path** — codex already wired `SHA256SUMS.txt` verification in `install.sh`. Signing extends that to "verified by Bluey, not just hash-matched". One more file in the release artifact, one more verify step in the installer.

3. **Unblocks safe auto-update** — without signature verification, `bluey check-update` (queued for v0.2.x) would just be replacing one set of unsigned bytes with another. Signed manifest is the precondition for auto-update.

4. **Protects against the next install** — every future `curl … | bash` benefits.

### Why local DB encryption is P0.5, not P0

1. **Smaller blast radius** — protects against malware in the user's account that reads files on disk. But that same attacker can also:
   - Read the OS keyring entries that hold decryption keys (assuming your fix uses keyring-derived key)
   - Read the running process memory
   - Capture screen
   - Log keystrokes

   Encrypting the SQLite file alone doesn't add much against an account-compromised attacker. It does add against:
   - Backup snapshots taken when Bluey isn't running
   - Lost laptop scenarios where another user logs in
   - Cloud sync mistakes that include the SQLite file

   Real but narrower than the release-manifest threat.

2. **Operationally complex** — SQLCipher + envelope encryption + keyring rotation + key-recovery flow is significantly more code surface than a manifest signer. Higher implementation risk for arguably less protection.

3. **The cloud cache is still authoritative** — once cloud sync is reliable, the local DB is functionally a cache. Cloud-only customers who don't care about local cache encryption can opt out entirely.

### Suggested order

1. **P0 (next round)**: Signed release manifest with embedded ed25519 public key
2. **P0.5**: `bluey check-update` that verifies the signed manifest before replacing binaries
3. **P0.6**: Dependency audit CI (`cargo deny` / `cargo audit`) — cheap and catches Known Vulnerable Crates
4. **P1**: Local SQLite encryption via OS keyring-derived key (SQLCipher or envelope)
5. **P1.5**: Windows ACL parity for `0700`/`0600` equivalents
6. **P2**: Certificate pinning (carefully — risk of bricking clients on cert rotation)

---

## 7. Pipeline State

```
✅ cargo fmt --all --check
✅ cargo clippy --all-targets -- -D warnings (workspace + server)
🔴 cargo test --all-targets — 1 failed: load_mic_device_setting_round_trips (B-1)
✅ cargo test in server — 88 passed (Stripe sig + redaction tests included)
```

Need B-1 fixed before this batch is committable.

---

## 8. Operational doc-privacy reminder (from user)

The user explicitly flagged: **operational docs, review files, logs, session links, device codes, support diagnostics, IP/user-agent metadata, and preprod hostnames are sensitive. Do not publish the full `docs/reviews` tree publicly.**

Concrete implications:
- `docs/reviews/`, `docs/rounds/`, `docs/work/` should NOT be served from `bluey.dev` or any public site
- The `bluey-dev/homebrew-bluey` tap should contain ONLY `Casks/bluey.rb` — not the whole repo
- Any release tarball MUST exclude `docs/reviews/`, `docs/rounds/`, `docs/work/`, and any handoff files
- Recommended: add a `.gitattributes` `export-ignore` or release-workflow exclude pattern to drop these dirs from `git archive` outputs

This is operator-side hygiene, not a code commit. Track in `docs/PRELAUNCH-CHECKLIST.md` if not already there.

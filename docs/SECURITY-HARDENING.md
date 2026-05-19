# Bluey Security Hardening

> **Security posture, what we do today, what we know is missing.**
> Mirrors Pinky's `SECURITY-HARDENING.md`.
>
> Last updated: 2026-05-19, post v0.1.0 GA.

This doc tells the truth about Bluey's security posture. Anything that
contradicts what's written here is a bug or an outdated doc.

---

## 1. Threat model (v0.1)

Bluey v0.1 runs entirely on the user's machine, talks to upstream LLM
providers using the **user's own API keys** (BYOK), and keeps every
transcript / cue / RAG embedding in a local SQLite file.

| Adversary | What they can do | What we mitigate |
|---|---|---|
| Local malware on the user's Mac | read SQLite DB, scrape API keys, watch screen | OS keyring storage for keys, capture-excluded NSWindow for overlay, anti-debug helpers (PT_DENY_ATTACH macOS / IsDebuggerPresent Windows / TracerPid Linux), obfstr on endpoint URLs |
| Network attacker (passive) | observe LLM provider traffic | TLS 1.2+ via reqwest with rustls-tls (no plaintext) |
| Network attacker (active) | MitM, redirect to malicious provider | TLS cert validation; provider URLs obfstr'd to discourage trivial replacement |
| Co-process on the user's Mac | tamper with overlay IPC | per-session token, length caps, state-machine validation in production reader (R12.2) |
| Curious developer with `xattr` access | read transcript SQLite | not mitigated in v0.1 (BYOK + local-first; full DB encryption is v0.3+ work) |
| Compromised LLM provider | leak prompt content | inherent to BYOK; user accepts the trust by configuring the provider |

For Layer 3 (cloud) the threat model expands significantly. That doc
will be written when the product server is built.

---

## 2. What's in place today

### Overlay IPC handshake (R11–R12)

- **Per-session token** generated at daemon startup via `getrandom`
  (256 bits OS entropy, R12.3). Passed to the overlay binary via
  `BLUEY_OVERLAY_SESSION_TOKEN` env var. Every event the overlay
  emits MUST carry that token in its JSON payload.
- **Length caps** on every variable-length field of every overlay
  event. Oversized payloads are dropped + warned.
- **State-machine validation** on inner-form events: `AttachFilesRequested`
  is only accepted when the daemon's `OverlayUiState` is `AttachOpen`;
  `InstructionsUpdated` only when `InstructionsOpen`. Cancel/error paths
  reset to `Idle` via `OverlayUiStateScope` (R13.1).
- **Single-`Arc<Mutex<>>`** ownership of the state machine (R12.2) so
  daemon-side handler transitions are observed by the production
  reader thread immediately.

Production reader implementation: `crates/cue-daemon/src/app.rs::validate_and_decode_overlay_line`.

### Overlay binary verification (R7+)

- `crates/cue-daemon/src/overlay.rs::verify_overlay_binary` resolves
  the candidate overlay path to its canonical realpath, then asserts
  it's inside the daemon's install directory. Prevents running an
  arbitrary binary placed by another process.
- `BLUEY_OVERLAY_BIN` env override is respected only in dev mode
  (R10 hardening).

### Anti-debug helpers (R10)

- macOS: `PT_DENY_ATTACH` set during daemon startup, refuses ptrace
  attach (best-effort; root can still circumvent).
- Windows: `IsDebuggerPresent()` checked at startup.
- Linux: `/proc/self/status` `TracerPid` check.

These are deterrents, not protections. A motivated attacker with root
on the box owns the daemon either way.

### Process masquerading (R8)

- macOS: argv overwrite (best-effort; one-time at startup; preserves
  null-termination).
- Linux: `prctl(PR_SET_NAME)` (16-byte name limit; non-ASCII may be
  truncated by the kernel).
- Windows: source path exists in `cue-stealth/src/windows.rs` but is
  not exercised on a real Windows machine in v0.1.

### Compile-time obfstr on secrets-of-shape

- API endpoint URLs (`https://api.openai.com/...` etc.) and auth
  header names are wrapped in `obfstr!()` so a `strings(1)` scan over
  the binary doesn't trivially reveal them. This is friction, not
  security; a determined disassembler defeats it.

### Keyring storage for API keys

- `cue_daemon::secrets::store_api_key` / `load_api_key` use the
  `keyring` crate which talks to:
  - macOS Keychain
  - Windows Credential Manager
  - Linux Secret Service / kwallet
- Keys are NOT stored in the SQLite settings DB. R8 nit fixed:
  `save_settings` rejects keys whose name contains `api_key` with an
  explicit error rather than silently dropping them.

### Local SQLite database

- WAL mode enabled (`PRAGMA journal_mode=WAL`) for crash safety.
- Foreign keys enforced (`PRAGMA foreign_keys=ON`).
- File mode is the OS default (no special permissions). On macOS
  this is `~/Library/Application Support/bluey/` which is in the
  user's home + sandboxed by macOS file ACLs.

### LLM provider TLS

- All HTTPS calls go through `reqwest` with the `rustls-tls` feature.
  Native TLS is not used; rustls is built into the binary.

---

## 3. What's missing / honest gaps

### v0.1 known limitations

- **Distribution artifacts are not code-signed or notarized.** v0.1 is
  terminal-only; users may need to `xattr -d com.apple.quarantine`
  if they downloaded via a browser. Documented in `INSTALL.md`.
- **No SQLite-level encryption.** Transcripts + RAG embeddings are
  on disk in plaintext. v0.3 territory; depends on a decision about
  where the encryption key would live (passphrase-derived vs
  keyring-stored vs cloud-anchored).
- **No certificate pinning.** TLS validates the provider's cert chain
  via the system trust store; we don't pin specific issuers. A
  compromised CA could MitM provider traffic.
- **anti-debug is best-effort and macOS-tested.** Windows + Linux
  paths exist but haven't been exercised against real attack
  scenarios.
- **No tamper detection on the binary.** A modified `bluey-daemon`
  on a user's machine continues to run. Adding a self-integrity
  check (sha256 of own image vs a manifest) is future work.

### Gaps that show up at Layer 2

- **Distribution endpoint is unauthenticated.** Path A/B/C all assume
  `/downloads/` is public. That's correct for binaries but means
  anyone who knows the URL can pull them.
- **No artifact signing.** Tarball sha256s are published, but not
  signed. A future `bluey-server` can sign manifests with an
  ed25519 key whose pubkey ships with the client.

### Gaps that show up at Layer 3

When `bluey-server` lands:

- Auth + session management.
- Stripe webhook verification.
- Per-tenant rate limits + budget caps on the managed Auto Router endpoint.
- Privacy / data deletion endpoints.
- Audit logs.
- SOC2-class controls (not promising this; just listing it).

These are all R14.9 / Stage 2+ work.

---

## 4. Verification commands

### Confirm the production-overlay validator is wired

```bash
cargo test -p cue-daemon --test overlay_production_path
# expect 24 tests passing
```

Key tests (codex regression coverage):

- `handler_transition_idle_to_attach_open_unblocks_attach_files`
- `handler_transition_back_to_idle_blocks_late_attach_files`
- `cross_thread_arc_visibility`
- `token_match_pong_accepted_in_idle`
- `token_mismatch_event_rejected`
- `oversized_question_field_rejected`
- `production_overlay_drops_overlong_path`
- `transcript_text_with_inner_type_field_does_not_dispatch_inner`

### Confirm token randomness

```bash
cargo test -p cue-daemon --lib token
# expect 6 tests passing including
#   token_is_64_hex_chars
#   token_is_unique_across_calls
#   token_has_no_prefix_pattern_from_old_uuid_impl
```

### Manual: confirm overlay window is capture-excluded

1. `bluey on` on a real Mac.
2. Open Snipping Tool / Screenshot.app and capture the screen.
3. Pill should NOT appear in the capture.
4. If it does: `sharingType = .none` regression — file as bug
   immediately.

---

## 5. Reporting a security issue

For now: file an issue in this repo and tag it `security`. When the
project goes public + has a paying user base, this section grows into
a proper coordinated-disclosure policy + a security@ address.

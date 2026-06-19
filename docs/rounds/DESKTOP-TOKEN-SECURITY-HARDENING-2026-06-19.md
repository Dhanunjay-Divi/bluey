# Desktop Token Security Hardening — 2026-06-19

## What Changed

Bluey desktop account tokens now default to OS secure storage instead of the local account JSON profile.

- `cue-cloud-client::SecureAccountStore` is the default desktop token store for CLI, daemon, dashboard, deep-link login, and managed local RAG embeddings.
- The local account profile still stores non-secret metadata: provider, API URL, user id, workspace id, device id, and linked time.
- Access and refresh tokens are stripped before writing the account profile.
- Older plaintext account-file tokens are migrated into OS secure storage on first load, then cleared from disk.
- Plaintext account-file token fallback exists only for local development with `BLUEY_ALLOW_PLAINTEXT_TOKENS=1` or `BLUEY_DEV_PLAINTEXT_TOKENS=1`.
- `bluey doctor` now reports token presence from a bounded OS-secure-store probe instead of looking for tokens in `account.json`.

## What Was Already In Place

- Provider keys for OpenAI, Anthropic, Deepgram, Gemini, and other managed providers live server-side only.
- Customer passwords are bcrypt-hashed on the server.
- Server refresh tokens are stored as hashes, not raw refresh tokens.
- Signed update manifests protect the auto-update path from trusting unsigned or tampered release metadata.
- Local files use private file/directory permissions where the platform supports them.

## Honest Boundary

This does not and cannot make an installed desktop binary impossible to inspect. A local admin/root user, malware running as the user, or a determined reverse engineer can inspect processes, memory, files, and binaries. Bluey’s practical protection is to avoid shipping provider secrets, store account tokens in OS secure storage, keep billing and model routing server-side, sign updates, and avoid logging secrets.

## Files Changed

- `crates/cue-cloud-client/src/tokens.rs`
- `crates/cue-cloud-client/src/lib.rs`
- `crates/cue-cli/src/doctor.rs`
- `crates/cue-cli/src/app.rs`
- `crates/cue-daemon/src/app.rs`
- `crates/cue-daemon/src/rag_indexer.rs`
- `crates/cue-dashboard/src/commands.rs`
- `crates/cue-dashboard/src/lib.rs`
- `web/index.html`
- `docs/SECURITY-HARDENING.md`
- `docs/HOW-IT-WORKS.md`
- `docs/rounds/END-TO-END-FLOW-AND-LIVE-TEST-GUARD-2026-06-04.md`

## Verification

Run:

```bash
cargo fmt --all --check
cargo test -p cue-cloud-client --all-targets
cargo test -p cue-daemon --all-targets
cargo test -p cue-cli --all-targets
cargo test --manifest-path server/Cargo.toml --lib
cargo clippy --all-targets -- -D warnings
```

Recommended live smoke after build:

1. Remove old local tokens or use a fresh macOS user profile.
2. Run `bluey on` and complete browser sign-in.
3. Confirm `account.json` has `access_token: null` / no token fields while balance and cloud answer routes still work.
4. Run `bluey logout` and confirm both Keychain/Credential Manager tokens and any legacy account-file tokens are cleared.

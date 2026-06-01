# Bluey Security Hardening

> Security posture, hard boundaries, client-protection strategy, and known gaps.
>
> Last updated: 2026-05-21, after managed cloud sync/RAG/STT auth and local
> private-permission hardening.

This doc is intentionally blunt: code that runs on a customer's machine can be
inspected, patched, dumped, and reverse engineered by a motivated local attacker.
Bluey's production security model must not depend on the desktop binary being
unreadable. The hard boundary is the server. The client is treated as an
untrusted, cache-capable interface.

Anything that contradicts this document is either outdated or a bug.

---

## 1. Non-Negotiable Security Model

### What we can protect strongly

- Provider API keys.
- Billing, account-credit state, request pricing, and metering.
- Account/session authorization.
- Server-side routing policy.
- Cloud transcript/RAG storage tenancy.
- Short-lived STT relay sessions.
- Release/update integrity once manifest signing lands.

### What we cannot make impossible

- Reverse engineering the desktop binary.
- Reading UI strings, local logic, and static assets from an installed app.
- Patching local branches in a modified binary.
- Bypassing client-side checks on a rooted/admin-owned machine.
- Extracting local plaintext files if the user's account or device is already compromised.

So Bluey's rule is:

> Never put a secret or business-critical decision exclusively in the desktop client.

Obfuscation, anti-debug, process naming, and self-checks are friction. They are
not security boundaries.

---

## 2. Current Threat Model

| Adversary | What they can do | Bluey posture |
|---|---|---|
| Curious customer with the installed binary | inspect strings, disassemble, patch local checks | no provider keys in client; server validates account credits, routing, STT session, and usage |
| Local malware in the user's account | read local DB/JSON, screen, process memory | OS keyring for auth tokens/API keys, private local file permissions, capture-excluded overlay, short-lived server tokens |
| Co-process sending fake overlay events | attempt IPC injection | per-session overlay token, length caps, state-machine validation, install-dir binary verification |
| Network attacker | observe or tamper with traffic | HTTPS/TLS via rustls; server-side auth; no static provider secrets on desktop |
| Modified Bluey client | send malformed requests, replay tokens, claim fake usage | server owns billing, idempotency, credit hard stops, request validation, STT relay token claim |
| Compromised Bluey server | access managed transcripts/RAG/provider keys | out of client scope; requires server ops hardening, secret rotation, backups, audit trails, and least-privileged infra |

---

## 3. What Is In Place

### Server-owned provider access

- Managed LLM/STT flow keeps upstream provider keys on `bluey-server`.
- Desktop login stores only Bluey account tokens in OS keyring.
- Customer desktop no longer needs Deepgram/OpenAI/Anthropic keys in normal managed mode.
- Direct BYOK/dev provider paths are gated behind explicit development flags such as `BLUEY_DEV_BYOK=1`.

### Managed STT relay

- `POST /stt/session` performs account/balance/trial checks before creating a relay session.
- `/stt/relay` requires the normal Bearer account token plus a Bluey-scoped session token.
- Relay tokens are random, short-lived, account-bound, and single-claimed.
- The server connects to Deepgram using server-held credentials; the desktop never receives the Deepgram key.
- Relay close records elapsed time so billing can reconcile the STT session.

### Cloud sync and RAG tenancy

- `/sync/batch`, `/sync/sessions`, `/sync/sessions/:session_id`, and `/rag/query` are auth-protected.
- Server tables are keyed by `account_id`; handlers load data through the authenticated account boundary.
- Batch and field sizes are bounded to reduce abuse and accidental giant uploads.
- Account export/delete paths include cloud sessions, transcripts, responses, artifacts, and RAG counts.

### Overlay IPC hardening

- Per-session overlay token generated from OS entropy and passed to the helper.
- Every overlay event must include the token when production validation is active.
- Length caps on every variable field.
- Event state machine prevents out-of-context attach/instructions events.
- Overlay binary resolution verifies that the helper lives inside the install directory.
- Developer overlay override and capture-visible debug mode are gated; `BLUEY_HOST_OVERLAY_CAPTURE_VISIBLE=1` is ignored unless the dev overlay gate is also enabled.

### Local filesystem hardening

- `AppPaths::ensure()` now creates data/config/runtime directories with private Unix permissions (`0700`).
- Meeting archive directory is private (`0700`).
- Active and archived meeting JSON files are written with private Unix permissions (`0600`).
- Local SQLite DB files are created/normalized with private Unix permissions (`0600`), with WAL/SHM sidecars normalized when present.
- Account/settings JSON already use private file permissions and API-key-shaped settings are rejected from non-secret settings storage.

### Keyring-backed local secrets

- Account tokens use `cue-cloud-client` keyring storage.
- STT API keys in developer paths use keyring-backed commands.
- Dashboard settings rejects secret-shaped keys instead of storing them in normal settings.

### Release/install integrity

- `scripts/install.sh` verifies `SHA256SUMS.txt` before installing downloaded archives.
- Native overlay helper verification enforces canonical install-dir containment,
  and now verifies a colocated SHA-256 sidecar when one is present.
- Operational install scripts ad-hoc sign macOS bundles and strip quarantine for the current Pinky-style alpha path.
- This is not the same as signed release manifests. SHA256 over HTTPS protects against accidental corruption and simple mirror mistakes; it does not protect against a compromised release host.

### Client-side friction

- Compile-time `obfstr` wraps provider URLs/header names in direct-provider code paths.
- Workspace release builds strip symbol tables and use thin LTO so distributed
  binaries expose less incidental implementation detail.
- Anti-debug helpers exist for macOS/Windows/Linux as best-effort deterrents.
- These measures are useful speed bumps, not trust anchors.

---

## 4. Pinky Parity Notes

Pinky's security wording maps well to Bluey, with one key adjustment: Bluey is
more server-managed, so provider access and metering should be even less
desktop-dependent.

| Pinky control | Bluey status |
|---|---|
| Hardened auth: bcrypt, JWT validation, token invalidation | Present in `bluey-server`: bcrypt cost 12, JWT access/refresh split, hashed refresh tokens, atomic refresh consume/revoke paths |
| Billing/replay/data access ownership gates | Present: managed router uses authenticated account, balance ledger, idempotency, account-scoped sync/RAG/export/delete |
| SQL parameterization | Present across reviewed Rust/SQLite paths via `rusqlite::params!`; keep reviewing every new raw SQL call |
| Host helper integrity SHA sidecars | Now partially present: overlay helper verifies sidecar if shipped; signed release manifest is still stronger and still P0 |
| Debug capture-visible escape hatch blocked from shipping | Present: capture-visible overlay mode is gated behind dev overlay enablement |
| Capture-excluded overlay | Present for normal macOS overlay paths; privileged capture/EDR can still see anything |
| Preprod/prod separation | Operationally documented; must be validated during deployment |
| Stripe multi-`v1` signature validation | Present: webhook verifier accepts any matching `v1`, enforces timestamp tolerance, and uses constant-time comparison |
| Sensitive logs/docs/session links | Tightened this round: cloud-client error bodies are redacted/truncated, Stripe error body logs redact URL/client-secret/payment-method fields, deep-link parse failures suppress raw URLs, and verification/reset links are not logged unless `BLUEY_DEV_LOG_AUTH_LINKS=1` is explicitly set |
| Terms/privacy/dispute logging review | Partially documented; legal copy and customer-facing policy still need final pass |

Recommended Bluey wording mirrors Pinky's honest version:

> Low-profile native overlay, capture-excluded in normal OS capture paths,
> server-managed provider access, account-scoped cloud memory, hardened auth,
> and audited release controls.

Do not use wording like "unbacktraceable", "undetectable", or "impossible to
reverse engineer".

---

## 5. Still Missing Before A Wider Paid Launch

### P0 Security Work

| Item | Why it matters | Direction |
|---|---|---|
| Signed release manifest | prevents compromised hosting from silently swapping binaries | ed25519-sign `latest.json` / archive hashes; embed public key in client installer/update check |
| Local DB encryption | protects local transcripts/RAG from casual file reads | SQLCipher or application-level envelope encryption using Keychain/DPAPI/Secret Service key |
| Server-owned transcript/RAG source of truth | avoids relying on local cache for continuity | continue cloud sync; add dashboard cloud session restore |
| Redaction audit | prevents secrets/tokens/transcripts leaking to logs | keep expanding tests around token/API-key/body logging; operational docs/logs/session links must be treated as private artifacts |
| Updater verification | unsigned alpha install needs safe update story | `bluey check-update` should verify signed manifest before replacing binaries |
| Rate-limit and abuse visibility | production API needs operator signals | metrics, alerts, per-account/per-IP throttles, suspicious retry counters |

### P1 Hardening

| Item | Why it matters | Notes |
|---|---|---|
| Certificate pinning / trust strategy | reduces CA/MitM blast radius | do carefully; pins can brick clients during cert rotations |
| Binary self-integrity check | detects naive local binary edits | friction only; modified clients can patch it out |
| Symbol stripping + release profile hardening | reduces easy static inspection | use `strip`, LTO, `panic = "abort"` where safe |
| Dependency audit | catches vulnerable crates | add `cargo deny` / `cargo audit` to CI |
| Cloud KMS/secrets manager | keeps provider keys out of env files long-term | initial droplet can start with env files; migrate before scale |
| Postgres + pgvector migration | robust multi-user cloud data plane | SQLite is fine for local cache and early server prototype; Postgres is the production cloud store |

---

## 6. What Not To Do

- Do not ship provider API keys in the desktop binary.
- Do not trust a client-supplied cost, provider, model, or usage number.
- Do not treat obfuscation as protection for billing or data access.
- Do not make the local fallback grant free server credits automatically; reconcile explicitly when online.
- Do not promise that code is unreadable or attack-proof.
- Do not leave debug visibility flags enabled in customer builds.

---

## 7. Verification Commands

### Local private permissions

```bash
cargo test -p cue-core app_paths::tests::ensure_uses_private_directory_permissions
cargo test -p cue-daemon storage::security_tests::meeting_store_writes_private_files
cargo test -p cue-daemon db::tests::file_database_is_created_private
```

### Overlay IPC validator

```bash
cargo test -p cue-daemon --test overlay_production_path
```

### Managed STT auth path

```bash
cd server
cargo test stt::tests
```

### Cloud sync/RAG auth path

```bash
cd server
cargo test sync_batch_round_trips_session_bundle_and_rag
cargo test validate_rejects_empty_or_huge_batches
```

### Install checksum path

```bash
bash scripts/install.sh --help
```

---

## 8. Reporting A Security Issue

For now: file an issue in the private repo and tag it `security`.

Before public launch, replace this section with:

- `security@bluey.sh`;
- coordinated disclosure policy;
- severity/SLA table;
- private security advisory workflow;
- release-note redaction rules.

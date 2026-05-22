# Round Handoff: Observability Phase 4 — `bluey doctor` + `bluey logs export --redact`

```
Branch:     feat/phase-3-round-12
Tip before: 1f11729
Tip after:  <hash> (HEAD after this round commits)
Author:     Kiro
Reviewer:   Codex
Round of:   Observability Round, Phase 4
Round shape: small (single crate, 2 new modules, ~600 LOC + 16 tests)
```

## 1. What changed

| File | Lines | Change |
|---|---|---|
| `crates/cue-cli/src/doctor.rs` | +281 | NEW. `bluey doctor` — redacted self-diagnosis snapshot |
| `crates/cue-cli/src/logs.rs`   | +275 | NEW. `bluey logs export [--no-redact]` — bundle + redact + zip log files |
| `crates/cue-cli/src/lib.rs`    | +2  | Wire new modules |
| `crates/cue-cli/src/app.rs`    | +44 | `Doctor` + `Logs { command }` variants in `Commands` enum + `LogsCommands` subcommand enum + dispatch arms |
| `crates/cue-cli/Cargo.toml`    | +5  | Deps: `regex`, `once_cell`, `zip`, `sha2`, `hex` |

Total: 2 new modules, 5 new dependencies, 16 unit tests, 0 dependencies on Phase 1/2/3 of the Observability Round.

## 2. Why

Per `docs/rounds/OBSERVABILITY-ROUND-PLAN.md` §6, `bluey doctor` is the
first-line support tool: a customer hits a problem, runs `bluey doctor`,
pastes the output, support has enough info to triage without asking for
five round-trips of "what version are you on, what permissions are granted,
where is your config dir."

`bluey logs export --redact` is the second-line tool: when doctor isn't
enough, support asks the customer to bundle their logs. The redactor
strips secrets BEFORE the customer mails the zip. Customer doesn't have
to grep their own logs for tokens.

Per the collaboration contract §11, this is the kiro-owned phase of the
Observability Round. Codex owns Phase 1 (Foundations: trace_id, request_id
middleware) and Phase 2 (rotated logs), which this Phase 4 will build on
once those land. **Phase 4 is intentionally implemented to work today
against current state**, with explicit messaging when Phase 2's rotation
isn't yet in place.

## 3. Detailed design

### 3.1 `bluey doctor`

Emits 7 sections, each labeled with a `── Section ──` header. Section
output is written to stdout, not stderr; one line per fact, aligned for
copy-paste readability:

| Section | Content |
|---|---|
| Build | `bluey version`, build profile (release/debug), git commit (only if `GIT_COMMIT_SHA` env was set at compile time) |
| Platform | `os`, `arch`, `sw_vers` output on macOS |
| Paths | data/config/runtime dirs + account.json/settings.json/state.json with file mode in octal |
| Account | logged in/out, provider, api_url, hashed user_id (12-char SHA-256 prefix), workspace_id, hashed device_id, linked_at, access/refresh token presence (NOT values) |
| macOS Permissions | text-only guidance (Accessibility/Mic/Screen Recording — verify in System Settings); F19 hotkey-debugging hint |
| Local DB | path, mode, size in bytes; falls back to "no local DB" when not yet created |
| Daemon Logs (tail) | last 20 lines of newest `daemon-*.log` if `~/Library/Logs/Bluey/` exists; otherwise documents "log rotation is Phase 2 dependency" |

**`account_id_hash_prefix(account_id) → 12 hex chars`** is the standard
hash function the Observability Round will use everywhere `account_id_hash`
is emitted. SHA-256 of the account_id, take first 6 bytes (12 hex chars).
48 bits is enough to disambiguate within one customer's logs.

**Redacted-by-default** — emails, real account IDs, and tokens are not
emitted as plaintext. The doctor output is intentionally safe-to-paste
into a public support ticket.

### 3.2 `bluey logs export`

Default: redaction ON. To opt out: `--no-redact`. The flag name was
chosen carefully: opt-IN flags for redaction are dangerous defaults
because forgetting it leaks secrets; opt-OUT requires conscious action.

Flow:

1. Discover log directory (`~/Library/Logs/Bluey/` on macOS,
   `~/.local/state/bluey/log/` on Linux).
2. If directory missing → print actionable message that mentions Phase 2
   dependency. Exit clean.
3. Find `*.log` files modified in the last `--days N` (default 7).
4. For each file: read, redact (if redact enabled), write into ZIP with
   0o600 perms preserved at the entry level.
5. Output zip is written 0o600 too. Default path is
   `~/Bluey-logs-YYYYMMDD-(redacted|raw).zip`.

**Redactor** (`logs::redact_log_content`) — regex patterns:

| Pattern | Replacement |
|---|---|
| JWT-shaped (3 base64url segments separated by `.`) | `<jwt>` |
| `Bearer <token>` | `Bearer <token>` |
| `bluey://<anything>` magic-link URLs | `bluey://<redacted>` |
| Stripe IDs (`cus_`, `pi_`, `sk_`, `whsec_`, `cs_`, `price_`, …) | `<stripe_id>` |
| Anthropic keys (`sk-ant-…`) — runs BEFORE OpenAI | `<anthropic_key>` |
| OpenAI keys (`sk-…`) | `<openai_key>` |
| Deepgram-shaped 40-char hex | `<provider_key>` |
| Email addresses | `<email>` |
| IPv4 addresses | `192.168.4.0/24` (last octet → 0/24) |
| Device codes | `code=<redacted>` |

**Preserved** for support correlation:
- `session_id=...` values
- `account_hash=<12-hex>` values (already hashed)
- `request_id`, `trace_id` once Phase 1 lands

Order matters. Anthropic runs before OpenAI because `sk-ant-…` matches
both patterns; the more-specific prefix needs first claim. AUTH_HDR was
removed because it overlapped greedily with BEARER and JWT.

## 4. Verification

```
✅ cargo fmt --all (clean)
✅ cargo clippy --all-targets -- -D warnings (clean)
✅ cargo test --all-targets:  453 passed, 0 ignored (workspace)
✅ cargo build --release -p cue-cli --bin bluey
✅ ./target/release/bluey doctor — runs, emits 7 sections cleanly
✅ ./target/release/bluey logs export — clean message when no log dir yet
✅ ./target/release/bluey logs export --help — flag docs render correctly
```

16 new unit tests in `cue-cli`:
- `doctor::tests::account_id_hash_prefix_is_stable_and_short`
- `doctor::tests::build_profile_returns_known_value`
- `logs::tests::redact_strips_bearer_token`
- `logs::tests::redact_strips_magic_link`
- `logs::tests::redact_strips_stripe_ids`
- `logs::tests::redact_strips_openai_key`
- `logs::tests::redact_strips_anthropic_key`
- `logs::tests::redact_strips_email`
- `logs::tests::redact_masks_ipv4_to_slash24`
- `logs::tests::redact_strips_jwt`
- `logs::tests::redact_preserves_session_id_and_account_hash`
- `logs::tests::redact_strips_device_code`
- `logs::tests::current_yyyymmdd_is_eight_digits`
- `logs::tests::default_output_path_includes_suffix`

Smoke output of `bluey doctor` on uno (real, not mocked):
```
============================================================
 Bluey Doctor
 (unix=…)
============================================================

── Build ──
  bluey version : 0.1.0
  build profile : release
  git commit    : (not embedded)

── Platform ──
  os            : macos
  arch          : aarch64
  ProductName:    macOS
  ProductVersion: 26.3
  BuildVersion:   25D125

── Paths ──
  data dir      : /Users/uno/Library/Application Support/cue (exists=true, mode=700)
  config dir    : /Users/uno/Library/Application Support/cue (exists=true, mode=700)
  …
```

## 5. Areas most likely wrong (focus your review here)

1. **The `current_yyyymmdd` function** is a hand-rolled civil-from-days
   conversion (Howard Hinnant). It avoids pulling in `chrono` for one
   timestamp format. Edge cases not exhaustively tested. If you'd rather
   pull in `chrono` (already a dep elsewhere), I'm fine with that.

2. **Permissions probe is text-only.** Doctor doesn't actually call
   `AXIsProcessTrusted` or `AVCaptureDevice.authorizationStatus` because
   those require objc2 deps for one probe. Worth a follow-up to add real
   probes via objc2-app-kit / objc2-av-foundation. For now the output is
   actionable ("verify in System Settings"). If you think real probes
   should land in this round, push back.

3. **DB summary is paths-only.** Doctor doesn't open the SQLite to count
   rows because `cue-cli` doesn't have a `rusqlite` dep and adding one
   bloats the CLI binary. Hint message tells the user how to run
   `sqlite3 .tables` manually. Worth a follow-up if you think the row
   counts are critical for support.

4. **Deepgram regex is conservative** (`\b[a-f0-9]{40}\b`). May match
   non-key 40-char hex strings (commit SHAs, content hashes). Conservative
   bias is "redact more, not less" — accept false positives.

5. **No log file is written by Bluey today.** Both commands gracefully
   handle this. Once Phase 2 (codex) lands rotation, Phase 4 should
   "just work" against the new files. No code changes needed in Phase 4.

6. **The flag is `--no-redact`, not `--redact`.** Default redact-on,
   opt-out style. Contract for support: tell customers `bluey logs export`
   (no flags) gives them a redacted zip. Push back if you want
   `--redact=false` style instead.

## 6. What this round did NOT do

- **Phase 1** (foundations: trace_id propagation, request-id middleware,
  account_id_hash threading through tracing fields). Codex-owned.
- **Phase 2** (daemon + dashboard log rotation via `tracing-appender`).
  Codex-owned. Phase 4 is forward-compatible: works today, will pick up
  rotated logs the moment they appear.
- **Phase 3** (overlay lifecycle emits + frontend Tauri error capture).
  Codex-owned.
- **Phase 5** (trace propagation through Tauri invoke/IPC). Codex-owned.
- **Phase 6** (standard field migration sweep). Kiro-owned, queued after
  Phase 1.
- Real macOS permission probes (Accessibility/Mic/Screen Recording).
  Doctor outputs guidance, not auto-checks. Could be a Phase 4.5
  follow-up if you find the guidance insufficient.
- SQLite row-count summary in doctor. Hint message points users to
  `sqlite3` instead.

## 7. Honest limitations

- **No Windows support** in either command. macOS + Linux paths only.
  Windows would need `%LOCALAPPDATA%/Bluey/Logs` + ACL handling.
- **No log-file streaming.** Whole-file `read_to_string` — fine for
  daily-rotated logs (max ~10s of MB) but would fail for unbounded
  growth. When Phase 2 rotation lands, it should cap each file at 50 MB
  with retention windowing.
- **Redactor regexes are heuristic.** The redactor catches the patterns
  we know about today; some new credential format shipped tomorrow
  won't be caught until we add a pattern. Phase 6's field migration
  sweep should reduce reliance on regex by emitting structured logs
  with named fields that can be redacted by name (like the cloud-client
  does for JSON bodies).

## 8. Next-round consequences

- Phase 5 (codex) trace propagation should emit `trace_id=<uuid>` as a
  structured field. The redactor preserves anything not in its pattern
  list, so trace_id will pass through unchanged. **Verify with a unit
  test in Phase 5 that adds a sample line containing a trace_id and
  asserts it's preserved.**
- Phase 2 (codex) should emit `daemon-YYYY-MM-DD.log` in
  `~/Library/Logs/Bluey/` with mode 0600. Phase 4's discovery code
  expects exactly that pattern.
- Phase 6 (kiro, after Phase 1) — when `tracing` calls switch to the
  standard fields struct, the redactor's pattern list can shrink because
  fields will already be structured.

## 9. Reviewer checklist

- [ ] Code review: `crates/cue-cli/src/doctor.rs`, `crates/cue-cli/src/logs.rs`
- [ ] Wiring review: `crates/cue-cli/src/app.rs` enum + dispatch
- [ ] Smoke: `./target/release/bluey doctor` runs cleanly
- [ ] Smoke: `./target/release/bluey logs export --help`
- [ ] Verify the redactor catches everything in your local logs (if
      Phase 2 rotation has shipped on your branch)
- [ ] Confirm `--no-redact` flag naming is acceptable

## 10. Verdict request

Per the collaboration contract §4, write the verdict at
`docs/reviews/REVIEW-OBSERVABILITY-PHASE-4-BY-CODEX.md`.

If 🟢, Phase 4 is closed. We move to Phase 1 (Foundations, codex) +
Phase 6 (Standard fields sweep, kiro) — or your preferred next
sub-phase.

If 🟡 / 🔴, name the items + I'll fix as a `fix(...)` commit per the
self-implementation rule.

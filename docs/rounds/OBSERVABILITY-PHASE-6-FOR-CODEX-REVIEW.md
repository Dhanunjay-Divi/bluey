# Round Handoff: Observability Phase 6 — standard field migration sweep

```
Branch:     feat/phase-3-round-12
Tip before: 0485a9b
Tip after:  <hash> (HEAD after this round commits)
Author:     Kiro
Reviewer:   Codex
Round of:   Observability Round, Phase 6
Round shape: small (mechanical sweep, 6 files, 31 changes)
```

## 1. What changed

| File | account_id → account_id_hash | email drops | Notes |
|---|---|---|---|
| `server/src/api/account.rs` | 1 | 1 | GDPR hard-delete log |
| `server/src/api/auth_routes.rs` | 8 | 8 | Email verify + password reset paths (4 each) |
| `server/src/api/router.rs` | 10 | 0 | Dispatch/deduct paths |
| `server/src/api/stt.rs` | 1 | 0 | Relay overrun warn |
| `server/src/api/usage.rs` | 1 | 0 | Usage event record failure |
| `crates/cue-dashboard/src/lib.rs` | 0 | 1 | Deep-link login success log |
| `scripts/migrate-tracing-fields.py` | NEW | | Reusable migration tool |
| **Total** | **21** | **10** | **31 mechanical changes** |

## 2. Why

Per the Observability Round plan §11, Phase 6 sweeps existing tracing
call sites to use the standard fields surface that Codex's Phase 1
landed:

- `account_id` → `account_id_hash = %cue_core::account_id_hash_prefix(...)` —
  raw account IDs no longer in logs; hashed prefix is the support
  join key (12 hex chars from SHA-256 first 6 bytes).
- `email` field → dropped — `account_id_hash` is sufficient for
  support correlation; email re-derivable by support if they have
  the account_id. This removes 10 PII surfaces from the log stream.

## 3. Verification

```bash
python3 scripts/analyze-tracing-calls.py
```

**Before** (at `0485a9b`):
```
Transitional findings: 22 (21 account_id + 10 email + 1 session-alias false positive)
```

**After** (at this commit):
```
account_id (raw):      0
email field:           0
account_id_hash:       21
session-alias false positive: persisted (analyzer parser limitation, not a real site)
```

Pipeline:

```bash
cargo fmt --all --check                     ✅
cargo clippy --all-targets -- -D warnings   ✅ (workspace + server)
cargo test --all-targets                    ✅ 459 passed (no regressions)
cd server && cargo test                     ✅ 90 passed
```

## 4. Migration script

`scripts/migrate-tracing-fields.py` is the reusable sweep tool. Three
modes:

- `--dry-run` (default): preview unified diffs without writing
- `--apply`: write changes to disk

Pattern set (regex-based, sed-style):

| Pattern | Action |
|---|---|
| `account_id = %<expr>` | → `account_id_hash = %cue_core::account_id_hash_prefix(&<expr>)` |
| `email = %<expr>,` (first/middle position) | drop the field + trailing comma+whitespace |
| `, email = %<expr>` (last position) | drop the leading comma + field |
| `session = X` (bare alias inside a tracing call) | → `session_id = X` |

The script processes a fixed list of target files derived from
`scripts/analyze-tracing-calls.py`. Future rounds can extend the
target list as new transitional fields appear.

## 5. Areas most likely wrong (focus your review here)

1. **Migration regex is paren-balanced via line heuristic, not a Rust
   parser.** Could miss exotic field expressions or break on
   pathological multi-line cases. I sanity-checked the diff before
   committing; no regressions in clippy/test. If you find a missed
   site, the analyzer will flag it and we run the migration again with
   an expanded pattern.

2. **`email` field is dropped, not replaced with a hash.**
   This is intentional per the Phase 6 plan §3.3: account_id_hash is
   sufficient for support correlation; emails are re-derivable on the
   server side. If your review thinks emails should be hashed and
   retained as `account_email_hash`, push back. I think drop is right
   because emails are not the join key.

3. **`account_id_hash_prefix` is called inline in every macro
   invocation.** Each call hashes the account_id at log time. The
   hash is cheap (SHA-256 of a short string), but the formatting
   could be lazy via `tracing`'s field skipping. For v0.2 alpha, the
   inline call is fine.

4. **The "session → session_id (1 site)" alias the analyzer reported is
   a false positive.** It's a string-literal parsing edge case in
   `scripts/analyze-tracing-calls.py` — a quoted message contains
   the substring `session = ` and gets misclassified as a field name.
   Manually verified: zero actual `session = ` field call sites exist.
   I deliberately did NOT fix the analyzer parser in this round to
   keep Phase 6 mechanical. Option for future: tighten the FIELD_PATTERN
   regex to require the field name appears outside string literals.
   Tracked in this handoff §8.

5. **Conformant call count unchanged: 47/206.**
   Phase 6 was a field-rename sweep, not a context-threading sweep.
   The non-conformant count (159) reflects info/warn/error calls that
   lack `trace_id` / `request_id` / `session_id`. Those will be closed
   by Phase 5 (codex-owned, daemon IPC + Tauri invoke trace minting)
   for the daemon-side calls, plus a future Phase 7 that adds
   per-handler `request_id` extraction in server handlers (currently
   only middleware emits it).

## 6. Honest limitations

- **No new tests added.** The migration is a refactor; existing tests
  cover the behavior of the surrounding code. The analyzer is the
  acceptance check that the field-shape contract is met. Adding a
  CI hook to run `analyze-tracing-calls.py --check-only` would lock
  this in for new commits — proposed for follow-up.

- **Phase 4 redactor unchanged.** Phase 4's `redact_log_content` (in
  `crates/cue-cli/src/logs.rs`) preserved `account_id_hash` and
  `session_id` already. No Phase 4 changes needed; verified by
  re-reading the redactor regex set.

- **`bluey doctor` unchanged.** Phase 1 already deduplicated doctor's
  `account_id_hash_prefix` to `cue_core::account_id_hash_prefix` in
  `9cd66d4`. Phase 6 is consistent with that.

- **Handler-level `account_id_hash` extraction NOT added in middleware.**
  The middleware runs before auth, so it cannot know the account.
  Per-handler additions of `account_id_hash` happened in this Phase 6
  sweep where the handler had `account.id` in scope (e.g.,
  `auth_routes.rs`, `router.rs`). The middleware itself stays at
  request_id + trace_id + status + latency_ms.

## 7. Reviewer checklist

- [ ] Sample-read `server/src/api/auth_routes.rs` lines 397-420 to
      verify the migration shape is what you expect.
- [ ] Sample-read `crates/cue-dashboard/src/lib.rs` line 638 to verify
      the email-only call drops cleanly to `tracing::info!("deep-link
      login success")`.
- [ ] Run `python3 scripts/analyze-tracing-calls.py` and confirm
      `account_id` and `email` are gone from the field histogram.
- [ ] Verify `cargo test --all-targets` is 459 passing (workspace) +
      90 (server) with no regressions.
- [ ] Decide whether to fold a CI gate (`analyze-tracing-calls.py
      --check-only`) into the prelaunch checklist now or in a separate
      round.

## 8. Followups

For the followup pile (not Phase 6 work):

- Add `analyze-tracing-calls.py --check-only` mode + CI hook
  (zero-non-conformant policy or zero-transitional policy as a gate).
- Expand `observe!` macro to support extras like `method` / `path`
  so the server middleware can emit through it.
- Tighten `FIELD_PATTERN` regex in the analyzer to ignore matches
  inside string literals (closes the false-positive on `session`).
- Phase 5 (codex) will add daemon-side trace propagation, which will
  bring most of the 159 non-conformant info/warn/error calls into
  the conformant set automatically (because they'll inherit a
  `trace_id` from context).

## 9. Verdict request

Per the collaboration contract §4, write the verdict at:

`docs/reviews/REVIEW-OBSERVABILITY-PHASE-6-BY-CODEX.md`

If 🟢, Phase 6 is closed and the Observability Round (per the original
plan) will need only Phases 2, 3, 5 to fully close — all codex-owned.

If 🟡 / 🔴, name the items and I'll fix as a `fix(...)` commit per the
self-implementation rule in §5.

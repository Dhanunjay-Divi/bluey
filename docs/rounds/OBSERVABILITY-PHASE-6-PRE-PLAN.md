# Observability Phase 6 — Standard Field Migration Sweep (Pre-Plan)

```
Branch:     feat/phase-3-round-12
Tip:        8b9c24a (Phase 4 closed)
Owner:      Kiro
Depends on: Codex's Phase 1 (standard fields struct in cue-core)
Status:     PRE-PLANNED — not started, waiting on Phase 1 to land
```

## 1. Why this pre-plan exists now

Codex is implementing Phase 1 (Foundations: standard fields struct,
`account_id_hash` helper, trace_id propagation, request-id middleware).
Phase 6 cannot start until Phase 1 names things. But we can
**pre-measure** what Phase 6 has to change so the sweep is mechanical
when it does start.

This doc captures:

1. The actual state of tracing call sites in the codebase TODAY,
   measured by `scripts/analyze-tracing-calls.py`.
2. The migration approach Phase 6 will use.
3. The acceptance criteria.

---

## 2. Current state (measured)

`scripts/analyze-tracing-calls.py` against `8b9c24a`:

```
Total call sites:  204
Conformant:        45  (has at least one of trace_id / request_id / session_id)
Non-conformant:   159  (missing all join-key fields)
```

### 2.1 By level

| Level | Count |
|---|---|
| warn | 115 |
| info | 41 |
| debug | 30 |
| error | 18 |

### 2.2 By component

| Component | Total | Notes |
|---|---|---|
| cue-daemon | 93 | Largest surface. Sweep target. |
| bluey-server | 51 | Already partially conformant (request_id middleware). |
| cue-dashboard | 44 | Tauri side. Smaller migration. |
| cue-stealth | 8 | Anti-debug + disguise call sites. |
| cue-cloud-client | 4 | Already redaction-aware; small migration. |
| cue-llm | 2 | |
| cue-router | 2 | |

### 2.3 Top field usage (current)

| Field | Count | Status |
|---|---|---|
| `error` | 71 | ✓ Standard (canonical for Error chain) |
| **`account_id`** | **21** | △ **Migrate → `account_id_hash`** |
| `provider` | 15 | ✓ Standard |
| **`email`** | **10** | △ **Drop or hash** |
| `request_id` | 7 | ✓ Standard |
| `status` | 4 | ✓ Standard |
| `model` | 3 | ✓ Standard |
| `session_id` | 2 | ✓ Standard |

### 2.4 Field-name inconsistencies (already canonical-ish)

Only one alias in the codebase: `session` → `session_id` (1 site).
Codex has been keeping field names canonical. **The migration shouldn't
need a regex sweep for `req_id` vs `request_id` style fixes** — those
don't exist.

### 2.5 PII findings in message strings

**Zero.** The Pinky leak-review parity work caught these — there are
no email-shaped, key-shaped, or stripe-id-shaped strings embedded in
tracing message format strings. Phase 6 doesn't have to chase down
hidden secrets in messages.

### 2.6 Transitional fields (the actual sweep work)

| Field | Sites | Migration |
|---|---|---|
| `account_id` | 21 | Replace with `account_id_hash = %account_id_hash_prefix(...)` |
| `email` | 10 | Drop the field; replace with `account_id_hash` if a join key is needed |
| `user_id` | 0 | (Not yet in any tracing call) |
| `device_id` | 0 | (Not yet in any tracing call) |

**Total mechanical changes: ~31 call sites across 8 files.** Manageable
in one focused commit.

### 2.7 Hot files (most call sites that need migration)

Per the analyzer's transitional findings:

- `server/src/api/account.rs` — `email` + `account_id` in account events
- `server/src/api/auth_routes.rs` — `email` in verification/reset paths
- `server/src/api/router.rs` — `account_id` in dispatch/deduct paths
- `server/src/api/stt.rs` — `account_id` in relay finalization
- `server/src/api/usage.rs` — `account_id` in usage events
- `crates/cue-dashboard/src/lib.rs` — `email` in deep-link login

Approximately 80% of the work is in `server/src/api/`. About 4 files
account for almost all the changes.

---

## 3. What Phase 6 will do

### 3.1 Goal

Every operation-shape `tracing::info!` / `tracing::warn!` / `tracing::error!`
call site emits the standard fields per `docs/rounds/OBSERVABILITY-ROUND-PLAN.md` §2:

- `component` (auto-set by per-crate setup once Phase 1 wraps the macros)
- `version` (auto-set, `env!("CARGO_PKG_VERSION")`)
- `platform` (auto-set, OS+arch at boot)
- `trace_id` (passed through context — Phase 1 owns the threading)
- `request_id` (per-hop, server middleware mints — Phase 1)
- `session_id` (when the operation belongs to a session)
- `account_id_hash` (Phase 1 helper) — replaces `account_id`, `user_id`, `email`
- `status` (`ok`/`err`/`degraded` at the operation boundary)
- `latency_ms` (operation duration)

### 3.2 Out of scope

- Removing useful fields like `provider`, `model`, `cost_cents_*` —
  those are already standard.
- Forcing every `tracing::debug!` to carry the full set. Debug is the
  developer escape hatch; standard-field discipline applies to
  `info`/`warn`/`error` at operation boundaries.
- Renaming the `error` field — it's already canonical for `Error` chains.

### 3.3 The sweep approach

Phase 1 will land:

```rust
// crates/cue-core/src/observability.rs (TBD; codex names it)
pub fn account_id_hash_prefix(account_id: &str) -> String { ... }
pub fn redact_email(email: &str) -> &'static str { "<email>" }
```

Or possibly a macro:

```rust
observe!(level: Level::Info, component, account_id_hash, status, latency_ms, "msg");
```

Phase 6 cannot finalize the migration tooling until Phase 1's API is
named. But the SWEEP is predictable:

1. **For every `account_id` field**: replace with
   `account_id_hash = %cue_core::observability::account_id_hash_prefix(&account_id)`.
2. **For every `email` field**:
   - In auth events where the email is the routing key → replace with
     `account_id_hash`.
   - In verification/reset events → drop entirely (account_id_hash is
     enough; the email gets re-derivable from account_id by support).
3. **The single `session` alias** → rename to `session_id`.
4. **Any operation-shape call (info/warn/error) at an HTTP/IPC boundary
   that lacks `trace_id`** → thread the trace_id through context.

### 3.4 Migration script

Once Phase 1 names the API, Phase 6 will ship `scripts/migrate-tracing-fields.py`
with three modes:

- `--dry-run` (default): print every change as a unified diff, NO file
  modifications. Reviewer can read the diff before applying.
- `--apply`: write the changes to disk. Pipeline-gate after.
- `--check-only`: exit 0 if all sites are conformant, 1 if any non-conformant.
  Useful for CI.

The script reuses `scripts/analyze-tracing-calls.py`'s parser.

### 3.5 Verification approach

After the sweep:

1. `python3 scripts/analyze-tracing-calls.py` — should show ZERO
   transitional findings and ZERO non-conformant info/warn/error at
   operation boundaries.
2. `cargo fmt --all --check && cargo clippy --all-targets -- -D warnings`.
3. `cargo test --all-targets` — unchanged from current 453 passing.
4. Live smoke: trigger a question, verify the same `trace_id` appears
   across daemon → cloud-client → server → provider log lines.
5. Run `bluey doctor` — verify the daemon log tail shows fully-fielded
   entries.

---

## 4. Pre-Phase-6 hardening that's already done (Phase 4)

The Phase 4 redactor (`crates/cue-cli/src/logs.rs::redact_log_content`)
already preserves the future standard fields:

| Field | Behavior |
|---|---|
| `trace_id` | Preserved (no pattern match) |
| `request_id` | Preserved |
| `session_id` | Preserved (test asserts this) |
| `account_id_hash` | Preserved (test asserts this with `deadbeef0123` example) |
| Bearer tokens / JWTs / Stripe IDs / provider keys | Stripped |

So Phase 6's standard fields are forward-compatible with Phase 4's
log export. **No Phase 4 changes needed when Phase 6 ships.**

---

## 5. Estimated effort

- Migration script (after Phase 1 API names land): ~3-4 hours
- Sweep (~31 call sites, mostly mechanical): ~2 hours
- Verification + test additions: ~2 hours
- **Total: ~7-8 hours of focused work for Phase 6**

This is consistent with the original Observability Round plan estimate
of "~4-6 hours" for Phase 6 (slightly longer because the analyzer
revealed more `account_id` sites than initially estimated).

---

## 6. Trigger to start Phase 6

Phase 6 starts when ALL of:

1. Codex's Phase 1 has landed (commit on the branch).
2. Phase 1 names the standard fields API (struct, helper, or macro).
3. Phase 1's review is 🟢 (per collaboration contract).
4. Working tree is clean (per §6 working-tree contract).

Until then, this pre-plan stays as the durable contract for what Phase 6
will look like.

---

## 7. Concurrent kiro work while Codex builds Phase 1

In parallel with Phase 1 (without touching code Codex will modify):

- ✅ This pre-plan doc (committed)
- ✅ `scripts/analyze-tracing-calls.py` (committed)
- ⏳ Pre-write `scripts/migrate-tracing-fields.py` SKELETON — apply rules
  TBD until Phase 1's API names land
- ⏳ Stand by for Phase 4 review feedback
- ⏳ Stand by for Mac smoke (Phase 2 of the deploy track) and server
  staging (Phase 3 of the deploy track)

---

## 8. Acceptance criteria for Phase 6 close

- [ ] `analyze-tracing-calls.py` reports 0 transitional findings
- [ ] `analyze-tracing-calls.py` reports 0 non-conformant info/warn/error
      at operation boundaries
- [ ] `analyze-tracing-calls.py --pii-only` returns exit 0
- [ ] `cargo fmt --all --check` clean
- [ ] `cargo clippy --all-targets -- -D warnings` clean (workspace + server)
- [ ] `cargo test --all-targets` — 453+ tests pass (no regressions)
- [ ] Live smoke: same trace_id appears in daemon, cloud-client, server,
      provider log lines for one F19 question
- [ ] `bluey doctor` log tail shows standard fields populated
- [ ] Round-close handoff doc at
      `docs/rounds/OBSERVABILITY-PHASE-6-FOR-CODEX-REVIEW.md`

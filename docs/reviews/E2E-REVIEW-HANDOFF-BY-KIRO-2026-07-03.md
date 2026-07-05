# End-to-End Review & Improvement Handoff — Kiro → Codex

```
Date:     2026-07-03
Author:   Kiro
Audience: Codex
Repo:     /Users/uno/Downloads/cue @ d99d670e (v0.1.55, workspace 0.1.60)
Scope:    Full-project end-to-end review — where Bluey can improve.
```

## Review depth (honest)

I **surveyed** the whole tree structurally (workspace layout, LOC, CI, test
posture, panic/debt density, secret scan) and **read deeply** the areas I've
reviewed before and re-checked here: the 429/capacity layer, the
capacity-busy typed path, and the SQLite/Postgres db abstraction. I did
**not** line-by-line read all ~30k server + ~16k daemon + ~14k overlay lines.
Findings below are grounded in what I measured; I flag confidence per item.

## What's healthy (keep doing this)

- **969 test functions**; CI gates `fmt` + `clippy -D warnings` + `build` +
  `test --all-targets` across Ubuntu/Windows(/macOS matrix). Strong bar.
- **No hardcoded secrets** found (scanned for `sk-…`, `AKIA…`). Keys are
  env/config only.
- Only **18 `#[allow(...)]`** across server+crates — clippy is not being
  widely muted.
- Idempotency invariants survived the Postgres cutover (partial unique index
  `idx_credit_batches_stripe_charge`, `idx_usage_events_dedupe`).
- The 429/capacity defense-in-depth (key pool + shuffle, provider fallback,
  fleet cooldown, self-throttle, typed CapacityBusy) is solid and coherent.

---

## P0 — Correctness / reliability (do first)

### P0-1 · Postgres has ZERO automated test coverage (HIGH confidence)
The production backend is Postgres, but CI has **no Postgres service** — the
db suite runs against SQLite `:memory:` only. Every db function hand-writes a
`match DbPool { Sqlite => …, Postgres => … }`, and **10+ modules** carry
untested PG branches (`accounts`, `refresh_tokens`, `idempotency`,
`auth_tokens`, `sync`, `metrics`, `account_data`, `link_codes`, `device_codes`,
`ops_audit`). `db/mod.rs` itself calls the SQLite `get()`-in-PG-mode "an
adapter coverage bug … caught only at runtime."

**Evidence it's already biting:** Round 64's `error serializing parameter 0`
(the `::bigint` interval cast in `usage.rs`) was found in **production logs,
not a test**. Prod is currently the Postgres test harness.

**Fix:** add a Postgres job to `ci.yml` (a `services: postgres:16` container),
apply `infra/postgres/server-runtime/*.sql`, and run the db suite against
**both** backends (env-select the pool). Highest-leverage reliability
investment available right now — converts a class of prod incidents into
caught-in-CI failures.

### P0-2 · 771 `unwrap`/`expect`/`panic!` in non-test server+daemon code (MEDIUM)
Many are safe (static regex, poisoned-lock propagation), but 771 is high
enough that panic-on-bad-input is a real risk in async handlers, where a
panic aborts the task and can leave state inconsistent. `router.rs` has 22,
`daemon app.rs` 55.

**Fix:** audit the request-handling hot paths first (`api/router.rs`,
`routing/dispatcher.rs`, `api/billing.rs`, daemon `app.rs` message loop).
Convert input-derived `unwrap`/`expect` to typed errors / `?`. Consider a
`clippy::unwrap_used`/`expect_used` gate on `server/src/api` once cleaned.

---

## P1 — Maintainability (schedule soon)

### P1-1 · God-files (HIGH confidence, measured)
| File | Lines |
|---|---|
| `crates/cue-daemon/src/app.rs` | **16,681** |
| `native/macos/cue-overlay/Sources/cue-overlay/main.swift` | **14,217** |
| `server/src/api/router.rs` | **8,092** |
| `crates/cue-cli/src/app.rs` | 3,722 |
| `server/src/routing/dispatcher.rs` | 3,056 |

These are review-hostile, merge-conflict magnets (the overlay file is exactly
where cross-agent conflicts happen), and slow the edit/compile loop. Split
along seams: daemon `app.rs` → message-loop / stt / answer / context / ipc
modules; `router.rs` → per-resource route modules; overlay `main.swift` →
composer / captions / canvas / context-chips.

### P1-2 · 354 TODO/FIXME/HACK/XXX (MEDIUM)
Triage into tracked items; delete confirmed dead code. A backlog this size
hides the load-bearing TODOs among stale ones.

### P1-3 · Commit the in-flight Round 321 STT work (LOW effort)
The Deepgram realtime STT hardening is uncommitted (5 files, +349). Land it
with the focused tests listed in its round doc so it isn't lost.

---

## P2 — Process (worth a conversation)

### P2-1 · Commit hygiene regressed
History drifted from Conventional Commits + the codex/kiro cross-review
contract to terse one-liners ("Harden live answer QA recovery"). If any
tooling/release-notes depend on the `type(scope): subject` format, this
breaks it silently. Recommend restoring Conventional Commits (cheap) and,
for money/auth/db-touching changes, the two-agent review gate.

### P2-2 · 588 round docs — nearly one per commit
Great traceability, heavy overhead at this cadence. Consider collapsing
"Document roundNNN deployment" docs into a single rolling CHANGELOG/WORKLOG
and reserving standalone round docs for substantive design changes.

### P2-3 · Regression-audit the reliability work
After 100+ commits since the 429/capacity + key-shuffle + capacity-busy
rounds, confirm those invariants still hold (a quick targeted test run of
`cue-cloud-client`, `cue-llm`, and the server routing tests).

---

## Suggested order

1. **P0-1** Postgres CI lane (stops prod-only db bugs) — biggest win.
2. **P1-3** commit Round 321 STT.
3. **P0-2** unwrap audit on request hot paths.
4. **P1-1** start splitting `daemon/app.rs` and `router.rs`.
5. **P2** process items as a team decision.

## Notes / boundaries

- I did not modify any code. The current working tree has codex's in-flight
  Round 321 changes (`cue-daemon/app.rs`, `server/stt.rs`, `Cargo*`) +
  `bluey-dev.db` (untracked, never touch) — all left alone.
- If you want, I'll take **P0-1** (the Postgres CI lane) myself as a
  self-contained change and hand it back for your review.

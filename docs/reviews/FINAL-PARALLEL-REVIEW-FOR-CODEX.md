# Final Round Review for Codex — Observability Close + Parallel Polish

**Branch:** `feat/phase-3-round-12`
**From:** Kiro
**To:** Codex
**Date:** 2026-05-22
**Subject:** What I shipped in parallel with your Phase 3 + recommendations for the next round

---

## What I did while you worked Phase 3

After your Phase 5 commit at `b30d5b0`, you started Phase 3 (overlay
lifecycle + frontend error capture). I went all-in on parallel work
in non-overlapping territory. Six commits landed:

| # | Commit | What | Why |
|---|---|---|---|
| 1 | `1e178cd` | `bluey support` — combined doctor + logs zip | Highest support-tooling leverage. Customer runs one command instead of two. |
| 2 | `a29ff48` | `bluey doctor` Summary header | At-a-glance triage. First 8 lines tell support if customer is blocked on something obvious. |
| 3 | `df367a8` | Server `/health` route + version metadata | Monitoring/uptime probes hit standard `/health` path. Returns version + commit + platform + server_time_ms. |
| 4 | `d3aa879` | Commit deploy-track docs (Mac smoke + server staging) | Promoted from /tmp staging to durable `docs/deploy/`. Operator playbooks for closed-alpha gates. |
| 5 | `2c92dba` | CI workflow + pre-commit hook | Locks in Phase 6 sweep policy. Prevents regression. |
| 6 | round-close + final review | Two new docs (this is the second) | Round-close summary + this paste-ready summary for you. |

All six commits are in `crates/cue-cli/`, `server/src/api/`,
`docs/`, `scripts/`, `.github/`. **Zero overlap with your Phase 3
files** (`native/macos/cue-overlay/`, `crates/cue-core/src/overlay.rs`,
`crates/cue-daemon/src/app.rs`, `crates/cue-dashboard/ui/`).

---

## Pipeline state at the round-close tip

```
✅ cargo fmt --all --check
✅ cargo clippy --all-targets -- -D warnings (workspace + server)
✅ cargo test --all-targets — 474 passed (workspace) + 90 (server)
✅ scripts/analyze-tracing-calls.py --check-only — exit 0
✅ scripts/observability-acceptance-smoke.sh — 6/6 assertions PASS
```

---

## Recommendations for your Phase 3

When Phase 3 lands, please:

1. **Use `cue_core::observability::ObserveFields`** for any new
   tracing call sites. The CI gate at
   `.github/workflows/observability-policy.yml` will fail if Phase 3
   introduces new transitional findings (raw `account_id`, raw
   `email`, etc.).
2. **Use `cue_core::sanitize_observability_id`** for any inbound
   trace_id from the dashboard's TS-side error capture path.
   Defense-in-depth against log injection.
3. **Extend `scripts/observability-acceptance-smoke.sh`** with a
   Phase 3 assertion: spawn a daemon, simulate an overlay lifecycle
   event, verify the JSON log line contains the standard fields.
4. Round-close doc at `docs/rounds/OBSERVABILITY-ROUND-CLOSE.md`
   covers the full ledger; please add a Phase 3 row under "Phase
   ledger" when you ship.

---

## Outstanding nits I flagged in earlier verdicts

These are non-blocking; tracked for v0.2.x polish round:

| Source | Item | Single-line fix? |
|---|---|---|
| Phase 5 N-1 | `debug!(trace_id, "daemon ipc request received")` — should be `info!` for level consistency with server middleware | Yes (1 LOC) |
| Phase 2 N-2 | No max-file-size cap on rotated logs (daily-only) | No, ~30 LOC `tracing-appender` byte-size rotation |
| Broader sweep | 159 non-conformant info/warn/error calls without trace_id field | No, ~few hundred call-site edits |

The Phase 5 N-1 fix is in your territory (cue-daemon/src/app.rs).
Other two are queued for a polish round.

---

## What "production-ready" actually looks like now

Per `docs/PRELAUNCH-CHECKLIST.md` "Honest gate definition":

| Shape | State |
|---|---|
| Internal-test-build | ✅ done |
| Closed-alpha | 🟡 needs (1) real-Mac smoke per `docs/deploy/PHASE2-MAC-SMOKE.md` (2) server staging deploy per `docs/deploy/PHASE3-SERVER-DEPLOY.md` |
| Paying-customer | 🔴 needs Stripe LIVE + production SMTP + DNS + pages + monitoring + signed release manifest + DB encryption |
| GA | 🔴 needs Postgres + multi-region + Windows parity + cert pinning |

Both closed-alpha gates are operator-side; no code work blocking.

---

## Next round candidates

In recommended order:

1. **Phase 3 close** (yours, in flight) — final round polish
2. **Closed-alpha gates** (operator) — real-Mac smoke + staging deploy
3. **Next-security-round** — signed release manifest, `bluey
   check-update`, dependency audit CI
4. **Polish round** — Phase 5 N-1 + Phase 2 N-2 + broader trace_id
   sweep
5. **Closed-alpha launch** (after 2 + 3)

I'm standing by for your Phase 3 review verdict request when it
lands. Tree is mine while these six commits flush in (small
window); should be clean by the time you're ready to commit Phase 3.

Anything blocking me you want me to pick up? Just paste a request
and I'll go.

— Kiro

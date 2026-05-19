# REVIEW: Server Stage 1 — Rust Skeleton Recheck

**Original commit:** `853be40 feat(server): bluey-server Rust skeleton (R14.9 stage 1)`
**Re-review tip:** `20cd0b3 fix(R14): clear remaining codex blockers — stale paths + speculative doc comment`
**Reviewer:** Codex
**Date:** 2026-05-19

## Per-Task Review

### S1.1 — Crate / Repo Placement

| Field | Value |
|-------|-------|
| Files | `server/Cargo.toml`, `server/src/lib.rs`, `server/src/main.rs` |
| Verdict | 🟢 accept |

**Findings:**

- `server/` as a temporary isolated crate remains fine. The `[workspace]` marker keeps it out of the main Cue workspace, and moving to a separate `bluey-server` repo later is mostly CI/deploy/doc work.

---

### S1.2 — Route Skeleton / Security Shape

| Field | Value |
|-------|-------|
| Files | `server/src/api/mod.rs`, `server/src/api/admin.rs` |
| Verdict | 🟢 accept for Stage 1 skeleton |

**Findings:**

- The original Stage 1 blocker was that `GET /admin/customers` was mounted publicly. At tip `20cd0b3`, it is now in the protected router behind `auth::require_auth` (`server/src/api/mod.rs:45-65`), so the public leak from the skeleton stage is fixed.
- Residual production issue: the route is still not admin-role gated. That is tracked as a Stage 2 / Stage 6 blocker, not a Stage 1 skeleton blocker.

---

### S1.3 — Database Schema

| Field | Value |
|-------|-------|
| Files | `server/src/db/mod.rs` |
| Verdict | 🟢 accept with later migration hardening |

**Findings:**

- The table set is still a reasonable v0.2 starting point: accounts, refresh tokens, device codes, credit batches, usage events, and Stripe webhook idempotency cover the planned flow.
- Inline idempotent migrations are acceptable for the scaffold. Once customer data exists, move to applied-state migrations; `CREATE TABLE IF NOT EXISTS` cannot safely express all future schema evolution.

---

### S1.4 — Balance / Credit Accounting

| Field | Value |
|-------|-------|
| Files | `server/src/db/balance.rs` |
| Verdict | 🟢 original FIFO blocker cleared |

**Findings:**

- The original Stage 1 FIFO gap has been addressed at the current tip: `deduct()` now wraps the account balance deduction and oldest-live-batch consumption in one transaction (`server/src/db/balance.rs:21-64`).
- Residual production issue: `credit()` and `sweep_expired()` still need stronger transaction/idempotency treatment before live Stripe billing. That is recorded in the Stage 6 review.

---

### S1.5 — Pricing

| Field | Value |
|-------|-------|
| Files | `server/src/pricing/mod.rs`, `docs/PRICING-MODEL.md` |
| Verdict | 🟢 original unit blocker cleared |

**Findings:**

- The original microcent constant bug is fixed at tip `20cd0b3`: `gpt-4o-mini` uses `150_000 / 600_000`, `gpt-4o` uses `2_500_000 / 10_000_000`, and Anthropic Sonnet placeholders use `3_000_000 / 15_000_000` microcents per 1M tokens (`server/src/pricing/mod.rs:32-72`).
- Residual product issue: code rounds billable customer cost up to whole cents while `docs/PRICING-MODEL.md` still advertises fractional-cent examples. That is recorded in the Stage 4 review because it affects customer-facing billing.

---

### S1.6 — Docs / Round References

| Field | Value |
|-------|-------|
| Files | `docs/PRODUCTION-READINESS.md`, `docs/PRICING-MODEL.md`, `crates/cue-router/src/speculative.rs` |
| Verdict | 🟢 accept |

**Findings:**

- `auto_recap` is now accurately documented as not routed through the classifier yet.
- Pricing docs have a snapshot date and formula source-of-truth language.
- Moved-path references are cleared except the expected self-reference inside `docs/work/HANDOFF-FROM-CODEX-TO-KIRO.md`.
- `SpeculativeRouter` now documents the default-ON internal-testing semantics and `BLUEY_SPECULATIVE_ROUTING` gate.

## Build & Test Verification

```bash
cd server && cargo test --lib          # ✅ 30 passed
cargo test -p cue-router --lib         # ✅ 30 passed
cargo test -p cue-cloud-client --lib   # ✅ 4 passed
```

## Overall Verdict

🟢 **ACCEPT** — The Stage 1 skeleton blockers are cleared at tip `20cd0b3`. Remaining production concerns are properly carried by later-stage reviews.

## Follow-ups for Next Batch

- Keep admin role gating in the Stage 2 / Stage 6 fix wave.
- Reconcile whole-cent billing with fractional-cent pricing docs in the Stage 4 fix wave.

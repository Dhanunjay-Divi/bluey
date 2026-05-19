# REVIEW: Server Stage 7 — Usage Aggregation and Event Ingestion

**Commit:** `ed5d711 feat(server): real /account/usage aggregation + /usage/event ingestion (Stage 7)`  
**Reviewer:** Codex  
**Date:** 2026-05-19

## Per-Task Review

### Stage 7 — Usage Reporting

| Field | Value |
|-------|-------|
| Files | `server/src/api/account.rs`, `server/src/api/usage.rs`, `server/src/db/usage.rs`, `server/src/db/mod.rs` |
| Verdict | 🔴 blocker |

**Findings:**

- 🔴 `server/src/db/mod.rs:102` / `server/src/db/usage.rs:27` — usage ingestion is not idempotent. The schema has only an index on `request_id`, and `usage::record()` always generates a new primary key and inserts a new row. A daemon retry of `/usage/event` with the same `request_id` double-counts cues and spend in `/account/usage`. Add `UNIQUE(account_id, request_id, kind)` or equivalent and make ingestion upsert/ignore duplicates.
- 🔴 `server/src/db/mod.rs:106` / `server/src/api/account.rs:61` — stored timestamps default to SQLite `datetime('now')` (`YYYY-MM-DD HH:MM:SS`) while `/account/usage` compares against an RFC3339 cutoff string (`YYYY-MM-DDTHH:MM:SS...`). Because this is a text comparison, the rolling-7-day window can exclude or include boundary-day rows incorrectly. Use one canonical timestamp format, or compare with SQLite datetime functions rather than string ordering.
- 🟡 `server/src/api/usage.rs:12` — `/usage/event` trusts client-supplied `cost_cents_to_bluey`, `cost_cents_to_customer`, provider, model, and flags. If this endpoint is only for local UX analytics, that may be acceptable, but any customer-visible spend/burn projections should prefer server-recorded router events or clearly mark client events as untrusted.
- 🟡 `server/src/api/account.rs:124` — tier classification is based on the current rounded-cent server costs while `docs/PRICING-MODEL.md` uses fractional-cent examples. This must be reconciled with the Stage 4 pricing decision.

## Cross-Task Findings

- Stage 7 depends on the Stage 4 pricing/idempotency decision. Once `/router/complete` gets request idempotency, reuse the same request id in usage ingestion.

## Build & Test Verification

```bash
cd server && cargo test --lib   # ✅ 30 passed
```

## Overall Verdict

🔴 **REQUEST CHANGES** — Usage reporting needs idempotency and timestamp consistency before it can drive customer-facing tier/burn projections.

## Follow-ups for Next Batch

- Add tests for duplicate `/usage/event` retry and boundary timestamps around the 7-day cutoff.

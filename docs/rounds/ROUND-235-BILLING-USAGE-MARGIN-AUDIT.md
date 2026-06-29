# Round 235 - Billing Usage Margin Audit

Date: 2026-06-29 18:45 EDT
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The owner saw the overlay balance at `$4.82 low` after previously seeing about
`$5.28`, and asked why roughly `46-50` cents disappeared. The owner also asked
for a breakdown of what customers are charged versus what Bluey pays upstream
to the API providers.

## Data Checked

- Active local account:
  - endpoint: `https://bluey.sh`
  - user: `codex-smoke-20260608183100@bluey.sh`
  - current balance: `$4.82`
- `bluey usage`:
  - Last 7 days: `484` cues, `$7.72` customer spend
  - Breakdown shown by current customer-facing CLI:
    - vision: `$2.54`
    - general: `$1.82`
    - balanced: `$1.71`
    - transcription: `$1.65`
- Account export generated locally for analysis only:
  - `bluey-export-20260629-223741.json`
  - The export contains customer usage rows, but not `cost_cents_to_bluey`.

## Findings

The visible drop from `$5.28` to `$4.82` was not one single 50-cent provider
call. The newest exported usage rows that add to `46` customer cents were:

- Deepgram STT dual-source rows:
  - microphone and system audio are billed as separate STT lanes
  - several short listens were billed at the per-event one-cent customer minimum
  - longer listens around `23-26` seconds billed `2` cents per source
- Three recent Anthropic balanced LLM calls:
  - `7` cents
  - `5` cents
  - `4` cents
- One earlier Anthropic balanced LLM call:
  - `6` cents
- Two embedding rows:
  - `1` cent each

Because the overlay balance can refresh after usage rows settle, accumulated
usage can appear as a sudden drop if the UI was stale for a while.

## Estimated Provider Actuals

The account export does not expose the stored `cost_cents_to_bluey`, so this
round computed provider actuals from exported provider/model/token rows using
the current pricing table in `server/src/pricing/mod.rs`.

Important: provider dashboards usually bill aggregated fractional token/audio
cost, not Bluey's per-event rounded cent values. For owner margin, aggregate raw
micro-costs are the better estimate.

### Last 7 Days

- Customer charged: `$7.72`
- Estimated provider actual: about `$2.09`
- Gross spread before infra/payment overhead: about `$5.63`

By provider/model:

- OpenAI `gpt-5.5`
  - events: `16`
  - customer charged: `$2.54`
  - estimated provider actual: about `$0.98`
  - tokens: `152,564` input, `7,401` output
- OpenAI `text-embedding-3-small`
  - events: `342`
  - customer charged: `$1.82`
  - estimated provider actual: about `$0.004`
  - tokens: `229,257` input
- Anthropic `claude-sonnet-4-6`
  - events: `42`
  - customer charged: `$1.71`
  - estimated provider actual: about `$0.50`
  - tokens: `117,931` input, `9,971` output
- Deepgram `nova-3`
  - events: `84`
  - customer charged: `$1.65`
  - estimated provider actual: about `$0.60`
  - audio seconds: `3,924`

### All Exported Usage

- Exported usage events: `1,254`
- Customer charged: `$15.80`
- Estimated provider actual: about `$2.90`
- Gross spread before infra/payment overhead: about `$12.90`

By provider/model:

- Deepgram `nova-3`
  - events: `828`
  - customer charged: `$9.42`
  - estimated provider actual: about `$1.32`
  - audio seconds: `8,638`
- OpenAI `gpt-5.5`
  - events: `16`
  - customer charged: `$2.54`
  - estimated provider actual: about `$0.98`
- Anthropic `claude-sonnet-4-6`
  - events: `54`
  - customer charged: `$2.02`
  - estimated provider actual: about `$0.59`
- OpenAI `text-embedding-3-small`
  - events: `356`
  - customer charged: `$1.82`
  - estimated provider actual: about `$0.004`

## Product Gaps

- `/account/usage` is customer-facing and only exposes customer spend.
- `/account/export` currently omits `cost_cents_to_bluey`, so owner margin
  cannot be audited directly from the export.
- There is no owner/admin report yet that compares:
  - customer charged
  - Bluey estimated upstream cost
  - provider actual aggregate estimate
  - provider/model/token/audio breakdown
  - margin after payment/infrastructure overhead
- The overlay shows `low` at `$4.82`; CLI says Auto Reload is ON at `$30` under
  `$5`. If Auto Reload did not run after crossing the threshold, that needs a
  separate reload-worker/idempotency audit.

## Recommended Fix

Add an admin-only cost report endpoint and CLI/dashboard view:

- `GET /admin/usage-costs?account_id=...&window=7d`
- include provider/model breakdown
- include customer cents, stored `cost_cents_to_bluey`, and raw provider
  micro-cost estimate
- include recent balance ledger events and STT reservation/settlement rows
- redact account identifiers by default and require admin auth

This would let us answer future "why did balance drop?" questions without
exporting account data or hand-recomputing from tokens.

## Verification

Commands used:

- `bluey usage`
- `bluey credits`
- `bluey export`
- `jq` aggregation over exported usage rows
- pricing source checked at `server/src/pricing/mod.rs`

The generated export file was not committed and should be treated as local
account data.

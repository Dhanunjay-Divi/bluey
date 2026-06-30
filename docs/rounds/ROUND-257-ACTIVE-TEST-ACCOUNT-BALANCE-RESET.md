# Round 257 - Active Test Account Balance Reset

## Trigger

The owner still saw a `$4.79` balance after the earlier internal-admin account recovery and asked why.

## Root Cause

The live account shown by the desktop/browser was not `internal-admin-20260606023943@bluey.sh`.

The current local desktop account profile points to:

```text
codex-smoke-20260608183100@bluey.sh
```

Live Postgres showed:

```text
codex-smoke-20260608183100@bluey.sh      balance_cents=479   reserved_cents=0
internal-admin-20260606023943@bluey.sh  balance_cents=1500  reserved_cents=0
```

So the `$4.79` display was accurate for the active desktop account. The earlier `$15.00` reset landed on the internal-admin account, not the account currently linked on this Mac.

## Usage Evidence

Recent ledger rows on `codex-smoke-20260608183100@bluey.sh` showed normal usage movement:

- `usage_deduction` rows for streamed LLM answers
- `stt_reserve` rows for microphone/system Deepgram reservations
- `stt_settle` rows returning unused STT reservation balance

Aggregate ledger counts for the account:

```text
stt_reserve      40 rows  -1076 cents
stt_settle       40 rows  +1343 cents
usage_deduction  47 rows   -108 cents
```

The newest rows before reset ended at:

```text
usage_deduction  -1  480 -> 479  llm_stream
```

## Fix

Applied an internal test credit to the active desktop account:

```text
account: codex-smoke-20260608183100@bluey.sh
amount: 1021 cents
before: 479 cents
after: 1500 cents
reason: codex_current_test_account_reset_to_15_round257
```

Also cleared a stale Auto Reload flag on that account because it had:

```text
auto_topup_enabled=1
square_card_id=null
stripe_payment_method_id=null
```

That state came from older test data. Current backend code already blocks enabling Auto Reload without a saved payment method, but the old row made the UI look like Auto Reload was on even though the worker could only skip it.

## Verification

Live Postgres after the reset:

```text
codex-smoke-20260608183100@bluey.sh  balance_cents=1500  reserved_cents=0  auto_topup_enabled=0
```

Latest ledger row:

```text
internal_credit  1021  479 -> 1500  codex_current_test_account_reset_to_15_round257
```

## Current State

The active desktop test account is now at `$15.00` with no reserved balance. Auto Reload is off until a real saved payment method exists.

## Remaining QA/Gates

- If the dashboard still shows `$4.79`, refresh the account page or click `Refresh balance`; `/account/me` now reads `$15.00` for the active account.
- Consider a cleanup migration/admin job that disables `auto_topup_enabled` for any account with no saved card, so older test rows cannot show an impossible Auto Reload state.

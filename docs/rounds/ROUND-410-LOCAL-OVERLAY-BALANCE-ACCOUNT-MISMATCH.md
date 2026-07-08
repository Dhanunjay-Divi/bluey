# Round 410 - Local Overlay Balance Account Mismatch

Date: 2026-07-07
Backup thread: 019e133e-d92a-7830-8df0-3a050a4e22f6
Branch: main

## Trigger

The local overlay showed `$5.44` after the internal admin account had been verified at `$15.00`.

## Finding

The local desktop profile is linked to:

```text
codex-smoke-20260608183100@bluey.sh
```

It is not linked to:

```text
internal-admin-20260606023943@bluey.sh
```

Live Postgres showed:

```text
codex-smoke-20260608183100@bluey.sh      balance_cents=544   reserved_cents=0
internal-admin-20260606023943@bluey.sh  balance_cents=1500  reserved_cents=0
```

So the overlay `$5.44` is the correct live balance for the active desktop account.

## Recent Ledger Evidence

Latest movements on `codex-smoke-20260608183100@bluey.sh`:

```text
stt_settle       +6  538 -> 544  completed:no_audible_audio
stt_settle       +4  534 -> 538  completed
stt_reserve      -6  540 -> 534  microphone
stt_reserve      -6  546 -> 540  system
usage_deduction  -1  547 -> 546  llm_stream
usage_deduction  -2  549 -> 547  llm_stream
```

## Decision

No balance change was made in this round. The issue is account mapping, not a balance display bug.

If the desired test account is `internal-admin-20260606023943@bluey.sh`, the desktop needs to be relinked to that account. If the desired active desktop test account is `codex-smoke-20260608183100@bluey.sh`, reload that account instead.

# Round 591 - Reviewed Communication Actions

**Date:** 2026-08-02
**Branch:** `feat/phase-jobs-full-autonomy-20260802`
**Status:** durable approval and execution authority implemented; provider workers remain disabled

## Result

Bluey Jobs now has a durable authority boundary for outbound recruiter replies and
interview calendar actions. A proposed action is stored as an encrypted,
account-scoped draft. It cannot be claimed by a provider worker until the user
explicitly approves it, and it cannot be dispatched through a disconnected mailbox.

The queue is idempotent and fenced. Reusing an idempotency key with the same payload
returns the original action, while reusing it with different content fails. A worker
claim receives a one-time lease token and monotonically increasing fence. Completion
requires both values plus real provider evidence. If a dispatch lease expires after a
possible provider side effect, Bluey records `side_effect_unknown` and does not retry
blindly.

## Supported Actions

- Reviewed replies through a Gmail or Outlook mailbox connection.
- Reviewed interview events through a Google Calendar or Outlook Calendar connection.
- Account-scoped list, detail, approve, and cancel operations.
- Durable SQLite and PostgreSQL schemas with matching tables and indexes.

Replies must cite a real inbound provider message already bound to the same account,
mailbox, and application. Calendar actions may be created without an inbound message,
but still require a connected account mailbox and explicit approval.

## Privacy Boundary

Action bodies, recipients, subjects, event attendees, and event details are encrypted
inside `action_json`. List responses omit payload text, idempotency keys, provider
object IDs, worker identities, lease tokens, attempt counters, and retry timing.
Account scoping is enforced at creation, reads, approval, cancellation, claim, and
completion.

## Execution Boundary

This round does not send email or create calendar events. It establishes the durable,
reviewed authority that Gmail and Microsoft provider workers must consume. Production
mailbox synchronization remains disabled. No Jobs production flag was changed.

The future provider worker must:

1. claim one approved action using its authenticated private worker identity;
2. perform the exact encrypted action against the bound provider account;
3. finish with the lease token, fence, and a real provider object ID;
4. report a definitive no-side-effect failure only when the provider proves that no
   email or event was created; and
5. reconcile `side_effect_unknown` actions before any human-approved retry.

## State Machine

```text
awaiting_approval -> approved -> dispatching -> sent
                                         |-> calendar_created
                                         |-> needs_input
                                         |-> failed (definitive no side effect)
                                         |-> side_effect_unknown (never blind retry)
awaiting_approval -> cancelled
approved          -> cancelled
```

## Verification

- Seven focused communication-action database tests passed.
- Rust formatting passed.
- SQLite/PostgreSQL Jobs schema parity passed with six tables and eleven indexes.
- CI guard self-tests passed.
- `git diff --check` passed.

The tests cover encrypted storage, tenant isolation, idempotency conflicts, source
message authority, Gmail/Outlook mapping, mailbox reauthorization, explicit approval,
fenced evidence completion, expired-dispatch ambiguity, and cancellation.

## Remaining Mailbox Gate

1. Implement authenticated Google and Microsoft provider workers against this queue.
2. Add OAuth grant/revocation and provider sandbox tests without logging tokens or OTPs.
3. Add provider-side lookup reconciliation for ambiguous email/event creation.
4. Add reviewed portal controls for draft text, approval, cancellation, and status.
5. Run live authorized sandbox tests before enabling mailbox synchronization.

# Bluey Dispute, Refund, And Credit Evidence Runbook

Date: 2026-07-02

## Purpose

When a payment dispute, refund request, chargeback, unexplained credit loss, or
auto-reload complaint arrives, create a complete evidence packet immediately.
Do not rely on memory or provider dashboards alone. Preserve the billing,
ledger, usage, consent, and account-state timeline while the data is fresh.

## First Response Rule

Within the first 15 minutes, produce a folder containing:

- Bluey account/billing evidence packet.
- Processor payment, dispute, refund, or subscription screenshots/exports.
- Receipt/invoice screenshot or PDF.
- Terms/privacy/refund/credit-loss consent proof.
- Relevant support messages, or a note that no support contact was found.
- Short owner-facing summary: what happened, usage evidence, current account
  state, deadline, and recommended action.

## Inputs To Collect

Ask the owner for whatever is available:

```text
Customer email:
Bluey account id:
Bluey session id / support code:
Processor: Square or Stripe, if known:
Payment id:
Order id:
Subscription id:
Dispute id:
Refund id:
Amount:
Balance before/after if known:
Reason:
Deadline:
Refund/cancellation already issued:
Customer messages:
```

One processor ID or Bluey account ID is better than email alone. Email fallback
works, but must be checked manually because a customer may have multiple login
accounts or payment records.

## Create The Evidence Folder

```bash
stamp="$(date -u +%Y%m%dT%H%M%SZ)"
email="customer@example.com"
safe_email="$(printf '%s' "$email" | tr '[:upper:]' '[:lower:]' | tr -cs 'a-z0-9@._-' '-')"
mkdir -p "$HOME/Downloads/bluey-disputes/${safe_email}-${stamp}"
cd "$HOME/Downloads/bluey-disputes/${safe_email}-${stamp}"
```

## Export The Bluey Packet

Use an admin browser session or authenticated admin request. Prefer exact IDs
over email selectors.

Potential selectors:

```text
https://bluey.sh/admin/support/accounts/<account_id>
https://bluey.sh/admin/ops/events?account_id=<account_id>
https://bluey.sh/account/export?format=zip
```

If Bluey later adds a dedicated dispute endpoint, it should produce the same
packet shape and write an admin access audit row.

The packet should include:

- account email hash and account ID
- current account status and deletion/restriction state
- payment processor and processor IDs
- checkout/reload amount and idempotency key
- auto-reload threshold, amount, enablement timestamp, and consent evidence
- account deletion consent evidence, including `DELETE`, data-loss consent, and
  credit-loss consent when applicable
- terms/privacy/refund-policy version accepted
- ledger rows with before/after balance, reason, request id, provider payment id
- usage rows inside the billing/dispute window
- provider/model/STT/search cost rows and Bluey customer charge rows
- session ids/support codes related to the complaint
- sanitized diagnostic logs and lifecycle events
- refund/dispute/revocation events

Do not include raw transcripts, raw documents, raw screenshots, provider API
keys, tokens, or unrelated user data in an evidence packet unless the owner has
explicitly approved including a specific excerpt.

## Processor Evidence Checklist

For Square:

- Dispute/payment/refund detail: IDs, reason, state, due date.
- Payment detail: amount, card checks, customer, order.
- Subscription or reload detail, if applicable.
- Receipt/invoice email or PDF.
- SCA/verification/payment-authentication evidence shown by Square.

For Stripe:

- Dispute detail: dispute ID, reason, state, due date.
- Charge/payment intent detail: amount, card checks, 3DS/SCA result,
  liability-shift result when present.
- Subscription/invoice/payment-method detail.
- Receipt email or PDF.

## Bluey Evidence Checklist

Include or confirm:

- Payment succeeded before credits or entitlement were granted.
- Auto-reload was explicitly enabled before any automatic top-up.
- Auto-reload idempotency key was unique and no duplicate reload fired.
- Ledger balance before/after matches the user-visible balance.
- Usage rows explain any balance drop.
- Provider actual/estimated cost rows are available for owner margin analysis.
- Refund/dispute state immediately restricted paid compute when required.
- Account deletion, if requested, required explicit data-loss and credit-loss
  consent and did not leave live streaming/billing access active.
- Original docs/screenshots in object storage are accounted for by export/delete
  jobs, without orphaned blobs.
- Session IDs/support codes exist for the incident and logs are sanitized.

## Access / Abuse Decision

Do not delete evidence. Do not delete the account while a dispute is open.

If a dispute/refund is confirmed and the owner approves access limitation:

- Move the account to restricted/no-paid-compute state.
- Stop router, STT, embeddings/RAG, file processing, web search, and auto-reload
  for the account.
- Cancel or pause the processor subscription/payment method where appropriate.
- Keep evidence/audit/billing rows for the dispute retention window.
- Keep only sanitized logs and indexed support metadata outside the user's
  export/delete scope.

## Evidence Summary Template

Create `summary.md` in the evidence folder:

```markdown
# Bluey Dispute / Refund / Credit Summary

- Customer:
- Account ID:
- Session/support code:
- Provider:
- Payment/reload/dispute ID:
- Amount:
- Reason:
- Deadline:
- Current Bluey account status:
- Current Bluey balance:
- Processor subscription/payment status:
- Refund/cancellation status:

## Timeline

- <timestamp> Checkout/payment/reload:
- <timestamp> Terms/privacy/credit consent:
- <timestamp> Credits granted:
- <timestamp> Usage/balance debit:
- <timestamp> Session/support code:
- <timestamp> Refund/dispute/account delete event:

## Recommendation

- Contest / accept / refund / ask customer to update payment / restore account:
- Why:
- Missing evidence:
```

## Submission Notes

- Keep the processor response short and chronological.
- Include only relevant diagnostic lines.
- Redact unrelated user data.
- Do not include secrets, full tokens, raw provider keys, passwords, or raw
  private session content.
- For unauthorized-payment disputes, lead with processor verification, checkout
  IP/user-agent where available, consent, and usage after purchase.
- For duplicate-charge disputes, lead with idempotency keys, processor IDs,
  ledger rows, and auto-reload timestamps.
- For product-not-received disputes, lead with receipt, account access, session
  usage, and support availability.
- For account-deletion complaints, lead with explicit `DELETE`, data-loss and
  credit-loss consent, deletion timestamp, and export/delete job status.

## If The Customer Cannot Be Found

Check these before concluding the customer is missing:

- Email spelling and case.
- Processor customer email versus Bluey login email.
- Payment/order/subscription ID in the provider dashboard.
- Whether webhook delivery failed and the provider payment exists without a
  local ledger row.
- Whether the user deleted the account and only redacted ops/audit rows remain.
- Whether the user logged in with a different account after checkout.

Record this as an incident if processor evidence shows a successful payment but
Bluey has no matching local payment, ledger, or access row.

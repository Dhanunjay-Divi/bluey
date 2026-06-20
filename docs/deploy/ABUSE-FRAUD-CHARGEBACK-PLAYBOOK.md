# Abuse, Fraud, And Chargeback Playbook

Bluey uses Square-hosted checkout and account credits. That reduces card-data
scope, but it does not remove fraud or dispute risk. This playbook is the
operator contract before wider paid alpha.

## Product Position

- Bluey sells non-transferable SaaS credits for cloud answers, speech, screen
  analysis, embeddings, and saved-session features.
- Credits have no cash value, cannot be transferred, and expire up to 365 days
  after purchase.
- Bluey stops paid cloud usage at zero. Do not allow negative balances.
- Manual reload is always available. Saved-card Auto Reload is opt-in only,
  requires a saved Square card, and credits balance only after a processor
  payment succeeds.

## Main Risk Scenarios

| Scenario | Signal | Immediate action |
|---|---|---|
| Stolen card reload | new account, new card, high usage immediately after reload, unusual geo/IP churn | put account in review, cap paid usage, preserve evidence |
| Chargeback/dispute | Square dispute notification or dashboard alert | freeze disputed credit batch if possible, stop auto reload, prepare evidence |
| Reload amount mismatch | Square paid amount, currency, metadata, reference id, or Bluey credited amount do not match | fail closed, do not credit, put account/event in operator review |
| Trial abuse | many accounts from same IP/device pattern, repeated zero-spend usage | reduce trial access, require verified email, block suspicious signup patterns |
| Provider-cost abuse | high STT hours or deep-model calls relative to credit | enforce server-side spend guard, capacity limits, and low-balance hard stop |
| Account sharing/device churn | many devices or locations on one account | require re-auth, add review flag, ask customer to confirm usage |
| Bot/API abuse | high request rate, repeated 401/403/429, malformed request IDs | rate-limit, block source, rotate tokens if needed |

## Evidence To Preserve

Do not store full card data; Square owns that. Preserve the operational evidence
needed to answer disputes and support tickets:

- Square payment/order IDs, webhook event IDs, checkout URL ID, amount, currency,
  and Square environment.
- Bluey account ID hash, email hash, credit batch IDs, usage IDs, request IDs,
  trace IDs, timestamps, and user-agent/IP hash where available.
- Terms/privacy version accepted at account creation or reload.
- Ledger movement: credit purchase, spend deductions, refund/dispute reversal.
- Product usage summary: route/lane, cost, duration, and whether speech/screen/docs
  were used. Avoid including transcript or document contents in dispute packets
  unless legal/support review explicitly approves it.

## Dispute Response Flow

1. Confirm the Square dispute or failed payment in the Square dashboard.
2. Find the matching Bluey ledger event by Square order/payment ID.
3. Mark the account `review_required` operationally and stop further high-cost
   usage if the disputed credit batch is still funding requests.
4. Preserve logs/support bundle for the dispute time range.
5. Prepare evidence: receipt, checkout event, terms acceptance, account creation,
   usage summary, and credit ledger.
6. If the dispute is lost or refunded, reverse remaining disputed credits and
   reconcile the account. Do not let account balance go negative without a
   deliberate collections/support decision.
7. Document outcome in the support log and update fraud signals if the pattern
   repeats.

## Paid Alpha Guardrails

Before inviting wider paid testers:

- Keep first reload amount modest (`$15` default). Auto Reload must stay opt-in
  with a visible threshold and reload amount.
- Treat Square reload math as a hard invariant: `Square paid cents ==
  Bluey expected cents == credited cents`, currency is `USD`, and the Square
  account/reference metadata points to the same Bluey account. Any mismatch is
  a review event, never an automatic balance credit.
- Keep provider-side billing alerts/caps where providers support them.
- Keep Bluey app-level spend guard enabled during alpha.
- Review high-spend accounts daily until monitoring is automated.
- Require verified email before meaningful paid usage.
- Treat repeated disputes as a hard account block.
- Do not market credits as money, cash balance, gift card, stored value, or
  transferable wallet.

## Operator Checklist

- [ ] Square webhook delivery is green for sandbox and production.
- [ ] One sandbox reload and one low-dollar live reload have credited correctly.
- [ ] One sandbox mismatch replay fails closed with no balance movement.
- [ ] Dispute notification path is known in Square dashboard.
- [ ] Support mailbox can receive billing/refund requests.
- [ ] Terms and privacy pages are live and linked from signup/reload.
- [ ] Admin/support runbook says how to freeze/reverse disputed credits.
- [ ] Logs can find a transaction by Square payment ID, Bluey account ID hash,
      request ID, or trace ID.

## What Not To Do

- Do not grant free credits automatically after webhook failure unless an operator
  has verified payment in Square.
- Do not retry a failed crediting webhook by manually editing balances without
  recording the Square event ID.
- Do not store card PAN/CVV/expiration in Bluey logs, DB, docs, or support notes.
- Do not expose provider keys or customer transcript contents in dispute evidence
  by default.

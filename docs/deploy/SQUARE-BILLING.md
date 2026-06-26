# Square Billing Setup

Bluey supports Square as the primary billing provider for hosted credit reloads.
Do not commit Square application IDs, access tokens, location IDs, or webhook
signature keys. Keep them in `/etc/bluey-api/bluey-api.env` or a secret manager.

Bluey sells non-transferable SaaS account credits for managed AI work. Do not
market them as a cash wallet, stored-value card, gift card, financial product,
or transferable balance.

Credits are valid for up to 12 months (365 days) from purchase. The server
spends the oldest unexpired credit batch first and stops managed usage at zero.

## Runtime Switch

The same server binary supports sandbox and production:

```ini
BLUEY_BILLING_PROVIDER=square

# preprod
SQUARE_ENVIRONMENT=sandbox

# production
SQUARE_ENVIRONMENT=production
```

When `SQUARE_ENVIRONMENT=sandbox`, Bluey reads `SQUARE_SANDBOX_*` values.
When `SQUARE_ENVIRONMENT=production`, Bluey reads `SQUARE_PRODUCTION_*` values.
This lets promotion switch billing credentials by changing one environment value
and restarting `bluey-api.service`.

## Required Values

Each environment needs:

- `SQUARE_*_APPLICATION_ID`
- `SQUARE_*_ACCESS_TOKEN`
- `SQUARE_*_LOCATION_ID`
- `SQUARE_*_WEBHOOK_SIGNATURE_KEY`

`APPLICATION_ID` is kept with the server config for completeness and future web
payments/UI flows. The server-side Checkout API call requires `ACCESS_TOKEN` and
`LOCATION_ID`.

## Hosted Checkout Branding

Square hosted checkout uses Square location-level branding. A stale Square
location logo can appear even when Bluey sends a correct `Bluey credits` line
item. Keep the active Square location named `Bluey` and configure online
checkout to use business-name branding instead of a framed logo unless the
Square Dashboard logo is already verified as Bluey.

Apply or verify the expected branding with:

```bash
scripts/bluey-square-branding.sh /etc/bluey-api/bluey-api.env
scripts/bluey-square-branding.sh /etc/bluey-api/bluey-api.env --check
```

Expected production checkout branding:

- Location name: `Bluey`
- Location business name: `Bluey`
- Location website: `https://bluey.sh`
- Checkout header: `BUSINESS_NAME`
- Checkout button color: `#20c7ff`
- Checkout button shape: `ROUNDED`

The cloud preflight runs this check when Square is the active billing provider.

## Webhook

Register this endpoint in Square:

```text
https://bluey.sh/billing/square/webhook
```

Subscribe to:

```text
order.updated
payment.updated
```

Bluey validates `x-square-hmacsha256-signature` with the configured webhook
signature key before processing any event. Completed Square orders are credited
through the same FIFO credit-batch ledger used by the rest of Bluey.

Operational requirements:

- The endpoint must return 2xx quickly for valid, already-processed events.
- Processing must be idempotent by Square event/order/payment ID; Square may
  retry or deliver events more than once.
- Bluey credits only when the signed Square event passes the reload invariant:
  the Bluey account metadata matches the Square reload reference, currency is
  `USD`, the Square paid amount matches `bluey_amount_cents`, the amount is at
  least the configured reload minimum, and exactly one Square payment ID backs
  the credit batch.
- Any account, amount, currency, or payment-id mismatch is treated as a fraud or
  integration signal. The webhook fails closed, no spendable balance is created,
  and the event must be reviewed against the Square dashboard before retrying or
  issuing an explicit internal credit.
- Webhook failures are not a reason to grant credits manually unless an operator
  verifies the payment in Square and records the Square event/payment ID.
- Production launch requires one sandbox reload smoke and one low-dollar live
  reload smoke proving webhook delivery, balance crediting, and idempotent replay.

## Current Scope

Implemented:

- `/billing/checkout` creates a Square hosted payment link.
- Outbound Square API calls pin `Square-Version: 2025-04-16`.
- `/billing/square/webhook` verifies the Square signature and credits completed
  reload orders/payment events.
- Square hosted-checkout orders carry Bluey metadata for account id and expected
  reload cents. Webhook processing rejects mismatches between metadata,
  reference id, Square total, line-item/tender amount, and the credited amount.
- Square saved-card Auto Reload validates the returned payment amount, currency,
  customer id, and reference id before crediting a completed payment.
- Preprod/prod credential switching via `SQUARE_ENVIRONMENT`.
- Integration tests for Square checkout and webhook crediting.

Compliance notes:

- Bluey uses Square-hosted checkout and does not collect, store, or proxy card
  numbers, CVV, or card expiration data.
- Bluey stores Square access tokens and webhook signature keys only in server
  environment/secret storage.
- Refund, support, privacy, and terms pages must be published before production
  traffic.
- Rotate Square credentials before production if they were ever pasted into a
  chat, ticket, or other non-secret channel.
- Disputes and chargebacks follow
  `docs/deploy/ABUSE-FRAUD-CHARGEBACK-PLAYBOOK.md`.

Supported:

- Manual Square reload is always available.
- Square saved-card Auto Reload is opt-in only. It requires a saved Square
  card and credits balance only after a completed Square payment succeeds.
- `/billing/portal` routes Square customers back to `/account?billing=square`
  for card-save and Auto Reload controls.

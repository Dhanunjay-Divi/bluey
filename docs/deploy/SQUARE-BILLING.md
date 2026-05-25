# Square Billing Setup

Bluey supports Square as the primary billing provider for hosted credit reloads.
Do not commit Square application IDs, access tokens, location IDs, or webhook
signature keys. Keep them in `/etc/bluey-api/bluey-api.env` or a secret manager.

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

## Webhook

Register this endpoint in Square:

```text
https://bluey.sh/billing/square/webhook
```

Subscribe to:

```text
order.updated
```

Bluey validates `x-square-hmacsha256-signature` with the configured webhook
signature key before processing any event. Completed Square orders are credited
through the same FIFO credit-batch ledger used by the rest of Bluey.

## Current Scope

Implemented:

- `/billing/checkout` creates a Square hosted payment link.
- `/billing/square/webhook` verifies the Square signature and credits completed
  reload orders.
- Preprod/prod credential switching via `SQUARE_ENVIRONMENT`.
- Integration tests for Square checkout and webhook crediting.

Deferred:

- Square saved-card auto top-up. Manual reload is the supported v0.2 path.
- A Bluey-hosted billing page for card management. `/billing/portal` currently
  routes Square customers back to `/account?billing=square`.


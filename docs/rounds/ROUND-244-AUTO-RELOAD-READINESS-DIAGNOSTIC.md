# Round 244 - Auto Reload Readiness Diagnostic

## Trigger

The owner noticed the local CLI showed:

```text
Balance $4.82 (auto top-up: ON, $30 at <$5)
```

but the account was already below the `$5` threshold and had not reloaded. The
ask was whether the current test account can actually auto-reload.

## Finding

The current smoke/test account cannot auto-reload yet.

Live `/account/me` returned:

- balance: `482` cents
- trial seconds remaining: `0`
- `auto_topup_enabled: true`
- threshold: `500` cents
- amount: `3000` cents
- billing provider: `square`
- Square environment: `production`
- `auto_topup_available: false`
- unavailable reason: `Save a card for Auto Reload before turning this on.`
- billing restricted: `false`

The account export confirmed there is no saved off-session payment method:

- Stripe customer: absent
- Stripe payment method: absent
- Square customer: absent
- Square card: absent
- Square card brand/last4: absent

So the server is right to skip the charge path. Auto Reload needs a saved card
before it can fire.

## Root Cause

The server already exposes the correct readiness fields on `/account/me`, but
the desktop client type only deserialized the older fields:

- `auto_topup_enabled`
- `auto_topup_threshold_cents`
- `auto_topup_amount_cents`

Because the CLI ignored `auto_topup_available` and
`auto_topup_unavailable_reason`, `bluey usage` printed a misleading plain
`ON` state even when the account was not charge-ready.

## Fix

Updated `cue-cloud-client` `AccountMe` to accept the newer billing-readiness
fields:

- `billing_provider`
- `auto_topup_available`
- `auto_topup_unavailable_reason`
- `saved_payment_method_label`
- `square_environment`
- `billing_restricted`
- `billing_restriction_reason`

Updated `bluey usage` to render:

- `OFF` when disabled
- `ON, $30 at <$5` when enabled and ready
- `ON, $30 at <$5 (Visa ending 4242)` when a payment label is available
- `ON, setup needed: ...` when enabled but no saved card is available
- `PAUSED, ...` when billing is restricted

Installed the rebuilt CLI locally so the current output now says:

```text
Balance         $4.82      (auto top-up: ON, setup needed: Save a card for Auto Reload before turning this on.)
```

## Verification

Passed:

```bash
cargo fmt -p cue-cloud-client -p cue-cli
cargo test -p cue-cli auto_topup_label -- --nocapture
cargo check -p cue-cloud-client -p cue-cli
git diff --check
cargo build --release -p cue-cli
install -m 755 target/release/bluey "$HOME/.bluey/bin/bluey"
install -m 755 target/release/cue "$HOME/.bluey/bin/cue"
"$HOME/.bluey/bin/bluey" usage
```

Targeted tests:

- `auto_topup_label_shows_ready_saved_card`
- `auto_topup_label_shows_setup_needed_when_enabled_without_card`
- `auto_topup_label_shows_paused_for_restricted_billing`

## Current State

- Local CLI installed and verified.
- No server change was required; the live server already returns the readiness
  fields.
- The test account still needs a saved Square card before Auto Reload can
  actually charge.

## Remaining Gates

- Save a test card through the account/reload flow, then re-check
  `/account/me` until `auto_topup_available: true`.
- Run one low-balance paid usage event after the card is saved and verify:
  - Square payment is created once
  - balance is credited exactly once
  - later Square webhook is idempotent
- Consider a follow-up admin/repair action for legacy rows where
  `auto_topup_enabled=true` but `auto_topup_available=false`, so product state
  cannot remain confusing.

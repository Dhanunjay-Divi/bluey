# Round 379 - Bluey Prepaid Billing Defaults

## Trigger

Owner clarified that Bluey should reuse Pinky's billing safety lessons without copying Pinky's subscription model. Bluey should be a prepaid wallet with one-time credit reloads and optional Auto Reload card-on-file.

## Product Rule

- Manual Add credits defaults to $15.
- Minimum reload is $15.
- Auto Reload defaults to: when balance is below $5, add $15.
- Auto Reload can be set higher, capped at $500 for the first version.
- Bluey must never credit spendable balance just because checkout was created. Credits are added only from confirmed processor payment ids.

## Changes

- Updated account creation defaults to $5 Auto Reload threshold and $15 Auto Reload amount.
- Updated SQLite and Postgres runtime schema defaults to match.
- Raised Auto Reload max from $100 to $500 and changed API behavior from silent clamping to explicit validation errors.
- Updated dashboard billing inputs, manual reload default, landing pricing card, and account preview copy from $30-first wording to $15-minimum prepaid wallet wording.
- Kept the existing Square/Stripe processor-confirmed crediting path intact:
  - Square hosted checkout creates a payment link only.
  - Square webhooks credit only completed Bluey reload payments.
  - Stripe checkout credits only from `checkout.session.completed`.
  - `credit_processor_payment` remains idempotent by provider payment id.
  - Square Auto Reload credits immediately only when Square returns `COMPLETED`, otherwise waits for webhook completion.

## Pinky Lessons Applied

- Processor owns card data; Bluey stores only provider/customer/card references and card brand/last4.
- Checkout creation is not treated as payment evidence.
- Duplicate processor webhook/payment events are safe because ledger crediting is idempotent.
- Card-on-file UI continues to use Square Web Payments SDK.

## Verification

- `node --check web/assets/bluey-site.js` passed.
- `cargo check --manifest-path server/Cargo.toml --bin bluey-server` passed.
- `cargo test --manifest-path server/Cargo.toml credit_idempotent_on_same_processor_payment_id -- --nocapture` passed.
- `cargo test --manifest-path server/Cargo.toml create_initializes_fifteen_minute_trial_budget -- --nocapture` passed.
- `cargo test --manifest-path server/Cargo.toml square_order_extracts_credit_from_completed_order -- --nocapture` passed.
- `cargo test --manifest-path server/Cargo.toml auto -- --nocapture` passed after updating the integration test expectation to the new $5/$15 default.
- `git diff --check` passed.

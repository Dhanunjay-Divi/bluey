# ROUND-454 Recreated Account Card Fallback

Date: 2026-07-09
Backup thread: 019e133e-d92a-7830-8df0-3a050a4e22f6

## Trigger

After deleting an account and creating the same email again, the Add Balance modal showed the embedded Square card section but the card frame rendered:

> An unexpected error occurred while using Card.

The account itself was recreated correctly. The failure was in the browser-side embedded Square card widget state, not in the balance ledger.

## What Changed

- Made the Square card widget account-scoped in `web/assets/bluey-site.js`.
- The card setup key now includes account id/email, billing provider, Square environment, application id, location id, and the target card container.
- If the account changes, including delete-and-recreate with the same email, Bluey clears the previous card widget and modal state before rendering billing again.
- If the embedded Square card form fails to load or tokenize, the Add Balance modal now:
  - hides the broken card iframe,
  - disables Auto Reload for that checkout if no saved card exists,
  - shows a clear recovery message,
  - lets the user continue with hosted Square checkout,
  - keeps a Retry card path for users who want to save a card.
- Separated the product paths:
  - One-time Add Balance uses hosted Square checkout when there is no saved card and Auto Reload is off.
  - Auto Reload setup opens the embedded Square card only when the user is actually saving a card.
  - The primary button is disabled while the secure card form is still loading instead of throwing a tokenization error.
  - The modal copy now says the card path is one step: add balance now and save the card for future Auto Reload.

## Why This Matters

One-time balance reload should not depend on the embedded card widget being healthy. Hosted checkout is the safer fallback because Square owns the page, browser frame state, and card collection flow.

Auto Reload is not a monthly subscription. For Bluey credits, it means card-on-file top-up. When the embedded card path is ready, the browser tokenizes the card, the server saves a Square card-on-file, charges the selected reload amount, credits the balance, and persists the Auto Reload threshold in one flow.

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check -- web/assets/bluey-site.js docs/rounds/ROUND-454-RECREATED-ACCOUNT-CARD-FALLBACK.md`

## Still Needed

- Live browser smoke after the next requested deploy:
  1. Delete and recreate a non-internal account.
  2. Open Add Balance.
  3. Confirm stale card errors do not persist across the recreated account.
  4. Confirm hosted checkout still opens if the embedded card form fails.
  5. Confirm Retry card remounts the Square card form.
  6. Confirm normal one-time Add Balance does not wait on the embedded card form when Auto Reload is off.

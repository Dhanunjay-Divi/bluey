# FIX-625: Close ATS Observation Categories to PII-Safe Values

> **Codex preflight:** Loaded `$bluey-ops` and reviewed the current Round 604
> observation schema and PII-free acceptance criterion locally.

## Issue

Layout observation challenge and confirmation categories accepted arbitrary
bounded tokens, including OTP-, cookie-, bearer-token-, and credential-shaped
values.

## Root Cause

The privacy validator used the generic identifier grammar for category arrays.
Length and character bounds prevent unbounded content but do not distinguish a
structural category name from a captured candidate or credential value.

## Fix Summary

- Replaced open token validation with small closed vocabularies for structural
  challenge and provider-confirmation categories.
- Required strict sorted uniqueness in both arrays.
- Added adversarial rejection coverage for OTP, token, cookie, credential, and
  unknown category shapes.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs/ats_certification_authority.rs` | Define and enforce PII-safe closed category vocabularies and negative tests. |
| `docs/work/FIX-625-jobs-ats-observation-category-privacy.md` | Record the privacy defect and repair. |

## Edge Cases Handled

- Structural `email_otp` and `sms_otp` categories remain representable without
  storing the OTP value.
- Duplicate or unsorted categories fail canonical validation.
- Captured secret-shaped values cannot be re-labeled as categories.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml \
  layout_observation_rejects_otp_token_cookie_and_credential_shaped_categories
node jobs/scripts/privacy-gate.mjs
```

## Known Limitations

- The worker transport must continue sending only structural category names;
  this schema intentionally cannot preserve free-form provider labels.

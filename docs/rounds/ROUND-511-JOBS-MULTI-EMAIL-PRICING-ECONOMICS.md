# Round 511 - Jobs Multi-Email, Pricing, And Economics

Date: 2026-07-10

## Goal

Make multiple email addresses understandable and safe for one Bluey Jobs user while keeping plan pricing profitable and predictable.

## Product Model

Bluey Jobs now treats these as separate concepts:

1. The Bluey login email authenticates the account.
2. A verified application email appears on resumes and employer forms.
3. A connected Gmail or Outlook inbox is authorized for application-status sync.

One Bluey Jobs account represents one job seeker. Application aliases delivered to the same mailbox do not consume additional inbox slots. Independent Gmail or Outlook accounts each consume one inbox slot.

The verified Bluey login becomes the first application email automatically. Users can add and label more application addresses, verify them with a six-digit code, set a default, and choose a verified address for each Career Track. The chosen address is frozen into the exact job-specific resume and application receipt.

## Plan Policy

| Plan | Price | Career Tracks | Completed packets | Application emails | Connected inboxes | Browser |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| Free | $0 | 1 | 5 reviewed | 2 | 1 | Review only |
| Pro | $29/month | 3 | 50 | 10 | 2 | Local |
| Cloud | $49/month | 5 | 100 | 25 | 5 | Local and cloud |

- Additional independent inbox: $4/month.
- Additional completed packet: $0.50 from the shared Bluey balance.
- Aliases and verified application-email strings are not separately charged.
- Inbox add-ons should join the recurring Jobs invoice rather than create a separate low-value card transaction.

## Implementation

- Added encrypted application-identity and mailbox-connection records to SQLite and PostgreSQL migrations.
- Added keyed email and provider-subject lookup hashes for uniqueness without storing plaintext lookup values.
- Added verification-code issuance, expiry, resend cooldown, and constant-time code checks.
- Added tenant-scoped identity and mailbox APIs with plan-limit enforcement.
- Added Career Track email selection and verified-identity validation.
- Added exact application-identity provenance to generated resumes and receipts.
- Added a compact Settings experience for application emails, inbox connections, limits, and plan comparison.
- Updated the local preview workspace for realistic multi-email and alias behavior.
- Updated Jobs terms and privacy copy for application identities, aliases, inbox authorizations, plan allowances, and the one-seeker-per-account rule.

## Economics

The internal planning model is documented in `jobs/UNIT_ECONOMICS.md` and is deliberately absent from customer UI.

At full included usage:

- Pro estimates $9.21 direct COGS, $19.79 contribution, and 68.2% contribution margin.
- Cloud estimates $20.39 direct COGS, $28.61 contribution, and 58.4% contribution margin.

At 60% packet utilization:

- Pro estimates a 74.1% contribution margin.
- Cloud estimates a 68.6% contribution margin.

The estimates include packet generation, cloud browser allowance, inbox processing, payment fees, shared infrastructure, and an 8% support/refund/fraud reserve. They exclude CAC, general R&D, founder time, and other company-wide fixed costs. Costs must be reviewed monthly against actual P50 and P95 workload data.

## UX Verification

The Settings page was checked in the in-app browser at desktop and 390 x 844 mobile sizes.

- Application email rows remain compact and readable.
- The add-email and verification dialogs fit without overflow.
- A preview identity can be added and verified end to end.
- Gmail aliases display under one inbox connection.
- Desktop and mobile layouts have no horizontal overflow.
- Bluey light/dark theme tokens remain intact.

## Automated Verification

- `cargo test db::jobs::tests --lib` in `server`: 9 passed.
- `cargo test mail::tests --lib` in `server`: 7 passed.
- `cargo check --bin bluey-jobs-api` in `server`: passed.
- `npm run typecheck` in `jobs`: passed across automation, browser, workflows, and portal.
- `npm run test` in `jobs`: 19 passed.
- `npm run build --workspace @bluey/jobs-portal` in `jobs`: passed.

## Scope

This round changes only Jobs web/API/data/legal surfaces and generated Jobs web assets. It does not change the Bluey host overlay, audio, meeting runtime, or native session behavior. It was not deployed in this round.

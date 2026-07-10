# Bluey Jobs Email Identities

## Product Rule

One Bluey account represents one job seeker. That person may use multiple verified application emails and connect multiple independent inboxes. A team, household, or agency managing multiple candidates needs one account per candidate so resumes, answers, receipts, and employer communication never cross profiles.

Bluey keeps three concepts separate:

1. **Bluey login email** authenticates the account.
2. **Application email** is a verified address placed on resumes and employer forms.
3. **Connected inbox** is one Gmail or Outlook authorization used to recognize application updates and follow-ups.

Changing an application email never changes the Bluey login. Several aliases delivered into one mailbox use one inbox connection. Two independent Gmail accounts use two connections even when the same person owns both.

## Customer Flow

- The verified Bluey login becomes the first application email automatically.
- A user can add another address, label it, and verify it with a six-digit email code.
- Every Career Track chooses a verified application email. The account default is used when a Track has no override.
- The selected identity is frozen into the job-specific resume and application receipt.
- An address cannot be verified on two Bluey Jobs accounts at once. It must be removed from the first account before it can move.
- The default identity cannot be removed until another verified identity becomes default.
- An identity used by a Career Track cannot be removed until the Track is changed.

## Plan Limits

| Plan | Price | Application emails | Connected inboxes | Career Tracks | Included packets |
| --- | ---: | ---: | ---: | ---: | ---: |
| Free | $0 | 2 | 1 | 1 | 5 reviewed |
| Pro | $29/month | 10 | 2 | 3 | 50 |
| Cloud | $49/month | 25 | 5 | 5 | 100 |

Application aliases are included and are not charged individually. An additional independent inbox connection is $4/month. Additional completed packets are $0.50 from the shared Bluey balance after the monthly allowance.

## Data And Security

- `jobs_application_identities` stores encrypted identity payloads plus a keyed lookup hash for global uniqueness.
- `jobs_identity_verifications` stores short-lived keyed OTP hashes, never verification codes.
- `jobs_mailbox_connections` stores encrypted display metadata. Provider subjects use keyed hashes for uniqueness.
- OAuth access and refresh tokens belong in the encrypted integration-secret store, never in API responses, receipts, logs, or browser storage.
- Mailbox aliases discovered by a provider are display metadata. An alias must still be a verified application identity before Bluey can place it on a resume or form.
- Disconnecting an inbox stops new status sync but does not rewrite prior application receipts.

## Scaling Shape

Mailbox sync is event-driven. Gmail uses push notifications and Microsoft uses change notifications, with idempotent cursors and periodic subscription renewal. Workers partition by account and mailbox ID, apply per-provider and per-mailbox rate limits, and emit normalized application-status events. No worker polls every mailbox continuously.

The API enforces tenant ownership and plan limits before a record enters a workflow. Idempotency keys cover provider subject, notification ID, normalized message ID, and application-status transition.

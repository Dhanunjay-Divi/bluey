# Bluey Jobs Architecture

## Product boundary

Bluey Jobs is a separate product surface at `/jobs`. It shares Bluey identity,
account balance, and payment authority. It does not import or modify the Bluey
meeting overlay, audio capture, transcription, or native session runtime.

The Jobs API can run as its own `bluey-jobs-api` process on port 8081. Caddy
routes only `/api/jobs/*` to that process. Bluey's existing API remains the
authority for login, account state, billing, and balance.

## Services

| Service | Responsibility | Scale unit |
| --- | --- | --- |
| Jobs portal | React customer experience and document previews | Static edge asset |
| Jobs API | Tenant-scoped profile, jobs, packets, applications, entitlements, receipts | Stateless API instance |
| Discovery workers | Licensed provider and public ATS feed collection, normalization, dedupe | Provider and region queue |
| Document workers | Resume import, packet generation, PDF/DOCX export | CPU worker |
| Temporal workers | Durable application, intervention, reminder, and status workflows | Task queue |
| Local Browser | User-owned isolated Chromium profile | User machine |
| Cloud browser pool | One encrypted, isolated Chromium session per active run | Browser container |

## Storage

- PostgreSQL is canonical for tenant data, packet metering, and workflow state.
- OpenSearch holds normalized searchable job documents, never account secrets.
- Redis or Valkey holds rate limits, leases, and short-lived runner presence.
- R2 or S3 holds resumes, screenshots, receipts, and encrypted browser bundles.
- Jobs JSON payloads are authenticated-encrypted at rest with AES-256-GCM.
- Browser cookies and cloud profiles use per-account envelope keys in production.
- Raw job-site passwords are never stored by Bluey Jobs.

The repository keeps SQLite parity for local development and single-node beta
testing. Multi-instance environments must use PostgreSQL.

## Core invariants

1. Every customer record is scoped by `account_id`.
2. Every final resume version belongs to exactly one canonical job.
3. A canonical job is metered once per account, even across retries or runners.
4. Packet commit, overage deduction, and ledger insertion are one transaction.
5. Auto-submit requires hard filters, the account threshold, and an automatable
   employer form. Restricted sites stay user-controlled handoffs.
6. Local and cloud execution use the same typed adapter contract.
7. Unknown required fields, CAPTCHA, 2FA, and assessments create interventions
   instead of guessed answers.
8. A submission receipt retains the job snapshot, exact resume, final answers,
   browser evidence, and timestamps.
9. Public ATS discovery constructs only known HTTPS endpoints, rejects
   redirects, caps pages and payloads, and never accepts a caller-supplied host.
10. Reusable answers resolve company first, then Career Track, then account;
    Auto-submit cannot use an unconfirmed generated fact.

## High-scale deployment

At the target of one million accounts and one thousand concurrent cloud
browsers, scale each queue independently:

- shard discovery by provider and use distributed per-host throttles;
- partition workflow task queues by region and runner type;
- allocate cloud browsers through short leases with heartbeats and hard TTLs;
- index canonical jobs asynchronously through an outbox;
- use account-aware cache keys and row-level tenant assertions;
- keep packet creation and metering idempotent under retries;
- autoscale browser pools on queue age, not only CPU;
- maintain regional object-store replication and Temporal recovery runbooks.

## Beta flag

Release builds return 404 for Jobs APIs unless
`BLUEY_JOBS_BETA_ENABLED=1`. Local debug builds remain enabled for development.
The static portal may be deployed before the API flag is enabled without
exposing customer data or automation endpoints.

## Operational completion gates

The portal, persistence model, metering, policy layer, public ATS discovery,
shared form planning, receipt model, Electron controller, and Temporal
boundaries are implemented in this branch. Public launch still requires
provider credentials and deployment-specific work for licensed discovery,
Gmail/Outlook OAuth, packaged desktop installers, fixture-backed submission
adapters, and the production cloud browser-pool activity. Those surfaces must
remain labeled beta until their integration suites pass.

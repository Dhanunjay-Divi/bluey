# Bluey Jobs

Bluey Jobs is an isolated product surface under `/jobs`. It shares Bluey
identity, billing, and account balance, but it does not import or modify the
meeting overlay, audio pipeline, or native session runtime.

## Packages

- `portal`: React 19 customer app. Vite builds it into `web/jobs`.
- `automation`: shared ATS, discovery, browser, receipt, and policy contracts.
- `browser`: Electron controller with one Playwright Chromium profile per verified application email.
- `runner`: isolated cloud Chromium runner with encrypted profile snapshots.
- `workflows`: Temporal worker and workflow-start gateway for durable cloud runs.
- `server/src/bin/bluey-jobs-api.rs`: independently deployable Jobs API process.
- `server/src/api/jobs.rs`: authenticated `/api/jobs/*` endpoints.
- `server/src/db/jobs.rs`: SQLite/Postgres tenant persistence and packet metering.

## Local development

```bash
cd jobs
npm install
npm run test
npm run build
npm run dev --workspace @bluey/jobs-portal
```

Open `http://127.0.0.1:5187/jobs/?preview=1` for the deterministic visual fixture.
Without `preview=1`, the portal uses the normal Bluey browser tokens and the
authenticated Jobs API.

The local browser controller needs Playwright Chromium once per developer
machine:

```bash
npx playwright install chromium
npm run build --workspace @bluey/jobs-browser
npm run start --workspace @bluey/jobs-browser
```

The cloud path runs the Jobs API, Temporal application worker, workflow
gateway, scheduled discovery worker, and browser runner as separate processes.
Required configuration is documented in `OPERATIONS.md`.
Unknown required fields and verification challenges preserve the browser and
pause the workflow for up to 24 hours; resolving the Intervention Inbox resumes
the same application.

The release Jobs API is dark unless `BLUEY_JOBS_BETA_ENABLED=1`. Run the
standalone service with:

```bash
cd server
BLUEY_JOBS_BETA_ENABLED=1 BLUEY_JOBS_API_PORT=8081 cargo run --bin bluey-jobs-api
```

See `ARCHITECTURE.md` for service boundaries, scale targets, and the remaining
provider-specific release gates. See `MULTI_EMAIL_ARCHITECTURE.md` for
application identities and inbox connections, and `UNIT_ECONOMICS.md` for the
internal pricing and margin model.

## Data guarantees

- Every customer row includes `account_id`.
- Every resume version includes one `job_id`.
- Canonical jobs deduplicate per account.
- Application state uses one validated enum.
- Packet metering is unique per account and canonical job.
- Overage balance deduction and its audit entry are atomic.
- LinkedIn and Indeed are handoff-only in background policy.
- Local and cloud runners consume the same adapter contract.
- Public Workday, Greenhouse, Lever, Ashby, and SmartRecruiters connectors share
  host-pinned, retry-bounded normalization. The scheduled, server-leased worker
  accepts only official identifiers for those five ATS families. An account can
  receive a source from an administrator or from one of its own verified public
  ATS imports; enrollment preserves the original Career Track and never turns
  manual, LinkedIn, Indeed, ZipRecruiter, Dice, unknown, or private links into
  scheduled discovery.
- Form planning uses confirmed facts and company/track/account answer memory;
  unknown required questions become interventions.
- Company Answer Memory overrides Career Track memory, which overrides account
  memory; an answer set on the current application always wins.
- Queue requests, Temporal activity retries, and intervention resumes use
  durable idempotency keys so a completed submit action cannot run twice.
- Local runs use encrypted, short-lived launch tickets. The custom protocol
  never carries a long-lived Bluey token or the application packet itself.
- Bluey Browser reports local results to the Jobs API and keeps the same visible
  page alive while the account owner completes an intervention.
- Browser navigation is checked at initial load and on redirects against
  private-network and credential-bearing targets.
- Every committed run retains the exact application bundle and evidence in one
  fingerprinted receipt.
- Machine-local document and screenshot paths are removed before receipts enter
  account storage.
- Application emails are verified independently from the Bluey login, selected
  per Career Track, and frozen into the resume and receipt.
- Separate Gmail and Outlook mailboxes are tenant-scoped and plan-limited;
  aliases inside one mailbox do not consume another connection.
- Email verification can match a fresh provider message to the active
  application and offer one-click approval. Only the encrypted provider
  message reference and expiry enter the intervention record; the OTP remains
  ephemeral. CAPTCHA and phone/app checks remain owner-completed handoffs.

## Source provenance

Research clones are kept under ignored `_refs/jobs-research/` directories.
Shipped adaptations, exact source commits, authorization, and license notices
are documented in `THIRD_PARTY_PROVENANCE.md`, `THIRD_PARTY_NOTICES.md`, and
`OPEN_SOURCE_RESEARCH.md`.

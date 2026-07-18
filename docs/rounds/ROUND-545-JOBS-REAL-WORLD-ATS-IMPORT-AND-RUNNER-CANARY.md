# Round 545: Jobs Real-World ATS Import And Runner Canary

Date: 2026-07-18

## Goal

Exercise the real Bluey Jobs path through public employer data without trusting
browser-supplied job facts or accidentally submitting an application:

1. import a direct employer ATS link;
2. normalize and verify the listing on the server;
3. evaluate freshness, employment type, location, sponsorship, and other hard
   filters using the shared eligibility decision;
4. generate a truthful, job-specific application kit;
5. preserve the Review-first approval and metering boundary; and
6. verify that local and cloud runners remain unavailable unless the account,
   distribution, gateway, and job capability gates all pass.

Final employer submission was intentionally not performed. That irreversible
action requires action-time confirmation naming the employer, role, candidate,
data being sent, and destination.

## Implementation

### Server-owned ATS import

`server/src/api/jobs_import.rs` adds strict, no-redirect, size-limited importers
for exact public ATS hosts:

- Lever
- Greenhouse
- Ashby
- SmartRecruiters
- Workday

The API imports company, title, location, workplace, description, compensation,
employment type, public identifier, and posting timestamp when the provider
exposes them. Unknown hosts are never fetched and remain manual Review-only
records. User-supplied company, score, verification, capability, and freshness
do not become authority for supported imports.

`server/src/api/jobs.rs` resolves the link before persistence and maps provider
failures to useful HTTP errors. `server/src/db/jobs.rs` stores structured
employment type and uses provider facts in the shared eligibility decision.

### Hard-filter and packet truth

- Imported stale jobs cannot prepare an application or enter a runner.
- Employment type uses the imported structured value before text inference.
- Sponsorship blockers include explicit no-sponsorship and citizenship-only
  language.
- Resume tailoring scores repeated and aliased JD terms without inventing facts.
- Generated headlines no longer contain placeholder locations.
- Packet review renders the real resume version and diff fields.

### Portal flow

The Add job dialog is URL-first. Supported links import and score in one action.
If a provider is unsupported or temporarily unavailable, the same dialog reveals
a manual fallback and clearly labels it Review-only.

## Public Listing Evidence

The release canary uses a public Ashby listing:

- Employer: OpenAI
- Role: Software Engineer, Codex - Enterprise Controls
- Canonical link:
  `https://jobs.ashbyhq.com/openai/fff02c39-1185-427c-bf89-70d7eaa5e3db`
- ATS identifier: `fff02c39-1185-427c-bf89-70d7eaa5e3db`
- Location: San Francisco
- Workplace: Hybrid
- Employment type: Full time
- Published: 2026-07-13
- Listed at verification time: yes

Ashby's public posting API and the public page title were used only to verify
observable employer facts. No hidden or state-changing employer endpoint was
called.

Two public Lever listings were also checked as negative freshness cases:

- Kestrel Intelligence, Software Engineer, published 2026-01-15; explicit US
  citizenship requirement.
- IntraFi, Full Stack Software Engineer, published 2026-05-07.

With the account's 14-day freshness policy, both must be blocked before packet
preparation.

## Candidate And Account Boundary

The signed-in production account used for the canary is masked as
`internal-admin-...3943@bluey.sh`. Its current candidate profile is ADITHYA
REDDY KOPPULA, imported from `Adithya_Reddy_Koppula_Resume_Rivian.docx`, with
Review first enabled and sponsorship required.

No candidate identity or resume is sent to an employer during import, scoring,
or application-kit preparation. Identity and the exact resume version are
frozen only in Bluey's application record and receipt preparation.

## Runner Boundary

Runner behavior remains intentionally strict:

- `awaiting_review` applications never appear in runner pickers;
- the queue API rejects `awaiting_review` until explicit approval;
- metering happens once after approval/commit, not during import or preview;
- local execution requires local-run entitlement and an available signed
  distribution;
- cloud execution requires cloud entitlement and an available gateway/pool;
- stale, blocked, handoff-only, unknown, or ineligible jobs cannot enter a
  runner; and
- uncertain submission side effects are terminal and never blindly retried.

The current Free account truthfully shows the invited-beta runner as unavailable.
This round does not claim a successful local or cloud employer submission.

## Verification

Completed before commit:

```text
cargo fmt --all -- --check
cargo test --manifest-path server/Cargo.toml
  585 server unit tests
  75 server HTTP integration tests
cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings

cd jobs
npm test
  automation 133
  browser 34
  runner 50
  workflows 35
  portal 55
  total 307 package tests
npm run typecheck
npm run build

git diff --check
```

The Vite build completed with one non-blocking warning for the main minified
bundle size. Production source maps are not emitted.

Chrome could enumerate the signed-in production and local preview tabs, but the
installed Chrome control extension timed out while claiming either tab even
after opening a fresh Profile 2 window. This is recorded as a QA-tool connection
failure, not a Bluey page failure. Signed-in production QA must be repeated after
deploy before claiming the canary complete.

## Post-Deploy Canary Checklist

1. Confirm `/jobs/` is 200 and missing `/jobs/assets/*.map` is 404.
2. Confirm unsigned `/api/jobs/workspace` is 401 and public internal Jobs routes
   remain 404.
3. Import the OpenAI Ashby URL from the signed-in account.
4. Confirm server-owned employer, title, location, workplace, employment type,
   date, capability, and eligibility render.
5. Prepare in Review first and confirm no allowance or balance is consumed.
6. Inspect the exact resume diff, answers, cover-letter state, identity, pause
   reasons, capability, and metering state.
7. Confirm the application cannot reach local/cloud selection before approval.
8. Confirm unavailable runner access is explained rather than simulated.
9. Do not click the employer's final Submit action without fresh action-time
   confirmation.

## Rollback

Rollback only the Jobs API binary and `/var/www/bluey/jobs` portal bundle to the
predeploy timestamped backups. Do not republish or replace the signed native
Bluey release and do not restart unrelated Bluey services.

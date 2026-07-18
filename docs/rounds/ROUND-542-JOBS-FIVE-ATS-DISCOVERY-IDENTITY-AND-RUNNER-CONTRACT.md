# Round 542: Five-ATS Discovery, Identity, And Runner Contract

Date: 2026-07-18

## Objective

Close the gap between the Jobs portal's product language and the executable Jobs
pipeline. This round extends the scheduled public-ATS discovery worker from
Greenhouse and Lever to all five initial ATS families, while preserving the
server-owned eligibility, verified identity, and runner gates that prevent an
untrusted job record from becoming an employer-facing submission.

This round does not claim that every employer tenant or arbitrary website is
certified for unattended submission. Discovery coverage, form-filling support,
and live submission certification are separate capabilities.

## Implemented

### Scheduled discovery for five ATS families

`jobs/workflows/src/discovery-provider.ts` now maps canonical, server-provisioned
source records to the shared public ATS library for:

| Provider | Canonical source identifier | Official endpoint shape |
| --- | --- | --- |
| Greenhouse | board token | `boards-api.greenhouse.io` |
| Lever | site name | `api.lever.co` |
| Ashby | board name | `api.ashbyhq.com` |
| SmartRecruiters | company identifier | `api.smartrecruiters.com` |
| Workday | `tenant~instance~site` | tenant `myworkdayjobs.com` host |

The workflow rejects unknown provider kinds, extra provider configuration, and
source-key/config mismatches. Workday host construction is derived from its
three validated identifiers; a caller cannot provide an arbitrary host or URL.

`server/src/db/jobs.rs` now provisions only those canonical provider records and
emits the normalized provider configuration consumed by the worker. This keeps
discovery authority server-owned rather than trusting a browser-supplied URL or
capability label.

### One eligibility decision before preparation or execution

`evaluate_job_eligibility` remains the shared server-side decision used by
matches, packet preparation, queueing, and browser-run startup. It checks:

- active availability and recent live verification;
- posting freshness;
- healthy discovery-source authority when a scheduled source owns the job;
- excluded companies and titles;
- salary floor;
- location and workplace policy;
- employment type;
- target experience range;
- sponsorship requirements;
- one active application per normalized company;
- atomic daily attempt limits;
- missing required facts and match threshold; and
- ATS capability.

Unknown sites cannot self-certify through request data or a source-name suffix.
Pasted or unknown forms remain Review-only. LinkedIn and Indeed remain packet
preparation/handoff surfaces under the current policy. The five known ATS
families remain `beta_review` until their exact runner implementation and live
tenant matrix are certified. A production operator must not describe them as
unattended Auto-submit before that gate changes through server-owned metadata.

### Verified application identities, never fabricated addresses

Bluey does not create fake candidate email addresses.

1. The verified Bluey login is bootstrapped as the first application identity.
2. An additional address is pending until its six-digit email code succeeds.
3. A keyed lookup hash prevents one address from belonging to two Bluey
   candidates simultaneously.
4. Every Career Track selects one verified application identity.
5. Packet approval freezes the identity ID and email into the resume, receipt,
   browser profile, and run contract.
6. A different email, Career Track, or resume can never bypass the canonical
   one-candidate/one-active-application-per-company guard.

Resume contact text is not trusted as an application identity. Changing a PDF or
DOCX contact email cannot silently create or verify an address.

### Portal login and browser profile model

Discovery feeds do not need candidate credentials. Employer application logins
are handled inside a Bluey Browser profile scoped to:

```text
Bluey account
  -> verified application identity
       -> isolated local or cloud Chromium profile
            -> employer/ATS cookies and session state
```

Bluey stores no raw employer-site password. The user signs in through the real
site inside the isolated browser. A required account confirmation, CAPTCHA,
phone/app 2FA, assessment, unknown legal question, or missing candidate fact
pauses the preserved run. Email OTP assistance may be offered only through an
explicitly connected inbox and still requires the authorized resolution action.

### Plan and runner behavior

| Plan | Packets | Application identities | Local runner | Cloud runner |
| --- | ---: | ---: | --- | --- |
| Free | 5/month | 2 | No | No |
| Pro | 50/month | 10 | Yes, only when the signed Bluey Browser distribution gate is enabled | No |
| Cloud | 100/month | 25 | Yes, when distribution is enabled | Yes, only when the production workflow origin and worker credentials are configured |

The queue API re-checks the entitlement and the same job eligibility decision.
`awaiting_review` cannot enter a runner. An approved packet is immutable and
binds the job, resume version, answer set, application identity, browser profile,
and checksum. Packet metering is idempotent; a retry or browser handoff does not
charge again.

## Universal portal strategy

"Every portal" cannot safely mean treating every public webpage as the same form.
Bluey's durable coverage model is:

1. Official or licensed discovery connectors find and refresh jobs.
2. Deterministic, versioned ATS adapters handle known application systems.
3. A constrained semantic form adapter prepares compatible direct-employer
   forms, but unknown sites stay Review-only until certified.
4. The runner pauses on an unrecognized required field rather than inventing an
   answer.
5. Adapter certification is enabled independently per provider/version after
   representative live-tenant tests and crash-after-submit reconciliation pass.

This model can expand to additional ATS families without weakening candidate
truth or allowing a fake listing to manufacture submission authority.

## Verification

Passed in this round:

- `cargo fmt --all -- --check`
- `cargo test -p bluey-server discovery_sources_support_only_canonical_five_ats_identifiers -- --nocapture`
- `cargo test db::jobs::tests:: --lib -- --nocapture` from `server/` (40 Jobs database and policy tests)
- `cargo clippy --all-targets -- -D warnings` from `server/`
- `npm test --workspace @bluey/jobs-workflows` (35 tests)
- `npm run typecheck --workspace @bluey/jobs-workflows`
- `npm test --workspace @bluey/jobs-automation` (133 tests)
- `npm run typecheck --workspace @bluey/jobs-portal`
- `npm test --workspace @bluey/jobs-portal` (45 tests)
- `npm run build --workspace @bluey/jobs-portal`
- `npm test` from `jobs/` (297 tests across automation, browser, runner, workflows, and portal)
- `npm run typecheck` from `jobs/`
- `git diff --check`

The new workflow tests cover each scheduled provider, canonical endpoint
construction, unsupported providers, and Workday source-key/config mismatch.
The database test covers all five canonical server records plus rejection of
unknown providers and malformed Workday tuples.

## Production gates still required

The code path is ready for configured source records, but production coverage
still requires operational evidence:

- provision and monitor real source identifiers for target employers;
- configure and run the durable scheduled discovery worker;
- certify representative Greenhouse, Lever, Workday, Ashby, and
  SmartRecruiters tenants independently;
- distribute the signed local Bluey Browser before enabling the local runner;
- run the isolated cloud browser pool and workflow worker before enabling Cloud;
- add contracted broad-market feeds for portals that do not expose suitable
  public ATS discovery; and
- retain Review-only behavior for unknown or drifted application versions.

No deployment or certification claim is made by this source-only round.

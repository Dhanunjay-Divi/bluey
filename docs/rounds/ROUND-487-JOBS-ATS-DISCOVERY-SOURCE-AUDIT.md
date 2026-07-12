# Round 487 - Jobs ATS And Discovery Source Audit

Date: 2026-07-11
Workstream: Discovery, matching, freshness, provider adapters, and submission
Source roots:

- `/Users/uno/Downloads/job and AI browser`
- `/Users/uno/Downloads/cue-bluey-jobs/_refs/jobs-research`

## Executive Verdict

Bluey's typed contracts, identity isolation, state machine, and receipt gate are
the correct foundation. The current branch is not yet a provider-certified
application engine:

- public ATS discovery is an exported library with no production caller;
- Workday, Greenhouse, Lever, Ashby, and SmartRecruiters share one generic
  native-input implementation;
- form planning and email-code safety helpers are not wired into runtime;
- freshness, track isolation, daily limits, company cooldowns, salary,
  sponsorship, and exclusions are not enforced at the queue boundary;
- a crash after submit click can retry the irreversible action.

The portal should continue to label these systems beta. It should not imply
live certification until provider-specific fixtures, sandbox tests, and
low-volume canaries pass.

## P0 Findings

### 1. Wire Discovery Into Production

`PublicAtsDiscoveryProvider` exists at
`jobs/automation/src/public-ats.ts:47-301` and is exported at
`jobs/automation/src/index.ts:6`. The Jobs server route map at
`server/src/api/jobs.rs:34-127` has no discover, refresh, cursor, or liveness
route. `POST /matches` accepts caller-supplied postings instead.

Required path:

```text
Career Track
  -> durable discovery request
  -> provider cursor and host throttle
  -> normalized global canonical job
  -> per-account/per-track match
  -> conservative remote liveness
  -> ranked Matches row
```

Required modules:

- New discovery workflow and activities in `jobs/workflows/src`.
- Provider cursor and liveness records in Postgres.
- Discovery API with progress and last-success state.
- OpenSearch indexing through an outbox.
- First-session onboarding must trigger this workflow rather than only navigate
  to Matches.

### 2. Replace The Generic Adapter Label With Real Provider Adapters

All named providers are definitions passed to one `StandardAtsAdapter` at
`jobs/automation/src/standard-adapters.ts:52-104`. The Playwright bridge at
`jobs/automation/src/playwright-page.ts:19-80` discovers only native `input`,
`textarea`, and `select` controls.

Missing surface coverage includes:

- iframes and embedded forms;
- shadow roots and rerendered controls;
- comboboxes, autocomplete, multiselect, contenteditable, date, and number;
- grouped radio and checkbox validation;
- provider login/account walls;
- conditional questions and review pages;
- post-upload filename and parser verification;
- explicit provider confirmation and duplicate-candidate outcomes.

Each certified adapter needs a separate implementation and version, even if
the adapters share lower-level field and evidence helpers.

### 3. Connect Form And OTP Safety Helpers

`planApplicationForm` exists at
`jobs/automation/src/form-intelligence.ts:90-142` and Answer Memory precedence
exists at `:144-174`. `planAuthenticationChallenge` exists at
`jobs/automation/src/challenge-handling.ts:55-126`.

Runtime execution at `jobs/automation/src/execute.ts:1-55` calls only the
standard adapter path. The helper contracts therefore do not currently protect
real browser execution.

Required integration:

- Adapter field discovery produces `ApplicationFormField` records.
- Planner resolves direct application answer, company memory, track memory,
  account memory, and verified profile facts in that order.
- Unknown required and sensitive fields create typed interventions.
- Email OTP evidence retains message reference and expiry through workflow
  state without persisting the raw code.
- Successful reuse increments Answer Memory use count and last-used time.

### 4. Make Track And Company Policy Server-Enforced

`JobPreferences` stores desired roles, locations, exclusions, sponsorship,
salary, daily limits, and `apply_once_per_company` at
`server/src/db/jobs.rs:206-254`. Queue eligibility at
`server/src/db/jobs.rs:1479-1548` does not enforce the full policy.

The current posting model has one `track_id` at
`server/src/db/jobs.rs:278-320`, while canonical uniqueness is account-wide.
Rediscovery through a second track can overwrite the posting's track instead
of preserving two independent matches.

Required data split:

```text
canonical_jobs
  provider + external ID + canonical URL + normalized job data

account_job_matches
  account + canonical job + Career Track + score + policy decision

applications
  one frozen track + identity + resume + answers + submission policy
```

Required queue checks:

- match belongs to the frozen track;
- daily limit has remaining capacity atomically;
- excluded company/title is blocked;
- salary, location, employment type, sponsorship, and authorization pass;
- same-company policy is explicit: block, ask, or allow another role;
- SDE and Data Engineering tracks cannot submit each other's resume;
- an existing advanced application is never silently replaced.

### 5. Use Conservative Freshness

Newly ingested records receive a current `last_verified_at_ms` without remote
verification at `server/src/db/jobs.rs:1382`. Queueing trusts that timestamp.
The discovery library, meanwhile, drops undated jobs. These semantics conflict.

Required statuses:

- `fresh`: provider membership or explicit live API response confirmed;
- `unknown`: bot block, timeout, 429, or transient 5xx;
- `closed`: provider membership removed, 404/410, or explicit closure;
- `stale`: verification age exceeds policy without affirmative closure.

Only `fresh` may enter unattended submission. `unknown` may remain visible but
must require review.

### 6. Harden Restricted-Source Policy

`jobs/automation/src/policy.ts:3-35` covers LinkedIn and Indeed but omits
ZipRecruiter. The server also uses URL substring checks at
`server/src/api/jobs.rs:2036` rather than parsed, exact host policy.

Required policy:

- exact hostname and registrable-domain matching;
- LinkedIn, Indeed, and ZipRecruiter are preparation and user-controlled
  handoff unless a documented provider contract explicitly allows automation;
- generic direct-employer fallback is review-first by default;
- semantic fallback never clicks submit without a high-confidence allowlisted
  form classification and an explicit account policy.

## Provider-Specific Reuse Matrix

| Provider | Best source references | Port or adapt | Bluey destination |
| --- | --- | --- | --- |
| Workday | `career-ops/providers/workday.mjs:118-315`, `career-ops/tests/providers/workday.test.mjs:89-310` | Port MIT URL construction, `Retry-After`, transient retry, partial pagination, location fallback, and early stop. | Split discovery provider plus `WorkdayAdapter`. |
| Greenhouse | `career-ops/providers/greenhouse.mjs:7-159`, `career-ops/web/src/lib/apply/greenhouse.ts:1-190` | Port MIT EU host, `first_published`, question schema, embedded form, and custom select knowledge. | `GreenhouseDiscoveryProvider` and `GreenhouseAdapter`. |
| Lever | `career-ops/providers/lever.mjs:8-154` | Port MIT EU discovery, location handling, and canonical apply URL. Use `ai-job-agent/scripts/lever-apply.js:56-185` only as fixture knowledge. | `LeverDiscoveryProvider` and `LeverAdapter`. |
| Ashby | `career-ops/providers/ashby.mjs:14-172`, `career-ops/web/src/lib/apply/session.ts:430-517` | Port MIT compensation and secondary-location parsing; adapt generic combobox handling. | `AshbyDiscoveryProvider` and `AshbyAdapter`. |
| SmartRecruiters | `career-ops/providers/smartrecruiters.mjs:50-226` | Port MIT public URL rewrite, complete location data, `status=PUBLIC`, and bounded pagination. | `SmartRecruitersDiscoveryProvider` and adapter. |
| Direct employer | `career-ops/web/src/lib/apply/session.ts:47-210`, `extract.ts:18-142`, `diagnose.ts:121-275` | Adapt frame selection, form-vs-search classification, upload verification, and read-back. | Constrained semantic adapter, review-first. |

## Additional High-Value Source Patterns

| Pattern | Source | Decision |
| --- | --- | --- |
| API-first liveness with unknown transient state | `career-ops/liveness-api.mjs:151-310`, `liveness-core.mjs:83-189` | Adapt. Do not call bot blocks or server errors closed jobs. |
| Canonical dedupe without merging sibling roles | `career-ops/dedup-tracker.mjs:175-338` | Adapt into global canonical job plus per-track match model. |
| Acronym-aware title, location, salary, and content filters | `career-ops/scan.mjs:73-390` | Port/adapt per track before daily limits. |
| Staged attempt taxonomy | `ApplyPilot/src/applypilot/discovery/workday.py:175-286` | Clean-room adapt only because ApplyPilot is AGPL. |
| Synthetic final text equals submitted | `AutoApply.../runtime/apply.py:68-117` | Reject. Confirmation must be provider-aware and evidence-backed. |
| LinkedIn cookie extraction and guessed defaults | `ai-job-agent/scripts/linkedin-easy-apply.js:48-187` | Reject. It conflicts with the isolated Bluey Browser and Answer Memory policy. |
| CAPTCHA token injection and stealth changes | ApplyPilot and legacy Selenium projects | Reject. Preserve browser for account-owner completion. |

## Required Fixture And Fault Tests

### Discovery

- Workday tenant URL, pagination, 429, 5xx, `Retry-After`, and partial result.
- Greenhouse US/EU, publication dates, embedded forms, and missing dates.
- Lever US/EU, canonical apply links, custom and multiple locations.
- Ashby compensation, secondary location, and closed posting.
- SmartRecruiters absolute API refs, public URL rewrite, and status filtering.
- Canonical URL drift, repost detection, sibling roles, and two tracks matching
  one canonical job.

### Forms

- iframe and nested iframe;
- React/select combobox and async autocomplete;
- hidden file input plus visible upload state;
- conditional question after previous answer;
- account wall, OTP, CAPTCHA, and assessment;
- legal consent and sensitive demographic question;
- review page with expected resume filename;
- validation error after submit;
- duplicate candidate notice;
- explicit confirmation versus ambiguous redirect.

### Failure Injection

- crash immediately after submit click;
- crash after upload but before provider acknowledgement;
- retry from `submitted_unknown` must probe and never click again;
- liveness 429/5xx must remain unknown;
- hostname spoof containing `indeed` or `linkedin` must not influence policy;
- ZipRecruiter must remain handoff-only;
- daily cap and same-company policy must be atomic under concurrent queues.

## Verification Reality

The current 51 automation tests, 4 runner tests, and 16 Rust Jobs database
tests pass. The five adapters are tested through one synthetic `FixturePage` in
`jobs/automation/tests/standard-adapters.test.ts:19-160`; those tests validate
the shared loop, not provider compatibility. The workflow package has no test
suite. Provider certification therefore remains outstanding.

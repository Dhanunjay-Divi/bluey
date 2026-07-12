# Bluey Jobs Open-Source Research

## Selection result

The reviewed repositories are useful as a pattern library, not as one product
to fork. Bluey keeps its own tenant model, user experience, billing,
idempotency, local/cloud runner contract, and operational boundaries.

The highest-value reusable capabilities were:

1. `career-ops`: mature public ATS discovery, defensive HTTP behavior,
   pagination, normalization, dedupe, trust checks, and broad provider tests.
2. `job-apply-plugin`: practical form-label aliases, common ATS questions,
   document fields, pre-submit confirmation, and sensitive-field confirmation
   patterns. Bluey must implement its own universal stop behavior for unknown
   required input.
3. `ai-job-agent`: two approval gates, answer-bank memory, status discipline,
   and exact application tracking.
4. `proficiently-claude-skills`: one folder and artifact set per job, visible
   resume changes, and consolidated human review.
5. AIHawk and ApplyPilot: wide profile/search configuration and independent,
   retryable stages for discovery, scoring, tailoring, export, and validation.

## What Bluey deliberately did not port

- Single-user YAML or JSON files as canonical production state.
- Selenium scripts coupled to one site's current CSS selectors.
- LinkedIn or Indeed background submission.
- Subprocess orchestration as a distributed workflow engine.
- Selectors, credentials, cookies, or personal sample profiles from a source
  repository.
- A generic resume reused across multiple jobs.
- Any behavior that silently invents a required answer when the user has not
  supplied or approved one.

## Bluey implementation outcome

- Five source-configured public ATS connectors normalize to one `NormalizedJob`;
  the scheduled server-leased runtime currently enables Greenhouse and Lever.
- Discovery requests are HTTPS-only, host-pinned, redirect-free, size-bounded,
  retry-bounded, and pagination-bounded.
- Form planning resolves confirmed profile facts and scoped answer memory in
  company, Career Track, then account order.
- Auto-submit rejects unconfirmed generated facts.
- Unknown required and sensitive questions create interventions; optional
  unknown fields are skipped.
- Every final run can produce a stable receipt with exact packet, documents,
  events, screenshots, confirmation, and fingerprint.

## July 11 source-audit correction

The earlier shorthand around `job-apply-plugin` was too broad. The source
requires confirmation before submission and special handling for sensitive
fields, but it does not provide a complete deterministic stop condition for
every unknown required field. Bluey owns that behavior in its typed
intervention model.

The phrase "fixture-backed" also means local synthetic/provider fixtures, not
live certification against every employer tenant. The shared executor is a
beta foundation until the live adapter certification gate passes.

## AI browser and research archive result

The 10 archives under `/Users/uno/Downloads/job and AI browser` were reviewed
as source and product references. They are most useful for source-ledger,
query-planning, citations, provider normalization, and replayable search
patterns. Bluey should build a separate `jobs/research` module from those
patterns rather than copying Perplexity-style clones or private endpoint
automation.

Required research invariants:

- Every source-backed claim has a source ID, content hash, retrieval time, and
  citation span.
- Research can improve ranking, matching, company context, and interview prep.
- Research cannot create unconfirmed profile facts or override user filters.
- Fetchers must block private networks, unsafe redirects, oversized responses,
  credentials, and cookie-bearing browsing by default.

## Adapter outcome

Greenhouse and Lever now use provider-specific, review-only state machines.
Workday, Ashby, and SmartRecruiters retain the shared fixture-backed beta form
executor. Local Bluey Browser and the cloud container consume the same adapter
contract. Live provider certification remains a release gate because employer
tenants can enable custom fields and authentication that no repository fixture
can fully represent. A semantic fallback remains constrained to direct employer
forms and does not override explicit submission policy.

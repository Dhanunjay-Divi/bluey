# Bluey Jobs Open-Source Research

## Selection result

The reviewed repositories are useful as a pattern library, not as one product
to fork. Bluey keeps its own tenant model, user experience, billing,
idempotency, local/cloud runner contract, and operational boundaries.

The highest-value reusable capabilities were:

1. `career-ops`: mature public ATS discovery, defensive HTTP behavior,
   pagination, normalization, dedupe, trust checks, and broad provider tests.
2. `job-apply-plugin`: practical form-label aliases, common ATS questions,
   document fields, and clear stop conditions for unknown required input.
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

- Five source-configured public ATS feeds normalize to one `NormalizedJob`.
- Discovery requests are HTTPS-only, host-pinned, redirect-free, size-bounded,
  retry-bounded, and pagination-bounded.
- Form planning resolves confirmed profile facts and scoped answer memory in
  company, Career Track, then account order.
- Auto-submit rejects unconfirmed generated facts.
- Unknown required and sensitive questions create interventions; optional
  unknown fields are skipped.
- Every final run can produce a stable receipt with exact packet, documents,
  events, screenshots, confirmation, and fingerprint.

## Next adapter gate

This round implements public discovery and the shared form-planning core. It
does not claim that production submission adapters are complete. Workday,
Greenhouse, Lever, Ashby, and SmartRecruiters browser adapters still need
fixture-backed prepare/fill/validate/submit implementations using the same
contracts for local and cloud runners. Stagehand remains a constrained fallback
only after no deterministic adapter matches.

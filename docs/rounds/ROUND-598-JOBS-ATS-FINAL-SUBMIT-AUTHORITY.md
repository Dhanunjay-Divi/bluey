# Round 598 - Jobs ATS Final-Submit Authority

**Date:** 2026-08-03

**Branch:** `feat/phase-jobs-full-autonomy-20260802`

**Status:** Implemented and verified; employer-facing distribution remains off

## Outcome

Bluey now separates form-filling coverage from authority to perform the final
employer-facing Submit action.

Greenhouse and Lever may reach final submission only through their exact,
versioned provider state machines. Workday, Ashby, SmartRecruiters and the
semantic fallback may fill and validate a form, but they must stop for review
or browser takeover before the final action. LinkedIn and Indeed remain handoff
surfaces.

## Authority Boundary

The shared capability registry owns, per ATS family:

- the customer-visible capability label;
- the implementation type and adapter version;
- whether original-source revalidation is required;
- whether explicit packet review is required; and
- whether final submission is allowed.

The execution wrapper checks this registry independently of adapter output. A
generic or malicious adapter that returns `submitted` without authority is
downgraded to `needs_input` and emits an authority-violation event.

The server independently derives capability from a parsed HTTP or HTTPS URL.
Exact provider hosts are required. Lookalike hosts, malformed URLs, private
schemes and caller-supplied capability labels cannot grant queue authority.

## Generic Adapter Behavior

Generic form automation may:

1. inspect the visible form;
2. fill known fields and upload the bound resume;
3. read back required answers and documents; and
4. advance through non-final `Next` steps.

It may not infer success from page text, click a final Submit button, or invent
confirmation evidence. Confirmation-like copy without a trusted provider
receipt becomes a review-only takeover.

## Verification

```text
Jobs automation tests:                    227 passed
Jobs automation strict TypeScript:         passed
Jobs automation production build:          passed
Server unit tests:                         829 passed
Server HTTP integration tests:              81 passed
Server runner-plan/schema tests:              2 passed
Server strict Clippy:                       passed
Rust formatting and compile checks:         passed
git diff --check:                            passed
```

The matrix covers exact provider hosts, spoofed hosts, misleading confirmation
text, generic final buttons, safe non-final navigation, malicious adapter
results and persisted server preferences used by queue-authority fixtures.

## Production Boundary

No production deployment, service restart or feature-flag change is part of
this round. Model generation, local Browser distribution, cloud Browser
distribution and mailbox sync remain disabled. Greenhouse and Lever still need
authorized tenant certification and fault testing before public unattended
submission can be enabled.

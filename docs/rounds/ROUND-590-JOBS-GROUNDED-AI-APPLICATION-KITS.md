# Round 590 — Grounded AI Application Kits

**Date:** 2026-08-02
**Branch:** `feat/phase-jobs-full-autonomy-20260802`
**Status:** implemented and verified in source; production capability remains off

## Result

Bluey Jobs can now construct a complete managed application kit without allowing a
model to invent candidate history. The existing tailored resume path now supports an
optional job-specific cover letter whose paragraphs cite exact Career Profile evidence.
The same truth checks used for resume rewrites reject unsupported protected facts,
metrics, employers, roles, locations, and stronger action claims before the kit can be
saved.

The server adds the target company and role itself, stores the exact cover letter in the
same transaction as the tailored resume, records its inclusion state in the packet
receipt, and sends that exact text to packet review. Review no longer shows a vague
cover-letter claim: it shows the final content or clearly says that no letter is included.

## Trust Boundary

The model may select and rewrite only evidence supplied in the bounded prompt. Every
cover-letter paragraph must cite one to six known evidence records. The server validates
the text, materializes server-owned framing, and persists the kit. Model output cannot
set legal answers, work authorization, salary, demographics, availability, target
company, target title, metering, eligibility, or submission state.

## Production Gate

`BLUEY_JOBS_MODEL_GENERATION_ENABLED` stays `0`. This round completes the code and test
foundation; it does not turn the feature on. A controlled corpus must still prove quality,
latency, spend reservation, provider failure behavior, and exact evidence coverage before
a limited rollout.

## Verification

- 247 Jobs-focused server unit tests passed.
- 16 Jobs HTTP/integration tests passed.
- Strict server Clippy passed.
- Rust formatting passed.
- 87 portal tests passed.
- Portal strict TypeScript and production build passed.
- Deployable Jobs assets were regenerated.
- `git diff --check` passed.

## Dependency Note

The production dependency audit reports two high React Router advisories limited to
React Server Components/server actions. Bluey Jobs is a client-only Vite SPA and does
not expose either affected mode. Pinning to an older router version reintroduced broader
client-side advisories, so the current version is retained with this applicability record.

## Next Gates

1. Discovery truth: freshness, repost deduplication, employer verification, and scam risk.
2. Durable runner ownership and side-effect-unknown reconciliation.
3. Immutable packet evidence objects with hash read-back and lifecycle deletion.
4. Provider acceptance and measured rollout before production enablement.

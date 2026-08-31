# IMPL: Phase 612B — Jobs Career Command Center

> **Codex preflight:** `$bluey-ops` was loaded. The current repository, local replacement handoff,
> and Round 612 were authoritative. The SSD archive was not used.

## Scope

Phase 612B records a second normal authenticated GiraffyReach interaction pass and establishes an
original, read-only Bluey Career Command Center. The implementation may compose already available
Jobs state for presentation and local navigation, but it cannot create a new write authority.

**Does:**

- documents the competitor's onboarding/readiness, editable streak, split-pane Jobs workflow,
  Title/Skill/Location/Company search modes, selected-job Apply Tools state, and contextual Resume
  Settings;
- records the profile tabs and advertised 5 MB PDF/DOCX import boundary, plus one explicitly
  synthetic upload/parse/audit test;
- records one bounded job-specific preparation flow that skipped missing-skill injection and
  created a `Saved`, not `Applied`, tracker item;
- records the LinkedIn five-step tour, manual ghostwriter, scheduler confirmation, and attribution
  toggle without connecting or publishing;
- records C2C Chat prompts/voice and its claimed ask-before-send boundary, plus Autopilot
  prerequisites, local-time schedule, displayed 25/hour and 50/day caps, targeting, dedupe claims,
  and sensitive Auto Reply fields/documents without sending a chat turn or enabling automation;
- records the current 17-tool Agent Connect documentation and its contradictory submit claims
  without generating a credential;
- records referral, roadmap, billing, and mobile bottom-navigation/desktop-only behavior;
- defines an original Bluey Command Center for readiness, today's focus, pipeline, source health,
  and recent evidence;
- implements that Command Center as a read-only composition of the existing `JobsWorkspace`, with
  basename-relative deep links and responsive desktop/mobile navigation;
- derives `ready`, `needs_action`, `reported`, and `optional` setup states without promoting
  profile resume strings or legacy Career Track fields into server authority, and keeps optional
  inbox correlation outside the required-readiness denominator;
- reuses one 12-hour effective discovery-health projection across Matches and Overview, and keeps
  Today's focus limited to fresh jobs on active Tracks that the latest candidate feedback has not
  passed, without making an eligibility claim;
- keeps the available `prepared`, in-flight, `submitted`, interview, offer, and
  `side_effect_unknown` projections semantically distinct, while leaving `eligible`, `approved`,
  and communication `sent` unavailable until the workspace carries their required receipts; and
- preserves Round 613 canonical taxonomy and Career Track enforcement as mandatory follow-up.

**Does NOT:**

- copy GiraffyReach source code, DOM, assets, copy, private API behavior, prompts, or feeds;
- grant Gmail, LinkedIn, or other OAuth access;
- accept a generated missing-skill claim, recruiter-audit rewrite, or final employer submission;
- approve, send, schedule, publish, bulk-apply, or mark the bounded tracker item as applied;
- upload an identity, immigration, work-authorization, or real candidate document;
- change a real candidate profile, resume policy, target, schedule, tracker status, or activity
  streak;
- generate, copy, rotate, or revoke an MCP/Agent Connect credential;
- send a referral, submit a roadmap request, enter checkout, pay, or change a subscription;
- add a provider write, server mutation, schema migration, worker, queue, secret, or production role;
- enable a feature flag, deploy, publish a release, or alter managed-cloud authority;
- resolve or bypass the Round 613 taxonomy/location gap; or
- access the SSD archive.

The authenticated audit did initiate two bounded mutation workflows in the signed-in GiraffyReach
account under explicit authority for a synthetic test: synthetic resume upload/parse and
job-specific resume preparation. Observed retained outcomes were one synthetic source resume, one
generated resume, one `Saved` tracker row, and one consumed visible resume credit. No private write
count was inspected. The Bluey implementation remains read-only and introduces no server or
provider write.

## Evidence Summary

| Area | Recorded evidence | Implementation boundary |
| --- | --- | --- |
| Command Center | Readiness cards, plan/counters, editable activity calendar, gaps/opportunities | Event-derived read-only status; corrections require receipts later |
| Jobs | Split pane; Title/Skill/Location/Company modes; `Selected / 0 jobs`; Gmail prerequisite | Local inspect/filter/navigation only; no bulk action |
| Profile | Sectioned candidate truth; PDF/DOCX import; one bounded synthetic parser/audit pass | Fixture retained; future Bluey imports become reviewed claim proposals |
| Preparation / tracker | Gap dialog, skip-injection path, generated resume, `Saved` row, separate mark-applied control | No employer submit, external send, or applied claim |
| LinkedIn | Five-step tour, ghostwriter, scheduler confirmation, attribution toggle | No grant, draft save, schedule, or publication |
| C2C | Chat prompts/voice; Gmail/resume/phone prerequisites; slots/caps/targets; Auto Reply fields/docs | No chat turn, mailbox grant, send, reply, or attachment |
| Agent Connect | 17 documented tools, connector model, conflicting submit statements | No MCP endpoint or credential in this batch |
| Referral / roadmap | Product entry points | No referral or request submission |
| Billing | Trial/plan/entitlement presentation | No checkout or entitlement change |
| Mobile | Compact bottom navigation; desktop-only limits on complex screens | Preserve read/review/blocker visibility on narrow screens |

Claims that were not executed remain labeled `Claimed`; inconsistent first-party statements remain
`Contradicted`; unproven implementation details remain `Unknown`. See
`docs/rounds/ROUND-612B-JOBS-GIRAFFYREACH-SECOND-PASS-AND-CAREER-COMMAND-CENTER.md` for the complete
evidence record.

## Bluey Product Boundary

The Career Command Center is a composition layer over current Bluey truth. It may show:

- readiness with evidence age, exact blocker, and an explicit distinction between authoritative,
  reported-only, actionable, and optional state;
- review-ready matches and changed/stale-source warnings;
- pipeline counts that do not collapse preparation, communication, and submission;
- source class, freshness, and original-source verification state;
- interventions and `side_effect_unknown` events; and
- safe navigation to already authorized local views.

It may not infer readiness from a colorful tile, treat client counts as authority, or turn a local
selection into approval. A filter changes presentation only. A navigation control changes route
only. Unavailable actions stay visibly unavailable.

The interface uses Bluey's own visual language and component system. High-level conventions such
as cards, tabs, split panes, mode selectors, guided steps, confirmation dialogs, and bottom
navigation are reimplemented independently. Bluey adds provenance, raw/canonical distinctions,
eligibility reasons, claim diffs, consent scope, evidence age, and receipt semantics.

## Security And Privacy Boundary

- Do not record competitor or user credentials, tokens, OAuth codes, connector URLs, account
  identifiers, profile values, or resume content.
- Do not expose last-four SSN, date of birth, immigration status, or identity-document data in a
  generic readiness surface.
- Do not model Gmail or LinkedIn as a Boolean `connected` value; a future integration state must
  include provider, account scope, permissions, expiry, revocation, and health.
- Do not represent a user by sending email, posting publicly, submitting an application, or
  changing an external account without exact action authority and a durable result.
- Do not retry an ambiguous provider write as though it were a harmless read.

## Phase 613 Dependency

This batch does not import or recreate the stale Round 574 branch wholesale. Round 613 remains
responsible for one server-owned role/skill/location library, token-safe skills, typed geography,
Career Track create/update/import normalization, exact `track.locations` enforcement, migration,
policy versioning, and exhaustive boundary tests.

The Phase 612B UI cannot be cited as evidence that matching, preparation, approval, queueing, or
submission has the required canonical policy. Production flags remain unchanged.

## Implementation Files

| File | Action | Purpose |
| --- | --- | --- |
| `docs/rounds/ROUND-612B-JOBS-GIRAFFYREACH-SECOND-PASS-AND-CAREER-COMMAND-CENTER.md` | Created | Second-pass evidence, clean-room UX architecture, read-only boundary, and acceptance contract |
| `docs/work/IMPL-PHASE-612B-JOBS-CAREER-COMMAND-CENTER.md` | Created | Phase 612B scope, safety limits, verification, and handoff |
| `CHANGELOG.md` | Updated | Records the original read-only Command Center and bounded audit evidence |
| `jobs/portal/src/App.tsx` | Updated | Adds the lazy Overview route and makes it the basename-relative home/fallback |
| `jobs/portal/src/App.test.ts` | Updated | Covers the canonical home and desktop/mobile navigation contract |
| `jobs/portal/src/components/AppShell.tsx` | Updated | Adds Overview to desktop and mobile while keeping five mobile tabs |
| `jobs/portal/src/types.ts` | Updated | Aligns optional posting verification time with the server's nullable wire value |
| `jobs/portal/src/lib/discovery-source.ts` | Created | Centralizes the effective 12-hour discovery-health and remediation projection |
| `jobs/portal/src/lib/command-center.ts` | Created | Derives readiness, matches, interventions, health, outcomes, integrations, and next actions |
| `jobs/portal/src/lib/command-center.test.ts` | Created | Covers derivation, fail-closed truth, state ordering, deduplication, and routing helpers |
| `jobs/portal/src/views/MatchesView.tsx` | Updated | Reuses the shared discovery-health projection without changing write authority |
| `jobs/portal/src/views/MatchesView.test.tsx` | Updated | Tests the extracted discovery-health boundary directly |
| `jobs/portal/src/views/CareerCommandCenterView.tsx` | Created | Renders the read-only daily operating view from server workspace state |
| `jobs/portal/src/views/CareerCommandCenterView.test.tsx` | Created | Covers truth labels, empty states, plans/runners, preview links, and responsive CSS contract |
| `jobs/portal/src/views/CareerCommandCenterView.css` | Created | Provides original responsive Command Center styling |
| `web/jobs/index.html` | Regenerated | Points the packaged Jobs portal at the current immutable build artifacts |
| `web/jobs/assets/**` | Regenerated | Includes the lazy Command Center CSS/JavaScript and the portal's immutable hashed artifacts |

## Verification

```text
Authenticated-observation label recheck      passed for documentation unit
No-secret / no-personal-data scan            passed for documentation unit
Markdown structure and whitespace check      passed for documentation unit
Phase 613 dependency recheck                 passed for documentation unit
Jobs workspace typecheck                     passed across all five packages
Jobs workspace tests                         passed: 139 files / 1,776 passed / 1 skipped
Portal production build                      passed: 2,296 modules transformed
Generated Command Center asset check         passed for final authority and empty-state copy
Desktop local-preview visual QA              passed at 1,280 x 720 CSS pixels
Phone local-preview visual/DOM QA            passed at 390 x 844; five tabs and Jobs settings reachable
Responsive contract tests                    passed for 1,040 px and 640 px breakpoints
Light-theme normal-copy contrast check       passed at WCAG AA 4.5:1 minimum
Bluey operating-doc preflight                passed
git diff --check                             passed
```

The production build emitted its existing advisory that some chunks exceed 500 kB; it did not fail
the build. Final verification performed no deploy, Bluey provider write, production read, external
account mutation, or feature activation. The earlier bounded competitor mutation workflows are
recorded separately above and in the Round 612B evidence document.

## Review Checklist

- [x] Observed, claimed, contradicted, and unknown behavior is distinguished.
- [x] Exact Jobs modes, LinkedIn tour steps, and MCP tool names are recorded.
- [x] No user-specific profile value, document content, credential, or token is present.
- [x] OAuth, upload, send, submission, publication, payment, and credential boundaries are explicit.
- [x] The UI is specified as an original Bluey implementation, not a competitor mirror.
- [x] Read-only scope cannot create provider, server, queue, billing, or production authority.
- [x] Resume and legacy Track references remain reported-only until authoritative read-back and
      canonical policy revisions exist; optional inbox state is excluded from required readiness.
- [x] Stale/never-synced source health and latest pass/restore state cannot surface optimistically.
- [x] Phase 613 remains mandatory.
- [x] The SSD archive was not accessed.
- [x] Final implementation diff contains no unrelated user changes.
- [x] Relevant tests/build and final no-secret checks pass.

## Handoff

The next implementation/review agent should:

1. treat the current repository and local handoff as authority;
2. keep Phase 612B read-only and make every unavailable/write action explicit;
3. verify the final UI against the acceptance criteria in the Round 612B document;
4. run relevant portal tests/build plus repository preflight checks;
5. leave deploy, flags, providers, OAuth, billing, credentials, and production untouched; and
6. begin Round 613 only as a separate reviewed batch from this branch's reviewed successor.

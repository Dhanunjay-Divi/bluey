# Round 582 - Jobs Match Filter Production Audit

Date: 2026-07-30

Status: implemented and verified on feature branch; not deployed

## Goal

Audit Bluey Jobs matching controls as a human user, fix every confirmed filter
defect, and prove that visible narrowing controls do not weaken the
server-authoritative rules governing preparation, approval, queueing,
metering, or submission.

## User Contract

Bluey has two intentionally different kinds of filters.

### Career Track Policy

Career Track settings decide whether a job is eligible:

| Policy | Authority |
| --- | --- |
| Role family and title exclusions | Server |
| Selected locations and workplace | Server |
| Employment and engagement type | Server |
| Compensation floor | Server |
| Experience and seniority | Server |
| Work authorization and sponsorship | Server |
| Company collision and duplicate application | Server |
| Posting freshness and live availability | Server |
| Daily application limit | Server |
| ATS capability and runner availability | Server |
| Required facts and packet evidence | Server |

These checks run during matching and are re-evaluated before preparation,
approval, queueing, and irreversible submission. A browser control cannot turn
an ineligible job into an eligible one.

### Matches View Controls

Matches controls only narrow what the user sees:

| Control | Behavior |
| --- | --- |
| Search | Case-insensitive company, role, location, and workplace search |
| Career Track | Shows one active track or all active tracks |
| Minimum fit | Includes jobs at or above 0-100% |
| Workplace | Exact normalized Remote, Hybrid, or On-site category |
| Not prepared | Hides jobs that already have an application packet |
| Outside my rules | Shows server-excluded jobs for inspection only |
| Passed | Shows jobs the candidate previously passed |
| Density | Comfortable or compact row layout |

Every control is encoded in the URL. Refresh, direct links, and browser
back/forward restore the same view. Matches-only state is removed when linking
to Settings so unrelated routes do not inherit stale filters.

## Confirmed Defects And Fixes

### Filters disappeared after refresh

All view state previously lived in `useState`. Round 582 adds a typed
`URLSearchParams` contract with validation and canonical serialization.

### Inactive Career Tracks leaked into the board

The prior board used every workspace track and every match. Round 582 builds
the selector, counts, passed state, and match pool from active tracks only.

### Workplace used substring matching

Ad hoc matching could produce false categories. Round 582 normalizes common
labels such as `Remote - US`, `Hybrid / 3 days`, `In person`, `Onsite`, and
`Office`, then compares exact categories.

### 100% fit could not be selected

The score range ended at 95. It now supports the complete 0-100 range and
clamps malformed URL input to a five-point step.

### Filtered-empty looked like no discovery

The previous empty view told users to add a job link even when hundreds of
jobs existed but were narrowed away. The board now distinguishes:

- no jobs match the current view filters;
- no passed jobs in this view;
- no jobs in the selected Career Track;
- jobs exist only outside Career Track rules;
- no discovered jobs exist for the account.

### Navigation leaked filter parameters

Plan and Career Track links used the complete Matches query string. They now
carry preview/scenario state only.

## Human Browser QA

Browser QA used the real Vite development build at desktop and mobile
viewports.

| Scenario | Result |
| --- | --- |
| Whitespace-only query | All rows remain visible |
| Remote workplace | Only remote rows |
| Hybrid plus 90% minimum | Correct combined result |
| Invalid track ID | Safe fallback to all active tracks |
| Refresh with combined filters | URL, rows, count, and mode preserved |
| Navigate to Settings and Back | Matches filters preserved |
| Passed-job direct link | Correct passed-only empty state |
| No result from combined filters | Clear-filters recovery shown |
| 125 matching jobs | Stable 50/100/125 pagination |
| Compact density | Stable layout |
| Desktop dark theme | No overflow or overlap |
| Desktop light theme | No overflow or overlap |
| Mobile 390 x 844 | No horizontal overflow |
| Mobile filter dialog | Fits viewport and remains operable |
| Browser console | No warnings or errors |

## Automated Verification

### Jobs TypeScript

- 479 tests passed:
  - automation: 188;
  - Browser: 100;
  - runner: 50;
  - workflows: 54;
  - portal: 87.
- All Jobs TypeScript packages passed type checking.
- All Jobs packages and the production portal built successfully.
- Ten new focused match-filter tests passed.

### Server Authority

- 781 server unit tests passed.
- 76 HTTP integration tests passed.
- Runner entitlement and PostgreSQL compatibility tests passed.
- Strict Rust formatting and Clippy passed.

The suite covers hard filters, stale jobs, live verification, restricted and
unknown sites, company collision, engagement type, sponsorship, experience,
packet integrity, explicit approval, one-time metering, cross-tenant IDs,
runner entitlements, signed worker operations, and side-effect-unknown
reconciliation.

### Security And Repository Gates

The following passed:

- privacy gate;
- Jobs schema parity;
- provenance/license gate;
- CI guard self-test;
- browser/server boundary;
- edge policy;
- `git diff --check`.

## Production Readiness Verdict

### Ready

- Career Track hard-filter enforcement.
- Matches view filtering and recovery.
- Review-first packet preparation and explicit approval boundary.
- Server-authoritative queueing and one-time metering.
- Tenant isolation and stale/closed-job rejection.
- Responsive light/dark portal behavior.

### Not Yet A Full Autonomous Public Launch

- R2 object-storage and backup replication still return `AccessDenied`.
- Model generation remains disabled.
- Local Browser distribution remains disabled.
- Cloud Browser distribution remains disabled.
- Universal unattended submission is not certified.

The first group is production-ready for reviewed merge. The second group must
not be described as complete or enabled because this round did not change those
systems or their flags.

## Protected Flags

Round 582 does not change:

```text
BLUEY_JOBS_MODEL_GENERATION_ENABLED=0
BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED=0
BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED=0
```

## Result

Bluey Jobs filters now behave predictably for a real user and remain aligned
with the server's stricter eligibility authority. No filter is cosmetic, no
view control bypasses a hard rule, and every narrowing state has a clear
recovery path.

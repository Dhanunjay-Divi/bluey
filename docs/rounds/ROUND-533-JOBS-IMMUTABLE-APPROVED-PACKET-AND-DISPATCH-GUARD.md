# Round 533 - Jobs Immutable Approved Packet and Dispatch Guard

Date: 2026-07-16

Status: implemented and verified locally; not deployed in this round.

## Scope

This round closes a submission-integrity gap in Bluey Jobs. Once a user approves
an application packet, the resume, answers, application identity, cover letter,
job details, and verified claims used by a browser runner must remain the exact
content the user reviewed.

The implementation covers Review-first approval, eligible Auto-submit packets,
local runners, cloud runners, and final receipt persistence.

## Approved Execution Snapshot

Approval now stores a versioned `approved_execution` snapshot in the application
receipt. It contains:

- application, job, and resume-version identifiers;
- the complete resume and cover-letter content;
- final application answers;
- verified claim identifiers;
- application identity, email, and isolated browser-profile identifier; and
- the employer, role, location, workplace, description, source, compensation,
  and canonical application URL.

The server canonicalizes this snapshot and signs its content with a SHA-256
checksum. Repeated approval is idempotent: Bluey validates and reuses the
existing snapshot instead of silently replacing it.

## Dispatch Boundary

`POST /api/jobs/applications/:id/runs` now:

1. requires a queued application with a frozen approved snapshot;
2. confirms the current application, job, resume, and identity still match the
   approved snapshot;
3. sends the frozen packet rather than rebuilding answers from mutable profile
   state;
4. validates cloud-runner availability before reserving work; and
5. waits for cloud workflow acceptance before creating local browser/run state.

The approved packet checksum is carried through the shared TypeScript runner
contract. Local and cloud runners therefore execute the same frozen packet.

## Receipt Boundary

Final receipt validation now rejects a result when:

- the approval snapshot is missing, malformed, or has a checksum mismatch;
- the job, resume, application identity, or application email differs from the
  approved packet;
- the runner reports a different approved-packet checksum; or
- the final answers differ from the exact approved answers.

Existing document and screenshot evidence checks remain in place. The result is
an auditable chain from user approval to runner execution to persisted receipt.

## Files

- `server/src/api/jobs.rs`
- `jobs/automation/src/contracts.ts`
- `jobs/automation/src/packet-guards.ts`
- `jobs/automation/src/receipts.ts`
- associated Jobs automation fixtures and tests

No native overlay, meeting runtime, audio, or existing Bluey Browser UI was
changed.

## Verification

| Check | Result |
|---|---:|
| Rust Jobs API tests | 8 passed |
| Full server library tests | 434 passed |
| Jobs automation test files | 16 passed |
| Jobs automation tests | 130 passed |
| Jobs automation typecheck | passed |
| Server Clippy with warnings denied | passed |
| Rust formatting | passed |
| `git diff --check` | passed |

The receipt suite includes explicit tamper tests for changed answers and an
incorrect approval checksum.

## Remaining Production Work

This round prevents packet drift; it does not claim that unattended application
automation is launch-certified. Remaining P0 work includes durable dispatch
acknowledgement/reconciliation, provider-specific ATS certification, scheduled
sanctioned discovery with source-health monitoring, and production browser
operations.

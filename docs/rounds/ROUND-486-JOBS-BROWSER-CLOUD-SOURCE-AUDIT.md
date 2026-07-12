# Round 486 - Jobs Browser And Cloud Source Audit

Date: 2026-07-11
Workstream: Local browser, cloud browser, intervention, recovery, and evidence
Source roots:

- `/Users/uno/Downloads/job and AI browser`
- `/Users/uno/Downloads/cue-bluey-jobs/_refs/jobs-research`

## Executive Verdict

Bluey has the best core browser architecture in the reviewed set: one typed
automation contract is shared by Electron and the cloud Playwright runner,
browser profiles are scoped by application identity, Temporal owns durable
workflow state, and receipts have a stable schema.

That foundation is not yet enough for unattended production submission. The
irreversible submit boundary is not crash-safe, active cloud browsers are held
only in process memory, takeover URLs do not connect to a real streaming
gateway, email-code policy is not wired into execution, and cloud profile
snapshots do not yet have a versioned object-store lifecycle.

## What Bluey Already Does Well

| Capability | Bluey reference | Assessment |
| --- | --- | --- |
| Shared execution contract | `jobs/automation/src/contracts.ts:35-157`, `jobs/automation/src/execute.ts:11-54` | Correct boundary. Local and cloud should continue to use one adapter package. |
| Local profile isolation | `jobs/browser/src/profile.ts:4-32` | Correctly scopes a persistent browser profile by Bluey account and application identity. |
| Cloud profile isolation | `jobs/runner/src/profile-store.ts:18-83` | Encrypts profile snapshots and separates identities, but needs a production key and version lifecycle. |
| Durable orchestration | `jobs/workflows/src/workflows.ts:10-61` | Temporal is a stronger base than the subprocess and in-memory queues in most reference projects. |
| Intervention model | `jobs/automation/src/contracts.ts:98-121`, `server/src/api/jobs.rs:885-1097` | Good typed model and account-scoped persistence. Runtime wiring remains incomplete. |
| Receipt model | `jobs/automation/src/receipts.ts:19-189` | Stronger than simple status rows because it can preserve exact documents, events, answers, and evidence. |
| Local launch capability | `jobs/browser/src/protocol.ts:1-22`, `server/src/api/jobs.rs:1429-1595` | Good short-lived capability design; no reusable Bluey token enters the custom URL. |
| Network guard | `jobs/automation/src/network.ts:4-117` | Useful application-level SSRF guard. It must be paired with container egress policy. |

## P0 Gaps Proven By The Code

### 1. Make Submission Crash-Safe

`StandardAtsAdapter.submit` clicks an employer submit button at
`jobs/automation/src/standard-adapters.ts:288`, while the cloud runner records
the idempotent result only after execution returns at
`jobs/runner/src/server.ts:64-71`.

If the process dies after the click and before result persistence, Temporal can
retry the whole browser activity and submit the same application twice.

Required state machine:

```text
prepared
  -> submit_started
  -> submitted_unknown | submitted_confirmed | submit_rejected
```

Rules:

- Persist `submit_started` before clicking.
- Never automatically click again from `submitted_unknown`.
- Probe the preserved browser, provider status page, application email, and
  canonical confirmation URL before resolving an unknown outcome.
- Bind the ledger key to account, canonical job, application identity, and
  resume version.

Target files:

- New `jobs/automation/src/submission-ledger.ts`
- `jobs/automation/src/standard-adapters.ts`
- `jobs/runner/src/result-store.ts`
- `jobs/workflows/src/workflows.ts`
- `server/src/db/jobs.rs`

### 2. Replace Process-Local Browser Ownership

Active cloud sessions and identity locks are Maps in
`jobs/runner/src/server.ts:48-49`. `allocateBrowser` currently creates a
deterministic identifier rather than leasing real capacity at
`jobs/workflows/src/activities.ts:26-35`.

Required production model:

- Redis or Valkey lease keyed by region plus application identity.
- Runner heartbeat, hard TTL, queue-age metric, and fencing token.
- One owner may mutate a browser profile at a time.
- Lease loss freezes submission and produces a recoverable intervention.
- Temporal workflow stores the lease ID and fencing token, not a process-local
  browser pointer.

### 3. Build A Real Takeover Gateway

The runner currently constructs a takeover URL at
`jobs/runner/src/server.ts:208`, but no authenticated stream or session broker
backs it. The Electron protocol also recognizes `takeover` and returns without
opening a concrete session at `jobs/browser/src/main.ts:346-355`.

Required components:

- `jobs/runner/src/takeover-gateway.ts`
- Short-lived account-scoped takeover grants.
- Bounded video/frame replay with input backpressure.
- Strict origin validation and one active controller.
- Audit events for open, control acquired, control released, and resume.
- Browser remains alive during CAPTCHA, app/phone 2FA, assessments, and unknown
  required questions.

Useful MIT reference patterns:

- `jobpilot/apps/terminal/Realtime/TerminalHub.cs:12-105`
- `jobpilot/apps/terminal/Hosting/OriginPolicy.cs:3-54`
- `career-ops/web/src/lib/apply/session.ts:519-537`

These patterns should be adapted to Bluey's Temporal and tenant model rather
than imported as another terminal service.

### 4. Version And Encrypt Cloud Profiles Properly

`jobs/runner/src/profile-store.ts:36-60` replaces the active profile directory
and writes one encrypted snapshot. A failed restore or interrupted write can
destroy the only usable state.

Required lifecycle:

- Per-account or per-identity data-encryption key.
- KMS-wrapped key with account and profile AAD.
- Versioned R2/S3 object keys.
- Write temporary object, verify hash, then atomically promote manifest.
- Keep the last known-good version until the next version is verified.
- Delete all versions when the application identity or account is deleted.

Useful MIT reference:

- `jobpilot/apps/api/src/common/crypto/crypto.service.ts:8-86`

Local profile secrets should use Electron `safeStorage` or the OS keychain.
Useful MIT reference:

- `job-hunter-team/desktop/auth/keyring-storage.js:112-169`

### 5. Wire Email Verification Into The Workflow

Strict OTP selection and approval logic exists in
`jobs/automation/src/challenge-handling.ts:55-126`, but the browser, runner, and
workflow do not call it. The policy helper is not a feature until it is connected
to mailbox evidence and browser resume.

Required flow:

1. Browser detects an email-code challenge using provider-aware diagnostics.
2. Workflow records expected employer, recipient identity, start time, and
   expiry.
3. Gmail/Outlook worker searches only the connected mailbox and matching
   recipient.
4. UI shows sender, masked recipient, age, and provider context.
5. User explicitly approves one code.
6. Raw code is sent directly to the active browser and is never persisted in
   logs, receipts, Answer Memory, or analytics.

### 6. Improve Page Diagnostics And Evidence

Current challenge detection is body-text regex at
`jobs/automation/src/standard-adapters.ts:112-148`. Current receipt evidence is
mostly one final screenshot at `jobs/runner/src/server.ts:205-224`.

Port or adapt these MIT patterns:

| Source | Useful behavior | Bluey destination |
| --- | --- | --- |
| `career-ops/web/src/lib/apply/extract.ts:18-142` | Frame-aware field and ATS metadata extraction | `jobs/automation/src/playwright-page.ts` |
| `career-ops/web/src/lib/apply/diagnose.ts:121-275` | Two-signal challenge detection and read-back validation | New `jobs/automation/src/page-diagnostics.ts` |
| `career-ops/web/src/lib/apply/session.ts:66-106` | Multi-frame application surface discovery | `jobs/automation/src/playwright-page.ts` |
| `career-ops/web/src/lib/apply/session.ts:176-210` | Step evidence and screenshot timing | `jobs/automation/src/receipts.ts` |
| `AutoApply.../backend/app/core/automation/runtime/observe.py:172-223` | Execution trajectory and step metrics | Clean-room implementation in `jobs/automation/src/execution-events.ts` |

Evidence should distinguish:

- local resume materialized;
- upload control accepted the file;
- employer UI displayed the expected filename;
- provider completed any asynchronous parsing;
- final review page still showed the expected attachment;
- submission confirmation was explicit rather than inferred.

## Source Disposition

| Source | License observed | Decision | Reason |
| --- | --- | --- | --- |
| Career Ops browser apply modules | MIT | Port and adapt | Best frame, diagnostics, liveness, and per-step evidence patterns. |
| JobPilot crypto, origin, and streaming modules | MIT | Adapt | Strong per-user encryption and bounded terminal-stream ideas. |
| Job Hunter keyring and redacting logger | MIT | Adapt | Useful local secret storage and structured log hygiene. |
| AutoApply runtime observation | No top-level license | Clean-room reimplement | Useful metrics taxonomy, but provenance is insufficient for direct copying. |
| Abbey worker/browser ideas | Commercial-use restriction in archive | Concept only | Generic worker and screenshot bounds are useful; source is not a clean direct-port candidate. |
| ApplyPilot Chrome profile cloning | AGPL-3.0 | Reject | Copies another browser profile/cookies and conflicts with Bluey identity isolation. |
| AIHawk/Selenium browser scripts | AGPL or source-available | Reference only | Hard-coded, single-user, and tied to changing site selectors. |
| Browser extensions reading everyday Chrome state | Mixed | Reject | Bluey intentionally uses a separate Jobs browser and profile. |
| Vane and Spy Search browser helpers | MIT | Reference only | Search/crawl helpers, not durable application-browser ownership. |

## Required Acceptance Tests

1. Kill the runner immediately after submit click; retry must not click again.
2. Lose the Redis lease during fill and during submit; only the fenced owner may
   continue.
3. Corrupt the newest profile snapshot; restore the prior verified version.
4. Run two emails under one Bluey account; cookies and employer sessions must
   remain isolated.
5. Restart a runner while an intervention is open; takeover must still reach
   the preserved browser.
6. Resolve email OTP from the wrong recipient, sender, or time window; Bluey
   must reject it.
7. Encounter CAPTCHA inside an iframe; body-text-only detection must not be the
   sole signal.
8. Upload the wrong file or receive an ATS parser error; the receipt must not
   claim the resume was submitted.
9. Attempt subresource access to loopback, link-local, metadata, and private
   network addresses; container egress and application guards must both block.
10. Scale two runner replicas against one identity; exactly one lease owner may
    launch Chromium.

## Product Consequence

The portal may continue to describe local and cloud runners as beta. It should
not describe unattended ATS submission as production-ready until the submit
ledger, distributed leases, takeover gateway, versioned profile store, wired
OTP flow, and provider-specific evidence tests pass.

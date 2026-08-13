# Round 608 - Jobs Cloud-First Web Automation

**Date:** 2026-08-13

**Branch:** `feat/phase-608-jobs-cloud-first-web`

**Status:** SOURCE COMPLETE AND LOCALLY VERIFIED - independently accepted; no flag, deployment,
provider, tenant, or production authority is claimed

## Objective

Make the browser-delivered Bluey Jobs portal and managed cloud runner the launch product:
customers use Bluey Jobs at `/jobs/automation`, approve or authorize an eligible Application Kit,
and Bluey queues the work to its managed Background runner. A customer does not install a separate
Bluey Browser application.

This is a portal and product-boundary change. It does not make a disabled cloud runner available,
certify an ATS, enable unattended submission, or alter the server's retained local-run authority.

## Product Decision

The launch path is:

```text
bluey.sh/jobs
  -> prepare or review an Application Kit
  -> Automation
  -> managed Background runner
  -> intervention or takeover in the web portal when required
  -> exact final-submit authority
  -> receipt or ambiguity-safe reconciliation
```

The customer-facing portal has one execution choice: the managed cloud Background runner. It does
not show local installation, architecture selection, native download, protocol launch, local queue,
or local-run pricing prompts.

The canonical route is `/jobs/automation`. Existing `/jobs/browser` links redirect to that route
and preserve only the portal's bounded preview state (`preview=1` and its optional scenario). The
compatibility redirect does not preserve arbitrary query parameters or restore local-run UI.

## Preserved Runtime Safety

Cloud-first does not mean browser state disappears. The managed runner still uses an isolated
browser runtime, and the portal must continue to show:

- an active managed browser session;
- current run status and progress;
- exact intervention detail;
- scoped takeover when available;
- email-code approval when separately authorized;
- the final form-review checkpoint; and
- the explicit final-submit approval required by the current application policy.

The active-run browser preview and cloud queue visuals remain product UI. Only installation and
local-run presentation becomes dead code.

The portal does not expose a generic pause/resume control in this batch. The prior control only
changed a session row and did not signal the Temporal workflow, so it could misrepresent a live
run. Pause/resume returns only with durable workflow-command authority and delivery proof.

## Availability Truth

Cloud execution remains server-authoritative. The Background runner is queueable only when its
plan entitlement, cloud-distribution gate, workflow credential, application eligibility, and all
other server authorities agree. When unavailable, the portal explains the bounded reason and
returns the customer to Applications or Review; it does not offer an installer as a fallback.

The portal must not combine retained `can_queue_local` server state with customer launch
availability. Auto-submit presentation, Matches, Applications, and ATS certification summaries use
only current cloud certification plus current cloud runner availability. A target certified only
for a parked local runner remains Review-only in the web product.

The existing server/local response types, local-run endpoints, release authority, recovery state,
and disabled local-distribution flag remain intact. Removing portal exposure is not permission to
delete unresolved local evidence or strand a possible employer-facing side effect.

## Phase 607 Boundary

Phase 607 local Bluey Browser update, rollback, and recovery work remains parked and unmerged from
the launch branch. Its source and evidence may be retained for a later P2 product if demonstrated
customer demand justifies an installable runner. Round 608 does not merge that work, publish its
artifacts, enable its flag, or treat it as a launch dependency.

`BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED` remains `0`.

## Acceptance Criteria

1. `/jobs/automation` is the canonical portal route and navigation label.
2. `/jobs/browser` redirects to `/jobs/automation` while preserving only a valid `preview` query.
3. The portal exposes one customer queue action: the cloud Background runner.
4. No install, native-download, processor-choice, protocol-open, setup, local-queue, or local-plan
   prompt remains in reachable customer UI.
5. Active managed browser session status, intervention, takeover, email-code approval, and
   final-review controls remain functional and truthful. No presentation-only pause/resume control
   is exposed.
6. Cloud access remains derived from the server response. An unavailable channel cannot be queued
   by changing client state, and retained local authority cannot satisfy web Auto-submit.
7. Unavailable, upgrade-required, and disabled cloud states route to Applications or Review with
   bounded copy and never imply installation will unlock execution.
8. Empty queue, in-flight queue, queue failure, and intervention-resolution states remain
   accessible and recoverable.
9. Desktop, mobile, light, and dark layouts have no dead installation controls, overflow, or
   regression in active-run and cloud-queue visuals.
10. ATS summaries distinguish reviewed beta automation from active cloud certification;
    local-only or stale authority appears Review-only and cannot grant unattended submission.
11. Runtime decoders and server/local authority remain unchanged unless a separate reviewed batch
    requires a contract change.
12. Local Browser distribution, cloud Browser distribution, model generation, mailbox, and
    communication flags are not changed by this batch.
13. Focused portal tests, complete portal tests, strict TypeScript, production build, generated
    bundle checks, privacy/CI gates, and diff hygiene pass before review can accept the batch.

## Verification Plan

- Route tests for canonical navigation and the bounded legacy redirect.
- Browser/Automation view tests proving the sole cloud queue path and absence of install/local UI.
- Regression tests for active session, intervention, takeover, email OTP, and final approval.
- Runner-access tests for available, disabled, upgrade-required, and unavailable cloud states.
- Portal typecheck, complete test suite, and production bundle rebuild.
- Search-based dead-code and CSS-selector audit after the source change settles.
- Responsive visual inspection in desktop/mobile light and dark modes.
- Full repository privacy, generated-output, and whitespace gates required by the final handoff.

## Next P0 Source Batches

The portal decision does not redefine broader completion. The next independent P0 work is:

1. durable original-source employer identity and job-risk verification so continuous discovery can
   become exact cloud queue authority rather than remaining Review-first;
2. a transactional workflow command outbox so queue intent and workflow start/resume delivery
   cannot diverge across process death or transport ambiguity; and
3. resume truth and provenance hardening so extraction, corrections, generated documents, and
   receipts remain bound to exact source evidence and source-layout fidelity.

Reviewed mailbox-to-reply/calendar intelligence and a unified immutable production
verification/deployment authority remain subsequent launch work.

## External Production Boundary

Round 608 cannot locally prove or authorize:

- Temporal, managed Chromium, PostgreSQL, Valkey, R2/KMS, network, takeover, or regional capacity;
- approved cloud-runner images and immutable artifact promotion;
- authorized Greenhouse, Lever, or other ATS test tenants and certification canaries;
- licensed discovery sources, employer-domain authority, or production risk-provider evidence;
- Google or Microsoft OAuth applications, consent, provider writes, or calendar delivery;
- live tenant mutations, canary accounts, monitoring/support ownership, or production flag changes;
  or
- any physical-device, signing, notarization, or native package evidence retained for possible P2
  local distribution.

The strongest honest claim before those gates pass is:

> Bluey Jobs is designed for cloud-first web automation with no customer runner installation; the
> managed Background runner remains unavailable until its independent production gates pass.

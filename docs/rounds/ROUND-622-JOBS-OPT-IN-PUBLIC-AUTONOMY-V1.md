# Round 622 — Jobs Opt-In Public Autonomy V1

> **Codex preflight:** Load `$bluey-ops` before implementation, verification, review, or
> release work. The current worktree and live service state are authoritative. Archived evidence
> is a fallback only for one specifically missing historical fact.

**Date:** 2026-08-30

**Branch:** `feat/phase-622-jobs-opt-in-autonomy-v1`

**Base:** `feat/phase-620a-jobs-business-messaging-simulator@3d41a455`

**Status:** LOCAL SOURCE ACCEPTED — public, capacity-limited launch requested; no invitation list;
hosted proof, artifact promotion, production activation, and real provider canaries are not claimed

**Source commit:** `55d24e95234c1fcc2db22a912c739dfd5760eb66`

**Generated portal commit:** `95bb966fd897db94599a0c7e2defe1fb02ea3912`

## Product Decision

Bluey Jobs V1 is a capped public release with opt-in autonomy. It is not invitation-only and it is
not a random percentage rollout.

- Any verified Bluey account may attempt to join while the public enrollment window is open.
- Admission is first-come under one durable hard cap. Once admitted, an account stays admitted.
- A public-account denial or global suspension always overrides admission.
- The initial production cap is 25 accounts after a two-account preproduction race canary.
- Capacity may increase to 100 and then 250 only after the observation gates in this round pass.
- The cohort gate decides who may enter Jobs. It does not silently authorize a job application,
  mailbox write, calendar write, or broader Career Track.

The user-facing milestone is “Bluey Jobs V1.” Existing signed Bluey desktop artifacts keep their
current semantic versions; this round does not relabel an already-published desktop binary as
`1.0.0`.

## V1 Autonomy Boundary

For an admitted account, Bluey may perform the following only after the user has connected the
required account and explicitly authorized the relevant Career Track or communication action:

1. continuously ingest and verify supported job sources;
2. score matches against the current Career Profile and Career Track policy;
3. generate a grounded resume variant, application answers, and optional cover letter from
   confirmed candidate facts;
4. queue and execute an application through a certified ATS adapter in an isolated managed browser
   runner;
5. pause for missing answers, CAPTCHA, MFA, identity, legal, demographic, or other intervention
   fields that Bluey is not authorized to infer;
6. synchronize a connected Gmail or Outlook mailbox and classify application updates; and
7. send an exact approved recruiter reply or create an exact approved interview calendar event
   through the connected provider, with reconciliation instead of a blind retry after ambiguity.

V1 does not permit fabricated qualifications, fake applications, arbitrary recipient selection,
bulk cold email, hidden LinkedIn automation, personal WhatsApp/iMessage automation, or bypass of a
provider security control. Autonomous replies under a durable account policy, verified recruiter
outreach, C2C workflows, MCP/agent delegation, and official business messaging remain successor
increments under the existing Phase 620B–F plan. They remain part of the overall product goal, not
evidence this V1 may claim before their code and provider gates exist.

## How The Runtime Is Split

Bluey does not run every task in a browser container.

| Work | Runtime | External effect |
|---|---|---|
| Source polling and normalization | fenced discovery workers | reads supported public/authorized sources |
| Matching and ranking | Jobs API/database | none |
| Resume, cover letter, and answer generation | Jobs API model route | stores a grounded candidate-owned artifact |
| Application orchestration | Temporal gateway and worker | schedules durable steps; no employer I/O itself |
| ATS navigation and final submit | rootless managed Playwright runner | employer-facing browser writes and final submit |
| Gmail/Outlook sync | server-owned mailbox worker | provider reads only |
| Recruiter reply/calendar execution | server-owned communication worker | exact provider write after current authority check |
| Ambiguous-result recovery | lookup-only reconciliation workers | reads receipts/provider state; never repeats a write blindly |

The managed browser runner is an ephemeral execution process bound to a durable encrypted profile
volume. A run receives only the exact application packet, release identity, runtime identity,
lease, and scoped credentials needed for that run. Before irreversible employer I/O it must obtain
a fresh database authorization that rechecks the account, cohort, entitlement, Career Track,
application packet, ATS certification, signed job integrity, release, runtime, budget, circuit,
and kill-switch state. The request-start and irreversible-effect markers are durable, so a timeout
or crash becomes `side_effect_unknown` and cannot cause an automatic duplicate submit.

The source implementation rechecks the public-beta authority in the same SQLite or PostgreSQL
transaction that grants managed-effect authorization, claims a fresh local or managed Browser run,
permits a fresh irreversible submit, claims a communication action, or records a provider request
start. Denial, suspension, deletion intent, or a disabled master gate prevents new external
authority. Exact replay of an already-issued local Browser claim, an already-started local or
managed submit, and lookup-only reconciliation remain available so Bluey can determine what
happened without repeating a possibly successful write.

The local-run HTTP boundary must preserve that database authority: master-off applies directly to
fresh resume mutation, while claim and submit reach replay-first transactional checks and result
remains reachable for terminal evidence. A disabled local-distribution flag or unready fleet closes
only a fresh claim inside the same transaction, after exact replay and current beta authority.
An exact local submit replay accepts the existing schema-3 review-first proof or schema-4
ATS-certified proof only when its stored ticket, release, application, session, proof, and evidence
capacity still match. Schema 4 additionally recovers the terminal ATS authority. The response uses
the database-owned timestamp from the original click transition, making an exact retry byte-
identical without granting new effect authority.

## Existing Authority Reused

This round must preserve, not replace:

- track-scoped auto-submit authorization and revision invalidation;
- grounded application-kit generation and evidence citations;
- ATS certification, original-source verification, and signed job-integrity authority;
- exact final-submit read-back and receipt rules;
- durable workflow command, request-start, cleanup, and reconciliation authority;
- signed managed-cloud release/runtime identities and role quorum;
- encrypted runner-profile recovery and bounded deletion/purge authority;
- reviewed Gmail/Outlook reply and calendar execution with provider reconciliation;
- launch holds, circuits, budgets, idempotency, audit, privacy, and account deletion fences; and
- the no-egress Phase 620A1 messaging simulator as test evidence only.

## Public Cohort Contract

The imported public gate must provide:

- states `draft`, `open`, `closed_to_new`, and `suspended`;
- a paired, bounded half-open enrollment window;
- a monotonic hard cap and cumulative assigned count;
- sticky public or administrator admission without invitation tokens;
- strict account verification and account-deletion fencing under the admission transaction;
- account denial override with revisioned compare-and-swap administration;
- authenticated public status containing only a schema version, access state, and bounded reason;
- aggregate metrics only, with no account identifiers, cap value, assigned count, or internal
  release identity in the public response; and
- the existing `BLUEY_JOBS_BETA_ENABLED` gate as the outer emergency kill switch.

Checked-in environment examples remain fail-safe at `0`. That is not the intended live cohort
state: the separately recorded production activation must turn on only the exact flags authorized
by the signed release after every corresponding canary below passes.

## Required Effect Gates

The V1 activation is valid only when each enabled capability has independent evidence:

| Capability | Required runtime gates | Required proof before production `1` |
|---|---|---|
| Public Jobs surface | `BLUEY_JOBS_BETA_ENABLED` | cohort migration/read-back, auth smoke, suspend/rollback |
| Grounded application kits | `BLUEY_JOBS_MODEL_GENERATION_ENABLED` | fixed corpus quality, citation, latency, spend, and failure canary |
| Managed applications | cloud distribution, managed runtime, workflow dispatch, cleanup | signed stored-byte release, role quorum, hosted Temporal, runner image/rootfs, ATS canary, rollback |
| Mailbox updates | `BLUEY_JOBS_MAILBOX_SYNC_ENABLED` | approved read scopes, provider sandbox, cursor/revocation/deletion canary |
| Recruiter replies/calendar | communication OAuth write, dispatch, and reconciliation gates | separate consent, exact Gmail/Outlook sandbox matrix, ambiguity recovery, stop control |

Local-browser distribution is not required for the no-install V1. Direct/global discovery and
source-verification capabilities may be activated only when their exact successor signed release
marks them true and their pre-effect runtime roles are live.

## Proof-To-Enable Sequence

1. Freeze one clean exact source commit and build the Jobs API, workflows, managed runner, and
   portal once without production credentials.
2. Verify the stored OCI/static bytes independently, including inventories, SBOM, provenance,
   schemas, protocols, runtime users, entrypoints, image digests, and no source maps or secrets.
3. Apply and read back the paired PostgreSQL cohort migration with state `draft`, cap `0`, and the
   outer Jobs gate off.
4. Deploy the candidate dark, prove database backup and restore, health/auth/rate-limit behavior,
   metrics and alert delivery, spend caps, account deletion, and a previous-artifact rollback.
5. Prove hosted Temporal start/update/reconciliation and multi-process PostgreSQL locking under
   retry, process loss, network loss, and response loss.
6. Attest every live role to the exact image digest and read-only-rootfs policy; prove real runner
   capacity plus encrypted-volume create/restore/purge.
7. In a Bluey-owned ATS/provider sandbox, perform one successful application, one intervention,
   one definitive no-effect failure, and one ambiguous-outcome reconciliation for every enabled
   adapter family. Never use a fake application against an unrelated employer.
8. In controlled Google and Microsoft tenants, prove mailbox connect/sync/revoke/delete, one reply,
   one calendar event, one timeout/unknown reconciliation, and global suspension.
9. Open preproduction public enrollment at cap `2`; race at least 20 verified accounts and prove
   exactly two sticky admissions with no over-admission.
10. Threshold-sign and promote the exact stored bytes, open production at cap `25`, and enable only
    the canary-proven capabilities for admitted accounts.
11. Observe for 24–48 hours. Suspend new effects immediately on any abort condition; expand to 100
    and then 250 only after a fresh reviewed evidence record.

## Initial Per-Account Limits

Until production measurements justify a signed increase:

- at most 10 completed application submissions per account per day;
- at most 2 concurrently running browser sessions per account;
- at most 5 provider communication writes per account per day;
- at most 1 application or provider write per idempotency authority;
- no automatic retry after `side_effect_unknown`;
- no automatic answer for legal, work-authorization, salary, demographic, disability, veteran,
  background, security-clearance, signature, CAPTCHA, MFA, or materially novel questions; and
- one-click account pause plus administrator global hold must block new external effects while
  preserving receipt and reconciliation work.

These limits must be database-authoritative. UI text, worker configuration, or a queue setting is
not sufficient authority.

## Abort And Rollback

Any of the following stops new effects before cohort expansion:

- duplicate or unreceipted external effect;
- cross-account credential, profile, document, message, or receipt exposure;
- application submitted with a failed form read-back or stale Career Track authorization;
- unsupported resume fact or material claim;
- provider write to the wrong thread, recipient, account, or calendar;
- ATS/provider circuit, revocation, quarantine, integrity, or source-verification failure;
- runner/runtime identity drift, stale quorum, lost kill switch, budget bypass, or cap overflow;
- unreconciled ambiguity beyond the bounded support window; or
- backup, restore, deletion, monitoring, alert, or exact rollback failure.

Rollback order is: suspend the cohort, close new workflow/provider dispatch, retain lookup-only
reconciliation and receipt persistence, set the outer Jobs gate to `0` if needed, deploy the exact
previous stored artifact, and verify the higher-sequence signed rollback. A rollback never deletes
evidence needed to determine whether an external effect occurred.

## Acceptance Criteria

1. Public admission is first-come, durable, capped, sticky, and never described or implemented as
   invitation-only.
2. An admitted account receives no external-write authority until it separately opts into the
   applicable Career Track/provider permission.
3. Repository defaults, missing configuration, database errors, stale runtime, and partial deploys
   fail closed; the production evidence record distinguishes those defaults from intentionally
   enabled live cohort flags.
4. Search, matching, generation, orchestration, browser execution, mailbox sync, provider writes,
   and reconciliation each report their real runtime and authority boundary.
5. The exact current stack passes full Jobs tests/typechecks/builds, server formatting/check/strict
   Clippy/tests, paired schema parity, privacy/provenance/CI guards, generated portal freshness,
   and an independent line-by-line security review with no P0–P2 finding.
6. Hosted artifact, PostgreSQL, Temporal, runner, provider, monitoring, backup/restore, canary, and
   rollback evidence is attached to the implementation and review records before activation.
7. Production cap `25` is opened only after cap `2` preproduction proof, and expansion requires the
   defined observation window and a new review.
8. At least managed applications and the exact approved recruiter communication path are enabled
   for the admitted production cohort; V1 is not declared autonomous while every effect gate is
   still off.

The first aggregate full-library run is recorded in FIX-776 and FIX-777: it passed 1,604 tests and
found seven stale test contracts after the new authority boundary. The bounded corrections change
tests only. FIX-778 subsequently groups the local distribution claim into one typed input after the
strict all-target source gate rejected its eight-position wrapper; runtime semantics remain
unchanged. Exact-tip full-suite evidence remains mandatory under criterion 5.

## Release Authority

Implementation, local tests, source review, and the user's product request authorize preparation
and evidence gathering. Production activation occurs only from the exact signed candidate after
the proof-to-enable sequence passes. Every activation, cap change, provider expansion, and rollback
must be recorded with exact artifact identifiers and observed results in the implementation and
review documents.

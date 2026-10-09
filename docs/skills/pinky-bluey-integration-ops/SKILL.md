---
name: pinky-bluey-integration-ops
description: Implement, test, operate or hand off the isolated Pinky UI with Bluey AI backend integration. Use for shared sign-in, owned AI sessions, source consent, integration preprod, metering and coordinated promotion; not for standalone Bluey UI or unrelated Pinky remote changes.
---

# Pinky + Bluey Integration Ops

Pinky is the customer surface for this phase: one sign-in, Pinky-styled web
and compact native UI, with Bluey supplying independently deployed AI.
Standalone Bluey UI is deferred. Keep both repos and backend release ownership.
Remote access and AI have independent lifetime, authority and entitlement.
No third repo is needed for this batch. Keep Pinky-specific UI/adapters in
Pinky and shared AI behavior in Bluey. Preserve standalone Bluey auth/device
linking, wallet, streaming and history without requiring Pinky; add regression
coverage when changing these boundaries. Deferred UI work is not product removal.

Load `bluey-ops` and `pinky-ops` for their respective repo work. Read current
`AGENTS.md`, branch/status and task docs before editing. This skill is navigation
and operating memory, not deployment authorization or proof of current state.

## Canonical context

This skill is versioned in Bluey at
`docs/skills/pinky-bluey-integration-ops/SKILL.md`. The owner's local installed
copy is `~/.codex/skills/pinky-bluey-integration-ops/SKILL.md`. Read:

- `docs/ops/PINKY-BLUEY-INTEGRATION-RUNBOOK.md` for infrastructure, isolation,
  credentials, release/recovery and current evidence.
- `docs/rounds/PHASE-626-PINKY-INTEGRATION-PREPROD.md` for source pins,
  decisions and I0–I7 acceptance gates.
- Pinky's `docs/ops/BLUEY-INTEGRATION-RUNBOOK.md` for the cross-repo pointer and
  owned preparation files.

Use the current feature worktrees recorded in those docs, not dirty canonical
checkouts. Verify refs rather than treating a historical skill snapshot as
release truth. Keep local installed skill bytes synchronized with the reviewed
Git copy; install using a normal file copy, not a symlink to a disposable worktree.

## Source and environment boundaries

- The old `codex/ai-assist-button-20260921` is reference only, never a required
  base or bulk-merge candidate. Obtain the accepted current Pinky source and
  overlap agreement before modifying media/auth/billing/native seams.
- Use the dedicated integration test instance. Its provisioned identity and
  budget are in the runbook. Active VM does not mean deployed application.
- Existing Pinky preprod/production, Bluey production and Bluey Jobs must not
  be changed by integration deploys. Separate origins alone are not isolation:
  prove services, users, DBs, storage, signing keys, cookies and updates.
- Do not copy production envs/customer data or use production-default deploy
  scripts. Preserve exact verified artifacts for later coordinated promotion.
- Use owner Mac/Windows and shared resource queues. No unnecessary hosted
  GitHub runners or heavy builds on the small integration host. Isolate test
  databases/build roots and clean task-owned temporary outputs on every exit.

## Integration invariants

- Target UX: Pinky sign-in is sufficient; no second Bluey desktop
  or connect-code step. Immutable-subject account binding and narrowly scoped
  issuer/audience/environment delegation remain required. Email matching or
  one pooled service wallet is not per-user ownership/accounting.
  Do not infer that this identity contract is implemented from the target UX.
- AI sessions are owned independently of remote session codes. Replace stale
  authority with the new owned lifecycle; never simply remove owner checks.
  AI failure, Stop or zero credit must not stop remote access.
- Viewer audio to AI defaults off. Playback/captions/Listen do not grant AI
  consent. Bind explicit revocable consent to the accepted source generation
  and AI session; recheck at dispatch and discard revoked/stale queued input.
  Preserve loopback exclusion; screen/mic/uploads have separate permissions.
- Keep provider keys on Bluey. Enforce per-account data and usage isolation,
  expiry/revocation, bounded streaming, cancellation and durable idempotency.
- $15 PAYG / $9 active-hour pricing remains a proposal until economics and
  billing semantics are accepted. Test balances only; no real charges/reload
  or simultaneous minutes+PAYG charging during preparation.
- Routine diagnostics are content-free. Approved history has its own tenant
  permissions, retention/export/deletion; it is not permission to log every input.

## Evidence and handoff

Report each gate separately: offline tests, VM/resources, accepted source,
live API/auth, billing/consent, physical Mac/Windows and promotion. No mock-only
latency/UX claim, no blanket “all done”, and no profile PASS as live release proof.
Record source/artifact digests, real live URLs, cost, rollback scope, open gates
and exact residues in the Git-backed runbook/round. Commit on feature branches.
Owner-approved actionable questions use private `owner-alerts` project
`bluey-pinky-integration`; never put the receiver, secrets or message history in Git.

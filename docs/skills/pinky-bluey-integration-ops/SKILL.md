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

For the current overlay/selected-text continuation, read
`docs/work/PHASE-626-SELECTED-TEXT-CONTINUATION-20261010.md`. It maps both repos,
new source-only capabilities and unclosed gates. Pinky's companion native skill
is `docs/skills/pinky-bluey-overlay-ops/SKILL.md` on its integration feature
branch; load it for native controls/opacity/keyboard/capture work. Do not confuse
locally committed skills with GitHub publication or live deployment.

This skill is versioned in Bluey at
`docs/skills/pinky-bluey-integration-ops/SKILL.md`. The owner's local installed
copy is `~/.codex/skills/pinky-bluey-integration-ops/SKILL.md`. Read:

- `docs/ops/PINKY-BLUEY-INTEGRATION-RUNBOOK.md` for infrastructure, isolation,
  credentials, release/recovery and current evidence.
- `docs/rounds/PHASE-626-PINKY-INTEGRATION-PREPROD.md` for source pins,
  decisions and I0–I7 acceptance gates.
- Pinky's `docs/ops/BLUEY-INTEGRATION-RUNBOOK.md` for the cross-repo pointer and
  owned preparation files in the integration feature checkout. The active
  Pinky release tree may not contain this additive runbook yet; absence is a
  promotion handoff gap, not authority to edit its concurrent release work.

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
- Use owner Mac/Windows and shared resource queues. No GitHub runners or hosted
  fallback for preprod, and no heavy builds on the small integration host.
  Production GitHub runners are a later coordinated Pinky release activity.
  Isolate test databases/build roots and clean task-owned temporary outputs
  on every exit. Use the local queue even for agent-started builds; source-only
  agents must not silently run Cargo/Go outside the owned harness.

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
- Latest requested pricing is $15 initial PAYG with optional auto-reload, and
  $7.99 per purchased AI hour for an active Remote subscriber (one or more
  hours; proposed default two, never an automatic purchase). Remote subscribers
  may also choose PAYG. Older $9/hour notes are superseded proposals. Economics,
  billable-time definition and purchase/reload semantics remain unqualified.
  Test balances only; no real charges/reload
  or simultaneous minutes+PAYG charging during preparation.
  Product direction is Remote Access and Pinky AI sections with one Billing
  entrypoint, not shared entitlement or an unproven mixed-payment transaction.
  Read Pinky's `docs/work/PINKY-AI-PRODUCT-BILLING-BOUNDARY-20261009.md` before
  changing money flows; preserve durable provider/customer/subscription
  attachment, paid-through/downgrade boundaries and standalone Bluey billing.
- Routine diagnostics are content-free. Approved history has its own tenant
  permissions, retention/export/deletion; it is not permission to log every input.

## Evidence and handoff

The receipts below are historical milestones, not current implementation status.
The 2026-10-10 continuation at the top supersedes older dimensions and statements
that selected text is absent. Only its explicit test/deployment gates are current.

The historical Phase 626 delegation/lifecycle foundation is described in
`docs/work/IMPL-PHASE-626-DELEGATION.md` and its matching review; it alone could
not dispatch or charge. The isolated authenticated text path now has live
artifact, PostgreSQL, lifecycle and synthetic accounting evidence. Read
`docs/work/PHASE-626-TEXT-PREPROD-ACCEPTANCE.md` for current pins and individual
gates, not branch HEAD or a historical skill snapshot. Context `active` is
still not billable time or AI entitlement. Strict STAR factual quality failed
despite label/transport success; private Otter memory is planned, not built.
Bounded actual Mac text stream/Stop/repeated authenticated reopen now passed;
the website is management-only with actual balances and request metadata, not
saved transcript content. Windows CLI compile/offline smoke and the compact
website's light/dark/320px navigation checks passed; full physical Windows UI,
device/media consent, real billing
and latency percentiles remain separate open gates. Current exact deployed pins
are Pinky `b7de1545` and Bluey `625b9131`; b68 remains Pinky's rollback. Pinky's
`docs/rounds/BLUEY-INTEGRATION-PRODUCT-SHELL-20261009.md` records the two-product
website, preserved remote/Billing semantics, runtime-race/Node50/Python26 and
actual light/dark/320px/hash/keyboard/API acceptance. This is not real AI billing.
Native QA artifact is Pinky `ae3eef3d`: unchanged 116×36 pill / 320×320 panel,
Auto/Stacked/Side by side non-overlapping CC+AI, guarded local keyboard access,
queued Mac AppKit/arm64/x86_64 and Dell native tests/builds, actual Mac synthetic
caption + real text Ask/Stop/Hide/reopen and light/dark checks. Read Pinky's
`docs/rounds/BLUEY-INTEGRATION-CC-AI-LAYOUT-20261009.md` for exact hashes and
limitations. Normal capture protection remains; the distinct explicitly
screenshot-visible synthetic QA variant is never installed or promoted.
Native QA is not a signed updater release. Streaming reading/selection has
actual AppKit policy tests, not a controlled physical live-update soak.
Fresh `f4654820` owner packaging exposed delayed Stop settlement: the five-poll
client stopped checking while accounting was still pending. That package is held.
Native `e3a44b81` adds bounded recovery and status-only Check; exact Mac package
passed two immediate Stop cycles without reopen, then a new answer and Hide/reopen.
Private Mac/Windows QA archives from packaging `5c76e131` are retained on the
dedicated instance, not publicly served or installed. Read Pinky's
`docs/ops/BLUEY-INTEGRATION-OWNER-DOWNLOADS.md` and Stop-recovery round for exact
hashes/pull commands. Windows native/offline tests passed but live auth/physical
GUI remain open. Do not infer full remote/media or production acceptance from
these text-only packages, or bypass OS/SSH protections to run them.
Latest compact native QA is Pinky `b3c60282`; packaging `415098b7` supersedes
the e3 preview. Read Pinky's compact-glass round and owner-downloads runbook for
exact hashes. AI-only has a 266×46 bar, 116×36 collapsed pill, no Remote/CC
controls, and answer Hide leaves the bar available. Combined bar is 456×46.
Queued exact-source Mac/Dell builds and fresh protected packaged Mac Ask/Stop
passed; both private archives have server hash readback. Combined CC physical
visual inspection hit ScreenCaptureKit failure and remains open, as does Windows
physical GUI/live auth. Play/listening/STT, selected files/named conversations
and avatar selection are not implemented. Use the design-only listening/context
contract before extending media; Remote captions never imply AI consent.
Optional pets belong in the existing brand-icon slot, not another window or
always-on animation. Normal protected helpers only in owner archives; no prod,
Jobs, other preprod, real billing or installed app changes.
Owner rejected the b3 preview as heavy and said CC appeared gone. The superseding
shared-bar refinement is tracked in Pinky's
`docs/rounds/BLUEY-INTEGRATION-SHARED-BAR-REFINEMENT-20261009.md`. Keep CC
discoverable in both modes: actual independent toggle in combined mode, neutral
disabled/source-unavailable in text-only AI QA. Do not confuse this preview with
removal of production captions. Preserve the 116×36 collapsed pill; secondary
answer guidance, appearance and layout belong in settings, not more bar chips.
The old reference branch's Packs UI is not attachment support in the new owned
text integration. Until the refinement receipt records exact artifacts and
actual checks, b3 remains the previous preview, not the revised design acceptance.
GitHub publishing is pending noninteractive Git
transport; verified private feature bundles retain unpublished source. Do not
access Keychain or rebuild GitHub history to bypass that boundary. Consult the receipt, not future branch
HEAD. No whole-product or production acceptance is implied.

Report each gate separately: offline tests, VM/resources, accepted source,
live API/auth, billing/consent, physical Mac/Windows and promotion. No mock-only
latency/UX claim, no blanket “all done”, and no profile PASS as live release proof.
Record source/artifact digests, real live URLs, cost, rollback scope, open gates
and exact residues in the Git-backed runbook/round. Commit on feature branches.
Owner-approved actionable questions use private `owner-alerts` project
`bluey-pinky-integration`; never put the receiver, secrets or message history in Git.

Reviewed skills/handoffs may be published separately through the connected
GitHub API on a documentation-only branch from the existing remote tip, using
`[skip ci]` and read-back verification. Do not recreate implementation history,
advance product branches, trigger hosted runners or imply source publication.

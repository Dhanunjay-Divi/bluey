# Bluey AI Phase 625 recovery and mainline handoff

> Codex preflight: read `$bluey-ops`, inspect current Git/PR state, and use the
> isolated test launcher before heavy validation. This document records source
> recovery, not a deployment or release claim.

## Owner direction

- Finish Bluey AI and the native desktop overlay first. Bluey Jobs remains a
  separate workstream and must not be bulk-merged into this batch.
- Recover all work from the interrupted UI/latency batch, review it, and merge
  through PRs. Never push directly to `main`.
- Preserve a concrete handoff for the next agent and stop at the owner's usage
  threshold. The owner confirmed stop at **25% remaining**. On 2026-09-21 the
  account reported 46% used / 54% remaining. Check actual account usage during
  work; checkpoint and hand off before crossing the remaining-usage threshold.

## Exact source state

- Canonical checkout: `/Users/uno/Downloads/cue`, branch `meeting-main`, commit
  `8d173f9ab52bb27df14fc00d8e4804f7dc1d293b`. It contains unrelated local changes
  and must be preserved. Do not reset, clean, stage, or deploy it by assumption.
- Fresh `origin/main`: `660f8d2b` (`fix: restore hosted answers and compact Bluey
  overlay (#33)`).
- Preserved diagnostic parent: PR #38,
  <https://github.com/Dhanunjay-Divi/bluey/pull/38>, head
  `ca046c48adcf9d2f082d86245a8e619254817b6a`. Its three commits are
  `f81a558d`, `f8b4d286`, and `ca046c48`.
- Interrupted branch `feat/phase-625-ui-latency` still points to `ca046c48`.
  Its old checkout `/private/tmp/bluey-phase623-hosted-ui` no longer exists.
  The old worktree index contains the committed base, not the later local edits.
- Recovery checkout:
  `/Users/uno/.codex/worktrees/bluey-phase625-recovery/cue`, branch
  `feat/phase-625-recovery-mainline`, created from the exact diagnostic parent.
- Recovery evidence/scripts are private and external to tracked source:
  `/Users/uno/Downloads/cue/.git/bluey-recovery-20260921/`.
- Do not prune old worktree metadata or delete transcript evidence while
  recovery is incomplete.

## Recovery inputs and method

The main task is `019f5571-9cf4-7b10-bcbe-83c291e530d5`. Its local transcript is
`/Users/uno/.codex/sessions/2026/07/12/rollout-2026-07-12T04-28-57-019f5571-9cf4-7b10-bcbe-83c291e530d5.jsonl`.
Descendant transcripts must be identified from their parent metadata, not from
unrelated task contents. Phase 625 began at 2026-08-31 08:20:45 UTC.

Recover successful code patches and explicit file snapshots in chronological
order. Translate only the old checkout root to the recovery checkout. Do not
execute arbitrary commands taken from transcripts. Deduplicate inherited tool
records and distinguish failed patches from successful writes. Account for
mechanical formatting and file-split operations that changed patch context.
Record every unrecovered or ambiguous edit; historical passing tests cannot
certify a reconstructed tree until rerun.

## Intended batch coverage

The interrupted batch included:

- Exact 112-by-30 compact pill; expanded macOS/Windows UI, answer styles,
  readable light mode, shortcuts, and Windows accessibility/DPI work.
- Browser-to-desktop sign-in bound to state plus PKCE, one-time grants and exact
  retries, and bounded legacy-client transition.
- Exact account generation fences across answers, audio, meeting state,
  diagnostics, and native event publication; cancellation of stale answers.
- Incremental answer rendering, disclosure holdback, native text buffer
  ownership and secure clearing, and bounded provider stream parsing.
- Metadata-only diagnostics, consent receipts and recovery, provider error
  redaction, retry/idempotency accounting, and request/session log sanitization.
- Public sample-only interactive demo; 112-by-30 pill and mobile/light/dark UI.
- Disposable test roots and cleanup on success, failure, and interruption.
- Daemon test-module extraction and task/FIX/review/round documentation.

The previous summary reported roughly 69 modified tracked files plus newly
created native modules, tests, and docs. File-level recovery must be reconciled
against the successful edit manifest before claiming this coverage is restored.

## Last unresolved correctness findings

These were open or being fixed when the session stopped; inspect the recovered
final code and tests before deciding which remain:

1. Diagnostics consent: a live dashboard grant must not be compensated by the
   daemon before the dashboard finishes. Use durable operation phase and owner
   instance authority; only orphaned/reconcile operations are compensatable.
   Startup must atomically orphan a previous instance. A missing server lookup
   does not prove a delayed grant cannot commit and must not clear pending state
   merely after a timer. Compensation uses exact operation/revision CAS.
2. Provider SSE parser: preserve scan progress across small network chunks;
   a near-limit frame fed one byte at a time must remain linear. Correctly
   handle split LF/CRLF delimiters at the exact byte cap.
3. Bound successful non-streaming provider bodies for completion, embeddings,
   and STT before JSON allocation; enforce parsed/output limits as appropriate.
4. Bound idempotency cached JSON at DB retrieval before deserialization and at
   persistence. Bound all replay fields and total payload, not only text.
5. Cached SSE replay uses UTF-8-safe bounded chunks instead of one event per
   whitespace fragment. Preserve exact text and emit a fixed over-limit error.
6. Free-form request IDs must not leak through cost-hold scope logs. Session IDs
   and lane values also need canonical safe logging. Audit `AnswerOpsEvent`
   metadata insertion; the last root inspection found raw request/session/trace
   values still inserted there before audit persistence.
7. Hidden-output provider failures release idempotency for retry; visible
   partial output terminalizes the request. Preserve conservative provider-cost
   settlement before customer billing and provider fallback.

## Verification and merge gates

Historical worktree evidence from 2026-08-31 included full Rust/server gates,
35 dashboard tests, 21 web tests, native macOS builds, strict C/sanitizers, and
Windows cross-link tests. Later fixes were still landing. None of that proves
the current reconstructed checkout passes.

For the recovered final tree:

1. Run meaningful focused regressions for the open findings above.
2. Run formatting, all-target Rust tests and strict Clippy for both workspaces,
   dashboard tests/build, web tests, native construction tests, release policy,
   and isolated-launcher self-test. Verify exact temporary roots are removed.
3. Obtain an independent final review of every changed area and update the
   implementation/review/FIX records with actual counts and limitations.
4. Commit recovered changes in logical units; push the feature branch and open
   a consolidated successor PR targeting `main`, retaining the three PR #38
   parent commits plus their reviewed corrections. Attach the PR to this task.
5. Merge the successor only after required CI passes and review is resolved;
   then close PR #38 as superseded with the exact successor reference. Do not
   merge the known-defective parent separately just to flatten the branch stack.
   Do not label zero-step failures passes.

The August 31 GitHub runs reported an Actions budget block. That was historical,
not proof that the block remained active on September 21. A fresh API check
confirmed the repository is public and CI uses standard GitHub-hosted runners.
PR #38 CI run `33372372821`, attempt 2, was rerun on September 21 at 06:39 UTC;
real Ubuntu, Windows, and macOS runners started. Ubuntu then failed the isolated
launcher self-test with `failure case left temporary workspace`, confirming the
known source defect rather than a billing blocker. Inspect the other jobs and
the final recovered-tree CI before merge. Do not increase spending settings to
work around the old annotation. The Phase 624 review remains pending final gates.

The base PR also has six independently verified P1 findings to reconcile with
the recovered fixes: launcher cleanup exit status; diagnostics-only upload
incorrectly requiring a live cloud session; deletion retry blocked by a retained
transition flag; upload consent revision not enforced transactionally; refreshed
tokens rotated only in memory; and a prepared deletion receipt reported as an
already-established deletion fence.

## Deployment is a separate remaining gate

Previous release blockers were physical Windows authentication/interactive QA,
the existing trusted Ed25519 release private key, and GitHub Actions capacity.
Recheck their availability; never generate a replacement signing identity to
work around missing key access. Cross-compilation is not physical Windows QA.

Build once, test those exact artifacts, and promote unchanged. Keep Jobs
generation/distribution flags unchanged. The current CLI/native packaging does
not itself ship the Tauri dashboard's new deep-link UI, so verify actual shipped
surfaces and rollout compatibility before claiming that flow is deployed.

## Immediate next action

Finish the successful-edit recovery manifest, replay/reconstruct source into
the managed recovery checkout, checkpoint it in Git, then close the specific
correctness findings and run the gates. Preserve partial recovery plus this
handoff if the owner's usage stop point arrives first.

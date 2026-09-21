# Bluey AI Phase 625 recovery and mainline handoff

**Status: partial recovery checkpoint; NOT mergeable or deployable.**

**September 21 publication update:** the owner authorized making the branch
retrievable for the next developer, not resuming implementation. Read
[START-HERE-NEXT-AGENT.md](../../START-HERE-NEXT-AGENT.md) for fetch instructions
and private-evidence prerequisites. The formerly uncommitted 36-line Windows
edit is now preserved unchanged in **`2e1d6976`**, still unverified and outside
the replay journal. This supersedes the local-only/uncommitted state in the
historical stop snapshot below. Jobs, mainline, and deployments remain untouched.

**Owner stop instruction (latest):** stop spending credits and hand this work
to the next developer. All three supporting agents were interrupted. Do not
resume automatically. Bluey Jobs is outside scope and was not modified.

**Historical stop snapshot:** committed recovery source is `512a94b1`; handoff tip
before this stop note was `01bb3823`. The branch is local only (no remote branch
was published). One additional **uncommitted** edit remains in
`native/windows/cue-overlay/main.c` from the briefly resumed predecessor
recovery agent. It has not been reviewed, tested, or entered into the replay
journal. Preserve and inspect that diff first; do not assume the Windows
blocker is resolved or blindly replay over it. All other recovery source was
clean at the stop check. No merge, release, deployment, or Jobs flag changed.

> Codex preflight: read `$bluey-ops`, inspect current Git/PR state, and use the
> isolated test launcher before heavy validation. This document records source
> recovery, not a deployment or release claim.

## Portable skill and next-agent ownership

Read the branch-local [$bluey-ops](../../.agents/skills/bluey-ops/SKILL.md)
and [agent round checklist](BLUEY-AGENT-ROUND-CHECKLIST.md). Personal skill files
are no longer required. This documentation-only update does not authorize
resuming recovery. Future owners below are **unassigned**; no agent was restarted.

| Work item | Next owner | Exact starting scope | Completion evidence |
|---|---|---|---|
| Recovery blocker | Unassigned recovery agent | Inspect `2e1d6976` for the unverified `native/windows/cue-overlay/main.c` WIP, then the exact predecessor evidence under “Next-agent start here” | Complete verified predecessor, hashes, journal entry; one blocked patch replayed safely |
| Remaining source | Unassigned recovery agent, after blocker | Remaining snapshots, chronological mutations, formatter checkpoints, and generated test split below | Complete source inventory reconciled to successful evidence; explicit unknowns |
| Correctness fixes | Unassigned implementation agent, after recovery | Seven “Last unresolved correctness findings” plus six parent PR P1s | Focused regressions and linked FIX records for every finding |
| Validation/review | Unassigned reviewer, after fixes | Reconstructed root/server/dashboard/web/native surfaces and launcher | Fresh gates, independent review, platform limitations; update IMPL/REVIEW/round |
| Mainline integration | Unassigned coordinator, after green gates and authorization | Consolidated successor to PR #38, not a standalone parent merge | Reviewed successor, required CI, exact merge SHA, updated handoff |
| Release | Unassigned release agent, separately authorized | Exact Windows/macOS artifacts and shipped UI surfaces | Hardware smoke, hashes, trusted-key release and rollback evidence; Jobs unchanged |

For each resumed or new round, copy the ownership and remaining-work fields from
[TEMPLATE-ROUND.md](TEMPLATE-ROUND.md). Link every task to its actual record;
do not mark an entire round complete while a listed task or gate remains open.

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

## Recovery checkpoint, September 21

The verified data-only corpus contains 703 literal patches plus one successful
generated test-module split. Compacted child journals flatten top-level times;
actual ordering comes from paired tool-output UUIDv7 completion timestamps.
Independent review confirmed the original 701 JSON-literal records had unique
call/output identities, no same-time overlapping-file conflicts, and no Jobs
paths. Two further literals use reviewed JavaScript Unicode-codepoint escapes;
they were decoded without evaluating JavaScript. Arbitrary historical shell or
JavaScript commands are never replayed.

Private recovery inputs and per-patch before/after hashes live under
`.git/bluey-recovery-20260921/` in the canonical repository. Use
`mutation_candidates-v3-supplemented.json`, `replay_safe.py`,
`reviewed_patch_ids.txt`, `manual_required_ids.txt`, and `replay-journal.jsonl`.
The supplemented manifest SHA-256 is
`e3a05e8ce76471acc097685e9d4ff44a203aa3a2e1df03c898ca85dd5aa2774a`.
The current journal, rather than this progress paragraph, owns the exact replay
position. At this checkpoint 79 literal patches had applied; the next patch,
`caee4ddfdc44`, stopped on a missing Windows sign-out precursor. Do not skip a
failed preimage or replay a historical failed patch as if it succeeded.

Complete captured diffs have restored streaming-latency and macOS sign-in
precursors. The Windows snapshot is only an early partial diff, not a complete
file delta; its remaining account-state handler is still missing. The complete SignInPresentation.swift file
and seven otherwise-uncovered core IPC, observability, secret-store, embedding,
and transcription files were restored from independently checked snapshots.
Details/hashes are in the private `manual-reconstruction-log.md` and saved diffs.
Other uncovered source files and intervening Windows changes remain under audit.
The partial source tree has not been compiled, reviewed for merge, or deployed.

Fresh PR #38 CI attempt 2 completed on all three platforms. Ubuntu and Windows
failed the launcher cleanup self-test. macOS passed formatting, Clippy and build,
then failed `replacement_ready_hydrates_while_exited_event_waits_for_command`
because its overlay fixture was outside the verified install directory (606
daemon tests passed, one failed, five ignored). Recheck this test after recovery.
These are actual source/test failures; the old Actions budget block is resolved.

## Next-agent start here

The owner requested this handoff before further work. Supporting agents have
finished their evidence audits. No new feature implementation should begin until
recovery is complete. The partial product-source checkpoint is **`512a94b1`**
on `feat/phase-625-recovery-mainline`; it preserves 37 files and is deliberately
not release-ready. Subsequent handoff-only commits may advance the branch tip.

```sh
cd /Users/uno/.codex/worktrees/bluey-phase625-recovery/cue
git status --short
git log -6 --oneline
git fetch origin
git diff --stat ca046c48..HEAD
shasum -a 256 /Users/uno/Downloads/cue/.git/bluey-recovery-20260921/mutation_candidates-v3-supplemented.json
```

1. Read this document, `IMPL-PHASE-625-RECOVERY.md`,
   `REVIEW-PHASE-625-RECOVERY.md`, and the private manual reconstruction log.
   Do not regenerate the corpus from the experimental extractor: v1/v2 and
   experimental v4 had known errors. The frozen supplemented v3 hash above is
   the reviewed input. It includes failed and read-only candidates; only the
   reviewed successful IDs are eligible for replay.
2. Restore the **complete missing Windows predecessor**, not just one line.
   The first Windows agent read already contains the full account-state block:
   journal `01a05877-21e4-7510-9b5d-5a01c42ef865`, call `call_O9qpz...`, output
   `ctco_01a058ae-87b8-70a3-ad31-6ece63153bc6` (near journal line 98).
   Find the exact full call ID from that output's `call_id`. The failed replay
   is `caee4ddfdc44e8747cb89ebb3cc4186118008d3983f13fe0cef1b2e2154759c6`
   / `call_99tnWZQXPiFUMMpLnSMncAqD`. The last successful literal is
   `12f471f74f952218acc2ba0c54d5a5f900da9aa3b928293744a2aeaa0693abb0`.
   Do not replay later account-state patches wholesale as a predecessor:
   they already contain changes owned by the currently blocked patch.
3. After recording and checking that restoration, resume **one patch** first:

```sh
/opt/homebrew/bin/python3 -B /Users/uno/Downloads/cue/.git/bluey-recovery-20260921/replay_safe.py \
  --manifest /Users/uno/Downloads/cue/.git/bluey-recovery-20260921/mutation_candidates-v3-supplemented.json \
  --max 1 --apply
git diff --check
```

4. Stop on every nonzero replay result. A clean hunk count does not prove a
   complete file delta: `sed` can truncate exactly between hunks. Verify source
   command bounds, output truncation markers, next-file boundaries, and hashes.
   All transcript parsing is data-only. Never execute captured JavaScript,
   shell, installers, cleanup commands, or credential operations.
5. Reconcile formatter checkpoints. Rustfmt 1.98 is required; formatting is
   source-mutating even when not represented by an apply-patch record. Whole
   root/server formatting is caught up through roughly 16:49. Later recorded
   checkpoints occur at 17:48/17:53, 18:47/18:50, 19:06/19:09/19:14/19:39,
   20:24/20:32/20:47/20:52, 21:01/21:34/21:36, 23:28/23:51/23:56, and many
   times from September 1 00:00 through 00:39. Inspect their exact tool inputs
   for file lists and toolchain, then run only separately reviewed formatter
   argv against this checkout. Cumulative root/server `cargo +1.98 fmt` can
   reconcile formatting-only differences; it cannot repair missing source.
6. The successful generated split is
   `call_Pp6nsTDFxFJpHMoVjEvXoW41` at 23:59:55.558Z in journal
   `01a05a3a-2233-7bd3-b838-aad33045611b`, near line 284. It moves the final
   inline `#[cfg(test)] mod tests { ... }` body into `app/tests.rs` and leaves
   `#[cfg(test)] mod tests;`. Recreate the transformation through apply_patch
   from the exact current source and record its before/after hashes. The
   replayer stops at `manual_required_ids.txt` until this is recorded.
   Earlier attempts `call_T686...` and `call_5VG...` failed; do not replay them.

## Remaining snapshot inventory

The following captures were independently located and apply-checked but **not
applied** before handoff. Output IDs are authoritative; human line/ordinal
numbers may differ by one. The source journal map is `selected_sessions-v3.json`.

- `crates/cue-cloud-client/src/types.rs`: call
  `call_F7Z34FR4q39sNcKkumBVaESA`, output
  `ctco_01a05964-c3f1-7ad3-aad9-b74363077734`; isolate its diff header through
  EOF. Review extraction: 67 lines / 2,394 bytes, SHA-256
  `4ce080ceffb3dd7f101067496c9f4c5ab63e627c452e177b950bdf32a7009f3b`.
- `Cargo.lock`: call `call_ysVh862dlQ2Fz6RbqXLJ0B4i`, output
  `ctco_01a05958-9950-7bb0-85c5-c05ea4dc3ca7`; first complete diff section.
  Review extraction: 182 lines / 2,914 bytes, SHA-256
  `5f2bbac77ecdb5ad937b370bbcae172be2dc1ec3895496034d738fe138b76979`.

The reviewer's print/awk pipelines sometimes add one EOF newline. Compare raw
capture and capture-plus-one-newline separately; document which hash is used.
Never normalize or alter actual source lines to make a hash match.

Still not recovered/verified: `crates/cue-llm/src/bluey_managed.rs`, changes to
`docs/work/FIX-588-account-bound-diagnostics.md`, complete macOS overlay
`Tests/*`, complete `web/assets/bluey-product-demo.css` and `.js`, and complete
`web/tests/*`. The final recorded status call `call_AlzyT24pRfiLCohdSxGIqiah`
listed 116 status paths, including directory entries; compare expanded files,
not only counts, against all successful mutations and snapshots. More missing
precursors may be exposed by later patch preimage failures.

## Handoff preservation and final gates

- The recovery checkout's source is checkpointed in Git. Canonical
  `/Users/uno/Downloads/cue` still has unrelated owner/other-task changes; do not
  claim those are cleaned, merged, or deployed. Preserve them untouched.
- Raw session journals and private recovery evidence are **local only**, under
  `.codex/sessions` and canonical `.git/bluey-recovery-20260921`. They are not
  pushed to the public repository. A next agent on another host must receive
  these through an explicitly approved private transfer, not a public PR.
- At handoff, root/server Rustfmt and diff checks pass. Reconstructed product
  build, strict Clippy, tests, visible UI, platform runtime, and latency gates
  remain unverified. The broad hygiene scan identified existing baseline
  fixture/development strings in `cue-cli/src/logs.rs` and dashboard commands;
  exact flagged lines were confirmed already present in `ca046c48`.
- Finish recovery and the specific correctness findings above, run the
  isolated-launcher self-test, then all relevant root/server/web/native gates.
  Preserve honest local-versus-cloud and consent/diagnostic boundaries.
- Open a reviewed consolidated successor PR to `main` only when source is
  coherent. Do not merge PR #38 independently. Merge only with green required
  CI, then perform exact-artifact Windows/macOS release validation and the
  existing trusted-key release workflow. No deploy or Jobs flag change has
  occurred during this recovery/handoff.

# Round 589 - Bluey local branch tip preservation

Date: 2026-08-02

Status: complete; remaining Codex-owned local-only tips are preserved remotely,
their product behavior is reconciled against main, and no stale runtime patch is
approved for production

## Objective

Finish the local Git audit that followed Round 588. Preserve exact commit tips that
existed only as local refs or diverged from a same-named remote, determine whether
their behavior is already present on current main, and make it safe to remove the
stale local aliases without losing evidence.

This round changes documentation only. It does not build or deploy the main API,
Jobs API, Jobs portal, discovery workers, Caddy configuration, or native artifacts.

## Source of truth

- Reconciliation base: `origin/main` at `dc6720b4f179b04622e067cb8fcb5b15370cb0fd`
- Reconciliation worktree: `/private/tmp/bluey-historical-reconciliation`
- Historical Jobs snapshot: `888547f8f939126043015c880b8efa5211e8694d`
- Meeting-owned worktree: `/Users/uno/Downloads/cue` on `meeting-main`

The meeting-owned worktree was not staged, reset, cleaned, merged, or deployed.

## Preserved branch tips

The following exact local tips now have dedicated remote archive refs:

| Former local ref | Exact tip | Remote archive ref |
| --- | --- | --- |
| `codex/jobs-discovery-p0` | `ae7d6fd249e7cafea0aab442a967937bb52be945` | `codex/jobs-discovery-p0-archive-20260802` |
| `codex/jobs-truth-spend-fix` | `f2736507fbe14ae64085fd4374f52a712ab9ef3e` | `codex/jobs-truth-spend-fix-archive-20260802` |
| local divergent `codex/bluey-interrupted-asks-round519-20260712` | `35cc03e67a830e6610324f3f838757976a674546` | `codex/bluey-interrupted-local-tip-archive-20260802` |

`git ls-remote` confirmed that each remote archive points to the exact local tip.
These refs are preservation evidence, not merge candidates.

The local `codex/bluey-jobs-20260710` tip at `325093b2` is already an ancestor of
current main. It does not require another archive branch.

## Hunk-level disposition

### Discovery publication

The archived discovery history has one patch that is not topologically represented
on main: `7dab82de` (`fix(jobs): make discovery publication atomic`). Current main
already contains the expanded behavior:

- bounded source enrollment through `DISCOVERY_MAX_SOURCES_PER_TRACK`;
- atomic completion through `complete_discovery_run`;
- immutable manifest counts and accepted/rejected accounting;
- replay, archive hash, timeout, and source-authority validation;
- focused API and database tests for publication and recovery.

Merging the old patch would replace newer split database modules and safety checks.
The archive remains available for historical comparison only.

### Provider spend truth

The archived spend history has one patch that is not topologically represented on
main: `3737d949` (`Harden Jobs truth and provider spend accounting`). Current main
already contains the expanded behavior:

- durable `jobs_provider_cost_holds` reservations and settlement;
- model-generation exposure and allowance checks;
- upstream spend admission for resume generation, router calls, STT, transcription,
  embeddings, and web search;
- startup cleanup and a supervised spend-truth janitor;
- integration and failure-injection tests for reservation recovery.

The later commits on the archived local branch are patch-equivalent to main. No old
spend-accounting commit should be replayed over the current implementation.

### Optional Kimi provider

The divergent local interrupted-work tip adds a managed Kimi K3 fallback provider.
Current main intentionally has no Moonshot or Kimi production route. The historical
patch lacks a funded live canary, current pricing/routing approval, release evidence,
and a production configuration decision. It is preserved remotely, but it is not
approved for main or deployment.

Adding a managed provider remains a separate product, billing, privacy, routing, and
release task that requires current code, tests, documentation, and live evidence
together.

## Local-ref cleanup rules

After the remote refs and this document are verified:

1. remove only the stale local aliases listed above;
2. keep the remote archive refs intact;
3. keep the historical Jobs snapshot branch checked out and clean;
4. leave `meeting-main` and all non-Codex-owned refs untouched;
5. never bulk-merge an archive or snapshot branch into main.

Deleting a local branch alias after its exact tip is remote does not delete its
commits or change any worktree file.

## Deployment disposition

No production deployment is required. The only repository changes are audit and
preservation documentation. Restarting services or replacing artifacts would add
risk without changing the product. Current production should instead be verified in
place.

## Verification

- all three remote archive refs resolve to their exact expected commit IDs;
- `codex/bluey-jobs-20260710` is an ancestor of current main;
- current main contains the expanded discovery publication and durable spend-truth
  implementations described above;
- no Moonshot or Kimi provider is enabled on current main;
- `git diff --check` passes for this round;
- the Bluey ops documentation preflight passes;
- the historical Jobs snapshot checkout remains clean;
- the meeting-owned `meeting-main` worktree remains untouched.

## Outcome

No Codex-owned local branch tip is now dependent on one machine for recovery. Current
main remains the only production source of truth, historical evidence remains
available by named remote ref, and obsolete or unproven runtime patches remain out of
the release path.

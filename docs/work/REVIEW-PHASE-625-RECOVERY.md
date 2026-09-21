# REVIEW: Phase 625 recovery checkpoint

> Codex preflight: read `$bluey-ops`; this verdict covers recovery evidence,
> not final product correctness.

**Date:** 2026-09-21

**Reviewer:** independent mainline/recovery audit agent, synthesized by root

**Base:** `ca046c48adcf9d2f082d86245a8e619254817b6a`

## Overall verdict

🔴 **REQUEST CHANGES — NOT MERGEABLE / NOT DEPLOYABLE.**

The 701 original JSON-literal records have verified ownership and actual
completion ordering, unique call/output IDs, and no additional relative-path
literals omitted by the independent scope audit. Two finite Unicode string
literals and one generated test split are separate, documented operations.
This corpus still does not represent the complete source: precursor snapshots,
formatter checkpoints, missing new files, and the generated split must be
reconciled. A passing patch application proves its preimage matches, not that
the resulting application is complete or correct.

## Blocking findings

1. Windows `main.c` is incomplete. The early snapshot is valid but only partial;
   patch `caee4ddfdc44` still expects an absent account-state handler. Never
   insert only one inferred line to hide the larger missing handler.
2. Only 79 literal patches are applied. Remaining mutations and final dirty-path
   inventory must be recovered and reconciled with complete source evidence.
3. The reconstructed tree has no full build/test/Clippy or native runtime proof.
4. The diagnostic parent PR has six verified P1s and fresh CI failures. Do not
   merge that parent separately to get around the successor's review.

## Verification evidence

Rustfmt and diff checks passed for preservation. Fresh PR #38 CI attempt 2 is
real and failed, not budget-blocked: Ubuntu/Windows fail launcher cleanup;
macOS builds then fails an overlay-fixture install-directory test (606 daemon
tests passed, one failed, five ignored). Those counts describe the parent,
not the reconstructed source. See the handoff for exact continuation commands.

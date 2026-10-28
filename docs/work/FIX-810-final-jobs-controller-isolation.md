# FIX-810: Final Jobs test controller isolation and termination bounds

> **Codex preflight:** `bluey-ops`; current source and local evidence are authoritative.

## Issue

Independent pre-execution review found three defects in the prepared final Jobs
workspace recipe. That recipe had not been run; current Rust session88910 is
separate and unaffected. No failed recipe execution is claimed.

## Root Cause

The command killed only the immediate npm/node child and awaited close without
escalation, so descendants retaining pipes could hang it. Its Git calls inherited
owner settings despite sanitized test children. Finally, it hashed only Jobs
bytes while using broader unchanged-source language for dirty scoped files.

## Fix Summary

- Verify that this Node controller is the canonical launcher's private process-
  group leader before spawning workload. On signal, timeout or output bound,
  signal only that exact owned group once, retain the stop handler and escalate
  to group SIGKILL after five seconds. The outer launcher then observes a
  non-success terminal and performs its verified cleanup; no detached subgroup
  is created by the controller.
- Pass the allowlisted private HOME/XDG environment to Git too; suppress system/
  global Git configuration and optional index writes. Keep repository-local
  configuration, and never invoke a remote Git operation.
- Hash every tracked and nonignored untracked file in the declared source scope,
  including new Rust/SQL files and the canonical launcher. Preserve HEAD/status
  checks and label the final event precisely `scopedSourceUnchanged`.
- Hash sequentially rather than opening thousands of scoped files through one
  Promise.all; this avoids per-process file-descriptor exhaustion before gates.

## Files Modified

| File | Change |
|------|--------|
| `docs/work/COMMAND-PHASE-636-FINAL-JOBS-GATES.md` | Corrected unexecuted validation recipe. |

## Edge Cases Handled

- npm/vitest descendants retain pipes or ignore TERM: five-second group escalation
  prevents the controller from hanging indefinitely.
- Self-delivered TERM does not recurse; a stopping guard makes group signaling
  single-entry. Signals never target the invoking owner/launcher group.
- Dirty files changing without a porcelain status transition are caught by hashes.
- Current untracked additive migrations/helper source are not omitted from identity.
- Expanded source inventory uses bounded file access, not unbounded concurrent opens.

## How to Test

Review the exact recipe, verify its Node syntax without running it, then qualify
the group/timeout path with a harmless owned child before final workspace gates.
Execute the recipe only after88910 is terminal/cleaned. Require expected2071
passes/zero skips, five typechecks/builds, unchanged scoped hashes/HEAD/status,
no owned child left and exact canonical-root cleanup. Final privacy rescan must
include this FIX and the new command document.

## Known Limitations

Harmless timeout observer48144 passed: inner expected exit137 in8352ms;
controller63313, child63346 and grandchild63347 absent, exact root absent and
parent empty. Both descendants ignored TERM/retained the pipe; actual reviewed
run()/stopOwnedGroup code forced bounded group termination. Final live workspace
gate14411 remains pending, including explicit Chromium process/open-file checks.
Canonical launcher group
semantics were checked at scripts/run-bluey-tests.sh:1135–1150; no launcher source
was changed. This is a validation-controller correction, not product acceptance,
new production authority or proof of full Linux cloud runtime execution.

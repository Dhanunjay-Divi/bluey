# FIX-805: Private Linux runner socket exceeded macOS limit

Preflight: `bluey-ops`; task-owned recovery also used `codex-storage-archive`.
Current source/local handoffs are authority; no SSD was used.
Status: diagnosed, corrective qualification pending.

## Issue

Harness `e5532431`, actual session10495, reached `vm_start` then exited1 with
`cleanup_unproven_state_retained`. No VM, image build or addon smoke passed.

## Root Cause

Lima2.2.1's reserved SSH socket path, including the private root, `/lima/`, profile
and `/ssh.sock.1234567890123456`, was105 bytes. macOS requires strictly less
than UNIX_PATH_MAX104. Lima rejected this before creating an instance.

The initial harness discarded output and overwrote the primary command failure
with cleanup failure. Lima's useful cause was stdout, so stderr-only logging
would not correct diagnosis. This is a qualification-harness defect, not a
production host, Dockerfile, candidate or employer-application defect.

## Fix Summary (pending qualification)

- Shorter private `/l` with unchanged profile randomness; preflight byte budgets
  for the known longest SSH/Docker sockets before any startup.
- Separate primary/cleanup failures; bounded create-new0600 failure stdout AND
  stderr, with only closed metadata in public reports.
- Preserve active/uncertain VM state. After full shutdown/deletion proof, retain
  only bounded diagnostics/owner marker, not large caches, snapshots or disks.

## Files Modified

| File | Change |
|---|---|
| `jobs/scripts/run-nonprod-linux-runner-tests.mjs` | Socket and private diagnostic/cleanup gates |
| `jobs/scripts/run-nonprod-linux-runner-tests.test.mjs` | Byte-budget, separation and retention regressions |

## Initial failure and recovery evidence

Exact retained root `/private/tmp/bluey-jobs-linux-p634-nNTvpE`, inode160653427,
uid501/mode0700, profile `p634-a06f2b1d67835ef8bd3f`. Canonical root was removed.
Locked diagnostic78399 reproduced the105-byte pre-instance rejection. Finally,
exact-profile stop returned0/not running, both private inventories were empty,
and no VM process, instance configuration or PID log existed.

Before recovery deletion, root/marker/inode/uid/mode and empty inventories were
revalidated; lsof found no open files. Private `colima delete --force --data`
returned0/ignored the nonexistent VM. After another immediate identity/open-file
check, only that task-created331MiB cache/Git snapshot/sparse disk state was
removed. Its path is absent and free space returned to about39GiB. These public
downloads/copies can be recreated; no user source, database, rollback, shared
cache, document, Codex history or unrelated profile was removed. All four HQ/
Pinky profiles remain stopped and the global Docker context remains `default`.

This specific evidence-backed recovery did not weaken automatic cleanup rules.

## How to Test

Run focused pure units and independent review, commit corrected admitted inputs,
then run the actual private-HOME/shared-lock/canonical Linux procedure anew.
Require VM/resource/image/native and cleanup gates, not a shorter string alone.

## Known Limitations

Actual Linux qualification remains failed/pending until that run passes. CI/
BuildKit, amd64, signed release, cloud/API lease, receipt, capacity and latency
remain unproven.

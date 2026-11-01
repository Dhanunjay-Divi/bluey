# FIX-846: Retain bounded listener failure evidence before private VM cleanup

Codex preflight: `bluey-ops`; native runtime and cleanup stay parent-owned.

## Issue and root cause

Attempt74516 reached listener observation and correctly failed closed on a new
UDP record. The error retained only its code; snapshots were discarded before
cleanup, so the parent could not distinguish a VM listener from ambient process
churn. A policy change based on that unknown would not be evidence-backed.

## Fix Summary

Share strict delta analysis between admission and diagnostic summarization.
Before admission, retain only per-transport delta counts, recorded-VM PID
membership count, baseline-PID-only count, unknown-PID count and an opaque
identity hash. Validate closed keys, count sums/bounds and hashes in the final
failure report. Clear it after successful admission so unrelated later errors
cannot carry stale classification. Do not publish raw endpoints, PIDs, FDs,
executable paths, arguments or environment. Do not add extra stdout events.

No exemption was added: every new UDP record still denies; every new TCP record
still requires the exact recorded VM PID and loopback management port.

## Files Modified

`jobs/scripts/run-nonprod-managed-runner-vm-tests.mjs` and its dedicated test.

## Verification and actual results

Parent syntax/14 pure tests pass; independent review finds no P0-P3 issue.
Fourth actual attempt30106 terminated1 at the listener gate with complete
cleanup proof. Its TCP delta1 was unknown-PID; UDP delta5 comprised three
baseline-PID-only records and two unknown-PID records, zero recorded-VM records.
These categories are not attribution authority: baseline PID reuse/exec change
and separate private helper processes remain unresolved. No D0 pass is claimed.

Frozen776-source manifest:
`ec17a7db1a4a8b0acd6bbcbe465c12172a18342835f0070be8e724309a6e64e1`.
Journal SHA-256:
`585afe0e336fe74e0dfc97a183a5a07d03cb2a50d523f23d967fd590deef98cf`.
Parent separately retained a stable76443-byte private hostagent log snapshot
with no automatic-forward attempt; SHA-256
`14f768734c744b65cc177bda4a05f5dba020dc84aa853155ed9f4649d1a20926`.
That snapshot does not convert the failed listener gate into a pass.

## Known limitations / next bounded work

macOS `ps comm` and `ucomm` returned only `node` for the parent's own process;
neither is actual executable-image authority. No heuristic PS exemption exists.
Parent verified the public native libproc path/UID/parent/microsecond-birth APIs
on its own PID using root-owned system Python. A fixed producer and tests are
being implemented separately; no listener policy depends on it yet. Pinned
Lima helper PID/config/management-listener mapping is under read-only review.
New/changed/unknown or VM-attributable UDP remains denied until explicitly
reviewed evidence supports the exact management boundary.

# FIX-843: Private VM preflight accepts the observed macOS socket protocol

Codex preflight: `bluey-ops`; current Jobs source and local handoff are authority.

## Issue

The first Phase639 compatibility attempt (parent session30976) stopped before
`vm_start`, at `source_snapshot`, with `listener_record_malformed`. No VM boot
was attempted. The canonical launcher exited1 and removed its run root/parent.

## Root Cause

The initial parser modeled `lsof -F pn` as only process/name fields. The installed
macOS tool also emits mandatory file-descriptor records. Its same-UID UDP output
included129 records, connected numeric endpoints and twelve literal unbound
`*:*` sockets; a64-record global limit and positive-port-only grammar were also
incorrect. The old unit fixtures reproduced the assumed format, not the tool.

## Fix Summary

Parse exact bounded process/file/name groups with unique descriptors, complete
endpoint validation and explicit unbound state. Capture at most64KiB/1024 global
records. Preserve descriptor and endpoint identity in snapshot deltas; never
discard an unrecognized record. Unchanged ambient sockets are not VM exposure.
Every new UDP record remains forbidden. New TCP listeners still require the
recorded private VM PID, exact loopback management port, and no connection or
unbound state; the permitted management delta stays bounded to64 records.

## Files Modified

- `jobs/scripts/run-nonprod-managed-runner-vm-tests.mjs`
- `jobs/scripts/nonprod-managed-runner-vm.test.mjs`

## Verification

Parent executed all13 pure tests: pass, zero failures/skips. Parent also passed
actual TCP12-record and UDP129-record parsing, including twelve unbound records.
Raw ambient endpoint/process metadata was not published. Independent re-review
and the next actual VM compatibility attempt remain separate requirements.

First-attempt journal SHA-256:
`a58f1ff3905055b3755d92299b5ea589fed23b9403c51d2555bc818ce86cccb1`.
After the harness had proved empty private inventories/socket absence, its
retained state was independently checked against captured device/inode/marker
hash and contained only the owner marker. Parent unlinked that exact marker and
removed the empty directory; final absence passed. No database, VM disk,
existing profile, rollback artifact or other task data was removed.

## Known Limitations

Parsing and static clearance do not prove VM boot, guest architecture, resources,
isolation, runtime startup or an application receipt. Unknown cleanup still
retains evidence and fails rather than silently deleting it.

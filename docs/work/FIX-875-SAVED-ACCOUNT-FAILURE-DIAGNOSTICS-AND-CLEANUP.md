# FIX-875: Saved-account failure diagnostics and known-negative cleanup

Preflight: `bluey-ops`; current code and local evidence authoritative.

## Issue and evidence

The original saved-account controller's default journey passed9/9. Its human
session visibly retained a synthetic prepared application after reload, but
failed a post-Finish HTTP read with generic `workspace_status`. That label was
shared by account, workspace and resume reads, and response status was not
retained. The actual endpoint/status/cause remains unknown; do not infer token
expiry, an API outage, or an interactive pass from the visual observation.

The private outer controller treated `inner_refused` as an unknown protocol
event and sent TERM to the canonical launcher while its normal failure cleanup
was in progress. This left an inactive, marker-bound owned run root. The
original cleanup claim remains failed. A separate canonical stale-recovery
invocation subsequently terminated0, removed that exact old root, and allowed
the verified empty private parent to be removed. No manual recursive deletion.

## Correction

`jobs/scripts/nonprod-http-read.mjs` is a small test-only helper. Human read
failures expose only a closed operation name, numeric HTTP status, raw byte
count/SHA256 and finite API/fixture liveness. No response body, token, URL,
candidate value, PID, command or stderr is emitted. The response cap uses raw
bytes, not decoded string length. Existing untagged error names and positive
report schema/authority remain unchanged.

`jobs/portal/nonprod/browser-api.integration.mjs` tags the seven relevant human
reads and emits the redacted event before cleanup. The private controller
validates that schema and admits only an exact, already-admitted negative
`inner_refused`; it waits for natural code1, checks root/parent/PID/process and
source absence/equality, records negative cleanup separately and still fails.
Malformed or unknown protocol retains containment. No success is synthesized.

## Verification

Canonical isolated launcher, closed environment, actual terminal0: four Node
syntax checks and14 tests (five new HTTP-diagnostic cases plus nine unchanged
report-validator cases),0failed/0skipped. Temporary run root was removed by the
launcher; the empty private parent was verified and removed separately.

New tests cover401/429/500 across finite operations, raw-byte digest/redaction,
invalid operations before requests, oversized/non-JSON responses, legacy and
positive reads, and child metadata exclusion. Independent read-only review
accepted the exact helper/test/integration/controller hashes with no remaining
P0-P2 findings. Fresh controller76260 terminated1: default9/9 passed with
cleanup; human7/8 named cases reached zero-effects after account/resume/packet
reload and isolation, then failed with `signal_sigterm` during final cleanup.
No HTTP-refusal event occurred; the original HTTP failure did not reproduce.

The outer negative branch now has actual evidence: natural launcher code1,
signal null, no protocol failure or controller interruption, followed by exact
root/parent/PID/related-process absence and source equality. Controller success
stayed false while `cleanupVerified` was true. Journal
`phase641-saved-75450f38bec4.jsonl` SHA256
`891bdcaebb9ccc226412b27e447f005d85cbe252dc439452d3cb01c41f30004f`.

## Limits and next gate

The new controller's natural negative cleanup is verified separately from
the still-failed positive human journey. Shutdown signal diagnosis is pending;
do not infer the sender from this trace or call7/8 a pass. Historical full
Rust1942/strict Clippy proof remains pinned to the
original778-file source, not relabelled to qualify these JavaScript changes.
No real application/email, candidate upload, provider call or production change.

## Successor observation

Actual61749, controller81ead295…, completed default9/9 plus human8/8 without
HTTP or interruption refusal; its private controller still failed after child
cleanup. Exact parser replay proved a separate13-versus8 case-mixing bug, fixed
under FIX882 with53 passing closed fragment probes and independent review.
Journal84fc7089… preserves the failed overall result with cleanup/source true.
Do not retroactively mark this controller run successful or attribute the old
SIGTERM to the new deterministic counting failure. Fresh full execution remains
required for the corrected controller.

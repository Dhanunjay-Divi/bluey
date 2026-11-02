# FIX882 — Separate saved-account bootstrap and interactive case sequences

Preflight `bluey-ops`; Phase641 test orchestration only, no product-source or
production change. Current source/local evidence remains authoritative.

## Issue and root cause

Actual61749 default9/9 passed with full cleanup. Its human journey passed8/8,
including prepared packet/API/PostgreSQL reload, isolation, zero effects and
child cleanup, but the private controller still refused after child completion.
No HTTP-refusal or interruption event occurred. The failed controller's retained
journal `phase641-saved-d19d0e8382d8.jsonl` SHA256 is
`84fc70890a1c20d0e328ec7eb013d99c9e340e930eddf0f2d0f1b114800ed1b1`.

The private parser appended five common `browser_case_passed` bootstrap events
and eight `human_browser_case_passed` events to one array, then compared all13
with the expected8 human cases. Replaying the exact hash-bound parser against
the retained actual stdout reproduced this deterministic mismatch without
starting any service. This explains this run's post-validation failure, not the
earlier unreproduced SIGTERM or original HTTP error. Neither historical failure
is relabelled as solved or successful.

## Correction

Keep default's exact9 cases. In human mode collect the exact five bootstrap
cases separately from the exact eight interactive cases; require both orders,
counts and names, reject missing/duplicate/reordered/unknown/cross-mode cases.
Do not ignore the bootstrap events or reduce the required human cases. Cross-
stream chronology remains enforced by the frozen child's sequential execution,
not inferred solely from two independent array comparisons.

Also record only one finite refusal-stage name for post-validation failures,
covering child/PG shutdown, case sequences, listeners/PIDs/open files, report,
modules/projection/source/controller readback. Outer protocol validates exact
keys/closed stage; no raw error, token, candidate value, URL, command or PID is
added. All positive/no-mutation/source/cleanup gates and natural code1 negative
cleanup remain unchanged. Unknown/malformed protocol still fails containment.

## Verification and limits

Preserved predecessor private controller SHA81ead295…; corrected controller
SHA`b959337fbe6be885ce9eb357dbb71322283a25508c3cfbf1767f8beb3ec5364b`.
Actual canonical fragment-probe run terminated0:53 passed/0failed/0skipped,
including actual13-event replay to strict5+8, unchanged default9, negative
sequences,14 interruption reasons and15 refusal stages. Root/empty parent were
removed. Syntax checks passed. These are private test-controller probes, not
API/browser/runner runtime proof. Independent read-only review accepted one
fresh supervised run.

The corrected full run30919 then terminated0 on unchanged780-file source manifest
`3e69003748a80af3aac1d9b5f9ea85a402e43a8733cec27b309e427b9993b7c6`.
Private evidence `phase641-saved-39113f5930ac.jsonl` SHA256
`07fc2f091544ef55819ef1d9e4f25cd73da05c0dab88f4731ea0c5e816a5fa1a`
records default9/9 and interactive8/8, both natural exit0. The operator used the
actual native browser to refresh the synthetic match, prepare Factual/Review
First, reload the Applications page and observe the persisted prepared packet,
then select Finish synthetic test. API/PostgreSQL isolation and zero effects
passed. Both modes proved owned processes/listeners absent, PostgreSQL stopped,
exact root/parent cleanup and source unchanged; outer control-home cleanup also
passed. This is actual saved-account synthetic runtime evidence, not installed-
dependency qualification, cloud auto-apply or production proof. The retained
failed predecessor remains failed; no original SIGTERM explanation is inferred.

No production account, candidate document, provider, real application/email,
image/startup qualification or release authority follows from this correction.

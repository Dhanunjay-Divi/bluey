# IMPL: Phase640 — Read-only provider-exposure evidence

> **Codex preflight:** `$bluey-ops`; current source/local evidence is authority.

## Scope

**Does:** implement the already-planned test-only read-only cost-evidence
prototype from accepted checkpoint32ca on a separate feature worktree. Reuse
actual database/receipt seams, correct retention assumptions and define acceptance.

**Does NOT:** claim a verified report yet, measure COGS, claim a cost advantage, change
billing, advance D2 before D1 in Phase639, query owner data or enable effects.

## Files Created / Modified

| File | Action | Purpose |
| --- | --- | --- |
| `docs/rounds/ROUND-640-JOBS-COST-EVIDENCE.md` | New | Exact scope/order/acceptance |
| `docs/work/COST-PHASE-638-COMPETITIVE-ACCEPTANCE.md` | Corrected | Fixed31-day retention vs configurable admission window |
| `server/src/db/jobs_provider_cost_holds.rs` | Test-only source | Bounded read-only provider-exposure prototype, runtime pending |
| `server/src/db/jobs/customer_data.rs` | Test-only source | Stored-metadata receipt validation and inline lifecycle cases |
| `server/src/db/jobs/tests.rs` | Test-only visibility | Reuse existing lifecycle/PG/snapshot helpers without changing behavior |
| `docs/work/FIX-859-release-guard-controller-cross-branch-count.md` | New | Preserve and reconcile cross-branch test-count controller failure |

## Build & Test

The first complete source draft was formatted successfully (three direct
rustfmt inputs plus `cargo fmt --manifest-path server/Cargo.toml --all -- --check`).
The corrected draft adds16 SQLite/one configured-PG cases. The first paragraphs
below preserve the pre-execution source-review checkpoint; the owner-resumed
execution record follows. Old1916-server/Phase639 evidence is historical context,
not new proof.

Independent review found P1 relational resume binding and P2 pre-materialization
text bounds (FIX856/857). Sol's corrections are now source-frozen and formatted;
independent re-review confirms no remaining P0-P3 source finding. SQL suppresses
oversized UTF-8 text before transport and streaming consumers charge the shared
budget before owned string copies. Both-backend relational-null/mismatch and
SQL-suppression negatives plus cumulative SQLite/output bounds are authored.
Do not treat formatting or authored tests as source/runtime acceptance.

Original corrected formatted SHA-256 values (run39075):

- Holds: `3f3ad4172e9f75105367003037779867011d1cab200ab717d38edc2a01028402`.
- Customer data: `8a24407ca3868cd7f4b282c2b863aa11a91a3aebfae2fbeeb12fbd2177f633f0`.
- Test-helper visibility: `7e746b3a219b328508bb9d43cc970ad6defcd3b1a9af10d15ea20fda3931f67d`.

The reviewer confirmed the private report has only inline test callers. FIX858
therefore narrows its30 new configuration attributes to `cfg(test)`, avoiding
an unused integration-library root without suppressing lint or adding an API.
This final narrowing was freshly formatted; strict Clippy remains unexecuted.

Fresh launcher self-tests completed with exit0 (session48297); the intentional
cleanup-negative branch returned39 before the overall success. Its residue
assertion passed. This is launcher qualification, not Rust/report execution.
Prospective privacy passed for all13 owned paths using a disposable Git index:
2,986 tracked paths /2,702 text files, with the real index untouched. The source
closure is778 files with manifest
`5d0a37d8afac19b0cec963f18aa93627d62e51446b0da9512b78d169e159bde1`.
This branch deliberately excludes Phase639's four uncommitted source additions;
its closure is not interchangeable with the separate782-file image closure.

Host free space remains about30GiB, below the unchanged32-GiB heavy-gate
preflight. No heavy command or new VM was dispatched. Phase639's782-file
source manifest remains `0f4e9b016e4a97d477f0dc13b928b9793340fe0f04cf23da3e7015819a16da32`;
its real index/generated portal are clean and owned VM/launcher roots absent.

Continuation verification: lightweight actual Node release suite47632 completed
37/37, zero failures/skips/cancellations/todos, child exit0. The one-off outer
controller returned1 because it incorrectly expected Phase639's40-test scope;
its failed journal is preserved. Independent review verified that Phase640's
test file equals32ca byte-for-byte, while the separate held Phase639 adds three
dynamic PAX cases. This is narrow child-pass evidence, not outer qualification;
see FIX859. No redundant rerun was performed to turn the wrapper green.

Canonical-launcher session12833 completed exit0 for CI guard self-tests, schema
parity (114 tables/95 indexes), and business-messaging simulator containment.
Its exact empty owned parent was checked and removed after launcher cleanup.
These are lightweight source/containment checks, not compilation or execution
of any of the17 new Rust cases. No Cargo/PG/VM/provider command ran.

Final prospective privacy includes FIX859:14 owned paths,2,987 tracked paths,
2,703 scanned text files, no issues and real index unchanged. Independent v2
controller review confirms only the intended scope/count fix; v2 is unexecuted.
Final owned static/release/self-test/privacy temporary roots are absent.

## Capacity disposition

Historical pre-resume disposition; superseded by the execution record below.

The same32-GiB admission condition has persisted through at least four goal
turns, including the earlier two-turn capacity audit and the two subsequent
cost/source/guard evidence turns. Independent low-resource work was completed
instead of immediately declaring a global block. It is now exhausted for this
frozen batch: no live compiler, PG, VM or worker is being awaited; the required
next step is actual compilation/database/image verification. Latest available
space is about30.1GiB. No task-owned disposable residue supplies the missing
capacity; owner files/system swap/rollback data must not be deleted to proceed.
Free at least3GiB locally, then re-admit exact source under the existing shared
machine lock. Do not weaken the guard, create paid resources or treat further
untested features as release qualification. Full Jobs scope remains unfinished;
no deployment, effect, push, merge or feature-flag authority is inferred.

## Owner-resumed execution and FIX862

The owner freed capacity. Actual39075 on778/5d0a passed fresh formatting, strict
both-feature all-target Clippy and all16 new SQLite cases. Its required-fresh
PostgreSQL case ran against an observed private PG17 cluster, then failed at
fixture insertion: indexed usage_events.request_id exceeded the native index
row bound (SQLSTATE54000). The full server stage did not run. All owned PG/
compiler/process/root cleanup and unchanged-source checks passed; the failed
journal remains preserved (see FIX862), not relabelled successful.

Only the inline adversarial fixture changed for FIX862, preserving both indexes
and requiring exact error/state/catalog proof. Five PostgreSQL stored-row paths
remain actual report tests; one indexed field instead has storage-rejection
plus explicitly projection-only evidence. SQLite retains six stored-row paths.
Independent review has no P0-P3 source finding. Current customer-data SHA:
`ea7591cbc98aa3c07aaac1857656356722a0213ede4463df62ba2abe0597e07f`.
Current778-file manifest:
`91ef6fae3f3e49c8a98c3566076abcbc9c8aa84069d02f036847069f3a06cdc0`.
Replacing just that hash reconstructs5d0a; holds/test-helper hashes are unchanged.

Actual successor74379 is running under the same shared lock/canonical launcher,
with the unchanged32-GiB admission and reviewed three-literal controller repin.
Fresh formatting, strict all-target Clippy and all16 exact SQLite cases passed.
The required real PostgreSQL case passed1/1 with zero failures/ignores/skips;
the controller verified the physical private PG17 cluster and then its PID and
postmaster-file absence before the no-PG full suite. Full-server execution and
terminal launcher/source/cleanup checks remain unverified until observed. Journal:
`phase640-rust-f1e300ef2190.jsonl`. No provider, employer or production action.

Current prospective privacy passed with16 owned paths,2,989 tracked paths and
2,705 text files using disposable index tree
`23b5c55154e31e524e6d4441d3403148f9b79c09`; the real index remained empty.
That is a privacy/static checkpoint, not full-server or release qualification.

### Terminal runtime qualification (supersedes the live label above)

Successor74379 completed with exit0 and controller success. All required stages
passed on the exact778/91ef source: formatting, strict both-feature all-target
Clippy,16 exact focused SQLite cases, one separately verified physical PG17 case,
and the complete all-target server gate. Actual full-suite counts were
`[1784,0,1,14,1,1,2,113,2,1,5,9]`, totaling1,933 with zero failures or ignored
tests. The environment-conditional PG case in the full suite is not credited as
another physical run. Exact source-after, module hashes and scoped status matched.
The launcher observed and removed the exact root and parent; all owned PIDs and
related processes were absent. The real index remained empty.

Durable journal:
`/Users/uno/.codex/visualizations/2026/08/04/019fcdb9-9999-70c3-a063-c83542b6b5f0/phase640-rust-f1e300ef2190.jsonl`
SHA-256: `a996e087c253d653dc54c7b449a571b5ea3a720c950f9519ef9b7ce93e8842d3`.
Raw stdout/stderr siblings are retained. This qualifies the bounded test-only
prototype, not a public report, historical completeness, invoice COGS, verified
employer delivery, a competitive price, managed startup or production release.

After terminal/source/cleanup proof, applied the independently reviewed
`jobs/UNIT_ECONOMICS.md` narrative correction only: fixed pruning threshold vs
admission window, held plus settled exposure counted once, and explicit limits on
metadata consistency/completeness/success. No price, model assumption or billing
policy changed. The runtime journal remains tied to its pre-documentation source;
the new778-file manifest is
`8ded31dc47fdb954570489b46bdd492dccf8cb3355e05d2d284bb475d6e8ecb1`.
Replacing only the UNIT_ECONOMICS hash with its prior HEAD bytes reconstructs
91ef exactly; all other777 inputs, including all executable source, are unchanged.
Durable proof `phase640-post-runtime-doc-inverse.json` has SHA-256
`27622175021a788d9f2d54864357c7587841e35adfb4d636ae91281b3f0eb73d`.

Post-documentation prospective privacy passed for all17 owned paths,2,989 tracked
paths/2,705 text files, with real index untouched (checkpoint tree
`f6440109ea3d78a1d51ad25066ed48267196bdd7`). Fresh canonical launcher self-test19325
completed exit0 including its intentional cleanup-negative status39 and final
residue assertion. These add static/launcher qualification, not new provider,
delivery or cost-superiority evidence.

## Deviations from Plan

The source-seam audit found fixed retention, overwritten usage cost, no independent
usage provenance and unavailable original-request fingerprint material. The
report must label those limits rather than invent data. Existing guarded PG
unit-case infrastructure can replace a new example/launcher without reducing
the actual database/immutability acceptance conditions.

## Known Follow-ups

- Strict source gates, both database fixtures and full server gate passed as above;
  no source/lint warning suppression was introduced.
- Preserve the independently reviewed evidence limits in any later public report.
- Original gate requirements remain in `COMMAND-PHASE-640-VERIFICATION.md`.
- Continue Phase639 actual image retry only after its host-capacity gate passes.
- Read-only original PR-chain refresh is in `AUDIT-JOBS-PR-CHAIN-20261101.md`;
  old published-head checks are not current local candidate evidence.

### Narrow next cost candidate (source audit, not implementation or savings)

Exact-input resume-plan reuse already exists in
`server/src/api/jobs_resume_generation.rs::generate`, and deterministic
provider-first selection exists in
`jobs/automation/src/execute.ts::ProviderFirstAdapterRegistry.resolve`. Do not
duplicate those optimizations or claim their savings without measurement.

The runner currently retains Chromium during pre-submit `needs_input`, and
`ActiveExecutionLease.scheduleHeartbeat` keeps renewing authority. A separate
bounded phase could cold-park only certified pre-submit `provider_review`
interventions after a configured deadline. Existing encrypted checkpoints,
`closeBrowserExecution`, `closeActiveRun` and `restoreCheckpoint` are building
blocks, not an already-safe parking path: recovery currently eagerly restores
pending interventions, so parking needs its own durable state.

Preserve the24-hour response window, exact account/identity/resume/packet/
release/checkpoint/result bindings, fresh lease/fence and approved-field
refill/readback on resume. Serialize parking against resume/purge/submit.
Exclude OTP/takeover and all `finalSubmitAttempted`/uncertain cases. Acceptance
must prove browser-child absence, restart durability, repeated park/resume
idempotency and zero employer POSTs on stale-authority/submit races. Measure
context-ms, RSS/CPU time, resume latency and provider-dispatch counts. Resource
reduction alone is not billed runner savings or complete recurring COGS.

## Review Checklist

- [x] Test-only source diff independently reviewed at exact frozen hashes.
- [x] No public route, owner data, effects or billing changes.
- [x] Both-backend actual fixtures and unchanged-state proof pass.
- [x] Unknown cost/completeness is not silently zero or invoice COGS.
- [x] Independent static review, prospective privacy and launcher self-test pass.
- [x] Actual bounded report-run cleanup and required runtime evidence complete.
- [ ] Whole-product cost, managed-runner and production-release gates complete.

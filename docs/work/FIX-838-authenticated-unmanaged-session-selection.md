# FIX-838: Authenticated unmanaged workers use managed-only session lookup

Preflight: bluey-ops/current AGENTS and local handoff; security review, SPARC
acceptance boundaries and bounded native-team ownership. No unreviewed framework
CLI/hooks/runtime is installed or executed.

## Issue

Full current-source gate15847 exited1 after Cargo's library stage failed101:
1763 passed,1 failed,0 ignored in2584.32s. Exact panic at tests.rs20737:10:
unmanaged authenticated worker may start with an absent managed tuple: NotFound.
Later all-target binary/integration/example stages were not reached. This is a
real compatibility regression, not a successful full gate or an assertion to relax.

## Root Cause

An authenticated worker context and a managed workflow tuple are different facts.
The worker APIs represent an authenticated unmanaged request as outer Some with
inner managed input None. Phase638's new session selectors tested only the outer
context, then invoked exact managed START-session resolution with None. A genuine
unmanaged run has no START history, so the strict helper correctly returned
NotFound. The unmanaged start test reproduced this exact path; the same selector
mistake is source-confirmed in SQLite/PG claim and SQLite/PG final-submit.

The direct generic claim fixture used an absent outer context and therefore did
not expose the real authenticated worker claim path. The existing PG managed/
unmanaged pairing test checks classifier components in shadow tables, not this
complete claim→ATS PhaseA→authorized start→PhaseB/replay journey. An environment
serialization issue is a separate fixture hygiene risk, not the deterministic cause.

## Fix Summary and invariants

- Four production selectors only. Actual managed input retains the exact durable
  START-session resolver. An authenticated claim with no managed input first calls
  the existing strict unmanaged classifier; only then may it use the historical
  aligned run/session ID. Keep later classifier/current-authority revalidation.
- Final-submit already classifies every input-absent request strictly. Change only
  its following selector to choose managed START lookup for inner Some, not merely
  an authenticated outer context. Do not move terminal click-started replay.
- Preserve context-free generic helpers, the strict session helper's None behavior,
  managed effect authorization, all signature/token/fence/volume/ATS/source/beta/
  account/deletion/capacity checks and transaction/lock order. Complete or partial
  managed history still denies missing managed input; absence is not fallback consent.
- Do not fabricate a START alias or remove managed history to make a legacy fixture
  pass. Legacy success must have genuinely never-managed history.

## Files Modified / ownership

- Parent: `server/src/db/jobs/execution_leases.rs`, this record/command/IMPL/handoff
  and changelog. Scoped rustfmt/whitespace pass; no runtime acceptance yet.
- Existing fixture worker: `server/src/db/jobs/tests.rs` only, retaining previous
  changes/default fixtures; new SQLite and actual configured-PG authenticated claim,
  start and exact replay coverage, plus legitimate managed omission negative coverage.
  Required-PG/fresh-DB/account verification/beta/ordinary lifecycle guards remain.
- Independent reviewer: REVIEW638 only; no runtime. Parent owns all builds/tests.

## Edge Cases Handled / acceptance

1. Transport authentication without a managed tuple must not require managed START.
2. Worker IDs/volume must still bind. Managed or partial managed history with omitted
   tuple must fail unchanged, never gain the aligned-session legacy fallback.
3. Real prepared legacy claim/ATS preflight progresses to one consumed binding,
   click_started, one capacity/canary reservation and exact proof; replay changes none.
4. Existing managed distinct-session admission, wrong-worker/omission denials,
   prepared retry/recovery and immutable terminal replay must remain valid.
5. Serialize the legacy test with global environment mutators, without calling that
   a production fix or substituting serialization for the four-selector correction.

## How to Test

Production/regressions are implemented, not runtime qualified. The shared managed
fixture now denies omitted managed CLAIM input before the legitimate managed claim,
with exact16-family-plus-application state equality, in both SQLite and configuredPG.
The correct managed claim then succeeds. This resolves the independent review's
additional P2 omission-coverage request without changing production authority.
New test filters (prefix db::jobs::tests::):

- sqlite_unmanaged_authenticated_worker_claim_submit_and_replay_accept_absent_managed_authority
- postgres_unmanaged_authenticated_worker_claim_submit_and_replay_accept_absent_managed_authority_when_configured
- postgres_managed_cloud_final_submit_omission_is_denied_without_mutation_when_configured

Snapshot volume/runtime families without an account column use whole tiny isolated
fixture tables, never an invalid account predicate or an owner database. Existing
context-free fixture defaults remain unchanged. Tests.rs frozen SHA256
bf0a534f3b9408ed57f3e90bed1b70b71bbf703b6887c73107bde463dc1a4004.
After source freeze and
independent review, run prospective-index privacy/whitespace, focused legacy/managed
tests and real isolated-PG regressions, then exact current full server/Jobs gates.
Use canonical temporary root/Cargo/home/data and shared mac-build lock, with PG
fresh per exact case and absent for the broad SQLite suite. Current test evidence
must be observed, not inherited from the failed e959 run.

## Observed failed source and cleanup

HEAD409498589b8d2e62b5ef54ebdee1372aeb8d005f;774-source
e9591654d8b9b91d7c58976887579f655dd0bd38f043fccb299bec9422cb19a3;
status6754505e7726bb0a1fb2c63b1c8f37767b2afe525e6f16218ed86c5dfa0f85d5
matched parent before/after. Formatting/strict both-feature all-target Clippy passed
1m59s; all41 actual isolatedPG cases and30 focused cases passed BEFORE the library
failure. PG exited0/PIDfile absent before SQLite. Exact root
run.1793228345.7208.3564214310 removed, parent/index empty,37GiBfree. No test runtime
remains live. These dated passes are not acceptance of subsequent corrected bytes.

## Known Limitations

PostgreSQL runtime compatibility failure is source-equivalent, not separately
observed yet. New actual PG regressions must establish it. All corrected source,
managed preservation, broader protocol, browser/native-cloud/provider/cost/latency
and release gates remain open. No push, production flags, employer application,
mail, browser/VM/Temporal or owner-database action is authorized by this correction.

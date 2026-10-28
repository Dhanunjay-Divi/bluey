# IMPL: Phase 636 — Jobs canonical employer reservations

Preflight: `bluey-ops`, security/SPARC/team workflow, Round636 and current code.
Base: `cd72989d`. Status: local correctness conditionally accepted; pre-format gates passed, final-byte/release holds remain.

Does: close the confirmed display-alias reservation hole using signed canonical
employer identity in both databases and current packet/execution authority.
Does not: infer corporate identity, rewrite history, change frozen v3 contracts,
claim a real submission or enable/deploy anything. Exact ownership, acceptance
and legacy/reconciliation requirements are in Round636. Chronological diagnostic
history follows; the closeout at the end records current results. No production
flag, external action, CI/release artifact or overall-goal completion is implied.

## First diagnostic attempt (not a causal alias reproduction)

Session45270 exited1 after compiling and running one fresh-PG case. It failed
the FIRST reservation with `current_execution_authority_denied`, before the
alias-denial assertion. Frozen test-file blob was
`50ad01bca4d780afa4d1e7c6e97a3c83a9240510`; production code was unchanged.
PG reported clean shutdown/server exit; exact owned root was absent and parent
empty afterward. This result is retained as a failed fixture, not evidence that
the alias path was reached or fixed.

Test-only correction makes the new alias fixture supply explicit `full_time w2`
before signed source publication (the existing packet-metering fixture retains
its original default). This follows FIX804's canonical employment/engagement
contract. New prechecks require ordinary queue eligibility with reason-code-only
diagnostics and BOTH complete current execution authorities under H/M/ATS/D
locks before A reserves. No production guard, receipt, policy or reservation is
fabricated or relaxed. Fresh execution is pending.

Second diagnostic session44231 also exited1 without reaching the alias boundary:
the new queue precheck reported `career_track_policy_review_required`. Test blob
`432502900b82ed251e08235cf61b0f4a8b56d2d0` stayed fixed. PG stopped cleanly,
exact root was absent and parent empty. Adding the second verified identity
advances account-input authority, so the original Track/packet cannot silently
follow that change. The fixture now uses normal `upsert_track` review for both
Tracks after final setup, then factual re-preparation before ordinary approval.
Neither diagnostic attempt is counted as the alias reproduction.

## Causal red reproduced

Fresh session81168 exited1 with the expected test assertion: `same signed
canonical employer aliases must not reserve twice`. It compiled in2m33s and
the selected case ran6.17s. Frozen test blob
`ef291fceaf70ef0e7bb91c5a822abab96ee04f49` was unchanged at terminal; no
production code was edited. Both ordinary queue checks and both complete
current execution-authority prechecks passed, A reserved successfully, and
both signed projections/receipt equalities were current with equal canonical
employer IDs/domains and distinct display keys immediately before B. B then
reserved successfully, causing only the intended assertion to fail.

This is actual private PostgreSQL reservation evidence, not an employer submit.
The wrapper reported clean PG shutdown/server exit; exact root was absent and
parent empty afterward. Schema/core implementation may now begin; no green
or repaired-product claim yet.

## Implementation under qualification (2026-10-28)

The schema worker supplied additive066/044 bindings, active ID/domain unique
indexes and shape/transition guards, with no guessed backfill. Parent wired
all reservation readers and four writers, current admission/exact replay,
eligibility, packet pre-meter validation, current execution and final ATS
context consumption. The existing display index and frozen Phase B metering
hash remain unchanged. Workspace mutable-input hashing includes new fields.
No production side effect or credential use is involved.

Separate schema review found one regression-guard omission: SQLite dot and
domain byte-length predicates were not pinned. Worker added both pins/removal
mutations and an `intranet` negative; parent reran parity114 tables/95 indexes
and CI guard self-tests successfully. Real migration/core runtime proof remains
pending. New pure tests cover aliases/rotation, distinct employers/name guard,
conflicting mappings, active legacy holds, released history, exact replay,
released renewal, invalid fields and legacy serialization. These are not yet
reported as passing.

First focused gate81433 exited1 at Cargo discovery/compilation, before any test
ran: the new `include!` file used module-inner `//!` documentation after existing
items (E0753). Parent confirmed all19 frozen source hashes unchanged, clean PG
shutdown, exact root absent and parent empty, then corrected only those three
lines to ordinary comments. No authority/test rule was changed. The initial
attempt is a build failure, not a runtime pass; fresh focused execution follows.

Focused replacement31836 exited0 on an unchanged19-file source manifest. The
actual private-PG alias case passed1/1 (6.12s), PG stopped cleanly/PID file was
absent, then private-home SQLite/no-PG execution passed core8/8, migration3/3
and registration1/1. Strict all-target Clippy passed in1m49s. The original
same-employer second reservation is now rejected specifically as a duplicate,
with attempt/application/metering snapshots unchanged. Parent observed the exact
root absent, parent empty and no owned process path afterward.

This is preliminary focused proof, not full-suite/concurrency/final-source or
release acceptance. Static review also identified five older direct active-
reservation fixtures that lack the new required binding. Those fixtures must
be corrected (or explicitly seeded as historical legacy cases), not allowed
through a weaker production trigger. Concurrency/own-binding drift/packet and
full regressions remain pending.

## Irreversible history and expanded regression pass

The five direct active-row fixtures now supply complete synthetic bindings,
using the application's frozen binding where the fixture exercises actual
authority. Quota/capacity/snapshot-only fixtures explicitly label synthetic
metadata and do not claim signed source authority. New database guards prevent
`submitted` from reopening and require `side_effect_unknown` to reconcile before
execution reentry. SQLite's update trigger is refreshed transactionally on
replay without rewriting any history. Core checks mirror those transitions.
An account-level history hold also detects submitted/unknown applications with
an absent or released attempt slot before new preparation, packet or execution.

Separate review found a P1 in the first history-hold implementation: preparation
excluded its own existing application, then could overwrite the very terminal
history requiring protection. Current source rejects terminal state or a
retained submitted timestamp before tailoring, includes the current job in the
history check, and rejects a terminal expected revision before commit writes.
The existing final compare-and-swap still fences concurrent outcome changes.
The reviewer accepted the correction statically; runtime proof remains pending.

Added tests cover normal nonterminal reprepare, same-job submitted/unknown
history with absent/released slots, unchanged resumes/receipts/billing,
concurrent PostgreSQL API admissions, independent database unique-index racing,
and stored attestation drift at replay/packet/current-execution boundaries.
Friendly API conflicts expose no account identity or candidate field values.
Fresh parity (114 tables/95 indexes), CI guard self-tests, formatting and diff
checks passed. Source is frozen under canonical focused session78617 with a
23-file SHA-256 manifest.

Session78617 exited1. Seven of eight selected actual PostgreSQL cases passed,
each in its own fresh database: all five canonical-employer cases (alias,
binding drift, API race, database constraint race and same-job terminal-history
preservation), the prior entitlement-before-reservation reread race and the
workspace lockable snapshot fixture. The existing ATS signed-canary concurrency
case failed because its losing serializable transaction returned PostgreSQL
`40001` instead of the asserted typed capacity denial. That failure is retained,
not skipped or accepted as the desired denial. No later SQLite/Clippy command
ran. All23 source hashes were verified unchanged after terminal; PG reported
clean stop/server exit, the exact root was absent, parent empty and no owned
runtime process remained. The source freeze is released for bounded diagnosis.

Additional private-PG legacy migration-body/replay coverage is being added in
a transaction-local synthetic schema: no runtime namespace or historical row
is rewritten. The ATS serialization failure is under separate read-only review;
final-source focused/full acceptance remains pending.

Session97759 also exited1 and was cleaned. On its unchanged23-file manifest,
all10 actual private-PG cases passed, including migration legacy/replay,
cross-application history holds, the formerly failing canary capacity race and
typed retry classification/bounds. After clean PG stop, core10, SQLite
migration5, registration1, API2, terminal-history1 and prior email/Track guard1
passed (20 SQLite/pure cases). The next quota fixture failed during its first
preparation; no later ATS SQLite cases or Clippy ran. The error rendered the
first Track-review message, but that alone does not establish the root cause:
the fixture had added handcrafted canonical-positive metadata to an unsigned
posting. Composed authority remains required, especially once a peer attempt
exists. A bounded worker is repairing this test with normal reviewed profile/
Track and signed synthetic source/integrity publication, not weakening guards.

The direct retry test proves typed error classification, bound exhaustion and
domain propagation; its returned CapacityUnavailable is a stub, not an actual
capacity read. The ordinary canary race passed97759. To strengthen that proof,
its current successor now holds the ATS lock until two real advisory waiters
are observed, ensuring both serializable snapshots precede any winner commit.
That later deterministic interleaving was subsequently exercised in87312 as
recorded below; it is not yet final-source acceptance.

## Combined gate — session87312 (failed quota fixture)

Source freeze resumed after the deliberate stop of21814. The reviewer confirmed
the quota seed mode is now caller-explicit: only the old unsigned name-only
fixture may opt into synthetic identity; both signed quota paths hard-require a
valid frozen receipt. The corrected fixture publishes every employer through
the ordinary signed synthetic source/integrity lifecycle and uses explicit W-2
categories. No production authority or expected quota/duplicate denial changed.

Session21814 was deliberately terminated through the verified canonical launcher
PID after review, before any selected test executed. Exit143 is not a test pass.
PG stopped/server exited; all23 hashes matched; exact root absent, parent empty,
no owned process remained. Its compile/discovery interruption is retained.

Session87312 selects the prior30 configured cases from IMPL629, replacing their
three canonical exact filters with the nine-case canonical prefix and adding
the two ATS retry/concurrency cases:38 actual private-PG cases in38 fresh DBs
are expected. Then it runs24 focused SQLite/core/API cases, strict all-target
Clippy, formatting and full all-target server tests. Expected counts are not
observed passes. Source23-file hashes/status/HEAD must remain unchanged until
terminal. Do not edit source or start a second heavy run while this handle is
live. No final-source/runtime or release acceptance yet.

Terminal update:87312 exited1 after all38 actual private-PG cases and20 focused
SQLite/core/API cases passed. The quota fixture then failed in its distinct-
employer loop with canonical_employer_mapping_conflict. No later gate ran.
The shared signed-integrity test helper hardcoded every employer ID to
employer-acme while allowing distinct domains; the new production mapping guard
correctly rejected those conflicting records. Current successor adds an
explicit employer-ID option only in test-support helpers, preserves the old
default for every existing entrypoint, and publishes distinct IDs/domains for
the distinct quota employers. It does not mutate stored authority or bypass
signed imports. All23 frozen hashes matched at terminal; PG stopped cleanly,
exact root absent, parent empty and no owned process remained. The fixture
successor must be freshly qualified, including two extra source manifest files.

Exact controller (offline/locked, generated loopback PG credentials, private
child HOME, central machine lock and canonical cleanup launcher):

```bash
/opt/homebrew/bin/python3 -B /Users/uno/Projects/shared-runners/runner_schedule.py with-machine-lock --machine mac-build -- env BLUEY_TEST_RUN_PARENT=/private/tmp/bluey-test-runs-phase636-current-rust2-20261028 scripts/run-bluey-tests.sh -- env -u NODE_OPTIONS -u NODE_PATH /opt/homebrew/bin/node --input-type=module <<'NODE'
import assert from "node:assert/strict";
import {spawn,spawnSync} from "node:child_process";
import {readFile,lstat,realpath} from "node:fs/promises";
import {createHash} from "node:crypto";
import {join} from "node:path";
const runRoot=await realpath(process.env.BLUEY_TEST_RUN_ROOT);
const files=[
"server/src/db/jobs.rs","server/src/db/jobs/attempt_employer_identity.rs",
"server/src/api/jobs.rs","server/src/db/jobs/tests.rs",
"scripts/run-bluey-tests.sh","jobs/scripts/run-nonprod-postgres-tests.mjs",
"server/src/db/jobs/eligibility.rs","server/src/db/jobs/applications.rs",
"server/src/db/jobs/workflow_commands.rs","server/src/db/jobs/execution_authority.rs",
"server/src/db/jobs/execution_leases.rs","server/src/db/jobs/workspace.rs",
"server/src/db/jobs/ats_certification_authority.rs","server/src/db/jobs/postgres_local_authority_tests.rs",
"server/src/db/mod.rs","infra/sqlite/server-runtime/066_jobs_attempt_employer_identity.sql",
"infra/postgres/server-runtime/044_jobs_attempt_employer_identity.sql",
"jobs/scripts/check-jobs-schema-parity.mjs","jobs/scripts/ci-guards-self-test.mjs",
"server/Cargo.toml","server/Cargo.lock","Cargo.toml","Cargo.lock"];
async function hashes(){return Object.fromEntries(await Promise.all(files.map(async file=>[file,createHash("sha256").update(await readFile(file)).digest("hex")])));}
const before=await hashes();
function git(args){const result=spawnSync("/usr/bin/git",args,{encoding:"utf8",timeout:10000});assert.equal(result.status,0);return result.stdout.trim();}
const head=git(["rev-parse","HEAD"]);
const scope=["server","crates/cue-core","jobs","infra",".github","Cargo.toml","Cargo.lock"];
const state=git(["status","--porcelain=v1","--untracked-files=all","--",...scope]);
const allowed=new Set(["PATH","USER","LOGNAME","LANG","LC_ALL","TERM","SDKROOT","DEVELOPER_DIR","CARGO_TARGET_DIR","CARGO_INCREMENTAL","TMPDIR","TMP","TEMP","SQLITE_TMPDIR","BLUEY_TEST_RUN_ROOT","BLUEY_DB_PATH","BLUEY_SERVER_DB_BACKEND","BLUEY_DATA_DIR","BLUEY_CONFIG_DIR","BLUEY_RUNTIME_DIR","BLUEY_LOG_DIR","CUE_DATA_DIR","CUE_CONFIG_DIR","CUE_RUNTIME_DIR","CUE_LOG_DIR"]);
const clean=Object.fromEntries(Object.entries(process.env).filter(([key])=>allowed.has(key)||/^CARGO_PROFILE_(DEV|TEST)_/.test(key)));
const rustup=await realpath(process.env.RUSTUP_HOME||join(process.env.HOME,".rustup"));
const parentEnv={...clean,HOME:process.env.HOME,CARGO_HOME:process.env.CARGO_HOME||join(process.env.HOME,".cargo"),RUSTUP_HOME:rustup};
const features="integration-test-support,business-messaging-simulator-test-support";
const recipe=await readFile("docs/work/IMPL-PHASE-629-JOBS-NONPROD-INTEGRATION-VALIDATION.md","utf8");
const priorFilters=[...recipe.matchAll(/--case-filter '([A-Za-z_][A-Za-z0-9_:]{0,255})'/g)].map(match=>match[1]);
assert.equal(priorFilters.length,30);assert.equal(new Set(priorFilters).size,30);
const canonical="db::jobs::postgres_local_authority_tests::postgres_canonical_";
assert.equal(priorFilters.filter(name=>name.startsWith(canonical)).length,3);
const filters=[...priorFilters.filter(name=>!name.startsWith(canonical)),canonical,
"db::jobs::ats_certification_authority_tests::postgres_concurrent_canary_phase_b_enforces_signed_concurrency_cap_atomically",
"db::jobs::ats_certification_authority_tests::postgres_phase_b_local_transaction_retries_only_typed_40001_and_is_bounded"];
assert.equal(filters.length,30);assert.equal(new Set(filters).size,30);

const crypto=["--config","profile.test.package.curve25519-dalek.opt-level=3","--config","profile.test.package.ed25519-dalek.opt-level=3","--config","profile.test.package.sha2.opt-level=3"];
let active;let interrupted=false;
const handlers=["SIGHUP","SIGINT","SIGTERM"].map(signal=>{const handler=()=>{interrupted=true;active?.kill(signal)};process.on(signal,handler);return[signal,handler]});
async function run(command,args,env,expectedCount){
 assert(!interrupted,"interrupted");
 const child=spawn(command,args,{env,stdio:["ignore",expectedCount===undefined?"inherit":"pipe","inherit"]});active=child;let output="";
 child.stdout?.on("data",chunk=>{process.stdout.write(chunk);output+=chunk;if(output.length>65536)child.kill("SIGTERM")});
 const ended=await new Promise(resolve=>{child.once("error",()=>resolve({code:null,signal:"spawn_error"}));child.once("close",(code,signal)=>resolve({code,signal}))});
 active=undefined;assert.deepEqual(ended,{code:0,signal:null},"focused_command_failed");
 if(expectedCount!==undefined)assert(output.includes("test result: ok. "+expectedCount+" passed; 0 failed; 0 ignored;"),"focused_test_count_mismatch");
}
try{
 console.log(JSON.stringify({event:"canonical_employer_current_gate_started",baseCommit:head,sourceFiles:before}));
 await run(process.execPath,["jobs/scripts/run-nonprod-postgres-tests.mjs","--pg-bin","/opt/homebrew/opt/postgresql@17/bin",...filters.flatMap(name=>["--case-filter",name]),"--","cargo",...crypto,"test","--offline","--locked","--manifest-path","server/Cargo.toml","--lib","--features",features],parentEnv);
 await assert.rejects(lstat(join(runRoot,"nonprod-postgres/data/postmaster.pid")),{code:"ENOENT"});
 const home=await realpath(join(runRoot,"nonprod-postgres/child-home"));const meta=await lstat(home);assert.equal(meta.uid,process.getuid());assert.equal(meta.mode&4095,448);
 const env={...clean,HOME:home,CARGO_HOME:join(home,".cargo"),RUSTUP_HOME:rustup,BLUEY_SERVER_DB_BACKEND:"sqlite"};
 assert(!Object.hasOwn(env,"BLUEY_TEST_POSTGRES_URL")&&!Object.hasOwn(env,"BLUEY_DATABASE_URL")&&!Object.hasOwn(env,"BLUEY_JOBS_REQUIRE_POSTGRES_TESTS"));
 console.log(JSON.stringify({event:"canonical_employer_private_sqlite_gates_started",postgresStopped:true}));
 const cargo=[...crypto,"test","--offline","--locked","--manifest-path","server/Cargo.toml","--lib","--features",features];
 await run("cargo",[...cargo,"db::jobs::attempt_employer_identity_tests::","--","--test-threads=1"],env,10);
 await run("cargo",[...cargo,"db::sqlite_migration_replay_tests::attempt_employer_","--","--test-threads=1"],env,5);
 await run("cargo",[...cargo,"db::postgres_migration_tests::attempt_employer_identity_migrations_are_registered_and_additive","--","--exact","--test-threads=1"],env,1);
 await run("cargo",[...cargo,"api::jobs::tests::employer_","--","--test-threads=1"],env,2);
 await run("cargo",[...cargo,"db::jobs::tests::canonical_employer_terminal_history_cannot_be_erased_by_same_job_preparation","--","--exact","--test-threads=1"],env,1);
 await run("cargo",[...cargo,"db::jobs::tests::application_email_and_track_cannot_bypass_company_guard","--","--exact","--test-threads=1"],env,1);
 await run("cargo",[...cargo,"db::jobs::tests::eligibility_enforces_company_and_daily_limits_from_active_reservations","--","--exact","--test-threads=1"],env,1);
 await run("cargo",[...cargo,"db::jobs::ats_certification_authority_tests::certified_cloud_context_runs_phase_a_then_atomic_phase_b","--","--exact","--test-threads=1"],env,1);
 await run("cargo",[...cargo,"db::jobs::ats_certification_authority_tests::canary_capacity_limits_deny_atomically_across_activation_day_account_and_concurrency","--","--exact","--test-threads=1"],env,1);
 await run("cargo",[...cargo,"db::jobs::ats_certification_authority_tests::phase_b_local_transaction_never_retries_untyped_or_domain_errors","--","--exact","--test-threads=1"],env,1);
 await run("cargo",["clippy","--offline","--locked","--manifest-path","server/Cargo.toml","--all-targets","--features",features,"--","-D","warnings"],env);
 console.log(JSON.stringify({event:"canonical_employer_full_rust_started",baseCommit:head,postgresStopped:true}));
 await run("cargo",["fmt","--manifest-path","server/Cargo.toml","--","--check"],env);
 await run("cargo",[...crypto,"test","--offline","--locked","--manifest-path","server/Cargo.toml","--all-targets","--features",features],env);
 assert.equal(git(["rev-parse","HEAD"]),head);assert.equal(git(["status","--porcelain=v1","--untracked-files=all","--",...scope]),state);assert.deepEqual(await hashes(),before);
 console.log(JSON.stringify({event:"canonical_employer_current_gate_finished",baseCommit:head,realPostgres:38,fullAllTargets:true,pureCore:10,sqliteMigration:5,registration:1,api:2,sqliteRegressions:6,strictAllTargets:true,sourceUnchanged:true}));
}finally{for(const[signal,handler]of handlers)process.off(signal,handler);}
NODE
```

## Successor gate — session39121 (failed source-cap fixture)

After87312 terminated and cleanup/source identity were verified, separate review
accepted the explicit signed employer-ID fixture option. Existing fixture entry
points keep employer-acme unchanged; only quota-specific distinct employers use
the new option. The same38 actualPG/24 focused/strictClippy/fullalltarget recipe
is now running with25 frozen source files. No result is inferred from an earlier
snapshot. Do not edit source, commit or start another heavy controller until
this exact session is terminal.

Reproduce the controller above with run parent
`/private/tmp/bluey-test-runs-phase636-current-rust3-20261028` and add
`server/src/db/jobs/job_integrity_authority.rs` plus
`server/src/db/jobs/production_positive_authority_fixture.rs` to its frozen file
manifest. These are the only controller changes from the recorded command.

Read-only current PR-chain evidence is separate:
`AUDIT-PHASE-636-PR-CHAIN-20261028.md`. It changed no remote branch/workflow.

Privacy snapshot57444 exited0 while39121 remained live:2,863 indexed paths /
2,587 text files passed. A fresh private alternate index included HEAD, current
tracked changes and the nine owned new Phase636 files; inherited REVIEW628 was
not added. The real index/status stayed unchanged and the exact privacy scratch
root was identity-checked, removed and observed absent. Later documentation
edits require a final closeout rescan; this is a dated snapshot, not release proof.

Session39121 exited1:38 actual private-PG cases and20 focused SQLite/core/API
cases passed, but the quota test's per-employer board enrollment hit the
separate per-Track source cap. No Clippy/full suite ran. All25 hashes matched;
PG stopped, exact root absent, parent empty and no owned process remained.
The bounded test-only correction uses one synthetic multi-employer board with
distinct exact jobs; signed employer IDs/domains and all ten reservations plus
duplicate/daily-limit assertions stay intact. Source limits were not relaxed.
Separate review accepted this correction statically.

Current session16096 is live/pending. To reduce repeated runtime work during
fixture iteration, it runs that SQLite quota case first using an owned,
credential-free HOME/cache directory. Only if it passes does the ordinary
private-PG wrapper create its cluster and execute38 cases. The remaining23
focused cases, strict Clippy and full all-target suite follow in the same
canonical run root. Exact recipe:
`COMMAND-PHASE-636-CURRENT-QUALIFICATION.md`. Preserve the25-file source freeze.
No final-source green or production approval is claimed.

Session16096 quota-first preflight passed1/1 (15.59s) after2m29s compilation.
This exercises normal signed imports under one board, ten distinct retained
employer slots, same-company rejection and the exact daily-limit assertion.
The existing wrapper subsequently reported its private PG17/pgvector cluster
ready;38 actual PG cases and remaining focused/Clippy/full gates are still
pending. This preflight pass is not an aggregate or release acceptance.

Further16096 observation: all38 actual PostgreSQL cases passed in fresh private
databases, then PG stopped cleanly and the controller checked its PID file
absent. The23 post-PG focused SQLite/core/API tests also passed, making24
focused cases with the earlier quota preflight. This includes the corrected
cloud-context and signed-canary capacity fixtures. Strict all-target Clippy is
now live; full all-target tests have not yet completed. Keep source frozen;
final manifest/status/HEAD and owned-root cleanup verification remain pending.

16096 strict all-target Clippy passed with warnings denied (1m49s), then server
formatting passed. The full all-target test command is now executing its1,732
library cases before remaining binaries/integration/example targets. No full
suite total or terminal pass is yet claimed. Sampled host free space at this
checkpoint was36GiB; that is not a measured peak/space-usage attribution. The
source freeze and final cleanup/hash requirements continue unchanged.

The live full suite reported one failure in
`queue_admission_tests::packet_commit_recomposes_current_effect_eligibility_in_both_dialects`.
Source remains frozen and the suite continues, so the final Rust failure report
and total are not yet available. Read-only inspection of the exact string
checks found the old guard still requires LIMIT1 in each relevant-reservation
loader; both now use LIMIT2 for owned row plus blocking peer. All other old
required fragments remain present, and canonical ID/domain/attestation fields
are present. This is a concrete stale source-contract mismatch, not permission
to weaken runtime constraints. Confirm the terminal panic, then replace the
obsolete bound assertion with explicit own/peer/canonical/legacy and unchanged
daily-budget/row-lock assertions, followed by fresh verification. No source
correction has been made while this run is live.

Session16096 is now terminal, exit1. Its full library result was1726 passed /
6 failed /0 ignored in2502.74s; binaries/integration/examples did not execute
after the library failure. The final reports confirm one obsolete `LIMIT 1`
source assertion and five shared execution-fixture employer-ID/domain conflicts,
not a relaxed or bypassed product guard. Prior38 actual private-PG cases,
24 focused cases, strict all-target Clippy and formatting passed on that snapshot.
Independent lightweight parity (114 tables/95 indexes) and CI-guard self-tests
also passed. All25 frozen hashes and HEAD matched after terminal; the exact
owned root was absent, its parent empty and observed owned PIDs absent.

Only after that verification, test-only corrections were made as described in
FIX-808 and FIX-809. They strengthen the source query contract and give distinct
cloud fixture employers distinct signed IDs while preserving existing defaults,
local fixtures and every lease/receipt/capacity assertion. No runtime product SQL,
authority check or migration changed for these six failures. Fresh executable
qualification and separate review are pending; no phase or release green is claimed.

Independent post-edit static review accepted FIX808/809 with no actionable
finding; scoped Rust formatting and diff checks passed. Successor session88910
is live using `COMMAND-PHASE-636-FIXTURE-REQUALIFICATION.md`: six exact former
failures first, quota seventh, then38 actual private-PG cases,23 remaining
focused cases, strict all-target Clippy, formatting and full all-target Rust.
This gives30 focused cases if all execute/pass. The same25-file source manifest
is frozen until actual terminal and cleanup/source revalidation; no pass inferred.

88910 early preflight passed all six former failures1/1 each, then the quota
case1/1 (15.48s), after2m32s initial compilation. These are seven focused passes
on the corrected snapshot, with unchanged lease/receipt/capacity and quota
assertions. The ordinary private PostgreSQL17/pgvector wrapper reported ready
and began its case-stage compilation. Actual38-case PG completion, remaining
23 focused cases, strict Clippy/full all-target Rust and final cleanup/source
identity verification are still pending. Preserve the live source freeze.

88910 subsequently passed38/38 actual PostgreSQL cases in fresh private databases,
including canonical alias races/binding drift, immutable-history protections and
the ATS typed40001/concurrency cases. The wrapper reported clean server exit,
and the controller checked the postmaster PID file absent before SQLite. All23
remaining focused cases passed, giving30 focused passes including the seven
preflights. Strict all-target Clippy is now live. Full all-target Rust, terminal
manifest/status/HEAD checks and owned-root cleanup are still pending; no final
acceptance or production authority follows from these intermediate gates.

88910 strict all-target Clippy subsequently passed with warnings denied (1m50s),
then server formatting passed. The full all-target command is now live and began
its1732 library tests. No full total, terminal/source/cleanup acceptance or
subsequent Jobs gate is claimed yet. Keep the source frozen until actual terminal.

Current privacy snapshot9365 exited0 during the live Rust run:2868 indexed paths /
2592 text files passed. A private alternate index included HEAD, tracked owned
changes and all14 owned new Phase636 files, including both controller documents,
FIX808/809 and HANDOFF636; inherited REVIEW628 stayed excluded. The child used
a closed environment without owner credentials/global Git config. Real staging/
status stayed unchanged; exact scratch root
`/private/tmp/bluey-privacy-phase636-m3LUo5` was identity-checked, removed and
observed absent. Later evidence-document edits require final closeout rescan;
this is current source/privacy evidence, not full-suite or release acceptance.

Session88910 is now terminal exit0. Complete all-target Rust passed1872 tests,
zero failed/ignored:1732 library (2503.49s),1 binary,134 integration and5 guarded
example tests. Configured-only SQLite early returns are not additional actual-PG
proof; actual private PG remains the separate38/38 selection. All30 focused
cases, strict all-target Clippy (1m50s) and formatting passed. The controller's
final25 hashes/HEAD/scoped status matched; parent independently rechecked all25
hashes/HEAD, exact root absence, empty parent, observed PIDs52059/59570 absent
and no process executable under that exact owned parent. Temporary build/data
storage was removed by the canonical launcher, not retained as a release artifact.

Pre-execution review of the final Jobs recipe found bounded-cleanup, private Git
environment and source-label/inventory defects, documented/corrected in FIX810.
Sequential hashing also avoids unbounded descriptor use. Node syntax and final
static review passed. Harmless timeout qualification48144 then exited0 as an
observer: the inner command intentionally exited137 after its exact group timeout;
elapsed8352ms, controller/child/grandchild63313/63346/63347 absent, exact proof
root absent and its parent empty. This is expected negative cleanup evidence,
not a failed product test or browser-cleanup proof.

Final Jobs workspace session14411 is now live under the same canonical launcher
and Mac-heavy lock with a fresh private HOME/Chromium/npm cache/portal output.
Exact command: `COMMAND-PHASE-636-FINAL-JOBS-GATES.md`. It freezes760 scoped
tracked/nonignored-untracked source files; starting manifest SHA256
`4a3bdad0200dfb10bfcef7e1d459664b728933d1ef7560d2d82210e7e196b122` and status SHA256
`e0e48bdee6ef3ba22cfb3bf1fccb8f7f0e838c6347b59bec8e2f43a813693913`.
All source must stay frozen until this run is terminal. Its2071 tests/zero skips,
five typechecks/builds, explicit private-browser process/open-file inventory,
final identity/cleanup, final launcher self-test/privacy/review remain pending.
FIX810 and the final Jobs command bring owned new Phase636 files to16; privacy
snapshot9365 predates those two documents and subsequent evidence edits.

Workspace14411 is terminal exit1 at its Node-summary parser, after npm test
exited0. Raw output proves Node8/7/17 passes/zero skips and Vitest804/219/308/
321/387 passes:2071 tests, no skipped test. Later typecheck/build/browser-file
inventory gates did not run and are not inferred. All760 scoped hashes plus
manifest/HEAD/status matched independently after terminal; exact root absent,
parent empty and no process executable under that private root. FIX811 records
the actual cause: Node24.1 emitted spec summaries (`ℹ pass N`) rather than TAP.
The parser now recognizes both anchored formats and requires exact total/pass/
zero-skip vectors. Eight pure parser checks passed (actual output, both synthetic
formats, five negatives), Node syntax passed, and product sources did not change.
The full workspace recipe has been restarted only after verified terminal/cleanup.
FIX811 brings owned new files to17; final privacy must include it.

Successor workspace55179 exited0. All2071 tests passed with zero skips (Node
8/7/17; automation804, Browser219, runner308, workflows321, portal387). All five
typechecks and builds passed; portal output stayed inside the private root with
config/dotenv discovery disabled, not web/jobs. Schema parity114tables/95indexes
and CI-guard self-tests passed. Before cleanup, explicit private-browser process
and lsof inventories were both zero. The760-file manifest/HEAD/scoped status
matched internally and independently afterward; exact root absent, parent empty
and no process executable under it. Fresh launcher self-test26934 exited0,
including its intentional negative cleanup cases and residue checks.

Privacy73988 passed2871 indexed paths/2595 text files with all17 owned new files,
inherited REVIEW628 excluded, real index unchanged and exact scratch
`/private/tmp/bluey-privacy-phase636-Edvqeg` removed/absent. Later closeout record
edits require a final rescan before staging/commit. No heavy process remains.

Observed nonfatal baseline warnings are NOT hidden or called zero-warning:
automation documents.test.ts exercises an unconfigured PDF.js standard-font
path, also present in production assertUsablePdf; read-only diagnosis found the
offline font-data configuration gap. Portal build has one503.99kB minified chunk
(131.61kB gzip), alongside a346.21kB index chunk (99.74kB gzip). Its importer/
ownership must be measured before attributing it to initial load or choosing a
split. These remain bounded follow-up fidelity/performance work before release;
no latency/capacity benchmark, real cloud runtime or production readiness claim
follows from passing correctness gates.

Final read-only residue inventory found all13 task-named Phase636 canonical
parents empty (three red, five current-Rust, four focused and one final-Jobs
parent). No retained per-run Cargo/database/Chromium root remains there. This
does not attribute whole-host disk changes or inspect/delete another task's data.

Final independent AI source review is conditional accept for this bounded local
correctness change, with no remaining P0–P3 finding in the employer-identity/ATS
retry diff. Runtime gates are parent-observed, not independently rerun or manual
QA. The P2 offline-font gap and P3 unmeasured chunk ownership remain release-
quality holds, along with all external/runtime/production gates. Preserve the
qualified code in local commits only; no push/PR mutation/merge/deploy is granted.
Run a final post-record privacy scan before staging/commit. Broader goal remains
active; warning remediation and actual runner/receipt proof are not completed here.

The checkpoint's prospective staged whitespace check then found one extra EOF
blank line in each NEW migration; it stopped before commit. FIX812 removes only
that final extra newline per file. Exact comparison to the original staged bytes
proved equality after trailing-newline normalization and a one-byte difference
each; schema parity passed114tables/95indexes again. No SQL token or historical
migration changed. Full88910/55179 results remain dated PRE-FORMAT snapshots,
not exact current-byte qualification. The next full gate must cover the formatted
bytes plus pending warning fixes. Local checkpoint is conditional, not release
approval. Future heavy preflights must whitespace-check the entire prospective
index, including owned untracked files. Owned new files now18; privacy must include
FIX812. No staged-check failure was suppressed or called a pass.

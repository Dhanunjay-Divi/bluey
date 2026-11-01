# Phase638 FIX838 corrected-source Rust qualification

Executed as89202, TERMINAL1. Format/strict Clippy and original red passed; the new
SQLite regression failed its snapshot account-column assumption. PG/full were not
reached. FIX839/new successor retain the failure and exact cleanup/source caveat.
The recipe below is the original admitted89202 command, not a green result.
This is a separate successor to failed gate15847.
Its five focused SQLite cases run before the PostgreSQL cases and long full
suite; any failed assertion stops later parent stages. Formatting/strict
both-feature all-target Clippy remain first. Real PostgreSQL tests use one
fresh private database per exact test; only after wrapper exit0, observed43/43
and absent postmaster PID may the SQLite focused/full all-target stages run.
The PG wrapper itself preserves its existing continue-to-summary behavior for
selected failures, but a nonzero wrapper exit prevents all later stages.

Same physical canonical worktree/HEAD409498589b8d2e62b5ef54ebdee1372aeb8d005f.
Frozen774-source manifest83ea8ce0d6c1061f3b6ccefcd10d3924f2f7d24f9c9818a0b1ae660a1fdd647a.
Tests.rs SHA256bf0a534f3b9408ed57f3e90bed1b70b71bbf703b6887c73107bde463dc1a4004.
Never substitute the old e959 pass/failure record for current-byte evidence.

Preserve all41 prior actualPG cases, add the two new actualPG FIX838 cases first.
Consolidate exactly the three existing module-qualified postgres_local_ selectors
to their common module-qualified prefix: three cases remain three cases, two
selector slots are recovered, and the32selector cap stays unchanged. Expected43
actual cases, all counted/discovered by the wrapper with missing/overlap guards.
No production-source test exclusion or raised wrapper limit.

Both support features and crypto test optimization profiles remain mandatory.
Five upfront cases cover original red, true authenticated unmanaged claim/start/
replay, complete managed claim omission/final-submit/wrong-worker denial and
legitimate managed start/replay, plus distinct managed START and legacy session
classifier preservation. Seven inherited preflights and23 inherited focused cases
remain. Minimum all-target floor1875 reflects three added tests, not a predicted
current summary vector; actual target counts must be observed. ConfiguredPG cases
returning early in broad SQLite are NOT actualPG acceptance; the43 actual cases
are reported separately.

Use shared mac-build lock and fresh private temporary Cargo/data/home/runtime,
no provider credentials or inherited PGURL. Minimum32GiB before dispatch; no
cleanup of other workspaces/shared artifacts. No browser/VM/Temporal/provider/
production/employer/email or owner DB action. Canonical launcher cleans success,
failure and signals. Later API preservation needs a separate fresh launcher root.

```sh
/opt/homebrew/bin/python3 -B /Users/uno/Projects/shared-runners/runner_schedule.py with-machine-lock --machine mac-build -- env BLUEY_TEST_RUN_PARENT=/private/tmp/bluey-test-runs-phase638-fix838-rust-20261028 scripts/run-bluey-tests.sh -- env -u NODE_OPTIONS -u NODE_PATH /opt/homebrew/bin/node --input-type=module <<'NODE'
import assert from "node:assert/strict";
import {spawn,spawnSync} from "node:child_process";
import {readFile,lstat,realpath,mkdir,symlink,statfs} from "node:fs/promises";
import {createHash} from "node:crypto";
import {join} from "node:path";
const runRoot=await realpath(process.env.BLUEY_TEST_RUN_ROOT);
const capacity=await statfs(runRoot,{bigint:true});assert(capacity.bavail*capacity.bsize>=32n*1024n**3n,"minimum_32GiB_free_before_full_rust_gate");
const sourceHome=join(runRoot,"source-home");await mkdir(sourceHome,{mode:0o700});
const gitEnv={PATH:"/opt/homebrew/bin:/usr/bin:/bin",HOME:sourceHome,GIT_CONFIG_NOSYSTEM:"1",GIT_CONFIG_GLOBAL:"/dev/null",GIT_OPTIONAL_LOCKS:"0"};
const sourceListing=spawnSync("/usr/bin/git",["ls-files","--cached","--others","--exclude-standard","-z","--","jobs","server","crates/cue-core","infra",".github","scripts/run-bluey-tests.sh","Cargo.toml","Cargo.lock"],{env:gitEnv,encoding:"utf8",timeout:10000,maxBuffer:8*1024*1024});assert.equal(sourceListing.status,0);
const files=[...new Set(sourceListing.stdout.split("\0").filter(Boolean))].sort();
async function hashes(){const result={};for(const file of files)result[file]=createHash("sha256").update(await readFile(file)).digest("hex");return result;}
const before=await hashes();
assert.equal(before["infra/sqlite/server-runtime/066_jobs_attempt_employer_identity.sql"],"eb40814a4b6c720d0c17b19ed41d03ec13626f4f117a4a292566fc43560ba3ea");
assert.equal(before["infra/postgres/server-runtime/044_jobs_attempt_employer_identity.sql"],"595f528a06198f6efe8378cca7a7fbbb0685c87e3e9f720c99029903bd07c830");
const sourceManifestSha256=createHash("sha256").update(JSON.stringify(before)).digest("hex");
assert.equal(files.length,774);
assert.equal(sourceManifestSha256,"83ea8ce0d6c1061f3b6ccefcd10d3924f2f7d24f9c9818a0b1ae660a1fdd647a");
function git(args){const result=spawnSync("/usr/bin/git",args,{env:gitEnv,encoding:"utf8",timeout:10000});assert.equal(result.status,0);return result.stdout.trim();}
const head=git(["rev-parse","HEAD"]);assert.equal(head,"409498589b8d2e62b5ef54ebdee1372aeb8d005f");
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
filters.push(
"db::jobs::queue_admission_tests::postgres_cloud_browser_integer_decoder_is_fallible_when_configured",
"db::jobs::runner_volume_purge_tests::postgres_runner_volume_f");
const localPrefix="db::jobs::postgres_local_authority_tests::postgres_local_";
assert.equal(filters.filter(name=>name.startsWith(localPrefix)).length,3);
filters.splice(0,filters.length,
"db::jobs::tests::postgres_unmanaged_authenticated_worker_claim_submit_and_replay_accept_absent_managed_authority_when_configured",
"db::jobs::tests::postgres_managed_cloud_final_submit_omission_is_denied_without_mutation_when_configured",
...filters.filter(name=>!name.startsWith(localPrefix)),localPrefix);
assert.equal(filters.length,32);assert.equal(new Set(filters).size,32);

const crypto=["--config","profile.test.package.curve25519-dalek.opt-level=3","--config","profile.test.package.ed25519-dalek.opt-level=3","--config","profile.test.package.sha2.opt-level=3"];
let active;let interrupted=false;
const handlers=["SIGHUP","SIGINT","SIGTERM"].map(signal=>{const handler=()=>{interrupted=true;active?.kill(signal)};process.on(signal,handler);return[signal,handler]});
async function run(command,args,env,expectedCount){
 assert(!interrupted,"interrupted");
 const child=spawn(command,args,{env,stdio:["ignore","pipe","inherit"]});active=child;let output="";
 child.stdout?.on("data",chunk=>{process.stdout.write(chunk);output+=chunk;if(output.length>4*1024*1024)child.kill("SIGTERM")});
 const ended=await new Promise(resolve=>{child.once("error",()=>resolve({code:null,signal:"spawn_error"}));child.once("close",(code,signal)=>resolve({code,signal}))});
 active=undefined;assert.deepEqual(ended,{code:0,signal:null},"focused_command_failed");
 if(expectedCount!==undefined)assert(output.includes("test result: ok. "+expectedCount+" passed; 0 failed; 0 ignored;"),"focused_test_count_mismatch");
 return output;
}
try{
 console.log(JSON.stringify({event:"phase638_fix838_current_gate_started",baseCommit:head,sourceFiles:files.length,sourceManifestSha256,root:runRoot}));
 const quotaHome=join(runRoot,"quota-preflight-home");await mkdir(quotaHome,{mode:0o700});
 const quotaCargo=join(quotaHome,".cargo");await mkdir(quotaCargo,{mode:0o700});
 const sourceCargo=await realpath(parentEnv.CARGO_HOME);
 for(const directory of ["registry","git"]){
  let cached;try{cached=await realpath(join(sourceCargo,directory))}catch(error){if(error.code==="ENOENT")continue;throw error}
  const info=await lstat(cached);assert(info.isDirectory());assert.equal(info.uid,process.getuid());
  await symlink(cached,join(quotaCargo,directory),"dir");
 }
 const quotaEnv={...clean,HOME:quotaHome,CARGO_HOME:quotaCargo,RUSTUP_HOME:rustup,BLUEY_SERVER_DB_BACKEND:"sqlite"};
 assert(!Object.hasOwn(quotaEnv,"BLUEY_TEST_POSTGRES_URL")&&!Object.hasOwn(quotaEnv,"BLUEY_DATABASE_URL")&&!Object.hasOwn(quotaEnv,"BLUEY_JOBS_REQUIRE_POSTGRES_TESTS"));
 console.log(JSON.stringify({event:"phase638_current_rust_static_gates_started",privateHome:true,noProviderCredentials:true}));
 await run("cargo",["fmt","--manifest-path","server/Cargo.toml","--all","--","--check"],quotaEnv);
 await run("cargo",["clippy","--offline","--locked","--manifest-path","server/Cargo.toml","--all-targets","--features",features,"--","-D","warnings"],quotaEnv);
 console.log(JSON.stringify({event:"phase638_current_rust_static_gates_passed",format:true,strictAllTargetsClippy:true}));
 const fix838Regressions=[
 "db::jobs::tests::unmanaged_cloud_worker_submit_and_replay_accept_absent_managed_authority",
 "db::jobs::tests::sqlite_unmanaged_authenticated_worker_claim_submit_and_replay_accept_absent_managed_authority",
 "db::jobs::tests::managed_cloud_final_submit_boundary_denies_omission_and_wrong_worker_without_mutation",
 "db::jobs::execution_session_authority_tests::sqlite_resolves_distinct_durable_start_session_after_actual_request_start",
 "db::jobs::execution_session_authority_tests::sqlite_checkpoint_legacy_session_contract_requires_no_managed_history"
];
 console.log(JSON.stringify({event:"phase638_fix838_focused_sqlite_started",tests:fix838Regressions.length}));
 for(const filter of fix838Regressions){
  await run("cargo",[...crypto,"test","--offline","--locked","--manifest-path","server/Cargo.toml","--lib","--features",features,filter,"--","--exact","--test-threads=1"],quotaEnv,1);
 }
 console.log(JSON.stringify({event:"phase638_fix838_focused_sqlite_passed",tests:fix838Regressions.length}));
 console.log(JSON.stringify({event:"phase638_canonical_employer_fixture_preflight_started",privateHome:true,noProviderCredentials:true}));
 const fixtureRegressions=[
 "db::jobs::queue_admission_tests::packet_commit_recomposes_current_effect_eligibility_in_both_dialects",
 "db::jobs::tests::execution_lease_excludes_parallel_runs_for_one_browser_profile",
 "db::jobs::tests::execution_receipt_authority_rejects_nonterminal_and_stale_leases",
 "db::jobs::tests::final_submission_authority_matches_cloud_lease_and_local_ticket_exactly",
 "db::jobs::tests::safe_terminal_lease_outcomes_release_unused_protected_capacity",
 "db::jobs::tests::terminal_receipt_authority_retains_exact_protected_capacity"];
 for(const filter of fixtureRegressions){
  await run("cargo",[...crypto,"test","--offline","--locked","--manifest-path","server/Cargo.toml","--lib","--features",features,filter,"--","--exact","--test-threads=1"],quotaEnv,1);
 }
 console.log(JSON.stringify({event:"phase638_canonical_employer_fixture_preflight_passed",tests:fixtureRegressions.length}));
 console.log(JSON.stringify({event:"phase638_canonical_employer_quota_preflight_started",privateHome:true,noProviderCredentials:true}));
 await run("cargo",[...crypto,"test","--offline","--locked","--manifest-path","server/Cargo.toml","--lib","--features",features,"db::jobs::tests::eligibility_enforces_company_and_daily_limits_from_active_reservations","--","--exact","--test-threads=1"],quotaEnv,1);
 console.log(JSON.stringify({event:"phase638_canonical_employer_quota_preflight_passed",tests:1}));
 const pgOutput=await run(process.execPath,["jobs/scripts/run-nonprod-postgres-tests.mjs","--pg-bin","/opt/homebrew/opt/postgresql@17/bin",...filters.flatMap(name=>["--case-filter",name]),"--","cargo",...crypto,"test","--offline","--locked","--manifest-path","server/Cargo.toml","--lib","--features",features],parentEnv);
 const pgEvents=pgOutput.split("\n").flatMap(line=>{try{return[JSON.parse(line)]}catch{return[]}});
 const pg=pgEvents.find(value=>value.event==="nonprod_postgres_cases_finished");
 assert(pg,"actual_private_pg_summary_required");assert.equal(pg.tests,43);assert.equal(pg.passed,43);assert.deepEqual(pg.failed,[]);
 await assert.rejects(lstat(join(runRoot,"nonprod-postgres/data/postmaster.pid")),{code:"ENOENT"});
 const home=await realpath(join(runRoot,"nonprod-postgres/child-home"));const meta=await lstat(home);assert.equal(meta.uid,process.getuid());assert.equal(meta.mode&4095,448);
 const env={...clean,HOME:home,CARGO_HOME:join(home,".cargo"),RUSTUP_HOME:rustup,BLUEY_SERVER_DB_BACKEND:"sqlite"};
 assert(!Object.hasOwn(env,"BLUEY_TEST_POSTGRES_URL")&&!Object.hasOwn(env,"BLUEY_DATABASE_URL")&&!Object.hasOwn(env,"BLUEY_JOBS_REQUIRE_POSTGRES_TESTS"));
 console.log(JSON.stringify({event:"phase638_canonical_employer_private_sqlite_gates_started",postgresStopped:true}));
 const cargo=[...crypto,"test","--offline","--locked","--manifest-path","server/Cargo.toml","--lib","--features",features];
 await run("cargo",[...cargo,"db::jobs::attempt_employer_identity_tests::","--","--test-threads=1"],env,10);
 await run("cargo",[...cargo,"db::sqlite_migration_replay_tests::attempt_employer_","--","--test-threads=1"],env,5);
 await run("cargo",[...cargo,"db::postgres_migration_tests::attempt_employer_identity_migrations_are_registered_and_additive","--","--exact","--test-threads=1"],env,1);
 await run("cargo",[...cargo,"api::jobs::tests::employer_","--","--test-threads=1"],env,2);
 await run("cargo",[...cargo,"db::jobs::tests::canonical_employer_terminal_history_cannot_be_erased_by_same_job_preparation","--","--exact","--test-threads=1"],env,1);
 await run("cargo",[...cargo,"db::jobs::tests::application_email_and_track_cannot_bypass_company_guard","--","--exact","--test-threads=1"],env,1);
 await run("cargo",[...cargo,"db::jobs::ats_certification_authority_tests::certified_cloud_context_runs_phase_a_then_atomic_phase_b","--","--exact","--test-threads=1"],env,1);
 await run("cargo",[...cargo,"db::jobs::ats_certification_authority_tests::canary_capacity_limits_deny_atomically_across_activation_day_account_and_concurrency","--","--exact","--test-threads=1"],env,1);
 await run("cargo",[...cargo,"db::jobs::ats_certification_authority_tests::phase_b_local_transaction_never_retries_untyped_or_domain_errors","--","--exact","--test-threads=1"],env,1);
 console.log(JSON.stringify({event:"phase638_canonical_employer_full_rust_started",baseCommit:head,postgresStopped:true}));
 const fullOutput=await run("cargo",[...crypto,"test","--offline","--locked","--manifest-path","server/Cargo.toml","--all-targets","--features",features],env);
 const summaries=[...fullOutput.matchAll(/test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;/g)];
 assert(summaries.length>=11,"all_target_summary_vector_missing");
 assert(summaries.every(value=>value[2]==="0"&&value[3]==="0"),"full_targets_failed_or_ignored");
 const fullCounts=summaries.map(value=>Number(value[1]));
 const total=fullCounts.reduce((sum,count)=>sum+count,0);assert(total>=1875,"full_target_baseline_shrank");
 console.log(JSON.stringify({event:"phase638_full_server_summary",counts:fullCounts,total,failed:0,ignored:0,configuredPgCoveredSeparately:pg.passed}));
 assert.equal(git(["rev-parse","HEAD"]),head);assert.equal(git(["status","--porcelain=v1","--untracked-files=all","--",...scope]),state);assert.deepEqual(await hashes(),before);
 console.log(JSON.stringify({event:"phase638_fix838_current_gate_finished",baseCommit:head,fix838Sqlite:fix838Regressions.length,realPostgres:pg.passed,fullCounts,total,fullAllTargets:true,pureCore:10,sqliteMigration:5,registration:1,api:2,sqliteRegressions:12,strictAllTargets:true,sourceUnchanged:true}));
}finally{for(const[signal,handler]of handlers)process.off(signal,handler);}
NODE
```

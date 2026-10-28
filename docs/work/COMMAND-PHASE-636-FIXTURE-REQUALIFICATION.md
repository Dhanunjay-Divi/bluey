# Phase636 fixture requalification controller

Successor to terminal session16096: test-only FIX808/809 corrections were
independently accepted statically. Six exact former failures run first, then
the quota case,38 actual private PostgreSQL cases,23 remaining focused cases,
strict all-target Clippy, formatting and full all-target Rust. Expected focused
count is30; no result is inferred before execution. Source freeze covers the
same25 files and current HEAD/scoped Git status. All runtimes/data/targets use
the canonical owned temporary root and cleanup supervision. No production or
external effect is authorized. Preserve this command and observed results.

```bash
/opt/homebrew/bin/python3 -B /Users/uno/Projects/shared-runners/runner_schedule.py with-machine-lock --machine mac-build -- env BLUEY_TEST_RUN_PARENT=/private/tmp/bluey-test-runs-phase636-current-rust5-20261028 scripts/run-bluey-tests.sh -- env -u NODE_OPTIONS -u NODE_PATH /opt/homebrew/bin/node --input-type=module <<'NODE'
import assert from "node:assert/strict";
import {spawn,spawnSync} from "node:child_process";
import {readFile,lstat,realpath,mkdir,symlink} from "node:fs/promises";
import {createHash} from "node:crypto";
import {join} from "node:path";
const runRoot=await realpath(process.env.BLUEY_TEST_RUN_ROOT);
const files=[
"server/src/db/jobs.rs","server/src/db/jobs/attempt_employer_identity.rs",
"server/src/api/jobs.rs","server/src/db/jobs/tests.rs",
"server/src/db/jobs/job_integrity_authority.rs","server/src/db/jobs/production_positive_authority_fixture.rs",
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
 console.log(JSON.stringify({event:"canonical_employer_fixture_preflight_started",privateHome:true,noProviderCredentials:true}));
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
 console.log(JSON.stringify({event:"canonical_employer_fixture_preflight_passed",tests:fixtureRegressions.length}));
 console.log(JSON.stringify({event:"canonical_employer_quota_preflight_started",privateHome:true,noProviderCredentials:true}));
 await run("cargo",[...crypto,"test","--offline","--locked","--manifest-path","server/Cargo.toml","--lib","--features",features,"db::jobs::tests::eligibility_enforces_company_and_daily_limits_from_active_reservations","--","--exact","--test-threads=1"],quotaEnv,1);
 console.log(JSON.stringify({event:"canonical_employer_quota_preflight_passed",tests:1}));
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
 await run("cargo",[...cargo,"db::jobs::ats_certification_authority_tests::certified_cloud_context_runs_phase_a_then_atomic_phase_b","--","--exact","--test-threads=1"],env,1);
 await run("cargo",[...cargo,"db::jobs::ats_certification_authority_tests::canary_capacity_limits_deny_atomically_across_activation_day_account_and_concurrency","--","--exact","--test-threads=1"],env,1);
 await run("cargo",[...cargo,"db::jobs::ats_certification_authority_tests::phase_b_local_transaction_never_retries_untyped_or_domain_errors","--","--exact","--test-threads=1"],env,1);
 await run("cargo",["clippy","--offline","--locked","--manifest-path","server/Cargo.toml","--all-targets","--features",features,"--","-D","warnings"],env);
 console.log(JSON.stringify({event:"canonical_employer_full_rust_started",baseCommit:head,postgresStopped:true}));
 await run("cargo",["fmt","--manifest-path","server/Cargo.toml","--","--check"],env);
 await run("cargo",[...crypto,"test","--offline","--locked","--manifest-path","server/Cargo.toml","--all-targets","--features",features],env);
 assert.equal(git(["rev-parse","HEAD"]),head);assert.equal(git(["status","--porcelain=v1","--untracked-files=all","--",...scope]),state);assert.deepEqual(await hashes(),before);
 console.log(JSON.stringify({event:"canonical_employer_current_gate_finished",baseCommit:head,realPostgres:38,fullAllTargets:true,pureCore:10,sqliteMigration:5,registration:1,api:2,sqliteRegressions:12,strictAllTargets:true,sourceUnchanged:true}));
}finally{for(const[signal,handler]of handlers)process.off(signal,handler);}
NODE
```

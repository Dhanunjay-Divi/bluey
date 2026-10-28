# IMPL: Phase 635 — Jobs current Rust qualification

Preflight: `bluey-ops`, Round635, current code/local handoffs. Base: `863042a0`.
Status: bounded local qualification accepted; privacy and separate AI review passed.

The bounded batch refreshes the older all-target Rust baseline after the guarded
example and shared test-fixture changes. It does not implement a new product or
start any production/provider action. Preserve inherited untracked REVIEW628.

Exact30 configured selections come from the already inspected IMPL629 recipe as
data only: a bounded identifier parser supplies direct argv, never evaluates
document commands. Each gets a new database through the existing private wrapper.
After observed PG shutdown/PID-file absence, a separate allowlisted child uses
the same private HOME/compiler caches and SQLite/no provider/test URL for full
server fmt, strict all-target Clippy and all-target tests. Counts and time are
observations, never inferred from earlier passes.

The prior full baseline took about42 minutes library plus9 minutes HTTP; no
promise of the current duration or latency/SLO follows. Source remains frozen
until the aggregate command terminates. No VM, browser service, overlay or owner
account is started. Phase634 host-listener evidence limitations remain explicit
as Round635 states; a read-only source audit cannot certify runtime isolation.

## Observations so far (not final acceptance)

Aggregate session61600 uses source
`863042a02afbdce4a898b8bc77978845612a569f` and owned root
`/private/tmp/bluey-test-runs-phase635-current-rust-20261027/run.1793142695.30233.14390119130`.
The wrapper reported30 tests,30 passed, no failed selections, each in its own
fresh private database. It then reported `nonprod_postgres_finished` status0,
shutdown attempted and server exited, followed by the controller's explicit
PID-file-absence assertion and `current_rust_sqlite_full_gate_started`.
Formatting and strict all-target Clippy returned zero before the sequential
controller started full all-target tests. Tests remain in execution; a partial
list of green tests is not aggregate success or final cleanup evidence.

The full library target subsequently passed1,705/1,705, zero failed/ignored,
in2,471.54s. Jobs API binary0 tests and server binary1/1 passed; business
messaging simulator14/14, real-serve1/1, context-revision1/1 and GDPR cleanup2/2
also passed before HTTP integration began. The aggregate remains running; these
are not final all-target or launcher-cleanup results. Configured-only early
returns in the SQLite stage are not represented as additional real PG cases.
Observed installed tools are rustc1.95.0 (`59807616e`,2026-04-14) and
cargo1.95.0 (`f2d3ce0bd`,2026-03-21), native macOS/ARM64. This does not qualify
a protected release capsule, Linux server build, CI or production artifact.

Read-only work during the freeze identified two separate successor concerns:
the VM needs measured host-listener policy (including VZ loopback management,
not an impossible literal-zero claim), and company reservation uniqueness uses
normalized display names rather than signed canonical employer identity.
Neither is retroactively claimed fixed or exercised by this qualification.

## Terminal executable evidence

Session61600 exited0. The controller printed `current_rust_quality_finished`
with the unchanged full source SHA, configuredPostgres30 and all three full
gate booleans true. No product/test/manifest source changed. The sequential
controller requires each command to close with code0 and null signal.

| Target | Passed | Failed / ignored | Observed seconds |
|---|---:|---:|---:|
| Library | 1,705 | 0 / 0 | 2,471.54 |
| Jobs API binary | 0 | 0 / 0 | 0.00 |
| Server binary | 1 | 0 / 0 | 0.00 |
| Business messaging simulator | 14 | 0 / 0 | 0.01 |
| Real-serve integration | 1 | 0 / 0 | 1.62 |
| Context artifact revision | 1 | 0 / 0 | 0.00 |
| GDPR webhook cleanup | 2 | 0 / 0 | 3.80 |
| HTTP integration | 113 | 0 / 0 | 539.80 |
| Runner plan matrix | 2 | 0 / 0 | 4.65 |
| Usage reservation schema | 1 | 0 / 0 | 0.00 |
| Guarded nonprod example | 5 | 0 / 0 | 0.00 |
| **All-target aggregate** | **1,845** | **0 / 0** | Separate target durations above |

The integration targets total134; binary1 and example5 are additional. The30
configured PG executions are reported separately, not added to this table as
distinct coverage; configured-only SQLite early returns remain explicit.
The full test command compiles all targets; no release-profile/server-Linux
build or protected-capsule admission is inferred.

Parent independently observed the exact run root absent and its specific
parent empty after aggregate exit. Controller/Cargo/library PIDs30310/39087/
39264 were absent, and no process command path matched the owned root.
PG had already stopped cleanly before SQLite/full Rust. During execution the
owned root was sampled at6,030,216KiB, not a peak measurement; whole-host free
space was about31GiB late in testing and38GiB after cleanup. Other host activity
precludes assigning the complete host delta to this run. No shared cache,
source, owner database, archive or user data was deleted.

Fresh launcher self-test session84754 exited0 under the shared machine lock.
Its deliberate active-process cleanup refusal/status39 is expected negative
coverage, followed by `Bluey test launcher self-tests passed`; it is not a
failure of the accepted aggregate. The exact self-test parent is checked at
closeout. This batch starts no VM, preview, runner or provider service.

## Closeout

The runtime parent is empty and the self-test parent absent. Indexed privacy
passed2,854 paths/2,578 text files. Separate AI review is green for this bounded
qualification only, with no remaining qualification finding. It independently
confirmed the separate P1 canonical-employer alias gap as an irreversible-submit
release blocker; that product issue is not claimed solved by passing tests.
The reviewer inspected procedure/code/docs, not an independent runtime rerun.
Only five documentation files are in this batch; inherited REVIEW628 remains
untracked and untouched. No production/overall-goal completion follows.

## Exact aggregate command

Run from the authoritative Jobs checkout. This controller retains only explicit
runtime/compiler variables, uses the existing wrapper's private child HOME after
PG shutdown, and rejects source/head changes through terminal. It executes
documentation identifiers as bounded argv data, never document shell commands.

```sh
/opt/homebrew/bin/python3 -B /Users/uno/Projects/shared-runners/runner_schedule.py with-machine-lock --machine mac-build -- env BLUEY_TEST_RUN_PARENT=/private/tmp/bluey-test-runs-phase635-current-rust-20261027 scripts/run-bluey-tests.sh -- env -u NODE_OPTIONS -u NODE_PATH /opt/homebrew/bin/node --input-type=module -e 'import assert from "node:assert/strict";
import {spawn} from "node:child_process";
import {readFile,realpath,lstat} from "node:fs/promises";
import {join} from "node:path";
const root=await realpath(process.env.BLUEY_TEST_RUN_ROOT);
const recipe=await readFile("docs/work/IMPL-PHASE-629-JOBS-NONPROD-INTEGRATION-VALIDATION.md","utf8");
const filters=[...recipe.matchAll(/--case-filter '\''([A-Za-z_][A-Za-z0-9_:]{0,255})'\''/g)].map(match=>match[1]);
assert.equal(filters.length,30);assert.equal(new Set(filters).size,30);
assert(filters.every(name=>name.startsWith("db::jobs::")||name.startsWith("api::jobs_")||name==="postgres_application_upload_waiting_past_capacity_expiry_mutates_nothing"));
const features="integration-test-support,business-messaging-simulator-test-support";
const crypto=["--config","profile.test.package.curve25519-dalek.opt-level=3","--config","profile.test.package.ed25519-dalek.opt-level=3","--config","profile.test.package.sha2.opt-level=3"];
let active;let interrupted=false;
const stop=signal=>{interrupted=true;active?.kill(signal)};
const handlers=["SIGHUP","SIGINT","SIGTERM"].map(signal=>{const handler=()=>stop(signal);process.on(signal,handler);return[signal,handler]});
async function run(command,args,env){assert(!interrupted,"interrupted");const child=spawn(command,args,{env,stdio:"inherit"});active=child;const closed=await new Promise(resolve=>{child.once("error",()=>resolve({code:null,signal:"spawn_error"}));child.once("close",(code,signal)=>resolve({code,signal}))});active=undefined;assert.deepEqual(closed,{code:0,signal:null},"quality_command_failed");}
const allowed=new Set(["PATH","USER","LOGNAME","LANG","LC_ALL","TERM","SDKROOT","DEVELOPER_DIR","CARGO_TARGET_DIR","CARGO_INCREMENTAL","TMPDIR","TMP","TEMP","SQLITE_TMPDIR","BLUEY_TEST_RUN_ROOT","BLUEY_DB_PATH","BLUEY_SERVER_DB_BACKEND","BLUEY_DATA_DIR","BLUEY_CONFIG_DIR","BLUEY_RUNTIME_DIR","BLUEY_LOG_DIR","CUE_DATA_DIR","CUE_CONFIG_DIR","CUE_RUNTIME_DIR","CUE_LOG_DIR"]);
const clean=Object.fromEntries(Object.entries(process.env).filter(([key])=>allowed.has(key)||/^CARGO_PROFILE_(DEV|TEST)_/.test(key)));
const rustup=await realpath(process.env.RUSTUP_HOME||join(process.env.HOME,".rustup"));
const parentEnv={...clean,HOME:process.env.HOME,CARGO_HOME:process.env.CARGO_HOME||join(process.env.HOME,".cargo"),RUSTUP_HOME:rustup};
function output(value){console.log(JSON.stringify(value))}
const headBefore=await new Promise((resolve,reject)=>{const child=spawn("/usr/bin/git",["rev-parse","HEAD"],{env:parentEnv,stdio:["ignore","pipe","inherit"]});let out="";child.stdout.on("data",b=>{out+=b});child.once("error",reject);child.once("close",code=>code===0?resolve(out.trim()):reject(new Error("git_head")))});
try{
 output({event:"current_rust_quality_started",sourceCommit:headBefore,configuredSelection:filters.length});
 await run(process.execPath,["jobs/scripts/run-nonprod-postgres-tests.mjs","--pg-bin","/opt/homebrew/opt/postgresql@17/bin",...filters.flatMap(name=>["--case-filter",name]),"--","cargo",...crypto,"test","--offline","--locked","--manifest-path","server/Cargo.toml","--lib","--features",features],parentEnv);
 await assert.rejects(lstat(join(root,"nonprod-postgres/data/postmaster.pid")),{code:"ENOENT"});
 const home=await realpath(join(root,"nonprod-postgres/child-home"));
 const meta=await lstat(home);assert.equal(meta.uid,process.getuid());assert.equal(meta.mode&4095,448);
 const env={...clean,HOME:home,CARGO_HOME:join(home,".cargo"),RUSTUP_HOME:rustup,BLUEY_SERVER_DB_BACKEND:"sqlite"};
 assert(!Object.hasOwn(env,"BLUEY_TEST_POSTGRES_URL")&&!Object.hasOwn(env,"BLUEY_DATABASE_URL")&&!Object.hasOwn(env,"BLUEY_JOBS_REQUIRE_POSTGRES_TESTS"));
 output({event:"current_rust_sqlite_full_gate_started",sourceCommit:headBefore,postgresStopped:true});
 await run("cargo",["fmt","--manifest-path","server/Cargo.toml","--","--check"],env);
 await run("cargo",["clippy","--offline","--locked","--manifest-path","server/Cargo.toml","--all-targets","--features",features,"--","-D","warnings"],env);
 await run("cargo",[...crypto,"test","--offline","--locked","--manifest-path","server/Cargo.toml","--all-targets","--features",features],env);
 const git=await import("node:child_process");const headAfter=git.spawnSync("/usr/bin/git",["rev-parse","HEAD"],{env,encoding:"utf8",timeout:10000});assert.equal(headAfter.status,0);assert.equal(headAfter.stdout.trim(),headBefore);
 const dirty=git.spawnSync("/usr/bin/git",["status","--porcelain=v1","--untracked-files=all","--","server","crates/cue-core","jobs",".github"],{env,encoding:"utf8",timeout:10000});assert.equal(dirty.status,0);assert.equal(dirty.stdout,"","source_scope_changed");
 output({event:"current_rust_quality_finished",sourceCommit:headBefore,configuredPostgres:30,format:true,strictAllTargets:true,fullAllTargets:true});
}finally{for(const [signal,handler]of handlers)process.off(signal,handler);}'
```

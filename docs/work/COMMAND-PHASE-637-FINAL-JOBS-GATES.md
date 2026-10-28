# Phase637 full Jobs qualification command

Observed34408 exit0:2085 tests/zero skips and source/cleanup checks passed.
Exact source/HEAD/status freeze;2085 expected tests with zero
skips, five typechecks/builds, actual portal config, schema/CI guards and private
engine process/open-file inventories. No production or external effects.

```sh
/opt/homebrew/bin/python3 -B /Users/uno/Projects/shared-runners/runner_schedule.py with-machine-lock --machine mac-build -- env BLUEY_TEST_RUN_PARENT=/private/tmp/bluey-test-runs-phase637-final-jobs-20261028 scripts/run-bluey-tests.sh -- env -u NODE_OPTIONS -u NODE_PATH /opt/homebrew/bin/node --input-type=module <<'NODE'
import assert from "node:assert/strict";
import {spawn,spawnSync} from "node:child_process";
import {readFile,realpath,mkdir,lstat} from "node:fs/promises";
import {createHash} from "node:crypto";
import {join} from "node:path";
const repo=process.cwd();
const root=await realpath(process.env.BLUEY_TEST_RUN_ROOT);
assert(root.startsWith("/private/tmp/bluey-test-runs-phase637-final-jobs-20261028/"));
const meta=await lstat(root);assert.equal(meta.uid,process.getuid());assert.equal(meta.mode&4095,448);
const home=join(root,"jobs-home");await mkdir(home,{mode:0o700});
const env={PATH:"/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin",HOME:home,
LANG:"en_US.UTF-8",TZ:"UTC",CI:"1",NO_COLOR:"1",
TMPDIR:process.env.TMPDIR,TMP:process.env.TMP,TEMP:process.env.TEMP,
SQLITE_TMPDIR:process.env.SQLITE_TMPDIR,BLUEY_TEST_RUN_ROOT:root,
BLUEY_SERVER_DB_BACKEND:"sqlite",BLUEY_DB_PATH:process.env.BLUEY_DB_PATH,
BLUEY_DATA_DIR:process.env.BLUEY_DATA_DIR,BLUEY_CONFIG_DIR:process.env.BLUEY_CONFIG_DIR,
BLUEY_RUNTIME_DIR:process.env.BLUEY_RUNTIME_DIR,BLUEY_LOG_DIR:process.env.BLUEY_LOG_DIR,
PLAYWRIGHT_BROWSERS_PATH:join(root,"playwright"),
PLAYWRIGHT_DOWNLOAD_CONNECTION_TIMEOUT:"30000",
npm_config_cache:join(root,"npm-cache"),npm_config_userconfig:join(home,"npmrc"),
npm_config_globalconfig:join(home,"global-npmrc"),
XDG_CONFIG_HOME:join(home,"xdg-config"),GIT_CONFIG_NOSYSTEM:"1",
GIT_CONFIG_GLOBAL:"/dev/null",GIT_OPTIONAL_LOCKS:"0"};
await mkdir(env.XDG_CONFIG_HOME,{mode:0o700});
const group=spawnSync("/bin/ps",["-o","pgid=","-p",String(process.pid)],{env,encoding:"utf8",timeout:10000});
assert.equal(group.status,0);assert.equal(Number(group.stdout.trim()),process.pid,"canonical_group_leader_required");
function git(args){const r=spawnSync("/usr/bin/git",args,{env,encoding:"utf8",timeout:10000,maxBuffer:8*1024*1024});assert.equal(r.status,0);return r.stdout;}
const head=git(["rev-parse","HEAD"]).trim();
const scope=["jobs","server","crates/cue-core","infra",".github","scripts/run-bluey-tests.sh","Cargo.toml","Cargo.lock"];
const beforeState=git(["status","--porcelain=v1","--untracked-files=all","--",...scope]);
const files=[...new Set(git(["ls-files","--cached","--others","--exclude-standard","-z","--",...scope]).split("\0").filter(Boolean))].sort();
assert(files.length>100);assert(new Set(files).size===files.length);
async function hashes(){const result={};for(const file of files)result[file]=createHash("sha256").update(await readFile(file)).digest("hex");return result;}
const before=await hashes();
const sourceManifestSha256=createHash("sha256").update(JSON.stringify(before)).digest("hex");
const scopedStatusSha256=createHash("sha256").update(beforeState).digest("hex");
let interrupted=false;let stopping=false;
function stopOwnedGroup(reason){
 if(stopping)return;stopping=true;interrupted=true;
 console.error(JSON.stringify({event:"phase637_jobs_gate_stopping",reason,ownedGroup:process.pid}));
 setTimeout(()=>{try{process.kill(-process.pid,"SIGKILL")}catch{process.exit(125)}},5000);
 try{process.kill(-process.pid,"SIGTERM")}catch{process.exit(125)}
}
const handlers=["SIGHUP","SIGINT","SIGTERM"].map(signal=>{const handler=()=>stopOwnedGroup(signal);process.on(signal,handler);return[signal,handler]});
async function run(command,args,cwd=repo,timeoutMs=300000){
 assert(!interrupted);
 const child=spawn(command,args,{env,cwd,stdio:["ignore","pipe","pipe"]});let output="";let timedOut=false;let oversized=false;
 const consume=chunk=>{if(oversized)return;process.stdout.write(chunk);output+=chunk;if(output.length>4*1024*1024){oversized=true;stopOwnedGroup("output_bound")}};
 child.stdout.on("data",consume);child.stderr.on("data",consume);
 const timeout=setTimeout(()=>{timedOut=true;stopOwnedGroup("command_timeout")},timeoutMs);
 const closed=await new Promise(resolve=>{child.once("error",()=>resolve({code:null,signal:"spawn_error"}));child.once("close",(code,signal)=>resolve({code,signal}))});
 clearTimeout(timeout);
 assert(!interrupted&&!timedOut&&!oversized,"command_interrupted_timed_out_or_output_bound");
 assert.deepEqual(closed,{code:0,signal:null},"jobs_gate_command_failed");
 return output.replace(/\x1b\[[0-9;]*m/g,"");
}
async function inventoryPrivateBrowser(){
 const inventory=()=>{const r=spawnSync("/bin/ps",["-axo","pid=,uid=,comm="],{env,encoding:"utf8",timeout:10000,maxBuffer:8*1024*1024});assert.equal(r.status,0);return r.stdout.split("\n").map(line=>line.trim().match(/^(\d+)\s+(\d+)\s+(.*)$/)).filter(Boolean).filter(row=>row[3].includes(env.PLAYWRIGHT_BROWSERS_PATH+"/"))};
 let rows=inventory();const forced=rows.length>0;
 for(const signal of ["SIGTERM","SIGKILL"]){for(const row of rows){assert.equal(Number(row[2]),process.getuid());const current=inventory().find(x=>x[1]===row[1]);if(current){assert.equal(Number(current[2]),process.getuid());assert.equal(current[3],row[3]);try{process.kill(Number(row[1]),signal)}catch(error){if(error.code!=="ESRCH")throw error}}}for(let attempt=0;attempt<50;attempt++){rows=inventory();if(!rows.length)break;await new Promise(resolve=>setTimeout(resolve,100))}if(!rows.length)break}
 assert.equal(rows.length,0,"private_browser_process_survived");
 const info=await lstat(env.PLAYWRIGHT_BROWSERS_PATH).catch(error=>{if(error.code!=="ENOENT")throw error});
 if(info){const r=spawnSync("/usr/sbin/lsof",["-nP","-a","-u",String(process.getuid()),"+D",env.PLAYWRIGHT_BROWSERS_PATH,"-F","pfn"],{env,encoding:"utf8",timeout:30000,maxBuffer:1024*1024});assert.equal(r.error,undefined);assert.equal(r.signal,null);assert.equal(r.status,1);assert.equal(r.stdout.trim(),"");assert.equal(r.stderr.trim(),"")}
 console.log(JSON.stringify({event:"phase637_private_browser_inventory",processes:0,openFiles:0,forcedCleanup:forced}));
 assert(!forced,"private_browser_required_forced_cleanup");
}
try{
 console.log(JSON.stringify({event:"phase637_jobs_gate_started",baseCommit:head,sourceFiles:files.length,sourceManifestSha256,scopedStatusSha256,privateHome:true}));
 await run(process.execPath,[join(repo,"jobs/node_modules/playwright/cli.js"),"install","chromium"],repo,240000);
 const tests=await run("/opt/homebrew/bin/npm",["run","test","--prefix","jobs"],repo,600000);
 const nodeTotals=[...tests.matchAll(/^(?:#|ℹ)\s+tests\s+(\d+)\s*$/gm)].map(match=>Number(match[1]));
 const nodeCounts=[...tests.matchAll(/^(?:#|ℹ)\s+pass\s+(\d+)\s*$/gm)].map(match=>Number(match[1]));
 const nodeSkipped=[...tests.matchAll(/^(?:#|ℹ)\s+skipped\s+(\d+)\s*$/gm)].map(match=>Number(match[1]));
 const vitestCounts=[...tests.matchAll(/Tests\s+(\d+) passed/g)].map(match=>Number(match[1]));
 assert.deepEqual(nodeCounts,[8,7,17],"node_test_count_mismatch");
 assert.deepEqual(nodeTotals,[8,7,17],"node_test_total_mismatch");
 assert.deepEqual(nodeSkipped,[0,0,0],"node_tests_skipped_or_summary_missing");
 assert.deepEqual(vitestCounts,[807,219,308,321,398],"workspace_test_count_mismatch");
 assert(!/Tests[^\n]*\d+ skipped/.test(tests),"vitest_tests_skipped");
 console.log(JSON.stringify({event:"phase637_jobs_tests_passed",nodeCounts,vitestCounts,total:2085,skipped:0}));
 await run("/opt/homebrew/bin/npm",["run","typecheck","--prefix","jobs"]);
 for(const workspace of ["@bluey/jobs-automation","@bluey/jobs-browser","@bluey/jobs-runner","@bluey/jobs-workflows"]){
  await run("/opt/homebrew/bin/npm",["run","build","--workspace",workspace],join(repo,"jobs"));
 }
 const portal=join(root,"portal-build");await mkdir(portal,{mode:0o700});
 const buildCode="import {build} from 'vite';await build({configFile:'vite.config.ts',envDir:false,build:{outDir:"+JSON.stringify(portal)+",emptyOutDir:false}});";
 await run(process.execPath,["--input-type=module","-e",buildCode],join(repo,"jobs/portal"),120000);
 await run(process.execPath,["jobs/scripts/check-jobs-schema-parity.mjs"]);
 await run(process.execPath,["jobs/scripts/ci-guards-self-test.mjs"]);
 const processes=spawnSync("/bin/ps",["-axo","pid=,ppid=,pgid=,comm="],{env,encoding:"utf8",timeout:10000,maxBuffer:8*1024*1024});
 assert.equal(processes.status,0);
 assert.equal(processes.stdout.split("\n").filter(line=>line.includes(env.PLAYWRIGHT_BROWSERS_PATH+"/")).length,0,"private_browser_process_survived");
 const openFiles=spawnSync("/usr/sbin/lsof",["-nP","-a","-u",String(process.getuid()),"+D",env.PLAYWRIGHT_BROWSERS_PATH,"-F","pfn"],{env,encoding:"utf8",timeout:30000,maxBuffer:1024*1024});
 assert.equal(openFiles.error,undefined);assert.equal(openFiles.signal,null);assert.equal(openFiles.status,1);
 assert.equal(openFiles.stdout.trim(),"");assert.equal(openFiles.stderr.trim(),"");
 console.log(JSON.stringify({event:"phase637_private_browser_closed",path:env.PLAYWRIGHT_BROWSERS_PATH,processes:0,openFiles:0}));
 assert.equal(git(["rev-parse","HEAD"]).trim(),head);
 assert.equal(git(["status","--porcelain=v1","--untracked-files=all","--",...scope]),beforeState);
 assert.deepEqual(await hashes(),before);
 console.log(JSON.stringify({event:"phase637_jobs_gate_finished",baseCommit:head,tests:2085,skipped:0,typecheckWorkspaces:5,builtWorkspaces:5,portalOutputPrivate:true,scopedSourceUnchanged:true}));
}finally{try{await inventoryPrivateBrowser()}finally{if(!stopping)for(const[signal,handler]of handlers)process.off(signal,handler)}}
NODE
```

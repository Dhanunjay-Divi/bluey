# Phase637 private document focused qualification command

Observed14801 exit0; see IMPL637 for actual counts and cleanup. This controller uses the canonical launcher/shared Mac queue,
private HOME/browser/output, source hashes, exact configured Chromium and bounded
group shutdown. It inventories its unique private engine path on success/failure
before returning to canonical cleanup. A forced cleanup is always a gate failure.
No owner accounts, real resumes, providers or production authority.

```sh
/opt/homebrew/bin/python3 -B /Users/uno/Projects/shared-runners/runner_schedule.py with-machine-lock --machine mac-build -- env BLUEY_TEST_RUN_PARENT=/private/tmp/bluey-test-runs-phase637-document-focused-20261028 scripts/run-bluey-tests.sh -- env -u NODE_OPTIONS -u NODE_PATH /opt/homebrew/bin/node --input-type=module <<'NODE'
import assert from "node:assert/strict";
import {spawn,spawnSync} from "node:child_process";
import {readFile,realpath,mkdir,lstat,symlink,writeFile} from "node:fs/promises";
import {createHash} from "node:crypto";
import {join} from "node:path";
const repo=process.cwd();
const root=await realpath(process.env.BLUEY_TEST_RUN_ROOT);
assert(root.startsWith("/private/tmp/bluey-test-runs-phase637-document-focused-20261028/"));
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
 console.log(JSON.stringify({event:"phase637_document_focused_started",baseCommit:head,root,sourceFiles:files.length,sourceManifestSha256,scopedStatusSha256,privateHome:true}));
 await run("/opt/homebrew/bin/npm",["run","typecheck","--workspace","@bluey/jobs-portal"],join(repo,"jobs"));
 await run(process.execPath,[join(repo,"jobs/node_modules/playwright/cli.js"),"install","chromium"],repo,240000);
 const tests=await run("/opt/homebrew/bin/npm",["run","test","--workspace","@bluey/jobs-portal","--","src/lib/bundle-policy.test.ts","src/lib/documents/browser-import.integration.test.ts"],join(repo,"jobs"),150000);
 assert.deepEqual([...tests.matchAll(/Tests\s+(\d+) passed/g)].map(match=>Number(match[1])),[11]);
 assert(!/Tests[^\n]*\d+ skipped/.test(tests));
 assert(tests.includes('"event":"document_import_qualification"'));
 const compiled=join(root,"compiled");await mkdir(compiled,{mode:0o700});
 await symlink(join(repo,"jobs/node_modules"),join(compiled,"node_modules"),"dir");
 await symlink(join(repo,"jobs/automation/assets"),join(compiled,"assets"),"dir");
 await writeFile(join(compiled,"package.json"),JSON.stringify({type:"module"}),{mode:0o600});
 await run(process.execPath,[join(repo,"jobs/node_modules/typescript/bin/tsc"),"-p","jobs/automation/tsconfig.json","--outDir",join(compiled,"automation")]);
 await run(process.execPath,["--input-type=module","-e","\nimport assert from \"node:assert/strict\";\nimport {mkdir,readFile,writeFile} from \"node:fs/promises\";\nimport {join} from \"node:path\";\nimport {pathToFileURL} from \"node:url\";\nimport {createHash} from \"node:crypto\";\nimport {PDFDocument,StandardFonts} from \"pdf-lib\";\nimport {getDocument} from \"pdfjs-dist/legacy/build/pdf.mjs\";\nconst root=process.env.BLUEY_TEST_RUN_ROOT;\nconst {materializeApplicationDocuments,assertMaterializedDocumentSnapshot}=await import(pathToFileURL(join(root,\"compiled/automation/documents.js\")));\nconst {pdfjsNodeDocumentOptions}=await import(pathToFileURL(join(root,\"compiled/automation/pdfjs-node-options.js\")));\nconst text=\"Synthetic Candidate built reliable offline PDF validation systems.\";\nconst pdf=await PDFDocument.create();const font=await pdf.embedFont(StandardFonts.Helvetica);\npdf.addPage([612,792]).drawText(text,{font,size:12,x:40,y:740});\nconst bytes=Buffer.from(await pdf.save());const source=join(root,\"synthetic-standard.pdf\");await writeFile(source,bytes,{mode:0o600});\nlet warnings=0,fetches=0;const warn=console.warn;console.warn=(...args)=>{warnings++;warn(...args)};\nglobalThis.fetch=()=>{fetches++;return Promise.reject(new Error(\"fixture_network_denied\"))};\nconst result=await materializeApplicationDocuments({resumePath:source,resumeVersionId:\"synthetic-standard\"},join(root,\"materialized\"));\nassert.equal(result.resume.sha256,createHash(\"sha256\").update(bytes).digest(\"hex\"));\nassert.deepEqual(await readFile(result.resume.path),bytes);assert.equal(result.resume.bytesBase64,bytes.toString(\"base64\"));\nawait assertMaterializedDocumentSnapshot(result.resume);\nconst task=getDocument({data:Uint8Array.from(bytes),...pdfjsNodeDocumentOptions(),disableFontFace:true,enableXfa:false,isImageDecoderSupported:false,isOffscreenCanvasSupported:false,stopAtErrors:true,useSystemFonts:false,useWasm:false});\ntry{const doc=await task.promise;assert.equal(doc.numPages,1);const page=await doc.getPage(1);const content=await page.getTextContent();assert.equal(content.items.map(x=>x.str||\"\").join(\" \").trim(),text)}finally{await task.destroy()}\nconsole.warn=warn;assert.equal(warnings,0);assert.equal(fetches,0);\nconsole.log(JSON.stringify({event:\"phase637_compiled_font_passed\",warnings,fetches,exactBytes:true,frozenSnapshot:Object.isFrozen(result.resume)}));\n"],join(repo,"jobs/automation"));
 assert.equal(git(["rev-parse","HEAD"]).trim(),head);
 assert.equal(git(["status","--porcelain=v1","--untracked-files=all","--",...scope]),beforeState);
 assert.deepEqual(await hashes(),before);
 console.log(JSON.stringify({event:"phase637_document_focused_finished",baseCommit:head,portalTests:11,syntheticBrowserCases:5,compiledFont:true,scopedSourceUnchanged:true}));
}finally{try{await inventoryPrivateBrowser()}finally{if(!stopping)for(const[signal,handler]of handlers)process.off(signal,handler)}}

NODE
```

// Persistent synthetic local QA. The caller must provide a reviewed loopback-only
// OS sandbox; process/listener observations are not an atomic socket guarantee
// against a hostile process owned by the same user. Never production or release.
import { spawn, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { constants } from 'node:fs';
import { lstat, mkdir, open, realpath } from 'node:fs/promises';
import { request } from 'node:http';
import { createServer } from 'node:net';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseLocalQaBuildBinding } from './local-qa-build-binding.mjs';
import { initializeLocalQaState, inspectLocalQaState, loadLocalQaSecrets } from './local-qa-state.mjs';
import { initializeLocalQaPostgres, startLocalQaPostgres } from './local-qa-postgres.mjs';
import { startLocalQaObjectStore } from './local-qa-object-store.mjs';
import { startLocalQaSmtp } from './local-qa-smtp.mjs';
import { startLocalQaGateway, LOCAL_QA_GATEWAY_BACKEND_FENCE_VERSION } from './local-qa-gateway.mjs';
import { createLocalQaDurableDraft } from './local-qa-durable-draft.mjs';
import { createLocalQaCandidateHandoffOwner } from './local-qa-candidate-handoff.mjs';
import { assertLocalQaCandidateResume, bindLocalQaCandidateReadback,
  requireLocalQaExpectedDraft } from './local-qa-candidate-readback.mjs';
export { assertLocalQaCandidateResume, bindLocalQaCandidateReadback,
  assertLocalQaExpectedDraft } from './local-qa-candidate-readback.mjs';
import { acquireLocalQaSessionLease, assertLocalQaSessionLease, releaseLocalQaSessionLease,
  initializeLocalQaSessionState, inspectLocalQaSessionState, loadLocalQaSessionSecrets,
  writeLocalQaBootstrapIntent, writeLocalQaBootstrapReceipt, requireLocalQaBootstrapReceipt } from './local-qa-session-state.mjs';
import { parseListenerRecords, assertListenerCapture } from './run-nonprod-managed-runner-vm-tests.mjs';

// The helper is copied into the project under jobs/scripts; resolve its physical
// project root from this module rather than the historical Phase653 checkout.
const ROOT=await realpath(fileURLToPath(new URL('../../',import.meta.url)));
const PYTHON='/Library/Developer/CommandLineTools/usr/bin/python3';
const PYROOT='/Library/Developer/CommandLineTools/Library/Frameworks/Python3.framework/Versions/3.9';
const HELPER=join(ROOT,'jobs/scripts/local-qa-process-info.py'),NATIVE=join(ROOT,'jobs/scripts/managed-runner-host-processes.py');
const PINS={
  [`${PYROOT}/bin/python3.9`]:'6e7ae61f68a3838094fc56590f84c52069a97d7816f6bb79e0e85995b340e464',
  [`${PYROOT}/Resources/Python.app/Contents/MacOS/Python`]:'295b2fd03b2b05f12b6f4887d70dbf43e5358173f8eec0909c121c9bdec2756f',
  [HELPER]:'28c1e2260197e3fcb6bee2adddeaa75a7f17c3a88180c370b541578d04074c8a',
  [NATIVE]:'bac1d707b8d077bde9ed6dd787d801550b198fefc908c82e25edb4cae3f6481a',
  '/usr/sbin/lsof':'deff85c7b4750a948a69fcd48eae3b394a37575e5048e5eb4a35b7a0482c3dc8',
};
const need=value=>{if(!value)throw new Error('local_qa_session_refused');};
const sha=bytes=>createHash('sha256').update(bytes).digest('hex');
const metadata=m=>[m.dev,m.ino,m.uid,m.mode,m.nlink,m.size,m.mtimeNs,m.ctimeNs].map(String).join(':');
const sleep=ms=>new Promise(resolveSleep=>setTimeout(resolveSleep,ms));
const maximumCandidateWindowMs=2*60*60*1000;
const candidateHandoffCleanupReserveMs=10*1000;
const mockReceiptRestarts=new WeakMap();

function candidateLifetime(expiresAtMs){
  need(Number.isSafeInteger(expiresAtMs));
  const elapsedStart=performance.now();
  const remaining=expiresAtMs-Date.now();
  need(remaining>0&&remaining<=maximumCandidateWindowMs);
  const elapsedDeadline=elapsedStart+remaining;
  return Object.freeze({expiresAtMs,remaining:()=>Math.min(expiresAtMs-Date.now(),elapsedDeadline-performance.now())});
}

// One owner for manual close, abort, and expiry. The caller can observe
// automatic teardown without assuming that a deadline proves resources closed.
export function armLocalQaCandidateShutdown(lifetime,signal,teardown){
  const {promise:termination,resolve:finish}=Promise.withResolvers();
  let timer,closing;
  function stop(reason){
    if(closing)return closing;
    clearTimeout(timer);
    signal?.removeEventListener('abort',abort);
    closing=Promise.resolve().then(teardown);
    void closing.then(
      ()=>finish(Object.freeze({reason,closed:true})),
      ()=>finish(Object.freeze({reason,closed:false})),
    );
    return closing;
  }
  function abort(){void stop('aborted');}
  function tick(){
    const delay=lifetime.remaining();
    if(delay<=0){void stop('expired');return;}
    // Recheck wall time as well as the monotonic cutoff after a clock change.
    timer=setTimeout(tick,Math.min(1000,Math.ceil(delay)));
  }
  signal?.addEventListener('abort',abort,{once:true});
  if(signal?.aborted)abort();else tick();
  return Object.freeze({close:()=>stop('owner'),termination});
}

async function absent(path){try{await lstat(path);return false;}catch(error){if(error.code==='ENOENT')return true;throw error;}}
async function checkedFile(path,expected,max,{executable=false,ownerOnly=false}={}){
  need(await realpath(path)===path);const file=await open(path,constants.O_RDONLY|constants.O_NOFOLLOW);
  try{const before=await file.stat({bigint:true});need(before.isFile()&&before.nlink>0n&&(before.uid===0n||before.uid===BigInt(process.getuid()))&&(before.mode&0o6022n)===0n&&before.size<=BigInt(max));
    if(ownerOnly)need(before.uid===BigInt(process.getuid())&&before.nlink===1n);
    if(executable)need((before.mode&0o7777n)===0o700n);
    const hash=createHash('sha256'),buffer=Buffer.alloc(65536);let count=0;
    for(;;){const r=await file.read(buffer,0,buffer.length,null);if(!r.bytesRead)break;count+=r.bytesRead;need(count<=max);hash.update(buffer.subarray(0,r.bytesRead));}
    need(BigInt(count)===before.size&&(!expected||hash.digest('hex')===expected)&&metadata(await file.stat({bigint:true}))===metadata(before)&&metadata(await lstat(path,{bigint:true}))===metadata(before));
    return before;
  }finally{await file.close();}
}
async function requireBoundFile(kind,path,expected,max,options){
  try{return await checkedFile(path,expected,max,options);}
  catch{throw new Error(`local_qa_${kind}_binding_refused`);}
}
async function tools(){need(await realpath(PYTHON)===`${PYROOT}/bin/python3.9`);for(const [path,hash] of Object.entries(PINS))await checkedFile(path,hash,32*1024*1024);}
function processInfo(pid){
  const r=spawnSync(PYTHON,['-I','-S','-B',HELPER,String(pid)],{encoding:'utf8',timeout:5000,maxBuffer:8192,env:{PATH:'/usr/bin:/bin',LC_ALL:'C'}});
  need(!r.error&&r.status===0&&r.signal===null&&r.stderr==='');let value;try{value=JSON.parse(r.stdout);}catch{throw new Error('local_qa_session_refused');}
  need(value&&Object.keys(value).join(',')==='pid,ppid,uid,ruid,pgid,birthSec,birthUsec,executable,executableDevice,executableInode'&&JSON.stringify(value)+'\n'===r.stdout&&
    value.pid===pid&&value.uid===process.getuid()&&value.ruid===process.getuid()&&Number.isSafeInteger(value.ppid)&&value.ppid>=0&&value.ppid<=2147483647&&
    Number.isSafeInteger(value.pgid)&&value.pgid>1&&value.pgid<=2147483647&&typeof value.birthSec==='string'&&/^[1-9][0-9]{0,19}$/.test(value.birthSec)&&BigInt(value.birthSec)<=18446744073709551615n&&
    Number.isSafeInteger(value.birthUsec)&&value.birthUsec>=0&&value.birthUsec<1000000&&typeof value.executable==='string'&&/^\/[\x20-\x7e]*$/.test(value.executable)&&
    typeof value.executableDevice==='string'&&/^(0|[1-9][0-9]{0,19})$/.test(value.executableDevice)&&BigInt(value.executableDevice)<=18446744073709551615n&&
    typeof value.executableInode==='string'&&/^[1-9][0-9]{0,19}$/.test(value.executableInode)&&BigInt(value.executableInode)<=18446744073709551615n);return value;
}
function listeners(port,pid){const args=['-nP','-a',...(pid?['-p',String(pid)]:[]),`-iTCP:${port}`,'-sTCP:LISTEN','-F','pfn'];
  const r=spawnSync('/usr/sbin/lsof',args,{encoding:'utf8',timeout:5000,maxBuffer:1024*1024,env:{PATH:'/usr/bin:/bin',LC_ALL:'C'}});need(!r.error);
  return parseListenerRecords(assertListenerCapture({stdout:r.stdout,stderr:r.stderr,code:r.status,signal:r.signal}));}
async function portAvailable(port){const socket=createServer();await new Promise((ok,bad)=>{socket.once('error',bad);socket.listen(port,'127.0.0.1',ok);});await new Promise((ok,bad)=>socket.close(e=>e?bad(e):ok()));}
async function ephemeralPort(){const s=createServer();await new Promise((ok,bad)=>{s.once('error',bad);s.listen(0,'127.0.0.1',ok);});const port=s.address().port;await new Promise((ok,bad)=>s.close(e=>e?bad(e):ok()));return port;}
async function readManifest(path){
  need(typeof path==='string'&&resolve(path)===path&&await realpath(path)===path);const file=await open(path,constants.O_RDONLY|constants.O_NOFOLLOW);
  try{const before=await file.stat({bigint:true});need(before.isFile()&&before.uid===BigInt(process.getuid())&&before.nlink===1n&&(before.mode&0o022n)===0n&&before.size>0n&&before.size<=65536n);
    const bytes=Buffer.alloc(65537);let size=0;for(;;){const r=await file.read(bytes,size,bytes.length-size,null);size+=r.bytesRead;need(size<=65536);if(!r.bytesRead)break;}
    need(BigInt(size)===before.size&&metadata(await file.stat({bigint:true}))===metadata(before)&&metadata(await lstat(path,{bigint:true}))===metadata(before));
    let manifest;try{manifest=JSON.parse(bytes.subarray(0,size).toString('utf8'));}catch{throw new Error('local_qa_session_refused');}
    need(manifest&&typeof manifest==='object'&&!Array.isArray(manifest)&&JSON.stringify(manifest)+'\n'===bytes.subarray(0,size).toString('utf8'));return manifest;
  }finally{await file.close();}
}
function call(port,path,{method='GET',token,body,headers={}}={}){
  const bytes=body===undefined?Buffer.alloc(0):Buffer.from(JSON.stringify(body));
  return new Promise((ok,bad)=>{const req=request({host:'127.0.0.1',port,path,method,headers:{...headers,...(token?{authorization:`Bearer ${token}`} : {}),...(body===undefined?{}:{'content-type':'application/json','content-length':String(bytes.length)})},timeout:15000},res=>{
    const chunks=[];let size=0;res.on('data',chunk=>{size+=chunk.length;if(size>1024*1024)res.destroy();else chunks.push(chunk);});
    res.once('error',bad);res.once('end',()=>ok({status:res.statusCode,bytes:Buffer.concat(chunks)}));});req.once('timeout',()=>req.destroy());req.once('error',bad);req.end(bytes);});
}
export async function startLocalQaSession({mode,stateDirectory,port,buildBinding,signal,onLifecycle,
  candidateReadOnly=false,candidateExpiresAtMs,durableDraftFixture,expectedPreparedDraft,
  durableMockReceiptFixture,expectedMockReceipt}){
  need(LOCAL_QA_GATEWAY_BACKEND_FENCE_VERSION===1&&(mode==='bootstrap'||mode==='start')&&typeof stateDirectory==='string'&&resolve(stateDirectory)===stateDirectory&&Number.isInteger(port)&&port>1024&&port<=65535&&port!==5187);
  need(typeof candidateReadOnly==='boolean'&&(candidateReadOnly?mode==='start':candidateExpiresAtMs===undefined));
  need(candidateReadOnly||expectedPreparedDraft===undefined);
  need((signal===undefined||signal instanceof AbortSignal)&&(onLifecycle===undefined||typeof onLifecycle==='function'));
  const lifetime=candidateReadOnly?candidateLifetime(candidateExpiresAtMs):undefined;
  const expected=expectedPreparedDraft===undefined?undefined:requireLocalQaExpectedDraft(expectedPreparedDraft);
  const mockRestart=expectedMockReceipt===undefined?undefined:mockReceiptRestarts.get(expectedMockReceipt);
  if(expectedMockReceipt!==undefined)need(candidateReadOnly&&expected===undefined
    &&durableDraftFixture===undefined&&durableMockReceiptFixture===undefined&&mockRestart
    &&mockRestart.stateDirectory===stateDirectory&&mockRestart.port===port
    &&candidateExpiresAtMs<=mockRestart.latestExpiryMs);
  // Validate and custody-check the controller-supplied binding before reading
  // QA state, taking a lease, or allocating listeners. No CLI/env fallback.
  const currentBinding=parseLocalQaBuildBinding(buildBinding);
  if(mockRestart){
    need(Object.entries(currentBinding).every(([key,value])=>mockRestart.buildBinding[key]===value));
    // Claim once, before the first await. Failed starts do not grant a retry
    // or a second candidate window against the same synthetic bootstrap.
    need(mockReceiptRestarts.delete(expectedMockReceipt));
  }
  await requireBoundFile('artifact',currentBinding.artifactPath,currentBinding.artifactSha256,1024*1024*1024,{executable:true,ownerOnly:true});
  await requireBoundFile('manifest',currentBinding.manifestFile,currentBinding.manifestSha256,65536,{ownerOnly:true});
  let seedDurablePosting, mockReceiptActor, fixtureRoot, fixtureOptions;
  const includeMockReceipt=durableMockReceiptFixture!==undefined;
  if(includeMockReceipt){
    need(process.platform==='darwin'&&mode==='bootstrap'&&!candidateReadOnly
      &&durableDraftFixture===undefined&&durableMockReceiptFixture
      &&Object.getPrototypeOf(durableMockReceiptFixture)===Object.prototype
      &&Object.keys(durableMockReceiptFixture).sort().join(',')==='artifactPath,artifactSha256');
    const {artifactPath,artifactSha256}=durableMockReceiptFixture;
    fixtureOptions=Object.freeze({artifactPath,artifactSha256,fixtureMode:'review_first_mock_receipt'});
  }else if(durableDraftFixture!==undefined){
    need(mode==='bootstrap'&&durableDraftFixture&&Object.getPrototypeOf(durableDraftFixture)===Object.prototype);
    const keys=Object.keys(durableDraftFixture).sort().join(',');
    need(keys==='artifactPath,artifactSha256'||keys==='artifactPath,artifactSha256,fixtureMode');
    const {artifactPath,artifactSha256,fixtureMode='w2'}=durableDraftFixture;
    need(fixtureMode==='w2'||fixtureMode==='c2c');
    fixtureOptions=Object.freeze({artifactPath,artifactSha256,fixtureMode});
  }
  if(fixtureOptions){
    fixtureRoot=process.env.BLUEY_TEST_RUN_ROOT;
    need(typeof fixtureRoot==='string'&&stateDirectory===join(fixtureRoot,'bluey-jobs-qa-draft')
      &&await absent(stateDirectory));
  }
  let observerFailed=false,phase='preflight';
  const emit=value=>{if(!onLifecycle||observerFailed)return;try{const result=onLifecycle(Object.freeze(value));
    if(result&&(typeof result==='object'||typeof result==='function')&&typeof result.then==='function'){observerFailed=true;Promise.resolve(result).catch(()=>{});}}
    catch{observerFailed=true;}};
  const check=()=>need(!signal?.aborted&&!observerFailed&&(lifetime===undefined||lifetime.remaining()>0));
  const setPhase=name=>{phase=name;emit({event:'stage',name});check();};check();
  const manifest=await readManifest(currentBinding.manifestFile);check();await portAvailable(port);
  let state,lease,pg,store,smtp,server,gateway,serverPort,sessionConfig,sessionSecrets;
  let candidateHandoff,closed=false,closePromise,serverClass='other';
  const verifyBackend=async ({host,port:requestedPort},allowMissing=false)=>{
    check();need(host==='127.0.0.1'&&requestedPort===serverPort&&server&&server.child.pid&&server.child.exitCode===null&&server.child.signalCode===null);
    await assertLocalQaSessionLease(stateDirectory,lease);await tools();const parent=processInfo(process.pid),before=processInfo(server.child.pid),artifact=await lstat(currentBinding.artifactPath,{bigint:true});
    need(before.ppid===process.pid&&before.pgid===parent.pgid&&before.executable===currentBinding.artifactPath&&before.executableDevice===String(artifact.dev)&&before.executableInode===String(artifact.ino));
    if(server.binding)need(server.binding===JSON.stringify(before));else server.binding=JSON.stringify(before);
    const all=listeners(serverPort),owned=listeners(serverPort,server.child.pid),after=processInfo(server.child.pid);need(JSON.stringify(before)===JSON.stringify(after));await tools();check();
    const exact=row=>row.pid===server.child.pid&&row.address==='127.0.0.1'&&row.port===serverPort&&!row.connected&&!row.unbound&&row.peerAddress===null&&row.peerPort===null;
    need(all.length<=1&&all.every(exact)&&owned.length<=1&&owned.every(exact));
    if(allowMissing&&(all.length===0||owned.length===0))return false;
    need(all.length===1&&owned.length===1&&JSON.stringify(all)===JSON.stringify(owned));return true;
  };
  let candidateResumePath;
  const api=async(path,options={})=>{const allowed=['/auth/signup/start','/auth/signup/confirm','/auth/login','/admin/jobs/public-beta','/api/jobs/beta-access','/api/jobs/workspace'];
    const candidateResumeRead=candidateResumePath!==undefined&&path===candidateResumePath&&options.method===undefined;
    const privateFixtureRoute=seedDurablePosting!==undefined&&(
      path==='/api/jobs/taxonomy'&&options.method===undefined
      || ['/api/jobs/onboarding/complete','/api/jobs/applications'].includes(path)&&options.method==='POST'
      || includeMockReceipt&&path==='/api/jobs/resume-source'&&options.method==='POST');
    need(allowed.includes(path)||candidateResumeRead||privateFixtureRoute);
    await verifyBackend({host:'127.0.0.1',port:serverPort});const response=await call(serverPort,path,options);
    if(candidateResumeRead||privateFixtureRoute)await verifyBackend({host:'127.0.0.1',port:serverPort});
    emit({event:'http',method:options.method||'GET',path:candidateResumeRead?'/api/jobs/resume-versions/:bound':path,status:response.status});check();need(response.status===200);
    try{return JSON.parse(response.bytes.toString('utf8'));}catch{throw new Error('local_qa_session_refused');}};
  const close=async()=>{
    if(closePromise)return closePromise;closed=true;
    closePromise=(async()=>{
      // Revoke concurrently with shutdown: ambiguous credential-file custody
      // must not keep the gateway alive or let the owner release a clean lease.
      const handoffClosed=Promise.resolve().then(()=>candidateHandoff?.revoke()).then(()=>true,()=>false);
      let safe=true,clean=true;
      if(mockReceiptActor){try{await mockReceiptActor.close();}catch{safe=false;}}
      if(gateway){try{await gateway.close();await portAvailable(port);gateway=undefined;emit({event:'gateway_closed',pid:process.pid,port});}catch{safe=false;}}
      if(server){try{const owned=server;if(owned.child.pid&&owned.child.exitCode===null&&owned.child.signalCode===null)need(owned.child.kill('SIGTERM'));
        const kill=setTimeout(()=>{if(owned.child.exitCode===null&&owned.child.signalCode===null)owned.child.kill('SIGKILL');},10000);
        let deadline;const result=await Promise.race([owned.terminal,new Promise(ok=>{deadline=setTimeout(()=>ok(null),15000);})]);clearTimeout(kill);clearTimeout(deadline);
        need(result);emit({event:'server_terminal',pid:owned.child.pid||null,port:serverPort,exitCode:result.code,signal:result.signal});const stderr=Buffer.concat(owned.stderr).toString('utf8');
        serverClass=[['purge_policy','validate managed runner-volume purge signing configuration'],['jobs_encryption','validate Jobs data encryption'],['config','load config'],['database','open postgres db'],['migration','run migrations'],['bind','bind 127.0.0.1:']].find(([,pattern])=>stderr.includes(pattern))?.[0]||'other';
        for(const chunk of owned.stderr)chunk.fill(0);owned.stderr=[];
        if(owned.child.pid)try{process.kill(owned.child.pid,0);throw new Error('live');}catch(error){need(error.code==='ESRCH');}
        await portAvailable(serverPort);server=undefined;emit({event:'server_reaped',pid:owned.child.pid||null,port:serverPort});if(result.code!==0||result.signal!==null||owned.overflow)clean=false;
      }catch{safe=false;}}
      if(safe&&smtp){try{const p=smtp.port;await smtp.close();await portAvailable(p);smtp=undefined;emit({event:'smtp_closed',pid:process.pid,port:p});}catch{safe=false;}}
      if(safe&&store){try{const p=Number(new URL(store.endpoint).port);await store.close();await portAvailable(p);store=undefined;emit({event:'object_closed',pid:process.pid,port:p});}catch{safe=false;}}
      if(safe&&pg){try{const p=pg.port;await pg.close();await portAvailable(p);pg=undefined;}catch{safe=false;}}
      if(seedDurablePosting){try{await seedDurablePosting.close();}catch{safe=false;}}
      if(!await handoffClosed)safe=false;
      if(safe&&lease){try{await releaseLocalQaSessionLease(stateDirectory,lease);lease=undefined;emit({event:'lease_released',pid:process.pid});}catch{safe=false;}}
      if(!safe)throw new Error('local_qa_session_cleanup_unproven');
      if(observerFailed)throw new Error('local_qa_session_observer_failed');
      need(clean);
    })();return closePromise;
  };
  try{
    if(fixtureOptions){
      const {createLocalQaDurablePostingSeeder,createLocalQaDurableMockReceiptActor}=
        await import('./local-qa-durable-posting-seeder.mjs');
      seedDurablePosting=await createLocalQaDurablePostingSeeder({root:fixtureRoot,stateDirectory,...fixtureOptions});
      if(includeMockReceipt){
        const {artifactPath,artifactSha256}=fixtureOptions;
        mockReceiptActor=await createLocalQaDurableMockReceiptActor({root:fixtureRoot,stateDirectory,artifactPath,artifactSha256});
      }
      need(typeof seedDurablePosting?.operation==='function'&&typeof seedDurablePosting.close==='function');
      need(await absent(stateDirectory));
    }
    check();if(mode==='bootstrap'&&await absent(stateDirectory))await initializeLocalQaState(stateDirectory);
    check();state=await inspectLocalQaState(stateDirectory);lease=await acquireLocalQaSessionLease(stateDirectory);
    emit({event:'lease_acquired',pid:process.pid});setPhase('state');
    const pgObserver=value=>{if(value.event==='started'||value.event==='identity_verified')emit({event:`postgres_${value.event}`,pid:value.pid,callerPGID:value.callerPGID,port:value.port,dataDirectory:value.dataDirectory,...(value.event==='identity_verified'?{systemIdentifier:value.systemIdentifier}:{})});
      else if(value.event==='exited')emit({event:'postgres_exited',pid:value.pid,exitCode:value.exitCode,signal:value.signal});else observerFailed=true;};
    if(mode==='bootstrap'){
      sessionConfig=await initializeLocalQaSessionState(stateDirectory,lease);
      need(!sessionConfig.bootstrapIntentPresent&&!sessionConfig.bootstrapReceiptPresent&&(await inspectLocalQaState(stateDirectory)).postgresInitialization==='unclaimed');
      setPhase('postgres_initialize');await initializeLocalQaPostgres({stateDirectory,onLifecycle:pgObserver});
    }else{sessionConfig=await inspectLocalQaSessionState(stateDirectory);need(sessionConfig.bootstrapIntentPresent&&sessionConfig.bootstrapReceiptPresent);}
    check();
    const secrets=await loadLocalQaSecrets(stateDirectory);sessionSecrets=await loadLocalQaSessionSecrets(stateDirectory);
    setPhase('postgres_start');pg=await startLocalQaPostgres({stateDirectory,onLifecycle:pgObserver,
      durableDraftFixture:seedDurablePosting!==undefined});check();
    let authority,expectedIds;
    if(mode==='bootstrap'){const result=await writeLocalQaBootstrapIntent(stateDirectory,lease,{systemIdentifier:pg.systemIdentifier});authority=result.receiptAuthority;need(authority);}
    else {
      if(mockRestart)need(pg.systemIdentifier===mockRestart.systemIdentifier);
      expectedIds=await requireLocalQaBootstrapReceipt(stateDirectory,pg.systemIdentifier);
    }
    setPhase('services_start');store=await startLocalQaObjectStore({directory:state.objectDirectory,qaId:state.qaId,bucket:state.bucket,prefix:state.prefix,accessKey:secrets.objectAccessKey,secret:secrets.objectSecret});emit({event:'object_started',pid:process.pid,port:Number(new URL(store.endpoint).port)});check();
    smtp=await startLocalQaSmtp({recipient:mode==='bootstrap'?sessionConfig.operatorEmail:sessionConfig.candidateEmail});emit({event:'smtp_started',pid:process.pid,port:smtp.port});check();
    serverPort=await ephemeralPort();await checkedFile(currentBinding.artifactPath,currentBinding.artifactSha256,1024*1024*1024,{executable:true,ownerOnly:true});
    const flags=['BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED','BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED','BLUEY_JOBS_MANAGED_CLOUD_RUNTIME_ENABLED','BLUEY_JOBS_WORKFLOW_COMMAND_DISPATCH_ENABLED','BLUEY_JOBS_WORKFLOW_COMMAND_RECONCILIATION_ENABLED','BLUEY_JOBS_WORKFLOW_CLEANUP_ENABLED','BLUEY_JOBS_COMMUNICATION_DISPATCH_ENABLED','BLUEY_JOBS_COMMUNICATION_RECONCILIATION_ENABLED','BLUEY_JOBS_MAILBOX_SYNC_ENABLED','BLUEY_JOBS_MODEL_GENERATION_ENABLED','BLUEY_JOBS_GLOBAL_ARCHIVE_ENABLED','BLUEY_JOBS_COMMUNICATION_OAUTH_WRITE_ENABLED'];
    const env={PATH:'/usr/bin:/bin',LANG:'C',LC_ALL:'C',TZ:'UTC',HOME:stateDirectory,TMPDIR:process.env.TMPDIR,RUST_LOG:'warn',BLUEY_API_HOST:'127.0.0.1',BLUEY_PORT:String(serverPort),BLUEY_PUBLIC_URL:`http://127.0.0.1:${port}`,
      BLUEY_JWT_SECRET:secrets.jwtSecret,BLUEY_JOBS_DATA_KEY:secrets.jobsDataKey,BLUEY_JOBS_BETA_ENABLED:'1',BLUEY_ADMIN_EMAILS:sessionConfig.operatorEmail,
      BLUEY_JOBS_RUNNER_PURGE_SIGNING_KEY_ID:sessionConfig.keyId,BLUEY_JOBS_RUNNER_PURGE_SIGNING_KEY:sessionSecrets.purgeSeed,BLUEY_JOBS_RUNNER_MINIMUM_BUILD_ID:sessionConfig.minimumBuild,
      BLUEY_JOBS_RUNNER_PURGE_VERIFYING_KEYS_JSON:JSON.stringify([{keyId:sessionConfig.keyId,publicKeyBase64url:sessionConfig.publicKey}]),...Object.fromEntries(flags.map(x=>[x,'0'])),...pg.env,...store.env,...smtp.env};
    await tools();const callerPGID=processInfo(process.pid).pgid;
    const child=spawn(currentBinding.artifactPath,[],{cwd:stateDirectory,env,stdio:['ignore','pipe','pipe']});let logSize=0,stderrSize=0;
    const owned={child,overflow:false,stderr:[]};server=owned;owned.terminal=new Promise(ok=>{child.once('error',()=>ok({code:null,signal:'spawn_error'}));child.once('close',(code,signal)=>ok({code,signal}));});
    for(const stream of [child.stdout,child.stderr])stream.on('data',bytes=>{logSize+=bytes.length;
      if(stream===child.stderr&&stderrSize<65536){const copy=Buffer.from(bytes.subarray(0,65536-stderrSize));owned.stderr.push(copy);stderrSize+=copy.length;}
      if(logSize>1024*1024&&!owned.overflow){owned.overflow=true;child.kill('SIGTERM');}});
    emit({event:'server_spawned',pid:child.pid||null,port:serverPort,callerPGID});check();
    let ready=false;for(let i=0;i<300;i++){check();need(child.exitCode===null&&child.signalCode===null&&!server.overflow);if(await verifyBackend({host:'127.0.0.1',port:serverPort},true)){ready=true;break;}await sleep(100);}need(ready);
    const signup=async(email,password,fingerprint)=>{await api('/auth/signup/start',{method:'POST',body:{email,password,terms_accepted:true,device_fingerprint:fingerprint}});
      const otp=smtp.takeCode('signup');need(typeof otp==='string'&&/^[0-9]{6}$/.test(otp));return api('/auth/signup/confirm',{method:'POST',body:{email,otp,device_fingerprint:fingerprint}});};
    setPhase('authentication');let operator,candidate;
    if(mode==='bootstrap'){
      operator=await signup(sessionConfig.operatorEmail,sessionSecrets.operatorPassword,'local-qa-operator');need(operator.account.is_admin===true&&operator.account.is_temporary===false&&operator.account.email===sessionConfig.operatorEmail);
      const cohort=await api('/admin/jobs/public-beta',{token:operator.access_token}),now=Date.now();
      await api('/admin/jobs/public-beta',{method:'PUT',token:operator.access_token,body:{expectedRevision:cohort.revision,state:'open',opensAtMs:now-60000,closesAtMs:now+7*86400000,hardCap:2}});
      const smtpPort=smtp.port;await smtp.close();await portAvailable(smtpPort);smtp=undefined;emit({event:'smtp_closed',pid:process.pid,port:smtpPort});check();smtp=await startLocalQaSmtp({recipient:sessionConfig.candidateEmail,port:smtpPort});emit({event:'smtp_started',pid:process.pid,port:smtp.port});check();
      candidate=await signup(sessionConfig.candidateEmail,sessionSecrets.candidatePassword,'local-qa-candidate');
      need(candidate.account.is_admin===false&&candidate.account.is_temporary===false&&candidate.account.email===sessionConfig.candidateEmail&&operator.account.id!==candidate.account.id);
      if(includeMockReceipt){
        const login=await api('/auth/login',{method:'POST',body:{email:sessionConfig.candidateEmail,password:sessionSecrets.candidatePassword}});
        need(login.account.id===candidate.account.id&&login.account.email===sessionConfig.candidateEmail
          &&login.account.is_admin===false&&login.account.is_temporary===false);
        candidate=login;
      }
    }else{
      operator=await api('/auth/login',{method:'POST',body:{email:sessionConfig.operatorEmail,password:sessionSecrets.operatorPassword}});
      candidate=await api('/auth/login',{method:'POST',body:{email:sessionConfig.candidateEmail,password:sessionSecrets.candidatePassword}});
      need(operator.account.is_admin===true&&candidate.account.is_admin===false&&operator.account.is_temporary===false&&candidate.account.is_temporary===false&&operator.account.email===sessionConfig.operatorEmail&&candidate.account.email===sessionConfig.candidateEmail&&operator.account.id===expectedIds.operatorId&&candidate.account.id===expectedIds.candidateId);
    }
    const beta=await api('/api/jobs/beta-access',{token:candidate.access_token});need(beta.access==='admitted');
    const workspace=await api('/api/jobs/workspace',{token:candidate.access_token});
    const mockReadback=mockRestart?await import('./local-qa-darwin-mock-receipt-journey.mjs'):undefined;
    const candidateReadback=mockReadback
      ?mockReadback.bindLocalQaMockReceiptReadback(expectedMockReceipt,candidate.account.id,workspace)
      :candidateReadOnly?bindLocalQaCandidateReadback(candidate.account.id,workspace,expected):undefined;
    if(candidateReadback?.resumeId){
      candidateResumePath=`/api/jobs/resume-versions/${candidateReadback.resumeId}`;
      const resume=await api(candidateResumePath,{token:candidate.access_token});
      if(mockReadback)mockReadback.assertLocalQaMockReceiptResume(expectedMockReceipt,resume);
      else assertLocalQaCandidateResume(workspace,resume);
    }
    if(mode==='bootstrap')await writeLocalQaBootstrapReceipt(stateDirectory,lease,{operatorId:operator.account.id,candidateId:candidate.account.id,systemIdentifier:pg.systemIdentifier,receiptAuthority:authority});
    check();await assertLocalQaSessionLease(stateDirectory,lease);
    if(includeMockReceipt){
      setPhase('durable_mock_fixture');
      const {runLocalQaDarwinMockReceiptJourney}=await import('./local-qa-darwin-mock-receipt-journey.mjs');
      const uuid=/^[a-f0-9]{8}-[a-f0-9]{4}-4[a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}$/;
      const owner=Object.freeze({host:'127.0.0.1',port:serverPort});
      const accountId=candidate.account.id, token=candidate.access_token;
      need(uuid.test(accountId)&&typeof token==='string');
      const fixtureCall=async(path,options={})=>{
        need(options&&Object.getPrototypeOf(options)===Object.prototype
          &&Object.keys(options).every(key=>['method','body','headers'].includes(key)));
        const reads=['/api/jobs/workspace','/api/jobs/taxonomy'];
        const writes=['/api/jobs/resume-source','/api/jobs/onboarding/complete','/api/jobs/applications'];
        need(options.method==='POST'?writes.includes(path):options.method===undefined
          &&(reads.includes(path)||(candidateResumePath!==undefined&&path===candidateResumePath)));
        const result=await api(path,{...options,token});
        if(path==='/api/jobs/applications'){
          const id=result?.application?.resume_version_id;
          need(candidateResumePath===undefined&&typeof id==='string'&&uuid.exec(id)?.[0]===id);
          candidateResumePath=`/api/jobs/resume-versions/${id}`;
        }
        return result;
      };
      const operationOptions=Object.freeze({systemIdentifier:pg.systemIdentifier,
        databaseUrl:pg.env.BLUEY_DATABASE_URL,dataKey:secrets.jobsDataKey});
      const evidence=await runLocalQaDarwinMockReceiptJourney({
        accountId,call:fixtureCall,
        async seedPosting(draft){
          need(draft.accountId===accountId);
          await verifyBackend(owner);
          try{
            const id=await seedDurablePosting.operation({...draft,...operationOptions},{signal});
            await verifyBackend(owner);
            return id;
          }catch(error){
            const code=/^local_qa_durable_fixture_[a-z_]{1,65}$/.test(error?.message??'')?error.message:'local_qa_durable_fixture_adapter_refused';
            emit({event:'fixture_refused',code});
            throw error;
          }
        },
        async publishMock(draft){
          need(draft.accountId===accountId&&candidateResumePath===`/api/jobs/resume-versions/${draft.resumeId}`);
          await verifyBackend(owner);
          store.admitMockReceipt({accountId,applicationId:draft.applicationId});
          const before=store.counters();
          let result;
          try{result=await mockReceiptActor.operation(draft,{...operationOptions,objectEnv:store.env,signal});}
          catch(error){
            const code=/^local_qa_durable_fixture_[a-z_]{1,65}$/.test(error?.message??'')?error.message:'local_qa_durable_fixture_adapter_refused';
            emit({event:'mock_fixture_refused',code});
            throw error;
          }
          await verifyBackend(owner);
          const after=store.counters();
          need(after.put===before.put+3&&after.get===before.get+3
            &&after.delete===before.delete&&after.denied===before.denied);
          return result;
        },
        async readCensus(draft,phase,runId){
          need(draft.accountId===accountId);
          await verifyBackend(owner);
          const result=await pg.readReviewFirstReceiptCensus(accountId,draft.applicationId,phase,runId);
          await verifyBackend(owner);
          return result;
        },
      }).catch(error=>{
        const stage=/^local_qa_darwin_mock_receipt_journey_refused:([a-z_]{1,32})$/.exec(error?.message??'')?.[1];
        emit({event:'mock_journey_refused',stage:stage??'unknown'});
        throw error;
      });
      const mail=smtp.counters();
      need(mail.captured===1&&mail.denied===0&&mail.duplicate===0&&smtp.takeCode('signup')===null);
      const systemIdentifier=pg.systemIdentifier;
      await close();
      // Hand the in-process owner an exact bootstrap result only after all
      // resources closed. A JSON copy cannot arm the candidate gateway.
      const latestExpiryMs=Date.now()+maximumCandidateWindowMs;
      mockReceiptRestarts.set(evidence,Object.freeze({stateDirectory,port,buildBinding:currentBinding,
        systemIdentifier,latestExpiryMs}));
      return Object.freeze({evidence,closed:true});
    }
    if(seedDurablePosting){
      setPhase('durable_fixture_prepare');
      const token=candidate.access_token;
      const prepare=createLocalQaDurableDraft({
        fixtureMode:fixtureOptions.fixtureMode,
        readWorkspace:()=>api('/api/jobs/workspace',{token}),
        readTaxonomy:()=>api('/api/jobs/taxonomy',{token}),
        saveOnboarding:(body,headers)=>api('/api/jobs/onboarding/complete',{method:'POST',body,headers,token}),
        async seedPosting(requested){
          const owned={host:'127.0.0.1',port:serverPort};
          await verifyBackend(owned);
          let jobId;
          try {
            jobId=await seedDurablePosting.operation({...requested,systemIdentifier:pg.systemIdentifier,
              databaseUrl:pg.env.BLUEY_DATABASE_URL,dataKey:secrets.jobsDataKey},{signal});
          } catch(error) {
            const code=typeof error?.message==='string'&&/^local_qa_durable_fixture_[a-z_]{1,65}$/.test(error.message)
              ?error.message:'local_qa_durable_fixture_adapter_refused';
            emit({event:'fixture_refused',code});
            throw error;
          }
          await verifyBackend(owned);
          return jobId;
        },
        prepareDraft:body=>api('/api/jobs/applications',{method:'POST',token,body}),
      });
      const preparedDraft=await prepare({accountId:candidate.account.id,candidateEmail:candidate.account.email});
      await close();
      return Object.freeze({preparedDraft,closed:true});
    }
    setPhase('gateway_start');
    gateway=await startLocalQaGateway({webDirectory:join(ROOT,'web'),manifest,backendPort:serverPort,
      port,candidateReadback,expiresAtMs:lifetime?.expiresAtMs,
      verifyBackend:async target=>{if(closed)return false;return verifyBackend(target);}});
    emit({event:'gateway_started',pid:process.pid,port});check();
    if(lifetime){
      const {origin}=gateway;
      const {candidateEmail:email}=sessionConfig;
      const ownerLifetime=mockReadback?{
        remaining:()=>lifetime.remaining()-candidateHandoffCleanupReserveMs,
      }:lifetime;
      // Only a qualified mock-receipt restart can offer explicit publication.
      // The trusted test owner calls it after its gateway readback. The path
      // is a fresh private leaf of the already bound disposable QA state.
      if(mockReadback)candidateHandoff=createLocalQaCandidateHandoffOwner({
        isActive:()=>!closed&&!signal?.aborted&&ownerLifetime.remaining()>0,
        async prepare(){
          check();
          await verifyBackend({host:'127.0.0.1',port:serverPort});
          const directory=join(stateDirectory,'candidate-handoff');
          await mkdir(directory,{mode:0o700});
          check();
          return {directory,origin,email,password:sessionSecrets.candidatePassword,expiresAtMs:lifetime.expiresAtMs};
        },
      });
      const owner=armLocalQaCandidateShutdown(ownerLifetime,signal,close);
      return Object.freeze({origin,email,expiresAtMs:lifetime.expiresAtMs,...owner,
        ...(candidateHandoff?{publishCandidateLogin:candidateHandoff.publish}:{})});
    }
    const result={origin:gateway.origin,email:sessionConfig.candidateEmail,configPath:join(stateDirectory,'.local-qa-session.json'),close};
    return Object.freeze(result);
  }catch{emit({event:'failure',stage:phase,code:observerFailed?'observer_failed':'refused'});try{await close();}catch(error){if(error?.message==='local_qa_session_cleanup_unproven')throw error;}emit({event:'failure_class',class:serverClass});throw new Error(`local_qa_session_start_failed_${serverClass}`);}
}
async function cli(){
  const [mode,...args]=process.argv.slice(2);need(['bootstrap','start','status'].includes(mode));const options={};
  while(args.length){const flag=args.shift();need(['--state-dir','--port','--manifest-file','--once'].includes(flag)&&!(flag in options));if(flag==='--once')options[flag]=true;else{need(args.length>0);options[flag]=args.shift();}}
  need(typeof options['--state-dir']==='string');
  if(mode==='status'){need(Object.keys(options).length===1);const state=await inspectLocalQaSessionState(options['--state-dir']);process.stdout.write(JSON.stringify({qaId:state.qaId,bootstrapIntentPresent:state.bootstrapIntentPresent,bootstrapReceiptPresent:state.bootstrapReceiptPresent,runtimeReady:false})+'\n');return;}
  need(typeof options['--port']==='string'&&/^[1-9][0-9]{0,4}$/.test(options['--port'])&&typeof options['--manifest-file']==='string');
  const controller=new AbortController(),handlers=new Map();
  for(const signal of ['SIGTERM','SIGINT','SIGHUP']){const handler=()=>controller.abort();handlers.set(signal,handler);process.on(signal,handler);}
  let session;
  const report=value=>{process.stdout.write(`LOCAL_QA_SESSION_EVENT ${JSON.stringify(value)}\n`);};
  try{session=await startLocalQaSession({mode,stateDirectory:options['--state-dir'],port:Number(options['--port']),manifestFile:options['--manifest-file'],signal:controller.signal,onLifecycle:report});
    if(options['--once'])for(const path of ['/jobs/','/login']){const response=await call(Number(options['--port']),path);report({event:'http',method:'GET',path,status:response.status});need(response.status===200);}
    process.stdout.write(JSON.stringify({origin:session.origin,email:session.email,configPath:session.configPath,qualificationOnly:true})+'\n');
    if(!options['--once'])await new Promise(done=>{const timer=setTimeout(done,2*60*60*1000);if(controller.signal.aborted){clearTimeout(timer);done();return;}controller.signal.addEventListener('abort',()=>{clearTimeout(timer);done();},{once:true});});
  }finally{try{if(session)await session.close();}finally{for(const [signal,handler] of handlers)process.off(signal,handler);}}
}
if(process.argv[1]&&resolve(process.argv[1])===fileURLToPath(import.meta.url))await cli().catch(error=>{process.stderr.write(error?.message==='local_qa_session_cleanup_unproven'?'local_qa_session_cleanup_unproven\n':'local_qa_session_failed\n');process.exitCode=1;});

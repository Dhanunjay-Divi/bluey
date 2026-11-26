import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { chmod, lstat, mkdtemp, readFile, realpath, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { armLocalQaCandidateShutdown, assertLocalQaCandidateResume, assertLocalQaExpectedDraft, bindLocalQaCandidateReadback,
  startLocalQaSession } from './local-qa-session.mjs';

const digest=value=>createHash('sha256').update(value).digest('hex');
const sourceSha256='c'.repeat(64);

async function fixture(t){
  const root=await realpath(await mkdtemp(join(tmpdir(),'local-qa-binding-order-')));
  const artifact=join(root,'artifact'),manifest=join(root,'manifest.json'),state=join(root,'state');
  await writeFile(artifact,'synthetic-artifact');await chmod(artifact,0o700);
  await writeFile(manifest,'{}\n');await chmod(manifest,0o600);
  t.after(()=>rm(root,{recursive:true,force:true}));
  assert.equal(await realpath(artifact),artifact);assert.equal(await realpath(manifest),manifest);
  assert.equal((await lstat(artifact)).mode&0o777,0o700);assert.equal((await lstat(manifest)).mode&0o777,0o600);
  assert.equal(digest(await readFile(artifact)),digest('synthetic-artifact'));assert.equal(digest(await readFile(manifest)),digest('{}\n'));
  return {artifact,manifest,state};
}

function binding(input){return {version:1,qualificationOnly:true,artifactPath:input.artifact,
  artifactSha256:digest('synthetic-artifact'),manifestFile:input.manifest,
  manifestSha256:digest('{}\n'),sourceSha256};}

async function stateAbsent(path){
  await assert.rejects(lstat(path),error=>error?.code==='ENOENT');
}

async function rejectsBeforeState(input,buildBinding,message,signal){
  let lifecycle=false;
  await assert.rejects(startLocalQaSession({mode:'bootstrap',stateDirectory:input.state,
    port:5217,buildBinding,signal,onLifecycle:()=>{lifecycle=true;}}),{message});
  await stateAbsent(input.state);
  assert.equal(lifecycle,false);
}

test('missing and malformed bindings reject before QA state or lifecycle work',async t=>{
  const input=await fixture(t);
  await rejectsBeforeState(input,undefined,'local_qa_build_binding_refused');
  await rejectsBeforeState(input,{...binding(input),extra:true},'local_qa_build_binding_refused');
});

test('artifact custody or hash failure rejects before QA state or lifecycle work',async t=>{
  const input=await fixture(t);
  await rejectsBeforeState(input,{...binding(input),artifactSha256:'0'.repeat(64)},'local_qa_artifact_binding_refused');
  await chmod(input.artifact,0o777);
  await rejectsBeforeState(input,binding(input),'local_qa_artifact_binding_refused');
});

test('manifest custody or hash failure rejects before QA state or lifecycle work',async t=>{
  const input=await fixture(t);
  await rejectsBeforeState(input,{...binding(input),manifestSha256:'0'.repeat(64)},'local_qa_manifest_binding_refused');
  await chmod(input.manifest,0o666);
  await rejectsBeforeState(input,binding(input),'local_qa_manifest_binding_refused');
});

test('valid custody controls reach the pre-abort check before manifest or state work',async t=>{
  const input=await fixture(t),controller=new AbortController();controller.abort();
  await rejectsBeforeState(input,binding(input),'local_qa_session_refused',controller.signal);
});

test('candidate readback binds only the exact unsubmitted review draft from workspace',()=>{
  const accountId='11111111-1111-4111-8111-111111111111';
  const resumeId='22222222-2222-4222-8222-222222222222';
  const matchPage={matches_page:{next_cursor:null}};
  const draft={state:'awaiting_review',submission_mode:'review_first',run_id:null,submitted_at_ms:null,
    resume_version_id:resumeId,receipt:{resume_version_id:resumeId,confirmation:null}};
  assert.deepEqual(bindLocalQaCandidateReadback(accountId,{...matchPage,applications:[]}),{accountId,resumeId:null});
  assert.deepEqual(bindLocalQaCandidateReadback(accountId,{...matchPage,applications:[draft]}),{accountId,resumeId});
  for(const change of [{state:'submitted'},{submission_mode:'auto_submit'},{run_id:'queued'},
    {submitted_at_ms:1},{resume_version_id:'another-resume'},{receipt:{...draft.receipt,confirmation:'sent'}}])
    assert.throws(()=>bindLocalQaCandidateReadback(accountId,{...matchPage,applications:[{...draft,...change}]}));
  assert.throws(()=>bindLocalQaCandidateReadback(accountId,{...matchPage,applications:[draft,draft]}));
  assert.throws(()=>bindLocalQaCandidateReadback(accountId,{matches_page:{next_cursor:'another-page'},applications:[]}));
});

test('retained candidate résumé must read back as the exact factual job snapshot',()=>{
  const id='22222222-2222-4222-8222-222222222222',job_id='synthetic-job',fingerprint='f'.repeat(64);
  const workspace={applications:[{job_id,resume_version_id:id,receipt:{job_snapshot_fingerprint:fingerprint}}]};
  const resume={id,job_id,mode:'factual',content:{provenance:{
    generated_for_job_id:job_id,job_snapshot_fingerprint:fingerprint}}};
  assert.doesNotThrow(()=>assertLocalQaCandidateResume(workspace,resume));
  for(const bad of [{...resume,id:'wrong'},{...resume,mode:'enhance'},{...resume,content:{provenance:{}}}])
    assert.throws(()=>assertLocalQaCandidateResume(workspace,bad));
});

test('restart proof refuses any draft other than the artifact prepared during bootstrap',()=>{
  const draft={applicationId:'11111111-1111-4111-8111-111111111111',
    resumeId:'22222222-2222-4222-8222-222222222222',jobId:'fixture-job',jobSnapshotFingerprint:'e'.repeat(64)};
  const application={id:draft.applicationId,resume_version_id:draft.resumeId,job_id:draft.jobId,
    receipt:{job_snapshot_fingerprint:draft.jobSnapshotFingerprint}};
  assert.doesNotThrow(()=>assertLocalQaExpectedDraft({applications:[application]},draft));
  for(const change of [{job_id:'other'}, {resume_version_id:'other'}, {receipt:{}}, {id:'other'}])
    assert.throws(()=>assertLocalQaExpectedDraft({applications:[{...application,...change}]},draft));
  assert.throws(()=>assertLocalQaExpectedDraft({applications:[]},draft));
});

test('C2C restart proof requires candidate handoff and no runner or authority',()=>{
  const accountId='33333333-3333-4333-8333-333333333333';
  const draft={applicationId:'11111111-1111-4111-8111-111111111111',fixtureMode:'c2c',
    resumeId:'22222222-2222-4222-8222-222222222222',jobId:'44444444-4444-4444-8444-444444444444',
    jobSnapshotFingerprint:'e'.repeat(64)};
  const eligibility={can_prepare:true,can_queue_local:false,can_queue_cloud:false,can_auto_submit:false,
    hard_failures:[],review_reasons:[{code:'c2c_submission_requires_candidate'}],
    passed_checks:['employment_type_allowed','engagement_type_allowed','track_employment_type_allowed',
      'track_engagement_type_allowed']};
  const receipt={job_snapshot_fingerprint:draft.jobSnapshotFingerprint,confirmation:null,resume_version_id:draft.resumeId,
    job_snapshot:{id:draft.jobId,employment_type:'contract c2c'},eligibility};
  const application={id:draft.applicationId,resume_version_id:draft.resumeId,job_id:draft.jobId,
    state:'awaiting_review',submission_mode:'review_first',run_id:null,submitted_at_ms:null,receipt};
  const workspace={applications:[application],browser_sessions:[],auto_submit_authorizations:[],
    matches_page:{next_cursor:null},matches:[{id:draft.jobId,employment_type:'contract c2c'}],
    entitlement:{plan:'free',local_browser:false,cloud_browser:false,used_packets:0}};
  assert.doesNotThrow(()=>assertLocalQaExpectedDraft(workspace,draft));
  assert.deepEqual(bindLocalQaCandidateReadback(accountId,workspace,draft),
    {accountId,resumeId:draft.resumeId,c2cMatchId:draft.jobId,c2cApplicationId:draft.applicationId});
  assert.deepEqual(bindLocalQaCandidateReadback(accountId,workspace),{accountId,resumeId:draft.resumeId});
  assert.throws(()=>assertLocalQaExpectedDraft(workspace,{...draft,fixtureMode:'w2'}));
  for(const change of [{browser_sessions:[{}]},{auto_submit_authorizations:[{}]},
    {applications:[{...application,state:'queued'}]},
    {applications:[{...application,receipt:{...receipt,approved_execution:{}}}]},
    {applications:[{...application,receipt:{...receipt,eligibility:{...eligibility,can_queue_cloud:true}}}]}])
    assert.throws(()=>bindLocalQaCandidateReadback(accountId,{...workspace,...change},draft));
  for(const matches of [[],[{id:accountId,employment_type:'contract c2c'}],
    [{id:draft.jobId,employment_type:'full_time w2'}],[...workspace.matches,{}]])
    assert.throws(()=>bindLocalQaCandidateReadback(accountId,{...workspace,matches},draft));
});

test('fixture bootstrap and expected prepared evidence cannot open an ordinary session',async t=>{
  const input=await fixture(t);
  const base={stateDirectory:input.state,port:5217,buildBinding:binding(input)};
  await assert.rejects(startLocalQaSession({...base,mode:'start',durableDraftFixture:{
    artifactPath:input.artifact,artifactSha256:digest('synthetic-artifact')}}),{message:'local_qa_session_refused'});
  for(const fixtureMode of ['c2c','arbitrary']){
    await assert.rejects(startLocalQaSession({...base,mode:fixtureMode==='c2c'?'start':'bootstrap',durableDraftFixture:{
      artifactPath:input.artifact,artifactSha256:digest('synthetic-artifact'),fixtureMode}}),{message:'local_qa_session_refused'});
  }
  await assert.rejects(startLocalQaSession({...base,mode:'bootstrap',expectedPreparedDraft:{}}),
    {message:'local_qa_session_refused'});
  await stateAbsent(input.state);
});

test('mock receipt bootstrap requires its own sealed fixture before any QA state',async t=>{
  const input=await fixture(t);
  let observed=false;
  const fixtureBinding={artifactPath:input.artifact,artifactSha256:digest('synthetic-artifact')};
  const base={mode:'bootstrap',stateDirectory:input.state,port:5217,buildBinding:binding(input),
    durableMockReceiptFixture:fixtureBinding,onLifecycle:()=>{observed=true;}};
  for(const overrides of [{mode:'start'},{durableDraftFixture:fixtureBinding},
    {durableMockReceiptFixture:{...fixtureBinding,fixtureMode:'w2'}},
    {durableMockReceiptFixture:null},{candidateReadOnly:true},
    {expectedPreparedDraft:{}}]){
    await assert.rejects(startLocalQaSession({...base,...overrides}),{message:'local_qa_session_refused'});
  }
  await stateAbsent(input.state);
  assert.equal(observed,false);
});

test('candidate read-only setting cannot activate during bootstrap',async t=>{
  const input=await fixture(t);
  await assert.rejects(startLocalQaSession({mode:'bootstrap',stateDirectory:input.state,port:5217,
    buildBinding:binding(input),candidateReadOnly:true}),{message:'local_qa_session_refused'});
  await stateAbsent(input.state);
});

test('mock receipt restart refuses a caller-supplied summary before any state or callbacks',async t=>{
  const input=await fixture(t);
  let observed=false;
  const base={mode:'start',stateDirectory:input.state,port:5217,buildBinding:binding(input),
    candidateReadOnly:true,candidateExpiresAtMs:Date.now()+30000,onLifecycle:()=>{observed=true;}};
  const summary=Object.freeze({sampleResumeImported:true,reviewFirstPrepared:true,
    mockReceiptPublished:true,applicationCount:1,submittedCount:1});
  for(const expectedMockReceipt of [summary,JSON.parse(JSON.stringify(summary)),null,'submitted']){
    await assert.rejects(startLocalQaSession({...base,expectedMockReceipt}),
      {message:'local_qa_session_refused'});
  }
  await stateAbsent(input.state);
  assert.equal(observed,false);
});

test('invalid or misplaced candidate deadlines refuse before state or callbacks',async t=>{
  const input=await fixture(t),now=Date.now();
  let invoked=false;
  const base={mode:'start',stateDirectory:input.state,port:5217,buildBinding:binding(input),
    onLifecycle:()=>{invoked=true;},candidateReadOnly:true};
  for(const candidateExpiresAtMs of [undefined,null,NaN,now-1,now+7201000,`${now+1000}`,1n,Symbol('deadline')]){
    await assert.rejects(startLocalQaSession({...base,candidateExpiresAtMs}),
      {message:'local_qa_session_refused'});
  }
  await assert.rejects(startLocalQaSession({...base,candidateReadOnly:false,candidateExpiresAtMs:now+1000}),
    {message:'local_qa_session_refused'});
  await stateAbsent(input.state);
  assert.equal(invoked,false);
});

test('candidate lifetime performs teardown once, exposes completion, and disarms abort',async()=>{
  const abort=new AbortController(),deadline=performance.now()+30;
  const {promise:started,resolve:begin}=Promise.withResolvers();
  const {promise:cleaned,resolve:finish}=Promise.withResolvers();
  let calls=0,settled=false;
  const owner=armLocalQaCandidateShutdown({remaining:()=>deadline-performance.now()},abort.signal,async()=>{
    calls++;begin();await cleaned;
  });
  void owner.termination.then(()=>{settled=true;});
  await started;
  abort.abort();
  const sameClose=owner.close();
  assert.equal(owner.close(),sameClose);
  assert.equal(settled,false);
  finish();
  await sameClose;
  assert.deepEqual(await owner.termination,{reason:'expired',closed:true});
  assert.equal(calls,1);
});

test('automatic candidate abort publishes failed teardown without hiding the close error',async()=>{
  const abort=new AbortController();
  const owner=armLocalQaCandidateShutdown({remaining:()=>60000},abort.signal,()=>{
    throw new Error('test_cleanup_refused');
  });
  abort.abort();
  assert.deepEqual(await owner.termination,{reason:'aborted',closed:false});
  await assert.rejects(owner.close(),{message:'test_cleanup_refused'});
});

test('manual candidate close cancels deadline and preserves first reason',async()=>{
  const abort=new AbortController();
  let calls=0;
  const owner=armLocalQaCandidateShutdown({remaining:()=>60000},abort.signal,()=>{calls++;});
  const close=owner.close();abort.abort();await close;
  assert.deepEqual(await owner.termination,{reason:'owner',closed:true});
  assert.equal(calls,1);
});

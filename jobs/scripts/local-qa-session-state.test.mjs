import test from 'node:test';
import assert from 'node:assert/strict';
import { createPrivateKey, createPublicKey } from 'node:crypto';
import { link, mkdtemp, readFile, realpath, rm, symlink, unlink, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { inspect } from 'node:util';
import { initializeLocalQaState, claimLocalQaPostgresInitialization } from './local-qa-state.mjs';
import { authenticateLocalQaLinuxApi } from './local-qa-linux-api-auth.mjs';
import { acquireLocalQaSessionLease, assertLocalQaSessionLease, releaseLocalQaSessionLease,
  initializeLocalQaSessionState as initialize, inspectLocalQaSessionState, loadLocalQaSessionSecrets,
  writeLocalQaBootstrapIntent, writeLocalQaBootstrapReceipt as writeReceipt, requireLocalQaBootstrapReceipt as requireReceipt } from './local-qa-session-state.mjs';

const operatorId='11111111-1111-4111-8111-111111111111',candidateId='22222222-2222-4222-8222-222222222222';
const systemIdentifier='18446744073709551615',held=new Map(),authorities=new Map(),poisoned=new Set();
const initializeLocalQaSessionState=path=>initialize(path,held.get(path));
const intent=async path=>{const result=await writeLocalQaBootstrapIntent(path,held.get(path),{systemIdentifier});if(result.receiptAuthority)authorities.set(path,result.receiptAuthority);return result.state;};
const writeLocalQaBootstrapReceipt=(path,ids)=>{
  const valid=ids.operatorId===operatorId&&ids.candidateId===candidateId;
  const receiptAuthority=valid?authorities.get(path):undefined;if(valid)authorities.delete(path);
  return writeReceipt(path,held.get(path),{...ids,systemIdentifier,receiptAuthority});
};
const requireLocalQaBootstrapReceipt=path=>requireReceipt(path,systemIdentifier);
async function fixture(t){
  const parent=await realpath(await mkdtemp(join(tmpdir(),'local-qa-session-test-'))),path=join(parent,'bluey-jobs-qa-session-test');
  await initializeLocalQaState(path);const lease=await acquireLocalQaSessionLease(path);held.set(path,lease);
  t.after(async()=>{if(!poisoned.has(path))await releaseLocalQaSessionLease(path,held.get(path));held.delete(path);authorities.delete(path);await rm(parent,{recursive:true});});return path;
}
test('session config is stable across inspect and idempotent init, with redacted secrets and matching keyring',async t=>{
  const path=await fixture(t),first=await initializeLocalQaSessionState(path),secrets=await loadLocalQaSessionSecrets(path);
  assert.deepEqual(await inspectLocalQaSessionState(path),first);assert.deepEqual(await initializeLocalQaSessionState(path),first);
  assert.equal(first.bootstrapReceiptPresent,false);assert.match(first.configSha256,/^[0-9a-f]{64}$/);
  assert.ok(secrets.operatorPassword!==secrets.candidatePassword,'synthetic passwords must differ');assert.deepEqual(Object.keys(secrets),[]);
  for(const rendered of [JSON.stringify(secrets),inspect(secrets)])for(const value of [secrets.operatorPassword,secrets.candidatePassword,secrets.purgeSeed])assert.equal(rendered.includes(value),false);
  const privateKey=createPrivateKey({key:Buffer.concat([Buffer.from('302e020100300506032b657004220420','hex'),Buffer.from(secrets.purgeSeed,'base64url')]),format:'der',type:'pkcs8'});
  assert.equal(createPublicKey(privateKey).export({format:'der',type:'spki'}).subarray(-32).toString('base64url'),first.publicKey);
  const again=await loadLocalQaSessionSecrets(path);assert.ok(again.purgeSeed===secrets.purgeSeed,'purge seed must remain stable');assert.ok(again.candidatePassword===secrets.candidatePassword,'candidate password must remain stable');
});
test('missing config after one-way PG claim cannot silently initialize',async t=>{
  const path=await fixture(t);await initializeLocalQaSessionState(path);await claimLocalQaPostgresInitialization(path);
  await unlink(join(path,'.local-qa-session.json'));await assert.rejects(initializeLocalQaSessionState(path));
});
test('bootstrap receipt requires claimed PG and distinct IDs; repeat is identical, conflict refuses',async t=>{
  const path=await fixture(t);await initializeLocalQaSessionState(path);
  await assert.rejects(writeLocalQaBootstrapReceipt(path,{operatorId,candidateId}));
  await claimLocalQaPostgresInitialization(path);await assert.rejects(writeLocalQaBootstrapReceipt(path,{operatorId,candidateId}));await intent(path);
  await assert.rejects(writeLocalQaBootstrapReceipt(path,{operatorId,candidateId:operatorId}));
  await assert.rejects(writeLocalQaBootstrapReceipt(path,{operatorId:'bad',candidateId}));
  const first=await writeLocalQaBootstrapReceipt(path,{operatorId,candidateId});assert.equal(first.bootstrapReceiptPresent,true);
  assert.deepEqual(await requireLocalQaBootstrapReceipt(path),{operatorId,candidateId});
  assert.deepEqual(await writeLocalQaBootstrapReceipt(path,{operatorId,candidateId}),first);
  await assert.rejects(writeLocalQaBootstrapReceipt(path,{operatorId:candidateId,candidateId:operatorId}));
});
test('missing receipt refuses start validation; partial receipt cannot be repaired implicitly',async t=>{
  const path=await fixture(t);await initializeLocalQaSessionState(path);await claimLocalQaPostgresInitialization(path);await intent(path);
  await assert.rejects(requireLocalQaBootstrapReceipt(path));
  await writeFile(join(path,'.local-qa-bootstrap.json'),'{"version":1', {mode:0o600});
  await assert.rejects(inspectLocalQaSessionState(path));await assert.rejects(writeLocalQaBootstrapReceipt(path,{operatorId,candidateId}));
});
test('validly shaped changed config cannot satisfy old receipt binding',async t=>{
  const path=await fixture(t);await initializeLocalQaSessionState(path);await claimLocalQaPostgresInitialization(path);await intent(path);
  await writeLocalQaBootstrapReceipt(path,{operatorId,candidateId});const file=join(path,'.local-qa-session.json');
  const value=JSON.parse(await readFile(file,'utf8'));value.candidatePassword='Qa-'+ 'a'.repeat(48);await writeFile(file,JSON.stringify(value)+'\n');
  await assert.rejects(inspectLocalQaSessionState(path));await assert.rejects(requireLocalQaBootstrapReceipt(path));
});
test('duplicate JSON keys and noncanonical config are refused',async t=>{
  const path=await fixture(t);await initializeLocalQaSessionState(path);const file=join(path,'.local-qa-session.json');
  const original=await readFile(file,'utf8');await writeFile(file,original.replace('{','{"version":1,'));
  await assert.rejects(inspectLocalQaSessionState(path));await writeFile(file,' '+original);await assert.rejects(loadLocalQaSessionSecrets(path));
});
test('symlink and hardlink substitutions refuse without replacing them',async t=>{
  const path=await fixture(t);await initializeLocalQaSessionState(path);const file=join(path,'.local-qa-session.json'),alias=join(path,'session-alias');
  await link(file,alias);await assert.rejects(inspectLocalQaSessionState(path));await unlink(alias);
  await unlink(file);await symlink('missing-target',file);await assert.rejects(initializeLocalQaSessionState(path));
});
test('exclusive lease prevents concurrent owner and forged mutation authority',async t=>{
  const path=await fixture(t),lease=held.get(path);await assertLocalQaSessionLease(path,lease);
  await assert.rejects(acquireLocalQaSessionLease(path));await assert.rejects(initialize(path,{}));
  await initializeLocalQaSessionState(path);await assertLocalQaSessionLease(path,lease);
});
test('changed lock is retained and cannot authorize mutation or release',async t=>{
  const path=await fixture(t),lease=held.get(path);await writeFile(join(path,'.local-qa-session.lock'),'{}\n');poisoned.add(path);
  await assert.rejects(assertLocalQaSessionLease(path,lease));await assert.rejects(initialize(path,lease));
  await assert.rejects(releaseLocalQaSessionLease(path,lease));await assert.rejects(acquireLocalQaSessionLease(path));
});
test('intent binds canonical system ID, and missing receipt blocks start',async t=>{
  const path=await fixture(t);await initializeLocalQaSessionState(path);await claimLocalQaPostgresInitialization(path);
  for(const id of ['0','01','18446744073709551616',1,'-1','1e3'])await assert.rejects(writeLocalQaBootstrapIntent(path,held.get(path),{systemIdentifier:id}));
  const first=await intent(path);assert.equal(first.bootstrapIntentPresent,true);assert.equal(first.bootstrapReceiptPresent,false);
  assert.deepEqual(await intent(path),first);await assert.rejects(writeLocalQaBootstrapIntent(path,held.get(path),{systemIdentifier:'12'}));
  await assert.rejects(requireLocalQaBootstrapReceipt(path));await writeLocalQaBootstrapReceipt(path,{operatorId,candidateId});
  await assert.rejects(requireReceipt(path,'12'));await unlink(join(path,'.local-qa-bootstrap.json'));await assert.rejects(requireLocalQaBootstrapReceipt(path));
});
test('browser Prepare has the real durable seed receipt before returning the signup callback', async t => {
  const path = await fixture(t);
  const sessionConfig = await initializeLocalQaSessionState(path);
  const sessionSecrets = await loadLocalQaSessionSecrets(path);
  await claimLocalQaPostgresInitialization(path);
  await intent(path);
  await assert.rejects(requireLocalQaBootstrapReceipt(path));

  const authReply = (email, isAdmin) => ({
    access_token: isAdmin ? 'synthetic.operator' : 'synthetic.candidate',
    refresh_token: 'synthetic.refresh',
    expires_in: 60,
    account: {
      id: isAdmin ? operatorId : candidateId, email, is_admin: isAdmin, is_temporary: false,
      balance_cents: 0, temporary_expires_at: null, trial_seconds_remaining: 0,
    },
  });
  const observed = await authenticateLocalQaLinuxApi({
    mode: 'bootstrap', sessionConfig, sessionSecrets,
    async call(route, options) {
      if (route === '/auth/signup/start') return {};
      if (route === '/auth/signup/confirm') return authReply(sessionConfig.operatorEmail, true);
      if (route === '/admin/jobs/public-beta') return { revision: 1 };
      if (route === '/auth/login') {
        assert.equal(options.body.email, sessionConfig.candidateEmail);
        return authReply(sessionConfig.candidateEmail, false);
      }
      if (route === '/api/jobs/beta-access') return { schemaVersion: 1, access: 'admitted', reason: 'admitted' };
      if (route === '/api/jobs/workspace') return { applications: [] };
      assert.fail('unexpected QA request');
    },
    async takeSignupCode() { return '000123'; },
    async switchSignupRecipient() { return true; },
    async readPreparedResume() { assert.fail('no prepared resume in this custody test'); },
    async commitBrowserBootstrap(ids) {
      await writeLocalQaBootstrapReceipt(path, ids);
      return true;
    },
    async signupCandidateInBrowser(_password, binder) {
      await assert.rejects(requireLocalQaBootstrapReceipt(path));
      assert.equal(await binder(candidateId), true);
      // The native initializer consumes this file before the browser journey returns.
      assert.deepEqual(await requireLocalQaBootstrapReceipt(path), { operatorId, candidateId });
      return { accountId: candidateId };
    },
  });
  assert.equal(observed.candidateId, candidateId);
  assert.equal(authorities.has(path), false);
});
test('receipt cannot survive lost or changed intent',async t=>{
  const path=await fixture(t);await initializeLocalQaSessionState(path);await claimLocalQaPostgresInitialization(path);await intent(path);
  await writeLocalQaBootstrapReceipt(path,{operatorId,candidateId});await unlink(join(path,'.local-qa-bootstrap-intent.json'));
  await assert.rejects(inspectLocalQaSessionState(path));await assert.rejects(requireLocalQaBootstrapReceipt(path));
  await assert.rejects(writeLocalQaBootstrapIntent(path,held.get(path),{systemIdentifier}));
});
test('partial intent cannot create new receipt authority after lease reacquisition',async t=>{
  const path=await fixture(t);await initializeLocalQaSessionState(path);await claimLocalQaPostgresInitialization(path);await intent(path);
  const oldLease=held.get(path),oldAuthority=authorities.get(path);await releaseLocalQaSessionLease(path,oldLease);
  const newLease=await acquireLocalQaSessionLease(path);held.set(path,newLease);
  const replay=await writeLocalQaBootstrapIntent(path,newLease,{systemIdentifier});assert.equal(replay.receiptAuthority,null);
  await assert.rejects(writeReceipt(path,newLease,{operatorId,candidateId,systemIdentifier,receiptAuthority:oldAuthority}));
  await assert.rejects(writeReceipt(path,newLease,{operatorId,candidateId,systemIdentifier}));await assert.rejects(requireLocalQaBootstrapReceipt(path));
});
test('lost receipt cannot be recreated with consumed authority or later lease',async t=>{
  const path=await fixture(t);await initializeLocalQaSessionState(path);await claimLocalQaPostgresInitialization(path);await intent(path);
  const consumed=authorities.get(path);await writeLocalQaBootstrapReceipt(path,{operatorId,candidateId});
  await unlink(join(path,'.local-qa-bootstrap.json'));
  await assert.rejects(writeReceipt(path,held.get(path),{operatorId,candidateId,systemIdentifier,receiptAuthority:consumed}));
  await releaseLocalQaSessionLease(path,held.get(path));const lease=await acquireLocalQaSessionLease(path);held.set(path,lease);
  assert.equal((await writeLocalQaBootstrapIntent(path,lease,{systemIdentifier})).receiptAuthority,null);
  await assert.rejects(writeReceipt(path,lease,{operatorId,candidateId,systemIdentifier}));
});
test('forged authority refuses and failed commit consumes the real capability',async t=>{
  const path=await fixture(t);await initializeLocalQaSessionState(path);await claimLocalQaPostgresInitialization(path);await intent(path);
  const lease=held.get(path),authority=authorities.get(path);
  await assert.rejects(writeReceipt(path,lease,{operatorId,candidateId,systemIdentifier,receiptAuthority:{}}));
  await assert.rejects(writeReceipt(path,{}, {operatorId,candidateId,systemIdentifier,receiptAuthority:authority}));
  const pending=join(path,'.local-qa-session-pending-test');await writeFile(pending,'x',{mode:0o600});
  await assert.rejects(writeReceipt(path,lease,{operatorId,candidateId,systemIdentifier,receiptAuthority:authority}));
  await unlink(pending);
  await assert.rejects(writeReceipt(path,lease,{operatorId,candidateId,systemIdentifier,receiptAuthority:authority}));
  await assert.rejects(requireLocalQaBootstrapReceipt(path));
});

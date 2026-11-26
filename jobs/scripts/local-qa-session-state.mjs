// Private session configuration, not evidence of a running or ready service.
// The OS owner is trusted; these path APIs do not defeat hostile same-UID races.
import { createHash, createPrivateKey, createPublicKey, generateKeyPairSync, randomBytes, randomUUID } from 'node:crypto';
import { constants } from 'node:fs';
import { link, lstat, open, readdir, realpath, unlink } from 'node:fs/promises';
import { join } from 'node:path';
import { inspect } from 'node:util';
import { inspectLocalQaState, loadLocalQaSecrets } from './local-qa-state.mjs';

const CONFIG='.local-qa-session.json', INTENT='.local-qa-bootstrap-intent.json', RECEIPT='.local-qa-bootstrap.json', LOCK='.local-qa-session.lock', TEMP='.local-qa-session-pending-';
const leases=new WeakMap(),receiptAuthorities=new WeakMap();
const UUID=/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
const SHA=/^[0-9a-f]{64}$/;
const PRIVATE_PREFIX=Buffer.from('302e020100300506032b657004220420','hex');
const PUBLIC_PREFIX=Buffer.from('302a300506032b6570032100','hex');
const exact=value=>JSON.stringify(value)+'\n';
const digest=value=>createHash('sha256').update(value).digest('hex');
const fail=()=>{throw new Error('local_qa_session_refused');};
const need=value=>{if(!value)fail();};
const identity=m=>[m.dev,m.ino,m.uid,m.mode].map(String).join(':');
const fileIdentity=m=>[m.dev,m.ino,m.uid,m.mode,m.nlink,m.size,m.mtimeNs,m.ctimeNs].map(String).join(':');
async function missing(path){try{await lstat(path);return false;}catch(error){if(error.code==='ENOENT')return true;throw error;}}
async function syncDirectory(path){const file=await open(path,constants.O_RDONLY|constants.O_DIRECTORY|constants.O_NOFOLLOW);try{await file.sync();}finally{await file.close();}}
async function context(path){
  const state=await inspectLocalQaState(path);await loadLocalQaSecrets(path);
  need(await realpath(path)===path);const root=await lstat(path,{bigint:true});
  need(root.isDirectory()&&root.uid===BigInt(process.getuid())&&(root.mode&0o7777n)===0o700n);
  const pending=(await readdir(path)).filter(name=>name.startsWith(TEMP));need(pending.length===0);
  return {state,root:identity(root)};
}
async function unchanged(path,ctx){
  const state=await inspectLocalQaState(path),root=await lstat(path,{bigint:true});
  need(state.qaId===ctx.state.qaId&&state.postgresInitialization===ctx.state.postgresInitialization&&identity(root)===ctx.root);
}
async function readFile(path){
  const file=await open(path,constants.O_RDONLY|constants.O_NOFOLLOW);
  try{const before=await file.stat({bigint:true});need(before.isFile()&&before.uid===BigInt(process.getuid())&&before.nlink===1n&&(before.mode&0o7777n)===0o600n&&before.size>0n&&before.size<=8192n);
    const buffer=Buffer.alloc(8193);let size=0;
    for(;;){const result=await file.read(buffer,size,buffer.length-size,null);size+=result.bytesRead;need(size<=8192);if(!result.bytesRead)break;}
    const bytes=buffer.subarray(0,size);need(BigInt(size)===before.size&&fileIdentity(await file.stat({bigint:true}))===fileIdentity(before)&&fileIdentity(await lstat(path,{bigint:true}))===fileIdentity(before));return bytes;
  }finally{await file.close();}
}
export async function acquireLocalQaSessionLease(path){
  const ctx=await context(path),lock=join(path,LOCK),bytes=Buffer.from(exact({version:1,qaId:ctx.state.qaId,root:ctx.root,pid:process.pid,nonce:randomBytes(16).toString('hex')}));
  const file=await open(lock,constants.O_WRONLY|constants.O_CREAT|constants.O_EXCL|constants.O_NOFOLLOW,0o600);
  let saved;
  try{await file.writeFile(bytes);await file.sync();saved=await file.stat({bigint:true});
    need(saved.isFile()&&saved.uid===BigInt(process.getuid())&&saved.nlink===1n&&(saved.mode&0o7777n)===0o600n&&saved.size===BigInt(bytes.length));
  }finally{await file.close();}
  await syncDirectory(path);need(fileIdentity(await lstat(lock,{bigint:true}))===fileIdentity(saved)&&(await readFile(lock)).equals(bytes));await unchanged(path,ctx);
  const lease=Object.freeze({});leases.set(lease,{path,qaId:ctx.state.qaId,root:ctx.root,lockIdentity:fileIdentity(saved),bytes,busy:false});return lease;
}
export async function assertLocalQaSessionLease(path,lease){
  const record=leases.get(lease);need(record&&record.path===path);
  const state=await inspectLocalQaState(path),root=await lstat(path,{bigint:true}),stat=await lstat(join(path,LOCK),{bigint:true});
  need(state.qaId===record.qaId&&identity(root)===record.root&&fileIdentity(stat)===record.lockIdentity&&(await readFile(join(path,LOCK))).equals(record.bytes));
}
export async function releaseLocalQaSessionLease(path,lease){
  const record=leases.get(lease);need(record&&!record.busy);record.busy=true;
  try{await assertLocalQaSessionLease(path,lease);await unlink(join(path,LOCK));await syncDirectory(path);need(await missing(join(path,LOCK)));leases.delete(lease);}
  finally{record.busy=false;}
}
async function withLease(path,lease,operation){
  const record=leases.get(lease);need(record&&!record.busy);record.busy=true;
  try{await assertLocalQaSessionLease(path,lease);return await operation();}finally{record.busy=false;}
}
function parse(bytes,keys){
  let value;try{value=JSON.parse(bytes.toString('utf8'));}catch{fail();}
  need(value&&typeof value==='object'&&!Array.isArray(value)&&Object.keys(value).join(',')===keys.join(',')&&exact(value)===bytes.toString('utf8'));
  return value;
}
function validateConfig(bytes,ctx){
  const value=parse(bytes,['version','qaId','root','operatorEmail','candidateEmail','operatorPassword','candidatePassword','keyId','seed','publicKey','minimumBuild']);
  need(value.version===1&&value.qaId===ctx.state.qaId&&value.root===ctx.root&&UUID.test(value.qaId)&&
    value.operatorEmail==='operator@bluey-qa.test'&&value.candidateEmail==='candidate@bluey-qa.test'&&
    /^Qa-[0-9a-f]{48}$/.test(value.operatorPassword)&&/^Qa-[0-9a-f]{48}$/.test(value.candidatePassword)&&value.operatorPassword!==value.candidatePassword&&
    /^qa-purge-[0-9a-f-]{36}$/.test(value.keyId)&&UUID.test(value.keyId.slice(9))&&value.minimumBuild==='runner-630.0'&&
    /^[A-Za-z0-9_-]{43}$/.test(value.seed)&&/^[A-Za-z0-9_-]{43}$/.test(value.publicKey));
  const seed=Buffer.from(value.seed,'base64url'),publicBytes=Buffer.from(value.publicKey,'base64url');
  need(seed.length===32&&seed.toString('base64url')===value.seed&&publicBytes.length===32&&publicBytes.toString('base64url')===value.publicKey);
  const privateKey=createPrivateKey({key:Buffer.concat([PRIVATE_PREFIX,seed]),format:'der',type:'pkcs8'});
  const derived=createPublicKey(privateKey).export({format:'der',type:'spki'});
  need(derived.length===PUBLIC_PREFIX.length+32&&derived.subarray(0,PUBLIC_PREFIX.length).equals(PUBLIC_PREFIX)&&derived.subarray(PUBLIC_PREFIX.length).equals(publicBytes));seed.fill(0);
  return value;
}
function validSystemId(value){return typeof value==='string'&&/^[1-9][0-9]{0,19}$/.test(value)&&BigInt(value)<=18446744073709551615n;}
function validateIntent(bytes,ctx,configHash){
  const value=parse(bytes,['version','qaId','root','configSha256','systemIdentifier']);
  need(value.version===1&&value.qaId===ctx.state.qaId&&value.root===ctx.root&&value.configSha256===configHash&&validSystemId(value.systemIdentifier)&&ctx.state.postgresInitialization==='claimed');return value;
}
function validateReceipt(bytes,ctx,configHash,intent,intentHash){
  const value=parse(bytes,['version','qaId','root','configSha256','intentSha256','systemIdentifier','operatorId','candidateId']);
  need(value.version===1&&value.qaId===ctx.state.qaId&&value.root===ctx.root&&SHA.test(value.configSha256)&&value.configSha256===configHash&&
    intent&&value.intentSha256===intentHash&&value.systemIdentifier===intent.systemIdentifier&&UUID.test(value.operatorId)&&UUID.test(value.candidateId)&&value.operatorId!==value.candidateId&&ctx.state.postgresInitialization==='claimed');
  return value;
}
async function load(path){
  const ctx=await context(path),bytes=await readFile(join(path,CONFIG)),value=validateConfig(bytes,ctx),configSha256=digest(bytes);
  let intent=null,intentSha256=null,receipt=null;
  if(!await missing(join(path,INTENT))){const raw=await readFile(join(path,INTENT));intent=validateIntent(raw,ctx,configSha256);intentSha256=digest(raw);}
  if(!await missing(join(path,RECEIPT)))receipt=validateReceipt(await readFile(join(path,RECEIPT)),ctx,configSha256,intent,intentSha256);
  await unchanged(path,ctx);return {ctx,value,configSha256,intent,intentSha256,receipt};
}
async function writeOnce(path,name,bytes,ctx,lease){
  const temp=join(path,TEMP+randomBytes(16).toString('hex')),destination=join(path,name);let file,created;
  try{
    file=await open(temp,constants.O_WRONLY|constants.O_CREAT|constants.O_EXCL|constants.O_NOFOLLOW,0o600);
    created=await file.stat({bigint:true});need(created.isFile()&&created.nlink===1n&&created.uid===BigInt(process.getuid()));
    await file.writeFile(bytes);await file.sync();const completed=await file.stat({bigint:true});await file.close();file=undefined;
    need(completed.dev===created.dev&&completed.ino===created.ino&&completed.isFile()&&completed.nlink===1n&&completed.uid===BigInt(process.getuid())&&(completed.mode&0o7777n)===0o600n&&completed.size===BigInt(bytes.length));
    await unchanged(path,ctx);await assertLocalQaSessionLease(path,lease);need(fileIdentity(await lstat(temp,{bigint:true}))===fileIdentity(completed));
    await link(temp,destination);await unlink(temp);created=undefined;await syncDirectory(path);
    need((await readFile(destination)).equals(bytes));await unchanged(path,ctx);await assertLocalQaSessionLease(path,lease);
  }finally{
    if(file)await file.close();
    if(created){try{const current=await lstat(temp,{bigint:true});if(current.dev===created.dev&&current.ino===created.ino)await unlink(temp);}catch(error){if(error.code!=='ENOENT')throw error;}}
  }
}
function publicState(loaded){return Object.freeze({qaId:loaded.value.qaId,operatorEmail:loaded.value.operatorEmail,candidateEmail:loaded.value.candidateEmail,
  keyId:loaded.value.keyId,publicKey:loaded.value.publicKey,minimumBuild:loaded.value.minimumBuild,configSha256:loaded.configSha256,bootstrapIntentPresent:Boolean(loaded.intent),bootstrapReceiptPresent:Boolean(loaded.receipt)});}
export async function initializeLocalQaSessionState(path,lease){return withLease(path,lease,async()=>{
  const ctx=await context(path),config=join(path,CONFIG);
  if(!await missing(config))return inspectLocalQaSessionState(path);
  need(ctx.state.postgresInitialization==='unclaimed'&&await missing(join(path,INTENT))&&await missing(join(path,RECEIPT)));
  const {privateKey,publicKey}=generateKeyPairSync('ed25519');
  const privateDer=privateKey.export({format:'der',type:'pkcs8'}),publicDer=publicKey.export({format:'der',type:'spki'});
  need(privateDer.length===PRIVATE_PREFIX.length+32&&privateDer.subarray(0,PRIVATE_PREFIX.length).equals(PRIVATE_PREFIX)&&publicDer.length===PUBLIC_PREFIX.length+32&&publicDer.subarray(0,PUBLIC_PREFIX.length).equals(PUBLIC_PREFIX));
  const value={version:1,qaId:ctx.state.qaId,root:ctx.root,operatorEmail:'operator@bluey-qa.test',candidateEmail:'candidate@bluey-qa.test',
    operatorPassword:'Qa-'+randomBytes(24).toString('hex'),candidatePassword:'Qa-'+randomBytes(24).toString('hex'),keyId:'qa-purge-'+randomUUID(),
    seed:privateDer.subarray(PRIVATE_PREFIX.length).toString('base64url'),publicKey:publicDer.subarray(PUBLIC_PREFIX.length).toString('base64url'),minimumBuild:'runner-630.0'};
  privateDer.fill(0);const bytes=Buffer.from(exact(value));validateConfig(bytes,ctx);await writeOnce(path,CONFIG,bytes,ctx,lease);bytes.fill(0);
  return inspectLocalQaSessionState(path);
});}
export async function inspectLocalQaSessionState(path){return publicState(await load(path));}
export async function loadLocalQaSessionSecrets(path){
  const {value}=await load(path),secrets={};
  for(const [key,item] of Object.entries({operatorPassword:value.operatorPassword,candidatePassword:value.candidatePassword,purgeSeed:value.seed}))Object.defineProperty(secrets,key,{value:item,enumerable:false});
  Object.defineProperty(secrets,'toJSON',{value:()=> '[Local QA session secrets redacted]'});
  Object.defineProperty(secrets,inspect.custom,{value:()=> '[Local QA session secrets redacted]'});return Object.freeze(secrets);
}
export async function writeLocalQaBootstrapIntent(path,lease,{systemIdentifier}){return withLease(path,lease,async()=>{
  const loaded=await load(path);need(loaded.ctx.state.postgresInitialization==='claimed'&&validSystemId(systemIdentifier));
  if(loaded.intent){need(loaded.intent.systemIdentifier===systemIdentifier);return Object.freeze({state:publicState(loaded),receiptAuthority:null});}
  need(!loaded.receipt);
  const value={version:1,qaId:loaded.ctx.state.qaId,root:loaded.ctx.root,configSha256:loaded.configSha256,systemIdentifier};
  const bytes=Buffer.from(exact(value));validateIntent(bytes,loaded.ctx,loaded.configSha256);await writeOnce(path,INTENT,bytes,loaded.ctx,lease);
  const current=await load(path);need(current.intentSha256===digest(bytes)&&current.configSha256===loaded.configSha256&&current.intent.systemIdentifier===systemIdentifier&&!current.receipt);
  const receiptAuthority=Object.freeze({});receiptAuthorities.set(receiptAuthority,{path,lease,intentSha256:current.intentSha256,configSha256:current.configSha256,systemIdentifier});
  return Object.freeze({state:publicState(current),receiptAuthority});
});}
export async function writeLocalQaBootstrapReceipt(path,lease,{operatorId,candidateId,systemIdentifier,receiptAuthority}){return withLease(path,lease,async()=>{
  let authority=null;
  if(receiptAuthority!==undefined&&receiptAuthority!==null){authority=receiptAuthorities.get(receiptAuthority);
    need(authority&&authority.path===path&&authority.lease===lease);receiptAuthorities.delete(receiptAuthority);}
  const loaded=await load(path);need(loaded.ctx.state.postgresInitialization==='claimed'&&UUID.test(operatorId)&&UUID.test(candidateId)&&operatorId!==candidateId);
  need(loaded.intent&&validSystemId(systemIdentifier)&&loaded.intent.systemIdentifier===systemIdentifier);
  if(loaded.receipt){need(loaded.receipt.operatorId===operatorId&&loaded.receipt.candidateId===candidateId);return publicState(loaded);}
  need(authority&&authority.intentSha256===loaded.intentSha256&&authority.configSha256===loaded.configSha256&&authority.systemIdentifier===systemIdentifier);
  const value={version:1,qaId:loaded.ctx.state.qaId,root:loaded.ctx.root,configSha256:loaded.configSha256,intentSha256:loaded.intentSha256,systemIdentifier,operatorId,candidateId};
  const bytes=Buffer.from(exact(value));validateReceipt(bytes,loaded.ctx,loaded.configSha256,loaded.intent,loaded.intentSha256);await writeOnce(path,RECEIPT,bytes,loaded.ctx,lease);
  return inspectLocalQaSessionState(path);
});}
export async function requireLocalQaBootstrapReceipt(path,systemIdentifier){const loaded=await load(path);need(loaded.receipt&&validSystemId(systemIdentifier)&&loaded.receipt.systemIdentifier===systemIdentifier);return Object.freeze({operatorId:loaded.receipt.operatorId,candidateId:loaded.receipt.candidateId});}

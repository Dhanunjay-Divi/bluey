import { resolve } from 'node:path';

const FIELDS=['artifactPath','artifactSha256','manifestFile','manifestSha256','qualificationOnly','sourceSha256','version'];
const need=value=>{if(!value)throw new Error('local_qa_build_binding_refused');};

// The caller is a separately admitted local qualification controller. This
// validates its closed input shape only; it is not production provenance,
// network/public authority, or a server route contract.
export function parseLocalQaBuildBinding(value){
  need(value&&typeof value==='object'&&!Array.isArray(value)&&Object.getPrototypeOf(value)===Object.prototype);
  need(Object.keys(value).sort().join(',')===FIELDS.join(','));
  const path=value=>typeof value==='string'&&value.length>0&&value.indexOf('\0')<0&&resolve(value)===value;
  const hash=value=>typeof value==='string'&&/^[0-9a-f]{64}$/.test(value);
  need(value.version===1&&value.qualificationOnly===true&&path(value.artifactPath)&&hash(value.artifactSha256)&&path(value.manifestFile)&&hash(value.manifestSha256)&&hash(value.sourceSha256));
  return Object.freeze({...value});
}

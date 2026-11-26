import assert from 'node:assert/strict';
import test from 'node:test';
import { parseLocalQaBuildBinding } from './local-qa-build-binding.mjs';

const hash='a'.repeat(64);
const binding=Object.freeze({
  version:1,
  qualificationOnly:true,
  artifactPath:'/private/tmp/qualified/bluey-server',
  artifactSha256:hash,
  manifestFile:'/private/tmp/qualified/web-manifest.json',
  manifestSha256:hash,
  sourceSha256:hash,
});

test('accepts the exact version-one local qualification binding schema',()=>{
  const parsed=parseLocalQaBuildBinding(binding);
  assert.notEqual(parsed,binding);
  assert.equal(Object.isFrozen(parsed),true);
  assert.deepEqual(parsed,binding);
});

test('rejects missing, extra, malformed, and non-qualification bindings',()=>{
  const invalid=[
    {},
    {...binding,extra:true},
    {...binding,version:2},
    {...binding,qualificationOnly:false},
    {...binding,artifactPath:'relative/bluey-server'},
    {...binding,manifestFile:'/private/tmp/qualified/../web-manifest.json'},
    {...binding,artifactSha256:'A'.repeat(64)},
    {...binding,manifestSha256:'a'.repeat(63)},
    {...binding,sourceSha256:'not-a-sha256'},
  ];
  for(const value of invalid)assert.throws(()=>parseLocalQaBuildBinding(value));
});

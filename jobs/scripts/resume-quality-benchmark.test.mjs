import assert from 'node:assert/strict';
import test from 'node:test';
import { compareResumeQuality } from './resume-quality-benchmark.mjs';

const rating = (evidencedFit, extra) => ({ evidencedFit, unsupportedClaim: false,
  protectedFactChanged: false, misleadingOmission: false, ...extra });
const pair = (id, baseline, tailored) => ({ id, baseline, tailored });

test('compares paired fit, reports both arms and does not count a factual failure as a win', () => {
  assert.deepEqual(compareResumeQuality([
    pair('a', rating(2), rating(3)), pair('b', rating(3), rating(3)), pair('c', rating(4), rating(3)),
    pair('d', rating(0), rating(4, { unsupportedClaim: true })),
    pair('e', rating(1, { misleadingOmission: true }), rating(4)),
    pair('f', rating(1), rating(4, { protectedFactChanged: true })),
  ]), { pairs: 6, better: 1, tied: 1, worse: 1, ineligible: 3,
    failures: { baseline: 1, tailored: 2 }, factualSafetyPassed: false, directionalFitImprovement: false });
});

test('a raw majority of fit wins cannot overcome one unsafe tailored résumé', () => {
  const win = pair('a', rating(0), rating(4));
  assert.equal(compareResumeQuality([win]).directionalFitImprovement, true);
  const blocked = compareResumeQuality([win, pair('b', rating(0), rating(4, { misleadingOmission: true }))]);
  assert.ok(blocked.better > blocked.worse);
  assert.equal(blocked.directionalFitImprovement, false);
});

test('rejects empty, duplicate, incomplete and invalid comparisons', () => {
  const complete = pair('a', rating(0), rating(4));
  assert.equal(compareResumeQuality([complete]).factualSafetyPassed, true);
  for (const input of [[], [complete, complete], [pair('b', rating(1))], [pair('a', rating(1), rating(5))],
    [pair('b', rating(1), rating(4, { unsupportedClaim: 'false' }))]]) {
    assert.throws(() => compareResumeQuality(input), /resume_benchmark_/);
  }
});

// Offline aggregation of blinded human ratings. No model calls or application writes.
export function compareResumeQuality(pairs) {
  if (!Array.isArray(pairs) || pairs.length === 0) throw new Error('resume_benchmark_empty');
  const seen = new Set();
  const count = { pairs: pairs.length, better: 0, tied: 0, worse: 0, ineligible: 0 };
  const failures = { baseline: 0, tailored: 0 };
  for (const pair of pairs) {
    if (typeof pair?.id !== 'string' || !/^[a-z0-9_-]{1,64}$/.test(pair.id) || seen.has(pair.id)) {
      throw new Error('resume_benchmark_pair_identity');
    }
    seen.add(pair.id);
    const { baseline, tailored } = pair;
    for (const rating of [baseline, tailored]) {
      if (!Number.isSafeInteger(rating?.evidencedFit) || rating.evidencedFit < 0 || rating.evidencedFit > 4
        || typeof rating.unsupportedClaim !== 'boolean' || typeof rating.protectedFactChanged !== 'boolean'
        || typeof rating.misleadingOmission !== 'boolean') throw new Error('resume_benchmark_rating');
    }
    const disqualified = rating => rating.unsupportedClaim || rating.protectedFactChanged || rating.misleadingOmission;
    const baselineFails = disqualified(baseline);
    const tailoredFails = disqualified(tailored);
    failures.baseline += Number(baselineFails);
    failures.tailored += Number(tailoredFails);
    if (baselineFails || tailoredFails) count.ineligible += 1;
    else if (tailored.evidencedFit > baseline.evidencedFit) count.better += 1;
    else if (tailored.evidencedFit < baseline.evidencedFit) count.worse += 1;
    else count.tied += 1;
  }
  const factualSafetyPassed = failures.tailored === 0;
  return { ...count, failures, factualSafetyPassed,
    directionalFitImprovement: factualSafetyPassed && count.better > count.worse };
}

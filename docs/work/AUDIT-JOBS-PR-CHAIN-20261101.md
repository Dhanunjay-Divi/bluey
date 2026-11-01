# Jobs PR chain — read-only November1 refresh

> **Codex preflight:** `$bluey-ops`, `$github-automation`; current source and
> connected exact-head GitHub evidence govern. No SSD/history archive needed.

## Scope

Read metadata/checks for existing PR26–32, bottom-up, using the connected account.
Every check response matched the requested PR's returned head and reported
`hasMore=false`. No check rerun, hosted dispatch, review submission, ref/PR
mutation, push, merge, retarget, deployment or production action occurred.
This is metadata/ancestry/coverage evidence, not a new source review or CI run.

## Exact state

All seven PRs remain **open drafts, unmerged**. Each PR27–32 base ref/SHA equals
the preceding PR's head ref/SHA. GitHub's `mergeable=true` metadata is not merge
approval, required-check-policy proof or a guarantee of semantic stack safety.

| PR | Exact head | Reported checks | Exact head ancestor of local32ca? |
| --- | --- | --- | --- |
| [26](https://github.com/Dhanunjay-Divi/bluey/pull/26) | `8845566bcb4a2c8c56a3c525e196676273b3433c` | 1 success,1 cancelled | Yes |
| [27](https://github.com/Dhanunjay-Divi/bluey/pull/27) | `936fba2419e093eb6dd9d27a764d3c4bc8c6fb25` | 10 success,5 skipped | Yes |
| [28](https://github.com/Dhanunjay-Divi/bluey/pull/28) | `b1ed19024b2a24488771e2cef6328bb655274fbb` | 9 success | Yes |
| [29](https://github.com/Dhanunjay-Divi/bluey/pull/29) | `792506b0281d29aaedaa1e87be284432b2353bb6` | 9 success | No, patch equivalent below |
| [30](https://github.com/Dhanunjay-Divi/bluey/pull/30) | `2e2a18a919b03a500e56713e4c0be2aa081f80ab` | 9 success | Yes |
| [31](https://github.com/Dhanunjay-Divi/bluey/pull/31) | `89d6b820ae46a42557c8ce76d9d632465a67b99c` | 9 success | Yes |
| [32](https://github.com/Dhanunjay-Divi/bluey/pull/32) | `66ad0fa87e9216d2190caeb742448df3c796cb3f` | 9 success | Yes |

The [26 Jobs/privacy annotation](https://github.com/Dhanunjay-Divi/bluey/actions/runs/31127376306/job/92703739318)
states that the hosted runner was not acquired after multiple attempts. That
proves a runner-acquisition cancellation, not a failed source test, account/quota
root cause or the reason for any unrelated platform warning. The stack is not
established green bottom-up. PR27's skipped candidate/authorization/promotion jobs
are not publication evidence.

[32's Jobs/privacy check](https://github.com/Dhanunjay-Divi/bluey/actions/runs/33329549414/job/99305550748)
is successful on66ad. This review did not inspect all success annotations/logs
or repository protection rules; do not upgrade “nine reported successes” into
final-candidate or production readiness.

## Local authority and patch equivalence

Both active Jobs branches start at accepted local checkpoint
`32ca96dbfbe151b1ca0294ad945ce58a4276087f`. Their current uncommitted changes are
not covered by66ad's CI:

- Phase639 image workspace:
  `/Users/uno/Downloads/cue-jobs-source-verifier-release-v3-20260831`; latest
  source782 manifest `0f4e9b016e4a97d477f0dc13b928b9793340fe0f04cf23da3e7015819a16da32`.
  Actual image archive/native/browser qualification remains open.
- Phase640 cost workspace:
  `/Users/uno/.codex/worktrees/jobs-cost-evidence/cue`; source implementation and
  adversarial review fixes are active, with compilation/runtime gates pending.

Read-only Git checks reconfirmed that remote29's792506b0 and local ancestor
2e2a18a9 have the same subject and stable patch ID
`19d38204bfa1c9033d46a2fb4357153ca1dee404`. This is patch equivalence, not commit
ancestry or permission to rebase/cherry-pick/merge. The original handoff's
`f50103e8` is an ancestor of66ad with six later commits; do not push that stale
handoff tip as if it were the current candidate.

## What the green image CI does not cover

The exact66ad `.github/workflows/jobs-ci.yml` builds an image using `docker
build`, CI-only zero contract hashes and a native-addon smoke. It does not
perform Phase639's single OCI archive import/full archive inspection, measured
browser smoke or normal managed-runner startup. Current32ca's workflow also has
later changes (launcher/audit/containment and broader integration gates); those
changes are not retroactively covered by66ad's result. Therefore old green CI
cannot resolve current66135's archive rejection or replace the held actual run.

## Remaining decisions

The existing release decision holds remain: authorized stack reconciliation,
26's exact-head CI resolution, final candidate/artifact verification, signed
registry read-back, approvals, hosted integration/capacity/canaries and rollback
evidence. No current check, local test or marketing audit authorizes external
effects. Preserve Phase639's capacity guard and Phase640's unverified status.

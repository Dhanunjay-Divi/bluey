# Phase636 — read-only PR-chain status audit

Observed:2026-10-28 10:42:58 UTC. Preflight: bluey-ops, github-automation,
current source/local handoff. No SSD or archived session was used.

## Scope and limits

Read connected GitHub metadata/check summaries for the original Phase611 chain,
bottom-up26–32. Compare exact returned head hashes to local Git history.
This is a current metadata/ancestry audit, not a new line-by-line source review,
new CI run, release gate, merge approval or production proof. No GitHub record,
branch, workflow, provider or deployment was changed. The PRs were linked to
this chat for traceability only.

Local authority remains
`/Users/uno/Downloads/cue-jobs-source-verifier-release-v3-20260831`,
branch `feat/phase-636-jobs-canonical-employer-reservations`, committed HEAD
`cd72989dc96cbab66c8fb4717c15200fb16828c0` plus the explicitly dirty Phase636
source under qualification. Preserve the inherited untracked REVIEW628.

## Exact returned heads and reported checks

| PR | Exact GitHub head | Observed status | Reported checks |
| --- | --- | --- | --- |
| [#26](https://github.com/Dhanunjay-Divi/bluey/pull/26) | `8845566bcb4a2c8c56a3c525e196676273b3433c` | open draft, unmerged | 1 success; 1 cancelled |
| [#27](https://github.com/Dhanunjay-Divi/bluey/pull/27) | `936fba2419e093eb6dd9d27a764d3c4bc8c6fb25` | open draft, unmerged | 10 success; 5 skipped |
| [#28](https://github.com/Dhanunjay-Divi/bluey/pull/28) | `b1ed19024b2a24488771e2cef6328bb655274fbb` | open draft, unmerged | 9 success |
| [#29](https://github.com/Dhanunjay-Divi/bluey/pull/29) | `792506b0281d29aaedaa1e87be284432b2353bb6` | open draft, unmerged | 9 success |
| [#30](https://github.com/Dhanunjay-Divi/bluey/pull/30) | `2e2a18a919b03a500e56713e4c0be2aa081f80ab` | open draft, unmerged | 9 success |
| [#31](https://github.com/Dhanunjay-Divi/bluey/pull/31) | `89d6b820ae46a42557c8ce76d9d632465a67b99c` | open draft, unmerged | 9 success |
| [#32](https://github.com/Dhanunjay-Divi/bluey/pull/32) | `66ad0fa87e9216d2190caeb742448df3c796cb3f` | open draft, unmerged | 9 success |

Every check summary above matched its returned PR head and had `hasMore=false`.
Conclusions are observed statuses, not independent proof of test coverage.
Except26's cancellation annotation below, annotations/logs were not audited here.
Release jobs skipped in27 are not
publication or promotion evidence. In particular, [26's Jobs/privacy check](https://github.com/Dhanunjay-Divi/bluey/actions/runs/31127376306/job/92703739318)
is cancelled, so the stack is not established green bottom-up.
Its exact-head failure annotation says a hosted runner was not acquired after
multiple attempts. That establishes a runner-acquisition failure, not a failing
source test or the cause of any separate Codex safeguard warning. No rerun was
started and no account/quota cause is inferred from that annotation.
[32's Jobs/privacy check](https://github.com/Dhanunjay-Divi/bluey/actions/runs/33329549414/job/99305550748)
is successful at66ad0fa8; its nine reported checks are all successful.

## Handoff reconciliation

The old Phase611 handoff's `f50103e8` is an ancestor of the current GitHub32
head, which has six later commits through `66ad0fa8`. Its checked-in Phase611
review already records source/CI green with hosted launch authority still
NO-GO. Do not push the old tip, rewrite that historical review or treat its
green as final-source qualification for Phase636.

The metadata refs form26→27→28→29→30→31→32, each current base ref/SHA matching
the preceding current head. Local HEAD contains exact heads26,27,28,30,31,32.
The exact29 head `792506b0281d29aaedaa1e87be284432b2353bb6` is not an ancestor.
This is not an omitted provider-refresh fix: local ancestor
`2e2a18a919b03a500e56713e4c0be2aa081f80ab` has the same subject and identical
stable patch ID `19d38204bfa1c9033d46a2fb4357153ca1dee404`.
Both PostgreSQL/SQLite current grant-upgrade paths retain the monotonic timestamp
maximum, and the stale-refresh test retains its strict advancement assertion.
This proves patch equivalence, not commit ancestry or automatic merge safety.

## Remaining release decisions

- Decide the authorized stack reconciliation/target strategy before merging;
  no rebase, retarget, cherry-pick, merge or push was performed by this audit.
- Resolve26's cancelled check under the approved CI/release workflow. No rerun
  or hosted dispatch is authorized/inferred here.
- Qualify the final Jobs candidate SHA/artifacts, not these older heads.
  Current session87312 remains separate local validation, not GitHub CI.
- Preserve all existing registry/read-back, signing/approval, hosted provider,
  capacity, canary, kill-switch and rollback gates. No production flag changes.

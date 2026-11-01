# Phase638 inherited PR-chain read-only reconciliation

Checked October 28, 2026. This is a metadata/ancestry/patch-equivalence audit,
not new line-by-line approval of every historical PR or evidence of a current
release. Source/local handoff remain authoritative; no SSD was needed.

## Live snapshot

Read all seven PRs bottom-up with GitHub CLI `pr view` (number, state, draft,
head SHA, base/head branch names, merge commit and current check rollup). All are
OPEN DRAFT, with no merge commit. They were already attached to this Codex task.
No PR, branch, workflow, reviewer, merge, push or deployment mutation was made.

| PR | Exact remote head | Returned check conclusions | Relation to local40949858 |
| --- | --- | --- | --- |
| [#26](https://github.com/Dhanunjay-Divi/bluey/pull/26) | `8845566bcb4a2c8c56a3c525e196676273b3433c` | No rollup returned | Ancestor of current HEAD |
| [#27](https://github.com/Dhanunjay-Divi/bluey/pull/27) | `936fba2419e093eb6dd9d27a764d3c4bc8c6fb25` | 10 success; 5 skipped | Ancestor of current HEAD |
| [#28](https://github.com/Dhanunjay-Divi/bluey/pull/28) | `b1ed19024b2a24488771e2cef6328bb655274fbb` | 9 success | Ancestor of current HEAD |
| [#29](https://github.com/Dhanunjay-Divi/bluey/pull/29) | `792506b0281d29aaedaa1e87be284432b2353bb6` | 9 success | Patch-equivalent via 2e2a18a9 |
| [#30](https://github.com/Dhanunjay-Divi/bluey/pull/30) | `2e2a18a919b03a500e56713e4c0be2aa081f80ab` | 9 success | Ancestor of current HEAD |
| [#31](https://github.com/Dhanunjay-Divi/bluey/pull/31) | `89d6b820ae46a42557c8ce76d9d632465a67b99c` | 9 success | Ancestor of current HEAD |
| [#32](https://github.com/Dhanunjay-Divi/bluey/pull/32) | `66ad0fa87e9216d2190caeb742448df3c796cb3f` | 9 success | Ancestor of current HEAD |

Jobs CI/privacy reports SUCCESS on #27–#32's respective heads, with linked jobs:
- [#27 Jobs CI](https://github.com/Dhanunjay-Divi/bluey/actions/runs/31150150355/job/92777914648)
- [#28 Jobs CI](https://github.com/Dhanunjay-Divi/bluey/actions/runs/31170778794/job/92841867953)
- [#29 Jobs CI](https://github.com/Dhanunjay-Divi/bluey/actions/runs/31745128790/job/94597742311)
- [#30 Jobs CI](https://github.com/Dhanunjay-Divi/bluey/actions/runs/31760859074/job/94646687832)
- [#31 Jobs CI](https://github.com/Dhanunjay-Divi/bluey/actions/runs/31779254192/job/94701212172)
- [#32 Jobs CI](https://github.com/Dhanunjay-Divi/bluey/actions/runs/33329549414/job/99305550748)

Those observations are scoped to the old exact heads/dates in their rollups;
they do not qualify current local Phase638 source or skipped release/promotion
jobs. #26 returned no rollup; this is missing evidence, not a passing check or
proof of failure. No check was rerun and no hosted quota was consumed.

## Stack identity

Every child base branch name matches the previous PR's head branch name. Local
commit ancestry passes each adjacent edge except #29→#30. Current HEAD contains
all remote PR tips except #29's exact commit identity. This exception was audited,
not bypassed or resolved by an automatic merge:

- #29 tip792506b0 and ancestor2e2a18a9 have identical stable patch ID
  `19d38204bfa1c9033d46a2fb4357153ca1dee404`, subject, four-file stat and hunks.
- Current SQLite and PostgreSQL provider-grant upgrade paths in
  `server/src/db/jobs/customer_data.rs` retain the monotonic shared authority
  timestamp `max(now, mailbox.updated_at+1, credential.updated_at+1)` before
  serialization. Refresh CAS still binds/revalidates that exact timestamp and
  full frozen credential snapshot.
- `stale_provider_refresh_cannot_overwrite_a_grant_upgrade` remains in current
  `server/src/db/jobs/tests.rs`; it rejects stale refresh and preserves the new
  revision/digest/token. FIX688 and the changelog entry are present.
- Phase638 does not edit that implementation or that regression. The independent
  read-only code/history audit found no missing CAS correction. Do not cherry-pick
  792506b0: it would duplicate the already applied patch. Current runtime regression
  evidence remains required and is included in the all-target server gate.

The initial handoff's f50103e8 is an ancestor of current #32 tip66ad0fa8, which
already contains later runtime normalization, CI compatibility and exact-tip
record commits. Do not push the obsolete handoff tip over that newer remote.
Current local40949858 descends from66ad0fa8; Phase638 remains uncommitted/unpushed.

## Release consequences

The structural chain/content check is now understood, but the stack is not approved
for automatic merge or promotion. Preserve all draft states and bases. Any eventual
merge/retarget/close/push needs the owner's explicit scope and current exact-tip
checks plus the remaining production gates. CI metadata is not attestation of
live cloud capacity, runner image, ATS receipts, OAuth readiness, cost or latency.

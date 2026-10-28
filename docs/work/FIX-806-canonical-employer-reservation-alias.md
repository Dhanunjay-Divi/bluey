# FIX-806: Canonical employer aliases bypass the active-company reservation guard

Codex preflight: `bluey-ops`, current code/local handoff and Round636.
Status: P1 causally reproduced in private PostgreSQL; correction under qualification.

## Issue and root cause

`jobs_attempt_reservations` uniquely constrains account plus company_key, where
eligibility/reservation/packet/cloud staging derive company_key only from the
posting display name. Integrity composition verifies and freezes a signed
canonical employer ID/domain, but reservation authority discards that identity.
Therefore two independently valid aliases with different normalized display keys
can evade the one-active-company guard across Tracks/application emails.

Evidence is in REVIEW635 and the unchanged source at base `cd72989d`; no real
employer application has been used to demonstrate the issue. Existing suffix/
punctuation alias tests cover only display names normalizing to the same key.

## Planned correction and verification

Keep the name guard, add independently constrained signed employer binding,
preserve legacy rows and reconciliation, and revalidate the own reservation at
packet/replay/pre-click. Round636 defines positive/negative/concurrency/migration
acceptance. Record actual red/green commands, source identity and cleanup here
after execution; none is claimed yet.

First diagnostic session45270 is not a causal red: A itself failed current
execution authority. Its private PG/root were cleaned. The alias test is being
corrected to provide explicit signed `full_time w2` categories and to assert
ordinary queue eligibility plus complete current authority for both packets
before reserving either. No product rule is changed to make a fixture pass.

Fresh session81168 reached the actual alias boundary on unchanged product code:
both complete execution-authority checks passed, A reserved, both exact signed
projections remained Verified, then B also reserved. The expected no-second-
reservation assertion failed (case6.17s, test blob
`ef291fceaf70ef0e7bb91c5a822abab96ee04f49`). PG/root cleanup was proven.
Earlier45270/44231 failures are retained fixture diagnostics, not causal reds.

The preliminary correction passed focused31836 (one actual PG alias case,
eight core cases, three SQLite migration cases, registration and strict
all-target Clippy). It is not final-source evidence: subsequent review and
regressions added irreversible-state guards, complete terminal-history checks
and fixture corrections. In particular, same-job preparation must not exclude
and then overwrite its own legacy submitted/unknown history. The corrected
preparation/commit guards and absent/released-slot snapshot tests are now in
fresh session78617; full acceptance remains pending.

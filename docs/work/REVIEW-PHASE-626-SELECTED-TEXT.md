# REVIEW: Phase 626 — selected-text boundary

Preflight: `bluey-ops`, `pinky-ops`, `pinky-bluey-integration-ops`.
Source range: `820f28ba..7a288472`. Reviewer: implementing coordinator self-review.
Date: 2026-10-10. This is not an independent release approval.

## Findings addressed

- Document context was not provider-visible on ordinary Ask: added a server-owned
  delegated USER evidence projector, leaving standalone projection disabled.
- Escaped JSON exceeded old body limits: exact create-only 128 KiB allowance,
  all other paths unchanged, with maximum escaped-content HTTP coverage.
- Storage LIMIT could hide uploads: transactional active count cap32.
- Delete retry could strand UI after response loss: same-owner tombstone replay.
- Session-only dispatch fence could miss deleted evidence: selected IDs and
  hashes rechecked in the managed provider dispatch fence.
- Fixed SQLite statement/row borrow lifetime and updated default-off fixtures.
- Kept empty-context cost estimates unchanged; selected evidence uses6x byte
  escaping bound plus envelope, not the insufficient2x estimate.

## Verification and verdict

Formatting/diff checks pass. At exact code revision `7a288472`, the final queued
gate passed 44 focused tests, the projector regression and strict all-target
Clippy. The owned temporary root was cleaned.
Pinky runtime-race/Node/Python passed separately; Mac native build and synthetic
visual checks are recorded in Pinky's continuation. PostgreSQL/live docs and
physical Windows are open. No production or dedicated preprod deploy occurred.

Verdict: hold deployment/promotion pending those gates. Source-only checkpoint,
not a claim that all Bluey functionality is integrated.

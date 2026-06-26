# Round 004 - Mac Windows Parity Rule

## Trigger

Owner clarified that when Bluey changes are made for Mac, the matching Windows behavior should also be handled when the feature exists on both platforms.

## Fix

- Updated the Bluey compaction handoff working rules to make platform parity explicit.
- Future Mac-side changes to overlay, install, attachment, capture, audio helper, local dependency, update, or packaging paths require a Windows parity check in the same round.
- The expected outcome is either:
  - implement the equivalent Windows change, or
  - document clearly why there is no Windows equivalent for that round.
- Advanced the canonical Bluey round sequence so the next new round doc should start at `ROUND-005-...`.

## Verification

- Confirmed the backup thread id remains in the handoff: `019e133e-d92a-7830-8df0-3a050a4e22f6`.
- Confirmed the handoff now names `ROUND-004-MAC-WINDOWS-PARITY-RULE.md` as the latest assigned round doc.

## Current State

- This was a documentation and continuity-rule update only.
- No product code changed in this round.

## Remaining Gate

- Future Bluey implementation rounds should include a Windows parity note in the round doc whenever Mac files are touched.

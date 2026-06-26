# Round 003 - Round Doc Numbering And Backup Thread Memory

## Trigger

Owner asked Bluey to use numbered round docs instead of unnumbered date-only filenames. A Pinky round doc was provided as a formatting example only; Bluey now keeps its own local sequence. Owner also emphasized that the old backup chat id must stay remembered because it contains the broader context.

## Fix

- Adopted Bluey-local numbered round docs:
  - filename shape: `ROUND-NNN-SLUG.md`
  - title shape: `# Round NNN - Title`
  - body shape: Trigger, Root Cause or Fix, Verification, Current State, Remaining QA/Gates as applicable
- Renamed the latest user-facing fix doc to:
  - `docs/rounds/ROUND-001-AUTOSEND-SILENT-LISTEN-CANVAS-SCREEN-FOLLOWUP.md`
- Renamed the latest audit doc to:
  - `docs/rounds/ROUND-002-END-TO-END-AUDIT-AND-CLEANUP.md`
- Left small compatibility pointer files at the old unnumbered paths so prior chat links still resolve.
- Updated the handoff to keep backup thread id `019e133e-d92a-7830-8df0-3a050a4e22f6` as the continuity anchor and to make Round 004 the next canonical Bluey round number.

## Verification

- Used the provided Pinky doc only as a formatting reference, not as Bluey's numbering source.
- Confirmed the Bluey handoff still carries backup thread id `019e133e-d92a-7830-8df0-3a050a4e22f6`.
- Preserved compatibility files for the two older unnumbered docs referenced in chat.

## Current State

- Canonical latest round docs:
  - `ROUND-001-AUTOSEND-SILENT-LISTEN-CANVAS-SCREEN-FOLLOWUP.md`
  - `ROUND-002-END-TO-END-AUDIT-AND-CLEANUP.md`
  - `ROUND-003-ROUND-DOC-NUMBERING-AND-BACKUP-THREAD.md`
- Next Bluey round doc should be `ROUND-004-...`.

## Remaining Gate

- Future Bluey final responses should link the numbered canonical round doc, not the compatibility pointer.

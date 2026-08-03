# Round 600 - Jobs Intervention Answer Reapproval

**Date:** 2026-08-03
**Branch:** `feat/phase-600-intervention-reapproval`
**Status:** Verified source slice; no runtime deployment or flag change

## Outcome

Bluey no longer resumes an application after a candidate supplies a missing
answer under an older Application Kit approval. The answer becomes a new packet
revision and the candidate sees the updated kit in Review first before Bluey can
obtain new execution authority.

## Authority Boundary

The transaction now performs the complete revision boundary:

1. Store or replace the structured application answer.
2. Mirror the answer into the exact receipt payload.
3. Remove the previously approved execution checksum.
4. Append immutable packet-revision metadata.
5. Resolve the intervention.
6. Return the application to `awaiting_review` and clear its run ID.
7. Release the active attempt reservation.
8. Release a prepared cloud lease or fail the waiting local ticket.
9. Remove stale local resume approval and leave the browser paused.

Only email-code approval and explicit final-submit approval may resume an
existing runner. Any run at or beyond `click_started` rejects answer changes and
continues through submission reconciliation.

## Portal Behavior

- The action now reads `Save answer for review`.
- The toast says the updated Application Kit requires review.
- Applications return to Review first instead of appearing queued.
- Browser sessions remain paused with a clear packet-review step.
- The next explicit packet approval creates the new frozen checksum and does not
  double-meter the prior packet.

## Verification

- 832 Rust unit tests passed.
- 82 signed HTTP integration tests passed.
- 93 portal tests passed.
- Strict Rust Clippy, TypeScript typecheck, portal production build, formatting,
  and diff checks passed.
- Dedicated tests cover cloud leases, local tickets, HTTP behavior, structured
  receipt answers, packet revision history, stale action replay, and
  side-effect-unknown rejection.

## Production Boundary

This round does not enable model generation, Browser distribution, mailbox
sync, or employer-facing execution. Those remain independent launch gates and
their production flags remain off until their own evidence is complete.

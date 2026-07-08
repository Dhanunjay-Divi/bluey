# Bluey 0.1.88

Released: 2026-07-05

## Summary

Production-beta release that combines the merged web UI branch with the latest backend, overlay, session history, latency diagnostics, and desktop account revocation fixes.

## Changes

- Merged the parallel web UI work and latest backend fixes into `main`.
- Added overlay history search by session id for easier support/debug lookup.
- Added fast-answer routing diagnostics so slow first-token paths can be traced.
- Fixed desktop revocation handling: if a linked desktop is removed from the web UI, local Bluey clears tokens and shows signed out after the next protected action or balance poll.
- Reduced overlay/native/web balance drift by refreshing account balance every 10 seconds while visible.
- Kept the manual refresh button as a fallback while making automatic refresh the normal path.

## Verification

```bash
cargo test -p cue-daemon watch_clear_notifies_subscribers --quiet
cargo test -p cue-daemon listen_auth_gate_clears_deleted_account_errors --quiet
cargo test -p cue-dashboard --lib --quiet
cargo test -p cue-cloud-client --lib --quiet
cargo check -p cue-daemon -p cue-dashboard
( cd crates/cue-dashboard/ui && npm run build )
node --check web/assets/bluey-site.js
git diff --check
```

Round docs:

- `docs/rounds/ROUND-395-FAST-ANSWER-LATENCY-TRACE.md`
- `docs/rounds/ROUND-396-OVERLAY-HISTORY-SESSION-ID-SEARCH.md`
- `docs/rounds/ROUND-397-DESKTOP-REVOKE-LIVE-BALANCE.md`
- `docs/rounds/ROUND-398-MAIN-MERGE-DEPLOY-0.1.88.md`

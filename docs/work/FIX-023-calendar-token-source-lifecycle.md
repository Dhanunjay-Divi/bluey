# FIX-023: Calendar Token and Source Lifecycle

## Issue

Calendar pollers, connection status, reconnect, and disconnect could use
different cached/keychain store instances and refresh an expired token
concurrently. During reconnect or disconnect, an old poller could persist a
rotated token after the replacement or clear operation.

## Root Cause

Each calendar source owned an untracked background task and callers constructed
independent token stores. `valid_access_token` serialized neither its
load-refresh-save transaction nor the lifecycle transition that replaces or
clears credentials. The connect helper also persisted new tokens before the
daemon had stopped the prior poller.

## Fix Summary

- Each provider now has one shared cached store and one async token-operation
  lock used by its poller and status validation.
- Token refresh serializes the complete load-refresh-persist transaction.
- Interactive authorization returns enriched tokens without persisting them.
- Reconnect explicitly shuts down and joins the old poller before saving the
  replacement token bundle, then publishes the new source.
- Disconnect shuts down and joins the poller before clearing the same shared
  store; the app no longer performs a second independent keychain clear.
- Sources retain and abort their refresh task on drop as a final safety net.

## Files Modified

| File | Change |
|------|--------|
| `crates/cue-calendar-cloud/src/oauth.rs` | Added serialized access-token refresh with concurrency regression coverage. |
| `crates/cue-calendar-cloud/src/lib.rs` | Exported the serialized token accessor. |
| `crates/cue-calendar-cloud/src/google.rs` | Accepted the shared lock/store, owned the poll task, and split authorization from persistence. |
| `crates/cue-calendar-cloud/src/microsoft.rs` | Accepted the shared lock/store, owned the poll task, and split authorization from persistence. |
| `crates/cue-daemon/src/calendar.rs` | Centralized provider stores, locks, source replacement, validation, and shutdown ordering. |
| `crates/cue-daemon/src/app.rs` | Routed connect/status/disconnect through the centralized lifecycle. |

## Edge Cases Handled

- Two simultaneous refresh callers issue only one provider refresh request and
  both observe the rotated token.
- Reconnect with a currently running source always records `stop` before `save`.
- Status does not reload a stale token bundle into a separate cache.
- Disconnect cannot be undone by an in-flight poller refresh.
- Dropping a source aborts its task even when explicit shutdown is skipped.

## How to Test

```bash
cargo test -p cue-calendar-cloud
cargo test -p cue-daemon --features cloud-calendar \
  reconnect_stops_old_source_before_publishing_new_tokens --lib
cargo check -p cue-daemon --features cloud-calendar
```

## Known Limitations

- Interactive OAuth still depends on provider availability, a working browser,
  and the configured public client IDs.

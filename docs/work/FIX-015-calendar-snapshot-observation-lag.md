# FIX-015: Calendar snapshot observation lag

## Issue

After a Google or Microsoft account connected, or after a provider poll found a
new or moved event, the meeting-prep scheduler could take up to five minutes to
observe the updated cloud snapshot.

## Root Cause

The cloud sources refresh their local snapshots independently every 45 seconds,
but the meeting-prep scheduler used a five-minute safety wake when its previous
snapshot contained no nearer event. The webhook endpoints do not currently
signal a user's desktop daemon, so nothing interrupted that sleep.

## Fix Summary

The scheduler now caps its event-driven sleep at the existing 30-second calendar
poll cadence. It still sleeps directly to a known event's warm-up boundary when
that boundary is nearer, while newly connected accounts and changed cloud
snapshots are observed promptly.

## Files Modified

| File | Change |
|------|--------|
| `crates/cue-daemon/src/calendar.rs` | Cap the snapshot observation delay at the normal calendar poll cadence and add a regression test. |
| `docs/work/FIX-015-calendar-snapshot-observation-lag.md` | Record the defect, fix, and webhook limitation. |

## How to Test

```bash
cargo test -p cue-daemon --features cloud-calendar calendar::tests
```

## Known Limitations

- The server webhook routes authenticate and acknowledge provider doorbells,
  but do not create or renew provider subscriptions and do not wake a desktop
  daemon.
- The working Phase-1 delivery path remains on-device incremental polling. A
  production webhook relay needs per-account subscription storage, renewal,
  ownership routing, and an authenticated device-notification channel.

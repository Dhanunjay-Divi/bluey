# FIX-017: Calendar Warmup Privacy and Provider Identity

## Issue

Calendar meeting-preparation drives were rendered and persisted as user
questions, exposing the internal prompt in meeting history and making it
eligible for local/cloud RAG indexing. Bluey's namespaced event key was also
being presented to connectors as though it were the provider's raw event ID.

## Root Cause

`answer_with_provider_runtime` treated every source as a user-authored exchange,
including the internal `warmup` source. Rehydrate, local RAG, and cloud-sync
paths had no compatibility filter for already-persisted warmup turns.

`DynamicCloudSource` correctly prefixed event IDs to avoid Google/Microsoft
collisions, but `UpcomingEvent` had only one identity field. The warmup prompt
therefore could not distinguish Bluey's internal key from the provider ID
required by calendar connectors.

## Fix Summary

- Internal warmup drives now render a safe system label and readiness answer
  card without entering `MeetingRecord` or app-owned conversation storage.
- Historical `source = "warmup"` turns are excluded from every overlay
  rehydrate path, local RAG rebuild, and cloud response/RAG sync.
- A narrow compatibility filter removes legacy warmup prompt/answer pairs from
  the older SQLite conversation store, whose rows did not include a source.
- `UpcomingEvent` now carries a typed provider and an exact
  `provider_event_id` separately from its globally namespaced internal `id`.
- Provider mapping and aggregation preserve raw Google/Graph occurrence IDs;
  warmup context includes the explicit provider and accepts Google's documented
  1024-character event-ID boundary.

## Files Modified

| File | Change |
|------|--------|
| `crates/cue-core/src/calendar.rs` | Added provider identity and raw provider event ID fields. |
| `crates/cue-calendar-cloud/src/google.rs` | Preserved Google occurrence ID and provider metadata. |
| `crates/cue-calendar-cloud/src/microsoft.rs` | Preserved immutable Graph event ID and provider metadata. |
| `crates/cue-daemon/src/calendar.rs` | Namespaced only Bluey's internal key and retained raw IDs. |
| `crates/cue-daemon/src/app.rs` | Kept warmup prompts out of user history and filtered legacy display/RAG paths. |
| `crates/cue-daemon/src/conversation.rs` | Filtered legacy internal warmup pairs from app-owned memory. |
| `crates/cue-daemon/src/cloud/sync.rs` | Excluded legacy warmup turns from cloud response and RAG batches. |

## Edge Cases Handled

- Google and Microsoft may return the same raw event ID; internal keys remain
  collision-free while connector IDs remain unchanged.
- Legacy events with an empty `provider_event_id` recover it from the old `id`.
- Provider text cannot forge the calendar-context closing marker.
- Attendee and provider fields are bounded; no more than 20 attendees enter
  warmup context.
- Ordinary conversation containing one warmup keyword is not filtered; the
  legacy matcher requires the complete internal prompt signature.

## How to Test

```bash
cargo test -p cue-calendar-cloud
cargo test -p cue-daemon --features cloud-calendar calendar_warmup --lib
cargo test -p cue-daemon --features cloud-calendar internal_warmup --lib
cargo test -p cue-daemon --features cloud-calendar legacy_warmup --lib
cargo test -p cue-daemon --features cloud-calendar dynamic_source_ --lib
```

## Known Limitations

- A connected agent may retain its own internal session transcript according to
  that agent's storage policy; this fix governs Bluey's UI, meeting records,
  app-owned conversation memory, local RAG, and cloud sync.
- Calendar content remains untrusted reference data supplied to the selected
  agent. Deterministic per-tool read-only authorization is a separate hardening
  task.

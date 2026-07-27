# FIX-006: Short Replies and Final STT Segments Were Dropped

## Issue

The full daemon test suite exposed a transcript dedup regression: when two
speakers both said a short reply such as “okay,” the second speaker's line was
dropped. A final segment could also be mistaken for a duplicate of its own
still-open partial.

## Root Cause

`is_near_duplicate_transcript` returned true for every normalized exact match
before checking whether the speaker changed or whether the prior segment was
final. A documented cross-speaker distinctiveness threshold had been removed
from the implementation while its test remained.

## Fix Summary

- Same-speaker final retries are still deduplicated.
- Cross-speaker exact, substring, and token-overlap matches must now be at least
  12 normalized characters on both sides.
- Partial segments are excluded from retry detection so the arriving final can
  replace the open partial through the existing finalization path.

## Files Modified

| File | Change |
|------|--------|
| `crates/cue-daemon/src/app.rs` | Restore distinctive cross-speaker echo gating and final-only comparisons; extend regression coverage. |

## Edge Cases Handled

- Two people can both say “okay” or “yes.”
- Long speaker bleed across mic/system sources is still removed.
- A final matching its preceding partial is committed rather than discarded.

## How to Test

```bash
cargo test -p cue-daemon duplicate_transcript_detection_ignores_recent_retries \
  --features "parakeet-stt cloud-calendar"
cargo test -p cue-daemon --features "parakeet-stt cloud-calendar" --lib
```

## Known Limitations

- Echo detection remains text-based; acoustic echo cancellation is the primary
  defense before transcription.

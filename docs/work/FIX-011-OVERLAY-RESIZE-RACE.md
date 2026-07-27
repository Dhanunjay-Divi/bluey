# FIX-011: Stale Pill Resize Can Override Expansion

## Issue

An in-flight collapsed-pill morph resize could finish after Expand and leave the
expanded overlay at pill dimensions.

## Root Cause

`setPillSize` did not verify the current collapsed state, and independent async
Tauri resize calls could complete out of intent order.

## Fix Summary

Collapse state is mirrored in a ref, resize intents receive monotonically
increasing generations, and native resize calls run through a serialized
promise tail. Stale generations no-op; Expand always wins as the newest intent.

## Files Modified

| File | Change |
|------|--------|
| `crates/cue-meeting-overlay/ui/src/lib/useCollapse.ts` | Serialize and generation-check native window resizes. |

## Edge Cases Handled

- A delayed expanded-size read cannot collapse after a newer Expand.
- Pill morph changes no-op while expanded.
- Resize failures do not poison the queue.

## How to Test

```bash
cd crates/cue-meeting-overlay/ui
npm run build
```

Rapidly expand while the pill changes state and confirm the final window remains
expanded.

## Known Limitations

- Browser-only development mode intentionally treats native resize calls as
  no-ops.

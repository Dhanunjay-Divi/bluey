# FIX-024: macOS Helper Verifier Empty-Array Cleanup

## Issue

A clean development reinstall built and staged `BlueyAudio.app`, then failed
while verifying the installed copy with:

```text
verify-app.sh: line 19: PROBE_MARKERS[@]: unbound variable
```

## Root Cause

macOS ships Bash 3.2. Under `set -u`, expanding an empty declared array with
`"${array[@]}"` raises an unbound-variable error. The verifier's exit trap
expanded `PROBE_MARKERS` and `TEMP_FILES` even when launch probes were disabled
and no cleanup entries had been added.

## Fix Summary

The trap now checks each array's length before entering its cleanup loop. This
preserves strict unset-variable checking while making both the normal
verification path and early exits safe on the system Bash.

## Files Modified

| File | Change |
|------|--------|
| `native/macos/cue-audio/verify-app.sh` | Guard empty cleanup arrays before expansion |
| `CHANGELOG.md` | Record the clean-install verifier fix |

## Edge Cases Handled

- Verification with launch probes disabled and both cleanup arrays empty.
- Verification that creates temporary profile files but no launch markers.
- Launch verification with both arrays populated.

## How to Test

```bash
bash -u -c 'values=(); test "${#values[@]}" -eq 0'
bash native/macos/cue-audio/verify-app.sh \
  native/macos/cue-audio/.build/BlueyAudio.app
BLUEY_VERIFY_LAUNCH=1 \
  bash native/macos/cue-audio/verify-app.sh \
  native/macos/cue-audio/.build/BlueyAudio.app
```

## Known Limitations

- The script intentionally targets the system Bash available on supported
  macOS versions; it does not require newer Bash-only syntax.

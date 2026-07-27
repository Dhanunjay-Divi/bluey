# FIX-025: macOS Microphone Entitlement

## Issue

The overlay showed the microphone warning after the user enabled both
`bluey-daemon` and `BlueyAudio` under System Settings → Privacy & Security →
Microphone.

## Root Cause

`native/macos/cue-audio/bundle-app.sh` enabled the hardened runtime for the
certificate-backed `BlueyAudio.app` signature but did not sign the helper with
`com.apple.security.device.audio-input`. macOS TCC therefore denied
`sh.bluey.audio` before it could prompt or begin capture. The existing verifier
checked only the optional restricted persistent-capture entitlement, so the
broken artifact passed installation validation.

## Fix Summary

Every BlueyAudio signature now carries the ordinary audio-input entitlement.
The optional persistent-content-capture entitlement is added to the same plist
only when its separately provisioned mode is enabled. Static verification fails
when audio-input is missing, catching the original defect without touching a
capture API or triggering a privacy prompt.

## Files Modified

| File | Change |
|------|--------|
| `native/macos/cue-audio/bundle-app.sh` | Sign every helper variant with audio-input |
| `native/macos/cue-audio/verify-app.sh` | Require audio-input during artifact verification |
| `CHANGELOG.md` | Document the microphone permission correction |

## Edge Cases Handled

- Ad-hoc development and certificate-backed bundles receive the same required
  microphone entitlement.
- Restricted persistent capture keeps its provisioning-profile validation and
  adds its entitlement without replacing audio-input.
- The verification probe remains permission-free and never records audio.

## How to Test

```bash
bash native/macos/cue-audio/bundle-app.sh
BLUEY_REQUIRE_STABLE_CODESIGN=1 \
BLUEY_EXPECTED_ARCHS=arm64 \
BLUEY_VERIFY_LAUNCH=1 \
bash native/macos/cue-audio/verify-app.sh \
  native/macos/cue-audio/.build/BlueyAudio.app
codesign -d --entitlements - --xml \
  native/macos/cue-audio/.build/BlueyAudio.app
```

After staging the rebuilt helper, reset only `sh.bluey.audio` if macOS retains
the pre-fix denial, grant Microphone once, and confirm that the helper remains
running and the overlay warning clears.

## Known Limitations

- An existing TCC record tied to the old code requirement may need one targeted
  reset and re-grant after installing the corrected helper.

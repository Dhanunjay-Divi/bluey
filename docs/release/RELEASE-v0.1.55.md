# Bluey 0.1.55

## Fixes

- Auto-send now fires faster after live captions pause. The macOS and Windows overlays use a `300ms` post-caption pause instead of the old `900ms` delay.
- Auto-send tooltip copy now says captions pause briefly, matching the faster behavior.

## Verification

- macOS overlay debug build passed.
- Release artifact scan blocks dev visible-overlay flags and known secret values before publishing.

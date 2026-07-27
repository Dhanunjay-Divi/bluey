# FIX-002: macOS audio permission re-prompts

## Issue

Bluey repeatedly appeared to request macOS audio permissions after the user had
already granted them. Failed audio-helper launches also produced bursts of
automatic retries and delayed transcription startup.

## Root Cause

There were three permission-lifecycle problems:

1. `bundle-app.sh` defaulted to ad-hoc signing even when the development Mac had
   a valid code-signing identity. `build-meeting.sh` and `build-macos.sh`
   explicitly passed `-`, so every rebuilt `BlueyAudio.app` had a new code
   requirement. `BlueyShot.app` had the same issue for Screen Recording and was
   copied by release scripts without being built there.
2. `reinstall-dev.sh` did not rebuild the app bundle, then force-signed its
   copied bundle ad-hoc. `install.sh` also force-signed every shipped `.app`
   ad-hoc, destroying a valid stable signature. Since TCC grants are associated
   with the app's code requirement, an updated helper could look like a new app.
3. `system_capture.rs` reduced every helper failure to a boolean and retried it
   up to five times. Permission denial was already assigned native exit code 3,
   but stderr was inherited and the exit classifier was never used. The
   microphone helper also connected its PCM socket before checking TCC, so a
   denial looked like an unexpected EOF and entered the same retry loop. In the
   opposite direction, any non-retryable microphone setup failure was surfaced
   as `PermissionDenied`, so signing, LaunchServices, and socket failures sent
   users back to System Settings even when permission was already granted.
4. Clean Make and GitHub release builds ran `cue-audio/build.sh`, which produces
   only bare executables. The arm64 Make target had an optional `.app` copy, but
   never invoked `bundle-app.sh`, so a clean build silently omitted the bundle
   while a dirty build could package a stale one. The GitHub artifact copied
   only bare helpers into both `bin/` and the dashboard resources.
5. Certificate-signed helpers unconditionally claimed Apple's restricted
   `com.apple.developer.persistent-content-capture` entitlement without an
   embedded provisioning profile. Static signature verification passed, but
   AMFI rejected the executable at launch with `No matching profile found`.

The macOS unified log confirmed one real microphone authorization prompt was
accepted, followed by many helper launches without another TCC
`AUTHREQ_PROMPTING` event. That distinguished process/retry churn from a new
system authorization decision on every launch.

## Fix Summary

- Auto-select a valid local code-signing identity for `BlueyAudio.app` and
  `BlueyShot.app`, while retaining an explicit `BLUEY_CODESIGN_IDENTITY`
  override and an ad-hoc fallback for Macs without a certificate.
- Preserve valid bundle signatures during development reinstall and archive
  install; only ad-hoc seal an unsigned or damaged legacy bundle.
- Capture and bound helper stderr, classify permission exit code 3 and denial
  messages, and stop automatic respawn for permission/setup failures.
- Connect one daemon-owned PCM socket, publish the helper PID and
  `permission_checking` state, then preflight microphone authorization before
  capture. Denial cannot masquerade as a stream crash, and shutdown can
  terminate the detached helper even while the first-use prompt is open.
- Add a per-launch microphone status sidecar. Only the helper's explicit
  `permission_denied` status maps to the permission UI; signing, launch, socket,
  and other setup failures map to an audio failure instead of another grant
  prompt.
- Explain during onboarding that macOS owns separate System Audio Recording and
  Microphone grants, instead of labeling a two-source request as microphone-only.
- Add a 250 ms read timeout to the stdout transport so an explicit stop is
  observed even while a helper is producing no PCM.
- Make helper bundling target-aware, require a certificate-backed signature for
  release packages, and verify the bundle identifier, designated requirement,
  architecture, and signature after each staging copy.
- Keep `persistent-content-capture` off by default. It can be enabled only when
  an approved matching provisioning profile is explicitly provided, validated,
  and embedded.
- Add a bounded no-capture `--launch-probe` and test both direct execution and
  LaunchServices using a unique sentinel/PID handshake, catching AMFI/profile
  failures without triggering a TCC prompt or leaking an old helper.
- Put `BlueyAudio.app` beside the daemon in release `bin/` and inside
  `Bluey.app/Contents/Resources/`, while retaining the bare aliases only as
  compatibility fallbacks.
- Correct the macOS permission descriptions to say that local STT is the
  default while an explicitly configured cloud speech provider receives audio;
  remove the inaccurate unconditional “audio never leaves your machine” claim.

## Files Modified

| File | Change |
|------|--------|
| `native/macos/cue-audio/bundle-app.sh` | Select a stable signing identity and pin `sh.bluey.audio`. |
| `native/macos/cue-audio/Sources/cue-audio/main.swift` | Preflight microphone access, emit a status sidecar, and expose a permission-free launch probe. |
| `native/macos/cue-shot/build.sh` | Select a stable signing identity for the Screen Recording helper. |
| `crates/cue-daemon/src/audio/system_capture.rs` | Classify terminal permission/setup failures, capture diagnostics, and suppress retry storms. |
| `crates/cue-meeting-overlay/ui/src/screens/Onboarding.tsx` | Explain the two distinct first-use macOS audio grants. |
| `scripts/build-meeting.sh` | Allow automatic stable-identity selection. |
| `scripts/build-macos.sh` | Allow automatic stable-identity selection. |
| `scripts/reinstall-dev.sh` | Rebuild the app bundle and preserve its signature while staging. |
| `scripts/install.sh` | Preserve valid shipped `.app` signatures. |
| `native/macos/cue-audio/verify-app.sh` | Validate signatures, profile/entitlement agreement, architecture, and direct/LaunchServices launchability. |
| `Makefile` | Build and verify the signed app from clean arm64, Intel, and universal release targets. |
| `scripts/build-macos-universal.sh` | Wrap and sign the universal audio helper after `lipo`. |
| `.github/workflows/release.yml` | Import the release certificate, stage the app in both runtime locations, and verify both copies. |

## Edge Cases Handled

- A machine without a code-signing certificate still receives a launchable
  ad-hoc-signed helper.
- Explicit `BLUEY_CODESIGN_IDENTITY=-` still forces ad-hoc signing when needed.
- Helper stderr is drained continuously but capped at 16 KiB in daemon memory.
- Transient process crashes remain retryable; only identified permission denial
  and terminal setup failures stop until an explicit user retry.
- LaunchServices verification does not use `open -W`; an immediate-exit probe
  can race its process waiter. The verifier instead requires a unique handshake
  and independently confirms that the reported helper exits.
- A stopped capture no longer waits indefinitely on a silent stdout pipe.
- User decision time in the first-use microphone prompt does not count against
  the post-authorization first-PCM watchdog.
- Tagged GitHub releases fail closed when their PKCS#12 signing secrets are not
  configured; manual workflow runs may still opt into the documented ad-hoc
  development fallback.
- Cross-compiled packages verify that the app's executable contains the target
  slice instead of accidentally shipping the CI runner's host architecture.
- Restricted entitlement mode fails closed without an approved profile, while
  ordinary development and release helpers remain free of that entitlement.

## How to Test

```bash
bash -n native/macos/cue-audio/bundle-app.sh native/macos/cue-shot/build.sh \
  scripts/build-meeting.sh scripts/build-macos.sh \
  scripts/reinstall-dev.sh scripts/install.sh

bash native/macos/cue-audio/bundle-app.sh
BLUEY_VERIFY_LAUNCH=1 \
  bash native/macos/cue-audio/verify-app.sh \
  native/macos/cue-audio/.build/BlueyAudio.app

# Must fail before signing when no approved profile is supplied.
BLUEY_REQUIRE_PERSISTENT_CAPTURE_ENTITLEMENT=1 \
  bash native/macos/cue-audio/bundle-app.sh

bash native/macos/cue-shot/build.sh
codesign --verify --deep --strict native/macos/cue-shot/.build/BlueyShot.app

make build-audio-app-darwin-arm64
BLUEY_REQUIRE_STABLE_CODESIGN=1 BLUEY_EXPECTED_ARCHS=arm64 \
  bash native/macos/cue-audio/verify-app.sh \
  native/macos/cue-audio/.build/BlueyAudio.app

cargo test -p cue-daemon audio::system_capture::permission_tests
cargo test -p cue-daemon audio::system_capture::tests --lib
```

For live verification, install the freshly signed helper, grant Microphone and
System Audio Recording once, toggle listening off and on, and confirm:

- `BlueyAudio.app` keeps identifier `sh.bluey.audio` and the same Team ID;
- `BlueyShot.app` keeps identifier `sh.bluey.shot` and the same Team ID;
- no new macOS authorization prompt appears after the grant;
- a denied permission produces one terminal daemon error rather than a series
  of `respawning` messages.

For GitHub tagged releases, configure these Actions secrets from a PKCS#12 that
contains the stable macOS code-signing identity and its private key:

- `BLUEY_MACOS_CODESIGN_P12_BASE64`
- `BLUEY_MACOS_CODESIGN_P12_PASSWORD`

The workflow imports the identity into an ephemeral keychain, deletes the
PKCS#12 immediately after import, and deletes the keychain after packaging.

## Known Limitations

- Migrating from the prior ad-hoc helper to the stable certificate may require
  one final grant because macOS sees a changed code requirement.
- On a Mac with no signing identity, the ad-hoc fallback cannot guarantee TCC
  persistence across changed builds. Release builds should always inject a
  stable distribution identity and follow the existing notarization process.
  `make package-darwin-*` therefore rejects ad-hoc signatures by default;
  `BLUEY_ALLOW_ADHOC_RELEASE=1` exists only for local archive smoke tests.
- `persistent-content-capture` remains unavailable until Apple approves the
  entitlement and release automation receives the matching macOS profile.
- Source-toggle ownership and duplicate capture starts are fixed separately in
  the daemon recording lifecycle; this fix makes permission failures safe even
  if a caller requests capture more than once.

# Settings UI Contract

This document separates Bluey's current, wired settings from future settings
that require server or lifecycle support. A control must not appear interactive
until its persisted behavior exists.

## Trust Rules

- Sign-in and cloud session sync are separate choices.
- Listening, screen analysis, and file attachment use visible controls.
- A toggle reflects persisted state; it is never preselected to drive consent.
- Read-only facts are labeled as facts, not rendered as fake switches.
- Capture exclusion is best effort and must not be described as invisibility.
- Bluey does not expose process impersonation or automatic disguise as a
  customer privacy control.
- Payment Auto Reload is off until the account owner selects it and approves a
  saved card, threshold, and amount.

## Current Desktop Data Controls

The implemented local state is `CueSettings.cloud_sync_enabled` plus the
explicit `cloud_sync_consent_granted` guard.

```json
{
  "cloud_sync_enabled": false,
  "cloud_sync_consent_granted": false,
  "raw_audio_retained": false,
  "training_enabled": false
}
```

Only `cloud_sync_enabled` is currently mutable:

- New installs default it to `false`.
- Browser or CLI sign-in does not change it.
- A legacy `cloud_sync_enabled: true` value without the consent guard is treated
  as off and must be selected again in Settings.
- Desktop Settings reads and writes the same local `CueSettings` file used by
  automatic sync scheduling.
- Turning it off prevents new automatic session uploads while local sessions
  remain available on that device.
- An explicit diagnostic `cloud sync` command may still perform a manual sync;
  that command itself is the user's deliberate action.

`raw_audio_retained` is a read-only `false` state in the current release. Bluey
uses temporary audio chunks for transcription and deletes its temporary local
chunk after the transcription request. The product currently has no retained
audio library and no retained-audio setting to wire.

`training_enabled` is also a read-only `false` state. The current product has no
training opt-in or customer-content training pipeline. Submitted prompts,
transcripts, audio, files, screenshots, and answers are not training data.

## Current Screen-Share Controls

The Settings surface exposes an explicit show/hide overlay checkbox backed by
the same command as F19 and the tray/menu-bar item. This changes overlay
visibility only. Listening and other background state remain separate.

Supported desktop overlay paths request capture exclusion from the operating
system. The UI must state that meeting apps, privileged tools, managed devices,
cameras, or custom capture paths can still show the overlay. Users should test
their own sharing setup.

## Current Account Controls

The web account surface owns:

- account and device state;
- prepaid balance and usage;
- one-time balance reload;
- opt-in saved-card Auto Reload;
- synced session viewing where records exist;
- account export and deletion requests.

Auto Reload HTML, modal state, and JavaScript must initialize from the server's
`auto_topup_enabled` value. A new or unavailable saved-card setup must render
off. Turning it on may open the real card setup flow; it must never happen from
opening the balance modal alone.

## Not Yet A Product Control

Do not add interactive UI for these items until the full persistence and
lifecycle behavior exists:

- cloud retention days with a verified deletion worker;
- artifact-specific retention;
- retained raw audio;
- customer-content training opt-in;
- workspace memory policy;
- app/domain capture exclusions;
- deletion progress by object store;
- organization policy enforcement.

Future server-owned settings need authentication, optimistic concurrency,
multi-device propagation, deletion enforcement, audit events, and regression
tests before they can replace this local contract.

## Validation

- A clean `CueSettings::default()` has cloud sync off until a verified account is linked.
- Verified sign-in enables cloud sync and records the local consent guard; the user can turn it off in Settings at any time.
- Legacy signed-in installs that already had sync enabled keep that state when the consent-guard field is introduced.
- Linking an account does not mutate cloud sync.
- Toggling cloud sync in desktop Settings persists across restart.
- A disabled sync setting prevents new automatic sync scheduling.
- Raw-audio and training rows remain read-only and off.
- Opening Add Balance leaves Auto Reload off unless it was already active.
- Turning Auto Reload on requires a real saved card setup or an existing saved
  method.
- F19 and the tray/menu-bar item say show/hide overlay, not invisible.

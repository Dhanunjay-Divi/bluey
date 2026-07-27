# REVIEW: MEETING-RELIABILITY — Calendar, Overlay, Audio, and STT Hardening

**Commit range:** `2fa8c0bc..meeting-main`
**Reviewer:** Codex multi-agent final review
**Date:** 2026-07-27

## Per-Task Review

### CALENDAR-OAUTH — Provider Authentication and Meeting Context

| Field | Value |
|-------|-------|
| Files | `crates/cue-calendar-cloud/`, calendar seams in `cue-core` and `cue-daemon`, onboarding/settings UI |
| Verdict | 🟢 accept |

**Findings:**

- Native Google and Microsoft public-client PKCE flows validate configuration
  before browser launch, serialize provider lifecycle changes, retain refresh
  credentials in the OS keychain, and fail with actionable status.
- Calendar baseline, pagination, delta, deletion, immutable provider identity,
  conferencing meeting ID, organizer, and attendee email/RSVP behavior are
  covered by hermetic tests.
- Internal meeting-prep prompts and provider identifiers are excluded from
  visible conversation history, persistence, RAG indexing, and cloud sync.
- Live consent and refresh remain externally gated by registered public client
  IDs and interactive provider accounts. No code blocker was found.

---

### OVERLAY-AUDIO — Pill, Source State, and Permission Recovery

| Field | Value |
|-------|-------|
| Files | `crates/cue-meeting-overlay/`, `crates/cue-core/src/overlay*.rs`, daemon audio/session paths |
| Verdict | 🟢 accept |

**Findings:**

- The collapsed pill is draggable, suppresses click-after-drag, and routes its
  controls through exact top-level IPC command types.
- Microphone and system-audio state remain source-specific across daemon IPC,
  collapse/expand, stop/retry operations, and secondary-source permission
  failures.
- Permission denial no longer enters an automatic relaunch loop; the user can
  open the relevant settings pane and explicitly retry only the denied source.
- The clean-install permission-deny/settings/retry path was exercised against
  the staged helper. It produced one source-specific prompt, reached an
  authorized sidecar state, kept one stable helper PID, and cleared the warning.

---

### STT-NATIVE — Prewarm and Signed Helper Reliability

| Field | Value |
|-------|-------|
| Files | `crates/cue-transcribe/`, daemon STT paths, `native/macos/cue-audio/`, `native/macos/cue-shot/` |
| Verdict | 🟢 accept |

**Findings:**

- Startup prewarm uses disposable stream state and shared single-flight
  Parakeet weights, reducing first-transcript latency without contaminating the
  live decoder.
- Audio capture generations, STT initialization, source shutdown, and idle
  watchdog behavior have focused regression coverage.
- The arm64 BlueyAudio and BlueyShot helpers build with stable signed identities.
  Signature, designated requirement, required audio-input entitlement,
  architecture, plist, direct no-capture, and LaunchServices no-capture checks
  passed.

---

### WEBHOOK-INSTALL — Doorbell Validation and Clean Replacement

| Field | Value |
|-------|-------|
| Files | `server/src/api/calendar.rs`, release/build/install scripts, workflow and deployment docs |
| Verdict | 🟢 accept |

**Findings:**

- Google and Microsoft webhook endpoints authenticate bounded change
  notifications; they intentionally do not exchange OAuth codes or carry event
  content.
- Incremental native polling remains authoritative until subscription renewal,
  account/device routing, and authenticated device nudges are implemented.
- Server calendar tests and warning-denied Clippy passed. Modified install,
  build, native helper, and release scripts passed syntax and packaging checks.
- Clean reinstall, visible overlay startup, source control, and collapsed-pill
  movement were exercised against the staged branch build.

---

## Cross-Task Findings

- Provider event IDs, conferencing meeting IDs, attendees, and occurrence
  identity remain distinct across calendar sync, warmup, banner approval, and
  overlay delivery.
- Exact IPC dispatch, source-aware permission state, and serialized provider
  replacement close the principal cross-component race and stale-state paths.
- OAuth bearer/refresh tokens remain on-device; webhook ingress carries only
  authenticated provider doorbells.
- No secrets, plaintext token fallback, privacy regression, or code blocker was
  found in the reviewed scope.

## Build & Test Verification

```text
cargo fmt --all -- --check
  ✅ full workspace

cargo test -p cue-core -p cue-calendar-cloud -p cue-meeting-overlay -p cue-transcribe
  ✅ 242 passed, 0 failed

cargo test -p cue-daemon --features parakeet-stt,local-memory,cloud-calendar
  ✅ 423 passed, 0 failed, 18 intentionally ignored

cargo clippy -p cue-daemon --all-targets \
  --features parakeet-stt,local-memory,cloud-calendar -- -D warnings
  ✅ passed

(cd server && cargo test calendar)
  ✅ 8 passed, 0 failed, 130 filtered

(cd server && cargo clippy --all-targets -- -D warnings)
  ✅ passed

npx --yes prettier@3.6.2 --check <18 changed UI files>
  ✅ all 18 passed

(cd crates/cue-meeting-overlay/ui && npm run build)
  ✅ 78 modules transformed; 3 pre-existing non-fatal advisories

native BlueyAudio bundle + strict verification; BlueyShot build/safe smoke
  ✅ passed

clean reinstall + visible pill drag + real BlueyAudio microphone grant/retry
  ✅ passed; authorized helper PID stable and warning cleared

git diff --check
  ✅ passed
```

## Overall Verdict

🟢 **ACCEPT** — No code blockers remain in the reviewed scope. Live provider
OAuth is externally gated on registered public-client applications and must be
completed before a production release claim.

## Follow-ups for Next Batch

- Register Google Desktop and Microsoft public-client applications, configure
  their public client IDs, and run interactive consent, refresh, attendee, and
  conferencing-ID smoke tests.
- Add webhook subscription renewal, tenant/device ownership routing, and an
  authenticated device nudge before treating webhooks as a low-latency delivery
  transport.

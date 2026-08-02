# ParakeetAI 3.7.0 Windows feature map

| Area | Static result | Evidence/boundary |
|---|---|---|
| Onboarding/auth | Observed | renderer/main auth cookies and onboarding flows |
| Mic/system audio | Observed | renderer mic + Chromium loopback, native AEC IPC |
| Meeting detection | Observed | Windows active capture-session enumeration and app allowlist |
| Interview assistance | Observed | live transcript/answer, auto-answer controls, presets |
| Screenshot/context | Observed | capture IPC and renderer context paths |
| Overlay/shortcuts | Observed | content protection, always-on-top, global shortcuts |
| Session/recovery | Observed | meeting state, recovery, utility-process boundary, forced update deferral |
| Auto-answer | Observed | renderer controls and cloud client calls; server enforcement unknown |
| Updates/telemetry | Observed | GitHub updater, Mixpanel, app logs |
| Job discovery/ranking | Not observed | no job-source/match/application system |
| Automated browser/profile isolation | Not observed | no ATS browser execution |
| Resume tailoring/diff/export | Not observed | interview context is not a job document workflow |
| Application answer memory | Not observed | no verified application fact model |
| ATS adapters/CAPTCHA/2FA/takeover | Not observed | no form execution path |
| Durable job queues/leases/receipts | Not observed | recovery is live-session state only |
| Email/calendar outcome tracking | Not observed | meeting detection is local audio-session based |
| Billing/deletion/privacy | Partial/unknown | account/plan/settings routes exist; server retention/deletion unreachable |

The Windows-native value is active-microphone detection and bounded AEC, not job
automation. The packaged Rust provides stronger evidence than minified renderer
strings, but it does not expose the cloud implementation.

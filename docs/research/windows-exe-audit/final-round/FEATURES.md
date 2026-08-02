# Final Round 2.4.0 Windows feature map

| Area | Static result | Evidence/boundary |
|---|---|---|
| Onboarding/auth | Observed | launch setup, PKCE/auth, permission/payment flows |
| Mic/system audio | Observed | native Windows WASAPI capture with pause/resume and AEC symbols |
| Local VAD | Observed | Silero ONNX and bounded/drop behavior |
| Interview modes | Observed | interview assistant, coding, system-design, phone/external, presets |
| Screenshot/context | Observed | capture halo/screenshot upload and structured answer panels |
| Overlay/shortcuts | Observed | multiple content-protected widgets and native keyboard monitor |
| Session recovery | Observed | explicit lifecycle, reconnect, persistence/resume, deferred updates |
| Video/mock coach | Observed client | Daily/video coach resources; backend behavior unknown |
| Reports/history | Observed | documents/transcripts/report/regenerate paths |
| Billing/telemetry/update | Observed | plans, Sentry/PostHog/Amplitude, publisher-pinned updater |
| Job discovery/ranking | Not observed | no posting/discovery/match engine |
| Automated job browser/profile isolation | Not observed | no ATS execution path |
| Resume tailoring/diff/export | Not observed | uploaded interview documents are not job tailoring |
| Application answer memory | Not observed | interview/session context is not verified reusable job facts |
| ATS adapters/CAPTCHA/2FA takeover | Not observed | no application forms |
| Durable application leases/idempotency | Not observed | session state machine is not a job submit ledger |
| Application receipts | Not observed | screenshots/reports are interview artifacts |
| Email/calendar outcomes | Not observed | no job-outcome correlator |
| Account deletion/privacy | Unknown | settings/auth controls exist; server hard-delete/retention unavailable |

Final Round is the strongest Windows low-latency interview implementation in the
set. Bluey is already stronger on the user's end-to-end job goal; it should adapt
audio lifecycle, structured interview panels, and publisher-pinned updates.

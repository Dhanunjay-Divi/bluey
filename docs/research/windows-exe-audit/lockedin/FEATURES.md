# LockedIn 1.8.8 Windows feature map

| Area | Static result | Evidence/boundary |
|---|---|---|
| Onboarding/auth | Observed | Clerk/Firebase desktop auth and welcome/intro routes |
| Interview presets/live copilot | Observed | role/session presets, transcript/answers, coding and learning flows |
| Mic/system audio | Observed on Windows | browser mic + selected-screen Chromium loopback (`electron.js:1620-1682`) |
| Screenshots/documents | Observed | full/area capture, image analysis, PDF/DOCX parsing/rendering |
| Helper collaboration | Observed | Socket.IO/WebRTC helper path and robotjs remote input (`electron.js:2936-3110`) |
| Overlay/shortcuts | Observed | always-on-top/content-protected widgets, global keyboard server |
| Session recovery | Observed | navigation to session, close-with-session info, renderer persistence/recovery |
| Billing/referrals/credits | Observed | Stripe purchase/credits/referral routes |
| Updates | Observed | S3 publisher-configured electron-updater |
| Job discovery/ranking | Not observed | no job-source or match system |
| Job browser/profile isolation | Not observed | helper control is collaboration, not isolated ATS browsing |
| Resume tailoring/diff/export | Partial context only | resume/document parsing exists; no evidenced job-specific tailoring/version diff/export pipeline |
| Answer memory | Not observed for applications | session presets/history do not establish confirmed reusable application facts |
| ATS adapters/fallback | Not observed | no Greenhouse/Lever/Workday form implementation |
| CAPTCHA/2FA/assessment takeover | Not observed for job applications | helper collaboration is not a bounded challenge state machine |
| Job queues/retries/duplicate prevention | Not observed | session recovery is not durable submit idempotency |
| Application receipts | Not observed | screenshots/documents are interview context, not submission evidence |
| Email/calendar outcomes | Not observed | no application outcome correlator |
| Account deletion/privacy | Unknown | sign-out/settings exist; backend hard deletion/retention not in installer |

LockedIn's Windows differentiation is collaboration/remote control, not job
automation. Bluey should reject the broad input-injection boundary and retain
human takeover inside an authenticated, preserved browser session instead.

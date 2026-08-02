# Cluely feature map

## Evidence standard

`Observed` means a reachable client route/control plus supporting implementation or RPC call was present in the signed bundle. `Not observed` means targeted searches and route/dependency review found no supporting implementation; it does not prove that an opaque server has no such capability. Vendor-library strings alone, including Clerk CAPTCHA/MFA/account text, are not treated as product features. File hashes and locations are in [static-bundle-map.txt](evidence/static-bundle-map.txt).

| Area | Static result | Evidence and boundary |
|---|---|---|
| Onboarding | Observed | Routed demos for sign-in, permissions, listening, hiding, asking, moving, notes, and subscription. Completion requires sign-in plus microphone, screen-recording and accessibility flags (`route-BzHuaDna.js`, `permissions-DaGfjhrd.js`; `main.js:16780-16846`). |
| Authentication | Observed | Clerk-backed browser/email sign-in; `cluely-v2://auth/success?token=…`; token held in main-process memory until renderer consumption (`main.js:15766-15794,16628-16665`). Public client key redacted from evidence. |
| Live microphone/system audio | Observed | SoX microphone and AudioTee system capture (`main.js:16025-16122`), local VAD, cloud transcription (`transcription-C5LDIt8L.js`). |
| Live copilot/chat | Observed | Cloud agent connection, transcript-backed questions, screenshot and partial-audio upload, reconnect/resume (`chat-BH3ET_qx.js`). |
| Screen context | Observed | Full matched-display PNG capture through `desktopCapturer`, three attempts at 500 ms; content protection enabled while capturing (`main.js:16743-16778`). No semantic accessibility/page-text capture was observed. |
| Invisible overlay | Observed, entitlement-gated | Content-protected always-on-top windows. Renderer gates invisible mode on a Pro Plus entitlement. Effectiveness against third-party capture is unvalidated. |
| Global shortcuts | Observed | Show/hide, ask, clear, settings, start/stop, move and scroll actions; Tab is captured while expanded (`main.js` plus control/chat renderer). |
| Session history | Observed | Session list/search/delete/resume, periodic refresh, title/post-processing polling, transcript/session state (`chat-BH3ET_qx.js` around bytes 668727-690506). |
| Meeting/calendar | Observed | Google Calendar OAuth connect/disconnect, upcoming meeting polling, meeting-linked session start, summaries, recent people and attendee briefs (`chat-BH3ET_qx.js`; `settings-BuvgGbrV.js`). Server semantics unknown. |
| Custom modes | Observed | Named prompt, active mode, 600 ms autosave, ordering, templates for sales/recruiting/meetings/lectures/interviews, and uploaded files (`settings-BuvgGbrV.js` around bytes 152004-165514). |
| Mode files | Observed | 20 MB per-file UI cap, free-plan count cap, server presign, HTTP PUT, metadata creation, and sync-status polling (`settings-BuvgGbrV.js`). File processing backend unknown. |
| Billing | Observed | Free meeting/message limits, Pro and Pro Plus gates, Stripe checkout/portal/change/reactivation, legacy RevenueCat entitlement migration. Current amounts are fetched, not embedded (`pricing-DouDVWHe.js`, `subscription-plan-selection-Ztn3FoAd.js`). |
| Settings | Observed | Audio devices/test, input/output language, invisible mode, launch at login, meeting usage, shortcuts, calendar, account, support/release notes, updater control, sign-out/onboarding reset. |
| Telemetry | Observed | PostHog at `https://ph.cluely.com`; identifies user with email/full name; records navigation/launch state/system info; exception integration can include console material (`posthog-VTzWjthB.js`, `route-BzU-sEaW.js` around byte 602409). Runtime flags remain unknown. |
| Updates | Observed | Immediate/hourly check, download, install path using electron-updater (`main.js`; `app-update.yml`). |
| Local/cloud browser execution | Not observed | No Playwright/Puppeteer/browser-control adapter, job portal route, or remote browser service client was found. Electron's Chromium renderer is product UI, not automated browsing. |
| Browser profiles/multi-account isolation | Not observed | No managed browser profile inventory, account-to-profile mapping, profile encryption, or browser-context orchestration was found. |
| Job discovery/ranking | Not observed | No job/search/source/ranking/application route or RPC procedure was found. |
| Resume import/tailoring/diff/export | Not observed | Mode file upload can provide context, but no resume-specific model, diff, versioning, tailoring, or document exporter was found. |
| Persistent answer memory | Not observed | Mode prompts/files and session history are context, but no confirmed reusable question-answer memory with scope/approval was found. |
| ATS adapters/fallback | Not observed | No Greenhouse, Lever, Workday, Ashby, SmartRecruiters, DOM form adapter, or semantic submission fallback implementation was found. |
| CAPTCHA/2FA/assessment takeover | Not observed | Clerk dependency contains generic auth challenge components; no Cluely job-automation intervention/takeover workflow was found. |
| Job queues/retries/duplicate prevention | Not observed | Chat reconnect and transcription concurrency exist, but no durable application queue, lease, idempotency, reservation, duplicate fingerprint, or unknown-side-effect state was found. |
| Submission receipts/evidence | Not observed | Screenshots are chat context; no final job submission, confirmation, artifact hash, application receipt, or evidence bundle was found. |
| Email/outcome tracking | Not observed | Calendar is present; no Gmail/Outlook mailbox connection, application-outcome correlation, or email-event tracking was found. |
| Account deletion/privacy center | Not observed in app routes | Sign-out/reset exists. The generic Clerk bundle contains account-deletion strings, but no Cluely route/control was found, so deletion behavior is unknown. |

## Workflow summary

The evidenced path is: sign in → grant OS permissions → optionally connect Google Calendar or select/create a mode → start/resume a meeting session → capture microphone/system audio → local VAD → cloud transcription → cloud agent answers using transcript, screenshot, and mode files → heartbeat/persist transcript → end/post-process → search/revisit sessions and people. It is a credible real-time conversation product, but it does not overlap Bluey's high-risk final-submit automation except at interview preparation and meeting assistance.

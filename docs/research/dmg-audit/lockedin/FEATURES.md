# LockedIn feature map

This map treats client wiring and UI as **observed**, plausible effects as **inferred**, and server/runtime-only behavior as **unknown**. Marketing copy is not used as implementation evidence. The byte-verified sources are listed in [asar-inventory.txt](evidence/asar-inventory.txt).

| Area | Static finding | Confidence and provenance |
|---|---|---|
| Onboarding and authentication | `locked-in:` deep link, Clerk identity, Firebase custom-token sign-in, auth state, profile UI, tutorial/permission flows; isolated first launch reached `#/sign-in` | Observed wiring plus runtime route: `public/electron.js:2466-2585`, [runtime evidence](evidence/runtime-unauthenticated.md); server token exchange/revocation unknown |
| Live interview/meeting copilot | Live session routes, transcript/answer state, automatic/manual answers, priority questions, presets, prompts, Meeting AI, Online Assessment, and mock/interview contexts | Observed client capabilities: [renderer-features.md](evidence/renderer-features.md); runtime quality/enablement unknown |
| Microphone and system audio | Custom per-architecture capture modules; mute/restart controls; native audio frameworks | Observed: `public/electron.js:270-701`, [native-components.txt](evidence/native-components.txt) |
| Screen context | Full/cropped screenshots, thumbnails, desktop source selection, screenshot copilot upload/answer paths | Observed: `public/electron.js:1285-1683` and renderer bundle; actual permission grants not tested |
| Stealth and floating UI | Always-on-top, all-workspaces, click-through, content protection, hidden taskbar, cursor/document windows | Observed: `public/electron.js:794-1033,2647-2669`; OS effectiveness unknown |
| Custom prompts and presets | Firestore-backed interview presets, custom prompts, optimization endpoint, copilot preferences | Observed: [storage-inventory.md](evidence/storage-inventory.md), [network-inventory.txt](evidence/network-inventory.txt) |
| Documents | File records, indexing/deletion endpoints, indexed context, document window | Observed client wiring in [renderer-features.md](evidence/renderer-features.md) and [network-inventory.txt](evidence/network-inventory.txt); backend parsing/index durability unknown |
| Resume import/review | File upload/index, job title/company/description context sent to `resume_review`, Resume Guru route, external resume product | Observed in [renderer-features.md](evidence/renderer-features.md) and [network-inventory.txt](evidence/network-inventory.txt). Structural diff, local tailoring pipeline, and PDF/DOCX export are not established in this DMG |
| Session history and reports | Firestore session/chat history, report generation/read calls, post-interview survey | Observed request/UI wiring in [renderer-features.md](evidence/renderer-features.md), [storage-inventory.md](evidence/storage-inventory.md), and [network-inventory.txt](evidence/network-inventory.txt); report generation correctness and retention unknown |
| Duo helper | Invite events, helper eligibility/membership/ICE calls, WebRTC peer/data-channel wiring | Observed in [renderer-features.md](evidence/renderer-features.md) and [network-inventory.txt](evidence/network-inventory.txt); helper identity/authorization experience unknown |
| Remote input | WebRTC `remote-control` messages can pass renderer state, preload `executeRemoteInput`, and main-process robotjs mouse/key/scroll execution | Observed capability wiring only; no claim of user consent or use. See [renderer-features.md](evidence/renderer-features.md) |
| Billing and usage | Subscription-info, payment creation, Stripe, coupons/referrals, credits/time packages, pricing views | Observed UI/API shapes in [renderer-features.md](evidence/renderer-features.md) and [network-inventory.txt](evidence/network-inventory.txt); current plans, enforcement, overages, refunds, and cancellation behavior unknown |
| Settings and account deletion | Profile, audio, shortcuts, stealth, prompts/presets, plan state, and Firebase Auth user deletion UI | Observed in [renderer-features.md](evidence/renderer-features.md) and [storage-inventory.md](evidence/storage-inventory.md). Cascading deletion of Firestore/files/backend/analytics/payment records unknown |
| Telemetry and attribution | Microsoft Clarity, GTM, LinkedIn Insight, Rewardful, Clerk telemetry, plus first-party URL tracking | Observed static scripts/calls in [network-inventory.txt](evidence/network-inventory.txt) and [renderer-features.md](evidence/renderer-features.md); runtime payloads and consent behavior unknown |
| Updates | S3 `electron-updater`, automatic checks/download, install-on-quit, prerelease/downgrade enabled | Static configuration plus runtime check attempt. The closed proxy blocked all remote response/download/install; see [runtime evidence](evidence/runtime-unauthenticated.md) |

## Bluey Jobs-required functionality not established by this DMG

| Required area | Static conclusion |
|---|---|
| Local/cloud job browser execution | Not found. Electron windows support LockedIn's own UI; no application-run browser controller is established. |
| Browser-profile and multi-account isolation | Not found. Inspected windows use the default shared Electron session; no per-job/account browser partition is created. |
| Job discovery and ranking | Not found in first-party client resources. |
| Resume tailoring, structural diff, and document export | Partial resume-review wiring only; no local structural diff/export workflow established. |
| Application answer memory | Interview prompts/presets are present, but no confirmed, scoped ATS answer-memory model was found. |
| ATS adapters and generic fallback | Not found. Marketing references were excluded as proof. |
| Application CAPTCHA, 2FA, assessments, and takeover | Online Assessment is a copilot context, not static evidence of job-application challenge handling. No ATS takeover flow was found. |
| Durable queue, leases, idempotency, and crash recovery | Client session recovery is present; durable application-worker semantics were not found. Backend behavior is unknown. |
| Duplicate application prevention | A running-interview/session check is present; no company/application deduplication model was found. |
| Submission receipts and evidence | Session screenshots/history exist, but no ATS submission receipt with document hashes, confirmation, and side-effect state was found. |
| Email/calendar outcome integration | No mailbox/calendar authorization or application-status synchronization path was established. Password-reset email and survey/report features do not qualify. |

## Feature flags and production/development behavior

Observed configuration names include `REACT_APP_ENDPOINT_SWITCH` and development/production Clerk publishable-key selectors. Main-process state includes a development flag, and updater code selects production/development YAML paths. Values are intentionally omitted. No independent flag registry, rollout percentage, or signed remote-configuration system was visible. Evidence: [asar-inventory.txt](evidence/asar-inventory.txt) and [electron-main-observations.md](evidence/electron-main-observations.md).

## Runtime checkpoint and remaining validation

The approved isolated pass validated the unauthenticated `#/sign-in` route, actual renderer sandbox flag, first-launch 51-file profile footprint, Firestore offline behavior, static-model-tier fallback, and automatic update attempt. It deliberately did not validate screen/audio permissions, auth, authenticated routes, server responses, helper/Duo, remote control, real update delivery, or backend data creation. Those remain separately approval-gated; see [runtime-unauthenticated.md](evidence/runtime-unauthenticated.md) and [limitations.md](evidence/limitations.md).

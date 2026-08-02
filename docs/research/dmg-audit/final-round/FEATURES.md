# Final Round feature map

## Observed product shape

The desktop is an interview-preparation and live-interview copilot, not a job-discovery/application-automation product. The first-party route map includes auth, onboarding/payment, copilot, career coaches, goals, preparation, reports, settings, and subscriptions (`renderer:37428-37516`). The claims below come from executable client routes/services/resources; marketing text alone was not used. [Feature evidence](evidence/features-static.txt)

## Feature inventory

| Area | Observed implementation | Confidence/evidence |
|---|---|---|
| Onboarding/auth | External-browser OAuth, PKCE/state callback, permission mission, payment mission | High; `main:9990-10001`, `main:10387-10560`, renderer routes |
| Live interview | Current-machine mic/system audio, live ASR, transcript, assistant generation, pause/force generation | High; native/service/UI contracts; [architecture](evidence/architecture-static.txt) |
| External/phone interview | Phone/external-audio mode and audio retrieval endpoint | High; service/API inventory |
| Language/model/style | Language, model, answer length, tone, programming language, and capability settings | High; `main:16728-17173` |
| Coding/system design | Streamed pattern, idea, code, walkthrough, tests, and design sections | High; interview socket/service contracts |
| Screenshot assistance | Display capture, app-window content protection, JPEG compression, upload | High; `main:16503-16576`, `main:12120-12129` |
| Local VAD | Bundled Silero ONNX, bounded processing queue | High; `main:6041 onward`; resource hash |
| Goals | Company/job-description goal CRUD | High; goal service/API calls |
| Resumes | PDF/DOCX picker/validation, list/upload/delete, interview context | High; `main:11456-11764`, `main:14569-14643` |
| Career coaching | Daily/Pluot video coach and mock interview resources | Medium-high; UI/service/CSP inventory; unauthenticated runtime only |
| Reports | List, document, transcript, regenerate | High; `main:14502-14568` |
| Billing | Plans, checkout, privilege/trial, subscription management/customer portal | High; `main:12611-13042` |
| Stealth/privacy | Content protection enabled by default and reapplied every 500 ms | High; `main:5968-5989`, `main:20180-20354`, renderer:35893-35942 |
| Meeting lifecycle | Native meeting-process monitoring and auto-close after tracked apps exit | High; `main:3807-3838`, `main:15535-15719` |
| Updates | Generic HTTPS feed, user-mediated download, install on quit | High; `main:5997-6021`, `main:6882-7124` |
| Support | Backend contact form | High; `main:11030-11133` |

## Workflow trace

1. The user authenticates in the system browser; `frai://callback` returns a code and state for PKCE exchange. Access/ID tokens remain memory-only; refresh token/cached user use the safeStorage wrapper. [Architecture evidence](evidence/architecture-static.txt)
2. On launch, the client checks subscription privilege, closes a server-reported active session, creates/launches a new one, connects the `/desktop` Socket.IO namespace, and starts ASR. `main:15808-16135`
3. Native capture supplies mic/system PCM. Local Silero VAD limits forwarded audio. The socket streams transcript and assistant/code/design events. [Architecture evidence](evidence/architecture-static.txt)
4. The interview-assistant widget can request a compressed screenshot and upload it for visual/coding help. `main:16503-16576`, `main:12120-12129`
5. Teardown closes ASR/room/session, clears ephemeral messages, and may trigger reports. Recovery is best effort and mostly memory-bound. `main:16205-16496`

## Resume behavior

Final Round imports resume documents for interview context but does not implement job-specific document tailoring, a visible before/after diff, or PDF/DOCX resume export in this artifact. Its local file validator reads PDFs/DOCX, but parser/read failure returns valid and lets upload proceed (`main:11485-11561`). Server-side parsing/scanning is unknown. [Feature evidence](evidence/features-static.txt)

## Answer memory and interventions

Live interview messages are kept in a capped in-memory session map; reports may persist server-side. There is no reusable account/track/company application-answer memory, no sensitive-question approval model, and no application-form intervention workflow. This absence is based on the complete route, preload, service, API, and storage inventory. [Feature evidence](evidence/features-static.txt)

## Billing and limits

The desktop exposes plan lists, Stripe-style checkout/customer management, subscription state, privileges, and trial remaining. Paid time periods include weekly through yearly and one-time variants in UI/service data. No explicit desktop overage UI was found; server enforcement is inaccessible. [Feature evidence](evidence/features-static.txt)

## Settings, deletion, privacy, telemetry

Settings cover interview model/style/language, permissions, shortcuts, general/account/subscription, and stealth. The account panel includes profile, subscription, and sign-out but no self-service deletion control (`renderer:35070-35119`). Telemetry uses Sentry, PostHog, and Amplitude; authenticated identity includes user ID/email/name, and some transcript/answer text reaches the Sentry-forwarded logging path. [Network evidence](evidence/network-static.txt) [Security evidence](evidence/security-static.txt)

## Application-automation absence map

The following are absent from this DMG's complete first-party product/code inventory:

- job discovery, ranking, career-track agents, freshness filtering, and deduplication;
- local/cloud job-site browser execution and profile/multi-account isolation;
- job-specific resume tailoring, visible diff, and PDF/DOCX export;
- reusable application answer memory;
- Greenhouse/Lever/Workday/Ashby/SmartRecruiters or generic ATS adapters;
- CAPTCHA, 2FA, assessment, or job-site takeover flows;
- application queueing, retries, leases, idempotency, crash recovery, and duplicate-submit prevention;
- application receipts, exact-document checksums, confirmation screenshots, or status evidence trails;
- Gmail/Outlook/calendar application-outcome tracking.

This is precisely where Bluey already has the stronger product foundation; see [BLUEY-GAP-MAP.md](BLUEY-GAP-MAP.md).

## Runtime-validation unknowns

- Authenticated onboarding and live-session UI details.
- Permission prompts and actual audio latency/quality.
- Coach/video behavior and report content.
- Server-side retention, deletion, billing, idempotency, and safety controls.
- Production update-install signature enforcement and rollback.

# Renderer feature and workflow observations

Primary source: `LockedIn.app/Contents/Resources/app.asar:build/static/js/main.8ad05cf1.js`, SHA-256 `e639c1c85646efe8da58a99b3dad0bf72e6cbbcf092a2b79abf3ec6572cdc913`.

Secondary sources: `build/index.html` (SHA-256 `fbd334bdd6ba2604c68a5b778ecfdc545508a756c4ba7730860e78348b81dfac`), `package.json`, `firestore.js`, and the main/preload observations. The renderer is minified and has no first-party source map, so source locators are bundle paths plus stable literals and call shapes, not original component line numbers.

## Observed UI and workflow wiring

- Routes/literals cover dashboard, session pilot, helper session, history, document window, job consultant, meeting AI, online assessment, post-interview, profile, resume guru, keyboard shortcuts, and interactive tutorial.
- Authentication wiring listens for the deep-linked Firebase custom token, signs in with Firebase, and associates a Clerk user ID. `lastClerkUserId` is stored in localStorage.
- Session experiences include live transcript/answer controls, manual and automatic answer generation, interview presets, custom prompts, priority questions, indexed documents, screenshot copilot, history, report generation, post-interview survey, and Duo helper invitations.
- Screenshot flows resize/upload captures for model assistance.
- Socket.IO is configured WebSocket-only with a Firebase ID token, unlimited reconnect attempts, and one-to-five-second retry delays.
- Duo creates an `RTCPeerConnection` with backend-supplied ICE configuration and a `remote-control` data channel. Messages are parsed as JSON and forwarded to `executeRemoteInput` only when renderer control state permits. This is observed wiring. The identity, consent, authorization, and revocation experience is unknown without runtime/server evidence.
- Before starting, the renderer calls `/check_running_session` with an eight-second abort timeout and queries active Firestore sessions. This is session duplication prevention, not a durable worker lease or idempotent job queue.
- Resume-related calls include file import/indexing, a `resume_review` request with job context, and links into the separate `resume.lockedinai.com` product. Static evidence did not establish a local resume diff, DOCX/PDF export, job-application queue, or ATS submission workflow.
- Billing UI/calls reference subscriptions, prices, coupons/referrals, credit/time packages, and customer-portal/payment creation.
- Settings include stealth, shortcuts, prompts/presets, audio, profile, plan state, and account deletion UI. The deletion UI invokes Firebase user deletion; cascading deletion of Firestore, object storage, or backend records is not established.

## Features not found in the static client

Searches of first-party static resources did not establish job discovery/ranking, browser-profile or multi-account isolation, ATS-specific application adapters, application-form answer memory, CAPTCHA/2FA takeover for application submission, durable job retries/leases, submission receipts, application email/calendar synchronization, or application outcome tracking.

Marketing strings mentioning ATS/resume benefits were not treated as proof of those implementations.

## Index document observations

`build/index.html` has no Content-Security-Policy meta tag or response-visible policy in this static artifact. It directly loads live scripts/resources for Stripe Buy Button, Microsoft Clarity, Rewardful, Google Tag Manager, and LinkedIn Insight, in addition to fonts and tracking pixels. Actual requests and any server response headers remain runtime unknowns.

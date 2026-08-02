# Littlebird feature map

Littlebird is a general contextual AI assistant with meeting, cross-application context, chat, knowledge, and integration surfaces. Static product code does not show a dedicated job discovery/application pipeline. Evidence: E-LB-FEAT-002 and E-LB-FEAT-007.

## Workflow coverage

| Requested area | Static finding | Confidence / boundary | Evidence |
| --- | --- | --- | --- |
| Onboarding | User Details → Accessibility → Meeting Notes → Integrations → Hummingbird → Data Sharing → Referral Code → Done | High; shipped route/source-map logic | E-LB-FEAT-001 |
| Authentication | Google, Apple, email OTP via Auth0; desktop browser login returns through custom URL scheme | High; shipped control flow | E-LB-FEAT-001 |
| Local browser execution | Electron renderer plus cross-app accessibility/browser parsers | High for context observation; no job runner | E-LB-ARCH-007, E-LB-ARCH-008 |
| Cloud browser execution | No cloud browser/session runner found | Static negative only | E-LB-FEAT-007 |
| Profile/multi-account isolation | Multiple OAuth integration accounts; no per-job browser-profile state machine found | Partial/general-purpose | E-LB-FEAT-004, E-LB-FEAT-007 |
| Job discovery/ranking | Not found | Static negative only | E-LB-FEAT-007 |
| Resume import/tailor/diff/export | Not found as a dedicated workflow | Static negative only | E-LB-FEAT-007 |
| Answer memory | General assistant memory/world model exists; job-question scoped answer memory not found | General capability is not job answer memory | E-LB-FEAT-002, E-LB-FEAT-007 |
| ATS adapters/fallback | No Greenhouse/Lever/ATS adapter registry found; ContextKit has generic app-parser fallback | Parser fallback is not form submission | E-LB-ARCH-008, E-LB-FEAT-007 |
| CAPTCHA/2FA/assessment/takeover | Account-login OTP exists; no job challenge/handoff model found | Static negative only | E-LB-FEAT-009 |
| Queue/retry/recovery | Helper supervision and WSS reconnect; queues are memory-only | High for client process behavior | E-LB-FEAT-008 |
| Duplicate prevention/idempotency | No job/application identity or durable idempotency model found | Static negative only | E-LB-FEAT-007, E-LB-FEAT-008 |
| Submission receipts/evidence | No job confirmation/exact-resume/screenshot receipt model found | Static negative only | E-LB-FEAT-007 |
| Email/calendar | Google Calendar and Gmail onboarding, multi-account integration model, email draft operations; parsers for Outlook/Slack/Teams and others | High for client paths; server results untested | E-LB-FEAT-004 |
| Outcome tracking | Meetings/activity exist; no application outcome tracker found | Static negative only | E-LB-FEAT-007 |
| Billing/usage | Four plan families, trials, feature limits, credits, pools, auto-refill/cap, invoices, Stripe portal, cancellation | High for UI/API paths; enforcement untested | E-LB-FEAT-005 |
| Privacy/deletion | Time-bounded/all context deletion, account deletion, export request, app/domain/category exclusions | High for client paths; backend completion untested | E-LB-FEAT-006 |
| Telemetry | Axiom, PostHog, Sentry, Singular, Product Fruits | High; signed bundle paths | E-LB-SEC-005, E-LB-SEC-006 |
| Updates | Signed Electron updater, feature-controlled channels, periodic checks, active-meeting deferral | High; shipped main/config | E-LB-ARCH-011 |

## Strongest product capabilities

1. **Cross-application context.** Forty-six parser resources, native accessibility observation, screenshot/JSON capture, and application/domain/category exclusions form a broad context layer. Evidence: E-LB-ARCH-008.
2. **Meetings.** Detection/status UI, calendar awareness, native audio/transcription helper, transcripts/import routes, and update deferral during calls form a coherent meeting workflow. Evidence: E-LB-FEAT-003.
3. **General knowledge workspace.** Chats, projects, journals, reports/routines, meetings, world model, arenas, MCP grants, and local search are backed by explicit routes/stores and Dexie tables. Evidence: E-LB-FEAT-002 and E-LB-ARCH-009.
4. **Consent-aware local tools.** Axon validates tool input and supports allow/ask/deny with risk classes and expiry. Evidence: E-LB-FEAT-008.
5. **Commercial controls.** Feature-specific limits, credits, team pools, refill caps, invoices, and retention flows go beyond a simple plan switch. Evidence: E-LB-FEAT-005.

## Important unknowns

- Whether native context redaction/exclusions prevent all sensitive captures in real applications.
- Whether server-side billing, deletion, export, integration, and usage enforcement matches client expectations.
- Whether authenticated feature flags reveal additional capabilities not present in the signed resources.
- Actual network request/response payloads, retention, and failure handling under an authenticated account.
- Transcription accuracy and permission-prompt behavior.

No marketing claim is used as proof; these unknowns require separately approved, credentialed runtime/server evidence.

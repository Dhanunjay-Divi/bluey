# Round 483 - Jobs Trust + Privacy Competitive Audit

Date: 2026-07-11

This is a product and legal-disclosure gap audit for counsel/product review. It is not legal advice. Research used public pages only; no accounts were created, no purchases were made, and no forms were submitted. No screenshots were captured.

## Sources Reviewed

- Bluey Jobs internal docs: `jobs/README.md`, `jobs/ARCHITECTURE.md`, `jobs/OPERATIONS.md`, `docs/rounds/ROUND-479-JOBS-AUTOMATION-EXECUTION-PIPELINE.md`
- Bluey public Privacy and Terms: https://bluey.sh/privacy, https://bluey.sh/terms
- AIApply: https://aiapply.co/privacy-policy, https://aiapply.co/terms-of-service, https://aiapply.co/
- ApplyBlast: https://applyblast.com/, https://applyblast.com/privacy, https://applyblast.com/terms
- LazyApply: https://lazyapply.com/privacy, https://lazyapply.com/terms, https://lazyapply.com/refundpolicy
- Sonara: https://www.sonara.ai/privacy-policy, https://www.sonara.ai/terms-conditions
- JobCopilot: https://jobcopilot.com/privacy-policy/, https://jobcopilot.com/terms/, https://jobcopilot.com/responsible-ai-job-applications/
- Simplify: https://simplify.jobs/privacy, https://simplify.jobs/terms, https://simplify.jobs/
- Teal: https://www.tealhq.com/privacy-policy
- Huntr: https://huntr.co/privacy
- Jobscan: https://www.jobscan.co/privacy, https://www.jobscan.co/gdpr
- EarnBetter: https://earnbetter.com/privacy/, https://earnbetter.com/tos/
- LoopCV: https://www.loopcv.pro/privacy/

Repo search note: `rg --files | rg -i '(^|/)(privacy|terms|security|policy|subprocessor|cookie|legal)(\\.|/|$)'` did not find standalone Bluey Terms/Privacy markdown files. The public Bluey policy copy is embedded in the site output and accessible at `/privacy` and `/terms`.

## Competitive Notes

AIApply has the most specific job-automation disclosures reviewed: it names AutoApply data such as prompts, drafts, submissions, application content, and mailbox metadata; says model providers are instructed not to train on prompts/outputs; explains automated ranking/form-filling and Review Mode; states AutoApply mailbox retention; and says Auto-Apply is non-refundable after applications are submitted.

ApplyBlast publishes public privacy/terms routes, but the pages are rendered through Next.js payloads rather than plain text. Publicly visible content includes system-generated email aliases, emails/attachments/confirmations, sharing with employers/ATS/job boards including sensitive information where required, deletion by email, and a statement that submitted information cannot be deleted from employer/platform systems. Its homepage claims careful matching, review optionality, and a 30-day interview-related refund promise.

Sonara discloses AI LLM use to search and apply on the user's behalf, profile/resume sharing with potential employers and career-service third parties, email aliases used for forwarded employer/platform messages and OTP/2FA extraction, account/profile deletion controls, and cancellation through account settings.

JobCopilot has a useful non-legal trust artifact: a responsible AI job applications framework. It names human-in-the-loop control, transparency, fair matching, no spam/duplicates, and employer-respect limits. Its privacy policy is less job-specific, but gives identifiable analytics retention and billing-record retention windows.

Simplify, Teal, Huntr, EarnBetter, and LoopCV set useful norms for resume/job-tracker products: disclose resume/CV/job-tracker data categories, identify AI processing or third-party AI providers when relevant, offer deletion/export or account controls, and avoid selling personal data. Teal is notably explicit about AI providers and anonymized/aggregated AI improvement. Huntr is strongest on consent before sharing a profile with employers and never selling personal data. EarnBetter is clear that the user remains responsible for deciding what to include and where to apply.

## 1. Already Implemented / Already Documented In Bluey

- Jobs is isolated from the meeting overlay/audio/native session runtime while sharing Bluey identity, billing, and account balance.
- Tenant scoping is a core invariant: customer rows include `account_id`; canonical jobs dedupe per account; packet metering is unique per account and job.
- Local and cloud runners use one typed adapter contract.
- Local browser profiles are scoped by `(account_id, application_identity_id)` so separate application emails do not share cookies.
- Cloud browser profile snapshots and browser-step results are documented as AES-256-GCM encrypted; plaintext profiles exist only while Chromium owns an active run.
- Raw job-site passwords are never stored by Bluey Jobs.
- Local launch URLs carry a short-lived application capability ticket, not a reusable Bluey token, application answers, or resume content.
- Navigation is checked against private, loopback, link-local, carrier-grade NAT, and credential-bearing targets.
- Unknown required fields, CAPTCHA, assessments, and phone/app 2FA pause for owner intervention rather than guessed answers. Matching email OTP approval is explicit and raw codes are not persisted or logged.
- Every committed run retains an exact application bundle and fingerprinted receipt with job snapshot, resume, final answers, browser evidence, timestamps, selected application identity, profile ID, screenshots/document hashes where applicable, and adapter version.
- Machine-local document and screenshot paths are stripped before receipts enter account storage.
- Application emails are verified independently from the Bluey login, selected per Career Track, and frozen into resume and receipt.
- Gmail/Outlook connections are documented as tenant-scoped, plan-limited external release gates with encrypted provider message references and expiry for OTP workflows.
- Bluey public Privacy/Terms already disclose user-approved context, attached files/screenshots/transcripts/generated responses, export/delete controls, 90-day cloud synced retention, lazy cleanup, account deletion, non-sale of data, and broad training/product-improvement use for the core Bluey product.
- Public Bluey Terms already forbid using Bluey to deceive third-party assessment/security systems, harvest credentials/session cookies, or bypass platform restrictions.

## 2. Better Trust / Privacy Ideas Worth Adopting

- Add a Jobs-specific public privacy explainer beside general Bluey policies. AIApply and ApplyBlast show that job automation needs its own disclosure language for resumes, applications, mailbox/alias data, employer submissions, and receipts.
- Publish a short responsible application automation framework, inspired by JobCopilot: human approval modes, transparent job selection, no spam/duplicate applications, employer respect/rate limits, no fabricated facts, and clear restricted-site handoffs.
- Create a "what Bluey Jobs stores" receipt explainer: exact resume version, answers, job snapshot, selected email identity, timestamps, confirmation evidence, screenshots/hashes, and what is excluded, such as local machine paths and raw OTPs.
- Add a plain "where your data goes" map: Bluey account, browser profile storage, object storage, AI/model providers, email providers, ATS/employers, payment provider, Temporal/workflow infrastructure, and support tooling.
- Adopt Huntr's consent framing for profile sharing: default private profile, explicit user action before employer/platform submission, and clear post-submission limits.
- Make deletion/export controls Jobs-aware: include resumes, generated packets, answer memory, career tracks, application identities, browser profiles/cookies, email references, applications, receipts, screenshots/documents, and workflow/intervention events.
- Add AI training/provider language specifically for Jobs. Competitors vary from "no provider training" to broad anonymized improvement. Bluey should be more specific than the current general Bluey training clause before beta.
- Show review-mode and auto-submit differences in product copy and policy copy. AIApply's Review Mode language is a useful public precedent.
- Add a clear refund/cancellation rule for paid Jobs automation where privacy/trust intersects: what happens after applications are submitted, what evidence Bluey keeps for disputes, and how cancellation affects active runs, profiles, and mailboxes.

## 3. Missing Controls Or Disclosures

- No Jobs-specific public Terms/Privacy section was found. Current public Bluey policies are meeting/work-assistant oriented and mention transcripts/audio/screenshots, but not resumes, job applications, employer submission, application identities, ATS evidence, browser cookies, or email aliases.
- No public disclosure yet for browser profile/cookie storage, profile encryption, identity isolation, cookie deletion/export limitations, or how local vs cloud Jobs browser profiles differ.
- No public disclosure yet for Gmail/Outlook scopes, mailbox metadata, refresh-token retention, provider message references, OTP handling, webhook subscriptions, or user disconnect behavior.
- No public subprocessor/provider list was found for Jobs-relevant systems such as cloud hosting, object storage, Temporal, email/OAuth providers, model providers, payment provider, ATS/job-source providers, analytics, and support tooling.
- No public "AI training and model-provider use" rule specific to resumes, applications, employer responses, job descriptions, and answer memory.
- No public deletion/export description for Jobs artifacts, especially browser profile snapshots/cookies, receipts/evidence, answer memory, application identities, and employer-submitted copies that Bluey cannot retract.
- No public disclosure that employer/ATS/platform privacy policies govern data after submission and Bluey cannot delete data from third-party systems after a user-directed application.
- No public claims matrix for "auto-submit," "review mode," "handoff-only sites," restricted sites, CAPTCHA/2FA/assessments, and no-guessed-facts behavior.
- No public human-review/support-access policy for failed applications, receipts, screenshots, resumes, or support bundles.
- No public retention windows for Jobs receipts and browser evidence. Current Bluey 90-day synced-session retention may conflict with Jobs receipts that are intentionally durable.
- No public explanation of application receipt/evidence as a user trust feature and dispute/debug artifact.
- No public cancellation/refund language for Jobs-specific automation, subscriptions/credits, or post-submission non-reversibility.

## 4. Public Claims That Cannot Be Verified Without Account / Payment

- AIApply: whether AutoApply actually exposes Review Mode, mailbox retention/deletion controls, provider choices, and refund handling exactly as described publicly.
- ApplyBlast: whether review-before-send is available to every user, whether its 30-day interview refund promise is enforceable in checkout/account flows, and how system-generated email access works.
- LazyApply: whether Google API Limited Use, deletion, export, and refund limits are implemented as stated; the refund page has public eligibility rules, but operational handling requires an account/purchase.
- Sonara: whether alias-email OTP/2FA extraction is opt-in per application, whether account deletion removes alias communications, and which third parties receive profile data.
- JobCopilot: whether the responsible AI principles are enforced technically in job selection/submission flows.
- Simplify, Teal, Huntr, Jobscan, EarnBetter, LoopCV: actual in-account export/delete behavior, AI provider settings, extension permissions, and application evidence cannot be confirmed from public pages alone.
- Bluey Jobs: whether the documented encrypted browser profile snapshots, receipts, provider OAuth, export/delete scope, and intervention controls are exposed in beta UI and customer-facing settings. The architecture/operations docs say the controls exist or are external release gates, but live customer behavior requires beta access and deployment credentials.

## 5. Ideas Bluey Should Deliberately Avoid

- Avoid "spray and pray" or maximum-volume language. It creates employer-trust, platform-abuse, and candidate-reputation risk.
- Avoid broad consent copy that implies Bluey may share sensitive career data with any career-service third party for vague purposes.
- Avoid telling users that AI-tailored resumes are undetectable or that employers will not know AI was used.
- Avoid pretending submitted applications can be deleted from employer/ATS systems after the fact.
- Avoid using raw mailbox content, OTPs, screenshots, resumes, or employer responses for unrestricted training.
- Avoid hidden account credentials or raw job-site password storage.
- Avoid bypass language for CAPTCHA, 2FA, assessments, LinkedIn/Indeed, or platform anti-automation rules.
- Avoid vague "we use reasonable security" as the only trust claim. Bluey already has stronger architecture; product copy should say the concrete user-relevant parts.
- Avoid non-refundable language without a clear pre-submission/post-submission distinction and support path.

## 6. Prioritized Pre-Beta Trust Checklist

P0 - Counsel/product must approve a Jobs-specific public policy addendum covering resumes, generated packets, answer memory, application identities, browser profile/cookies, receipts/evidence, employer submissions, email/OAuth integrations, AI/model provider processing, and post-submission third-party limits.

P0 - Product must expose Review Mode vs Auto-submit as first-class settings. Auto-submit copy should say it only uses confirmed facts, hard filters, account thresholds, supported automatable forms, and no restricted-site bypass.

P0 - Add Jobs export/delete scope to account controls and docs. Include explicit handling for resumes, packets, applications, receipts, answer memory, browser profiles/cookies, email references, and retained operational/audit records.

P0 - Publish a restricted-site and intervention policy: LinkedIn/Indeed handoff-only in background, CAPTCHA/2FA/assessments owner-completed, unknown required facts paused, no guessing.

P0 - Define and disclose retention for Jobs receipts and browser evidence separately from general 90-day Bluey session retention.

P1 - Add provider/subprocessor disclosure for production beta: hosting/object storage, workflow infrastructure, AI providers, email/OAuth providers, payment processor, analytics/support/security tooling, and licensed job-source providers if applicable.

P1 - Add an in-product "application receipt" view with user-visible evidence, hashes, selected identity, submitted timestamp, confirmation URL/text where available, and exact resume/answers used.

P1 - Add email/OAuth connection copy: scopes, refresh-token storage, disconnect behavior, OTP handling, message references, mailbox/alias retention, and plan limits.

P1 - Add support/human-review policy: when staff may inspect receipts or logs, redaction defaults, minimum necessary access, and whether support can see resume/application content.

P1 - Add cancellation/refund copy for Jobs: pre-submit cancellation, post-submit non-reversibility, active-run cancellation, and receipt retention for disputes.

P2 - Publish a responsible automation page and link it from Jobs onboarding, settings, and Terms.

P2 - Add privacy-safe analytics taxonomy for Jobs that forbids raw resumes, answers, employer messages, OTPs, screenshots, and job-site cookies in analytics events.

## 7. Suggested Non-Legal Product Copy Themes For Counsel Review

- "Your applications stay reviewable: every submitted application keeps the exact resume, answers, job snapshot, email identity, timestamp, and confirmation evidence Bluey used."
- "Bluey does not invent facts. If a required answer is missing, the run pauses and asks you."
- "Some sites require you to take over. CAPTCHA, phone/app verification, assessments, and restricted job boards stay user-controlled."
- "Separate application emails use separate browser profiles, so one identity's cookies do not bleed into another."
- "Local launches use a short-lived run ticket, not your Bluey login token or resume content in the URL."
- "Employer and ATS systems receive the application information you choose to submit. After submission, their privacy policies control their copy."
- "Review Mode lets you approve before Bluey submits. Auto-submit only runs inside your confirmed filters and supported forms."
- "Delete/export for Jobs should include resumes, generated packets, application receipts, answer memory, browser profiles where available, and email integration records."
- "For counsel review: model providers should process resume/application content only to provide the requested feature unless a user has separately opted into product-improvement use."

## Bottom Line

Bluey Jobs is architecturally stronger than most public competitor disclosures on isolation, encrypted browser profiles, no raw job-site passwords, no guessed facts, intervention handoffs, and exact receipts. The pre-beta gap is public trust packaging: Bluey needs Jobs-specific disclosures and controls that translate those internal guarantees into user-readable promises before asking users to connect resumes, browsers, email, and employer applications.

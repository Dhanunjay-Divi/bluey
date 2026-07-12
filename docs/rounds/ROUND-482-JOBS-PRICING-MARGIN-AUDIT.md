# Round 482 - Jobs Pricing And Margin Audit

Date: 2026-07-11
Workstream: Pricing And Retention
Branch reviewed: `codex/bluey-jobs-20260710`

## Scope And Guardrails

This audit used only public competitor pages and local read-only repository
inspection. I did not create accounts, buy anything, submit forms, or modify
product code. Pricing, refund, trial, and competitor-limit findings are
date-sensitive and should be rechecked before public launch.

Screenshots were not embedded because the relevant pricing and policy evidence
was publicly extractable as text and this output was requested as one round
document. The most useful screenshot targets, if a launch/legal packet needs
binary evidence later, are LazyApply pricing, Wobo pricing, Autojob pricing,
ApplyPass pricing, and AutoApply.jobs pricing.

## Public Sources Reviewed

- AIApply home and FAQ sections: https://aiapply.co/
- AIApply terms: https://aiapply.co/terms-of-service
- LazyApply pricing: https://lazyapply.com/
- LazyApply refund policy: https://lazyapply.com/refundpolicy
- LoopCV pricing: https://www.loopcv.pro/pricing/
- JobCopilot pricing: https://jobcopilot.com/pricing/
- JobCopilot terms: https://jobcopilot.com/terms/
- Autojob pricing: https://autojob.app/en/
- ApplyPass home/pricing/FAQ/terms: https://www.applypass.com/, https://www.applypass.com/pricing, https://www.applypass.com/faq, https://www.applypass.com/legal/terms-of-service
- JobHire.AI FAQ/refund/blog page: https://jobhire.ai/faq, https://jobhire.ai/refundpolicy, https://jobhire.ai/blog/jobhire-ai-reviews
- Wobo pricing/terms: https://www.wobo.ai/pricing/, https://www.wobo.ai/terms-of-service/
- AutoApply.jobs pricing: https://autoapply.jobs/pricing
- Sonara home/terms: https://www.sonara.ai/, https://www.sonara.ai/terms-conditions
- Square fees: https://squareup.com/us/en/payments/our-fees
- Temporal pricing: https://temporal.io/pricing and https://docs.temporal.io/cloud/pricing
- Browserbase pricing: https://www.browserbase.com/pricing and https://docs.browserbase.com/account/billing/plans

## Competitor Pricing And Policy Snapshot

| Product | Public pricing and allowances checked on 2026-07-11 | Trial or free entry | Overage or add-ons | Browser/cloud and email limits | Cancellation and refund language |
| --- | --- | --- | --- | --- | --- |
| AIApply | Main page says Premium/Pro is available monthly or annually, but exact public dollar price was not shown in the accessible page text. Auto-Apply credits are separate and sold in packs such as 100 or 250. | Limited free account for core-tool exploration, not a timed full trial. | Auto-Apply credits are the main add-on. Exact current credit pack prices were not publicly verifiable without account/checkout. | Public page does not verify multiple application emails, inbox connections, per-browser limits, or cloud runner limits. | Homepage says cancel anytime from account settings and refund requests are case-by-case. Terms say Auto-Apply is non-refundable after applications are submitted, with pro-rata refunds only for material service reductions. |
| LazyApply | Basic $99/year for 15 applications/day and 1 resume profile. Premium $149/year for 150 applications/day and 5 resume profiles. Ultimate $999/year for 1,500 applications/day and 20 resume profiles. | No public free tier found. | No public overage model found. | Pricing page asks for the Gmail address to which the plan is added. It appears browser/account-linked, but multiple-email or inbox support was not verified. | Advertises a 30-day money-back guarantee. Refund policy limits this to initial subscriptions, requests within 30 days, fewer than 100 jobs applied, and product failure as advertised. Renewals are excluded. |
| LoopCV | Public pricing page confirms a free forever plan and paid plans starting at EUR 9.99/month. Exact tier allowances were not extractable from the public text in this pass. | Free forever, no card required. | Paid plans unlock higher daily application volumes, priority processing, and advanced filters. Exact overages not public. | Public FAQ says the browser extension is used when job boards require login and lets LoopCV apply without asking for usernames/passwords. Multiple application emails/inboxes were not verified. | Public pricing FAQ says users can upgrade, downgrade, or cancel at any time. Refund terms were not verified from a primary public page in this pass. |
| JobCopilot | Premium starts from $0.93/day with 1 copilot and up to 20 job matches daily. Elite starts from $1.05/day with 3 copilots, up to 50 matches daily, resume tailoring, and hiring-manager contact credits. Weekly, monthly, and quarterly plans are advertised. | No public free plan or trial found on pricing page. | Hiring-manager contact credits appear in Elite. No application overage pricing found. | Public features include a Chrome Extension and automated applications. Multiple email/inbox limits were not public. | Terms allow refunds only if JobCopilot is notified within 7 days of a technical issue preventing proper use. Late requests are not eligible. |
| Autojob | Free forever: 100 job applications/month, 1 active campaign. Base EUR 20/month: 1,000 applications/month, 10 active campaigns. Pro EUR 50/month: unlimited applications and active campaigns. | Free, no card required. | No public overage pricing found. | Mentions Auto Apply, One click apply, Local Apply, and campaigns. Multiple application emails/inboxes were not public. | No public refund text found in this pass. FAQ says billing cancellation is handled by emailing support. |
| ApplyPass | Basic Free: up to 7 applications/week. Momentum $99/month: up to 100 applications/week. Premium $199/month: up to 400 applications/week. Software-engineering focused. | Public home says 100 free job applications and no card required; pricing table shows Basic free. | Optional coaching/resume services are add-ons. No per-application overage found. | FAQ says dashboard shows applications and submissions; terms also say users will not be informed which jobs ApplyPass applies to, so the public pages conflict. Multiple email/inbox support not verified. | Terms say refunds only when legally required, with EU 14-day example. Cancellation may be by email or website, but requires 10 days notice before renewal. |
| JobHire.AI | FAQ says prices are shown after signup, so exact public plan prices were not verified. Three plans are named Standard, Pro, and Extended. | FAQ says no free trial. | Additional tools can be purchased. Exact add-on prices not public. | Cloud/agent model implied. Multiple application emails/inboxes not public. | Refund policy says initial subscription refund is available if at least 15 days pass with no interview invitations and the request is within 30 days. Renewal refund window is 24 hours. A company blog claims a one-click cancel button, but the actual dashboard was not publicly verifiable. |
| Wobo | Free: 5 jobs/day. Unlimited: $34.99/month, swipe-to-apply, unlimited applications. Autopilot: $44.99/month, Wobo finds and applies, unlimited applications, 5-day trial. | Free tier plus 5-day Autopilot trial. | No application overage found. | Cloud service. Terms explicitly acknowledge anti-bot and reCAPTCHA risk. Multiple emails/inboxes not public. | Terms say recurring billing continues unless canceled before renewal or before submitting the 11th application, whichever comes first. This is a high-friction retention mechanic Bluey should avoid. |
| AutoApply.jobs | Freemium: 1 query and 3 jobs by human experts once at signup. Basic $30/month: 50 jobs. Professional $90/month: 250 jobs. Human experts, AI-assisted. | Freemium package starts automatically at signup. | Extra credits via support/custom invoice. | Extension-limited EasyApply jobs on free package. Multiple emails/inboxes not public. | 7-day money-back guarantee if under 20% quota consumption. Dashboard cancellation/delete-account paths are described publicly. |
| Sonara | Home page describes AI job search and applying, but public dollar pricing and allowances were not verified. | Trial terms exist, but current trial length/price was not verified publicly. | Not verified. | Cloud service implied. Multiple emails/inboxes not public. | Terms say refunds may be issued if cancellation notice is provided before the trial ends, if a trial exists; refunds are not guaranteed after trial expiration. |

## 1. Already Implemented In Bluey Jobs

- Free, Pro, and Cloud entitlement policies are implemented in
  `server/src/db/jobs.rs`: Free is $0 with 1 track, 5 applications, 2
  application emails, 1 inbox, no local/cloud browser; Pro is $29/month with 3
  tracks, 50 applications, 10 application emails, 2 inboxes, local browser;
  Cloud is $49/month with 5 tracks, 100 applications, 25 application emails, 5
  inboxes, local plus cloud browser.
- Current plan fields expose period start/end, used applications, local/cloud
  browser flags, overage price, monthly price, application-email limit,
  connected-inbox limit, and additional-inbox price.
- The code sets packet overage at $0.50 and additional independent inboxes at
  $4/month.
- Monthly entitlement rows auto-create on Free and reset usage when the
  entitlement period ends.
- The API enforces track limits, application-email limits, connected-inbox
  limits, and local/cloud runner access based on entitlement.
- Metering is atomic and idempotent per account plus canonical job. Retries,
  regeneration, and handoffs do not double-charge. Overage deducts from shared
  Bluey balance in the same transaction and writes a balance ledger entry.
- The portal already hides internal "packet" language from customers and uses
  "applications" in plan UI, remaining-count UI, and signed-out pricing copy.
- Multi-email support is stronger than most public competitors: Bluey separates
  login email, verified application email, and connected Gmail/Outlook inbox;
  aliases inside one mailbox do not consume another inbox connection.
- Local and cloud runners are plan-gated. Browser sessions are isolated by
  account and application identity, with one active run per identity.
- Application receipts, exact tailored resume versions, submission evidence,
  status-email evidence, Answer Memory, and intervention resumption are already
  implemented in the Jobs model.
- Paid plan self-service is intentionally not live. Current plan changes go
  through the admin entitlement route, while the UI uses "Request Pro/Cloud"
  mailto links.

## 2. Better UX And Product Ideas Worth Adopting

- Keep Bluey's Free plan as a real start, but make the "what counts" rule more
  visible: reviewed/tailored applications count once per job; retries, edits,
  and handoffs do not. The current footnote says this, but the upgrade moments
  should repeat it.
- Add a small usage detail drawer from "applications left this month": reset
  date, counted jobs, overage state, and balance warning. This would beat
  competitors that show only raw volume.
- Use only four upgrade triggers: more applications, local browser, cloud
  background runner, and more inboxes/tracks. Do not introduce a separate
  "credits" mental model unless the shared Bluey balance is already in use.
- Make cancellation and refund language public before paid launch. The best
  retention move is trust: cancel from account billing, access through the paid
  period, confirmation email, no hidden retention sequence, and a clear refund
  rule for technical failure before any submission was completed.
- Keep no annual Jobs plans for the first 60 production days. Competitors push
  annual discounts and large commitments, but Bluey still needs P50/P95 cloud
  run, intervention, support, and refund data.
- Use retention features that make the job search feel alive without changing
  pricing: status sync, follow-up reminders, interview events, saved answers,
  company-specific Answer Memory, exact receipts, and "resume used for this
  job" recovery.
- Add upgrade copy at the moment of need, not as a marketing wall: "Cloud keeps
  this application moving while your computer is off" is clearer than another
  plan comparison table.
- Show that separate inbox slots are for separate Gmail/Outlook mailboxes, not
  aliases. This is a durable product differentiator because competitors rarely
  disclose multi-email handling publicly.
- For Cloud, show "applications" as the allowance, not browser minutes. Browser
  minutes are an internal abuse and margin control; exposing them would make
  pricing confusing.

## 3. Missing Technical Capabilities

- Self-serve Jobs subscription checkout, recurring Square catalog mapping,
  plan-change webhooks, cancellation webhooks, renewal failure handling,
  downgrade-at-period-end, and refund/dispute state are still missing. Round
  471 already identifies recurring product and webhook mapping as a release
  gate.
- Additional inboxes have a $4/month constant and UI copy, but no public
  self-serve recurring add-on invoice flow is implemented.
- The Free plan currently resets monthly in entitlement code. If Free remains
  monthly, Bluey needs stronger abuse controls than the docs currently spell
  out: duplicate-account checks, device/account velocity limits, verified
  application email before use, no cloud/local automation, and no overage unless
  balance or a payment method exists.
- There is no public Jobs refund/cancellation policy page tied to the actual
  plan lifecycle. General Bluey billing may exist, but Jobs needs its own
  application-count and submission-specific language.
- Cloud plan entitlements do not yet expose plan-specific cloud concurrency,
  maximum active browser time, daily run limits, semantic fallback dollar caps,
  or idle TTLs to the account model. Operations describes runner TTLs and
  leases, but plan-level cost caps are not first-class entitlements.
- Overage warnings should be preflighted before queueing a run when the monthly
  allowance is exhausted and shared balance is low. The database fails closed
  on insufficient balance, but the UX should prevent surprise failures.
- Production launch still depends on external gates: Square recurring products,
  Gmail/Outlook OAuth, licensed discovery providers, R2/S3 receipt upload,
  Temporal/browser credentials, browser takeover streaming, signed installers,
  and live ATS certification.
- Account exports for cancellation/retention are not called out: customers
  should be able to keep receipts, submitted resumes, and application history
  after canceling.

## 4. Claims That Cannot Be Verified Publicly

- Competitor interview-rate, hire-rate, "80% more likely", and "we don't stop
  until you're hired" claims cannot be verified from public pages.
- AIApply exact Premium/Pro dollar price and current Auto-Apply credit pack
  prices were not publicly verifiable without account or checkout access.
- JobHire.AI exact prices were not public; the FAQ says pricing appears after
  signup.
- Sonara current plan prices, allowances, trial length, and overage model were
  not public in accessible pages.
- Competitor multiple-application-email support and independent mailbox/alias
  behavior were mostly not public. LazyApply publicly asks for a Gmail account,
  but that does not verify multi-email semantics.
- Competitor browser/cloud limits, session caps, CAPTCHA handling, proxy
  policies, and background browser isolation were generally not public.
- Competitor cancellation dashboards cannot be verified without accounts. Any
  "one-click cancel" claims are unverified unless the public terms also state
  them.
- Competitor exact supported ATS coverage and whether applications are truly
  submitted, merely prepared, or human-reviewed are not fully verifiable from
  public pages.

## 5. Ideas We Should Deliberately Avoid

- Do not compete on "unlimited" or 1,500 applications/day. That attracts abuse,
  weakens employer trust, and makes cloud/browser cost unpredictable.
- Do not run LinkedIn or Indeed in background automation. Bluey's current
  handoff-only policy is the safer product and platform posture.
- Do not create non-refundable application credits that become unclear once
  an automated run starts. Keep included monthly applications plus shared
  balance overage.
- Do not make cancellation depend on emailing support, a 10-day notice window,
  or an unusual trigger like "before the 11th application."
- Do not hide submitted jobs or prevent customers from seeing exact materials.
  Receipts and evidence should remain a Bluey advantage.
- Do not sell one Bluey Jobs account for multiple candidates, households, or
  agencies. The one-seeker-per-account rule protects identity, receipts, email,
  and answer memory.
- Do not make pricing depend on browser minutes, model tokens, ATS family, or
  support events. Use those as internal controls only.
- Do not offer annual discounts before real production cost distributions and
  refund/dispute rates are measured.
- Do not promise interviews or jobs. A technical refund for service failure is
  cleaner than an outcome guarantee tied to recruiters.

## Conservative Bluey Plan Model

These are estimates, not customer-facing promises. They intentionally use a
more conservative cost posture than `jobs/UNIT_ECONOMICS.md` to stress-test
the $49 Cloud plan.

Assumptions checked or set on 2026-07-11:

- Square Online API payment fee: 2.9% + $0.30 per recurring invoice, per
  Square public pricing.
- Paid-account shared infrastructure allocation: $1.50/month for Pro and
  $2.00/month for Cloud.
- Free active-account allocation: $0.25/month, assuming review-only use and
  aggressive dormant-account throttling.
- Connected inbox operating cost: $0.30/month per independent inbox.
- Local/review completed application cost: $0.10, covering AI tailoring,
  document generation, workflow/search/storage overhead, and supportable
  evidence creation.
- Cloud completed application cost: $0.16, covering the local application cost
  plus runner/container/browser time, screenshots, retries, and semantic
  fallback allowance.
- Support, refund, fraud, and dispute reserve: 10% of plan revenue.
- Free does not include local or cloud browser automation.

| Plan scenario | Revenue | Estimated direct cost | Contribution | Gross margin |
| --- | ---: | ---: | ---: | ---: |
| Free, 5/5 reviewed applications used | $0.00 | $1.05 | -$1.05 | negative |
| Pro, 50/50 applications used | $29.00 | $11.14 | $17.86 | 61.6% |
| Pro, 30/50 applications used | $29.00 | $9.14 | $19.86 | 68.5% |
| Cloud, 100/100 applications used | $49.00 | $27.12 | $21.88 | 44.7% |
| Cloud, 60/100 applications used | $49.00 | $20.72 | $28.28 | 57.7% |

Formula notes:

- Free full use: `5 * $0.10 + 1 * $0.30 + $0.25 = $1.05`.
- Pro full use: `50 * $0.10 + 2 * $0.30 + $1.50 infra + $1.14 Square + $2.90 reserve = $11.14`.
- Pro 60% use: `30 * $0.10 + 2 * $0.30 + $1.50 infra + $1.14 Square + $2.90 reserve = $9.14`.
- Cloud full use: `100 * $0.16 + 5 * $0.30 + $2.00 infra + $1.72 Square + $4.90 reserve = $27.12`.
- Cloud 60% use: `60 * $0.16 + 5 * $0.30 + $2.00 infra + $1.72 Square + $4.90 reserve = $20.72`.

Interpretation:

- Pro at $29/month is healthy even under conservative assumptions if support
  does not spike.
- Cloud at $49/month is viable only with measured usage and hard cost caps. At
  full allowance utilization, this conservative model falls below a 60% gross
  margin target. Keep the $49 price for invited beta, but if Cloud P80 usage is
  above 70 included applications or P95 cost exceeds $0.16/application for two
  billing periods, either move Cloud to $59/month or lower the included
  allowance to 75 before offering annual discounts.
- Free is intentionally loss-making. It should be treated as acquisition and
  trust-building, not as an unlimited monthly utility.

## Recommended Bluey Pricing Posture

- Keep current public packaging for beta: Free $0, Pro $29/month, Cloud
  $49/month.
- Keep overage at $0.50 from shared Bluey balance, but show a preflight warning
  before queueing once allowance is exhausted.
- Keep extra independent inboxes at $4/month, charged on the recurring invoice,
  not as separate small card transactions.
- Do not introduce application-credit packs. Monthly included applications plus
  balance overage is easier to understand and already matches the code.
- Treat Cloud as invited beta until actual cost curves stabilize. The customer
  can see "100 applications"; internal controls should enforce browser TTL,
  semantic fallback budget, idle timeout, max retries, and per-identity
  serialization.

## Abuse Controls And Upgrade Triggers

Abuse controls to enforce before public paid launch:

- verified Bluey login and verified application email before any counted Free
  application;
- one active Free account per seeker/device/payment fingerprint where legally
  and technically appropriate;
- no Free local/cloud browser automation;
- low daily Free preparation cap in addition to the monthly allowance;
- host-pinned discovery, SSRF/private-network rejection, page/payload caps, and
  retry bounds;
- cloud session idle TTL, max runtime, max retries, and semantic fallback cost
  ceiling;
- connected inbox sync must remain push/event based, not polling;
- overage requires positive shared balance or saved billing path;
- LinkedIn/Indeed remain handoff-only.

Upgrade triggers that should stay simple:

- "You have 3 applications left this month" and "You're out of included
  applications."
- "Run locally with Bluey Browser" when a Free user tries a browser run.
- "Keep running while your computer is off" when a Pro user queues Cloud.
- "Add another Career Track" at the track limit.
- "Connect another Gmail/Outlook mailbox" at the inbox limit.
- "Use a separate application email" at the application-email limit.

## Retention Features That Should Not Complicate Pricing

- Exact receipts with resume, answers, screenshots, confirmation, and evidence.
- Inbox status sync, follow-up reminders, and interview calendar evidence.
- Company, Career Track, and account Answer Memory.
- Preserved interventions for CAPTCHA, 2FA, assessments, and unknown questions.
- Local/cloud continuity through the same adapter contract.
- Freshness and availability checks that explain why stale jobs disappear.
- A cancellation export that preserves submitted applications, receipts, and
  tailored resumes.

## Bottom Line

Bluey should not chase the market's highest application counts. The defendable
position is a cleaner, trustier workflow: fewer but better applications,
visible materials, separate application identities, preserved browser handoffs,
exact receipts, and public billing terms that do not punish cancellation.

The existing Free/Pro/Cloud structure is coherent. The largest business risk is
Cloud gross margin under high utilization, not Pro. Keep Cloud invite-gated
until real browser costs are measured, then either raise Cloud to $59 or reduce
included Cloud applications before adding discounts.

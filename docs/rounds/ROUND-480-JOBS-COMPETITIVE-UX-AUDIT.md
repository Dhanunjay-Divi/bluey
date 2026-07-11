# Round 480 - Jobs Competitive UX Audit

Date: 2026-07-11

Scope: public pages only. No accounts were created, no purchases were made, and no forms were submitted. No screenshots were captured.

## Bluey Baseline

Bluey Jobs is already more technically rigorous than most public competitor positioning suggests:

- Isolated `/jobs` surface with shared Bluey identity, balance, and billing, but no coupling to meeting/audio/native runtime.
- Six-step onboarding for a Career Profile, resume import, work history, education/skills, goals, and submission defaults.
- Career Tracks for separate searches, locations, roles, application emails, daily limits, job freshness, excluded companies, and one-application-per-company controls.
- Matches screen with ranked jobs, fit score, freshness, workplace, manual job-link add, filters, review-first/auto-submit choice, and application preparation.
- Per-job resume versions with visible diffs, factual/enhance mode, claim provenance, PDF/DOCX export, and receipts tied to exact materials.
- Applications timeline with intervention inbox, packet review, application states, remembered answers, browser takeover, and submission receipt evidence.
- Local Bluey Browser and cloud runner options, isolated browser profiles per application email, preserved CAPTCHA/2FA/assessment handoffs, one-click email OTP approval, and exact receipts.
- Answer Memory with account, Career Track, and company scope precedence.
- Plan UI with Free/Pro/Cloud tiers, included application counts, overage metering, application email limits, inbox slots, and retry/handoff no-double-charge language.
- Implemented execution pipeline for Workday, Greenhouse, Lever, Ashby, and SmartRecruiters, with LinkedIn/Indeed treated as handoff-only in background policy.

## Public Competitor Notes

| Product | Public URL | Public claim pattern | Bluey comparison |
| --- | --- | --- | --- |
| AIApply | https://aiapply.co/ | End-to-end toolkit: job board, tailored resumes/cover letters, auto-apply, interview prep, resume scanner, translator, and credits for auto-apply. Public copy claims high-match sourcing and dashboard control, but detailed credit pricing is behind the app. | Bluey matches the core tailoring/auto-apply/receipt path and is stronger on provenance and intervention safety. Bluey lacks interview prep, translation, and marketing-level proof/social reassurance. |
| ApplyBlast | https://applyblast.com/ | Public page is JS-heavy; search-visible copy says AI matches jobs, tailors resume/cover letter, auto-applies, and can apply against preferences like salary floor, location, seniority, and dealbreakers. Pricing was not visible without further app access. | Bluey has comparable preference controls and stronger documented execution/receipt architecture. Need clearer public-facing explanation of dealbreakers and what will never be submitted. |
| Perplexity Comet job applications | https://builtin.com/artificial-intelligence/perplexity-comet-job-search and https://www.perplexity.ai/encyclopedia/jobseekers | Public article describes AI-browser assistance for LinkedIn networking, tab organization, company research, form fill, resume tailoring, cover letters, interview prep, and Easy Apply automation with review required for more complex submissions. | Bluey is purpose-built and safer for job applications, but lacks adjacent research/networking/company intelligence that makes browser-agent products feel broad and useful. |
| JobCopilot | https://jobcopilot.com/pricing/ | Premium starts from `$0.93/day`, one Copilot, up to 20 job matches daily, automation, save-for-review, tracker, resume builder, cover letter builder, mock interviews. Elite starts from `$1.05/day`, three Copilots, up to 50 daily matches, resume tailoring, and hiring-manager contact credits. | Bluey's Career Tracks resemble Copilots but are less narratively clear. Bluey has stronger control/receipt mechanics, but less obvious daily value and no interview/networking modules. |
| LoopCV | https://www.loopcv.pro/pricing/ | Free forever auto-apply, scans 30+ job boards, matches CV, dashboard, no credit card, paid from EUR 9.99/month for more volume/filters; browser extension for logged-in job boards without asking for passwords. | Bluey should not race LoopCV on free volume. It should win on fit, truthfulness, evidence, and owner control. |
| Simplify | https://simplify.jobs/copilot | Free extension, autofill on 100+ job boards/ATS including Workday, Greenhouse, iCIMS, Taleo, Avature, Lever, SmartRecruiters; no limit on Copilot autofill, tracker/job matches/basic resume builder free. | Simplify's extension coverage is broader. Bluey is deeper on background execution, receipts, and interventions. Missing iCIMS/Taleo/Avature coverage affects perceived completeness. |
| Teal | https://www.tealhq.com/pricing | Free forever, unlimited resumes, unlimited job tracking, keyword matching, templates, email templates by stage; paid plans `$13/week`, `$29/30 days`, `$79/90 days`; clear no-credit-card start and cancellation messaging. | Bluey has automation depth Teal lacks, but Teal is stronger as a low-pressure job-search CRM and resume workspace. Bluey needs a calmer first-session "organize first" mode. |
| Huntr | https://huntr.co/pricing | Free resume builder, PDF export, two tailored resumes, two application packets, tracker for 100 jobs, job clipper, map view, unlimited contact management and autofills; Pro `$40/month` with unlimited tailoring, AI reviews, cover letters, metrics. | Huntr's free organization value is high. Bluey has richer automation but should borrow job-search metrics, contact tracking, and low-stakes packet limits language. |
| Jobscan Auto Apply | https://www.jobscan.co/auto-apply | Strong quality/control positioning: finds matching jobs, drafts tailored answers, review/approve before every submission, sourced from Lever, Workable, and 20+ ATS platforms; explicitly rejects bulk-apply spam. | This is the closest UX positioning match. Bluey already supports review-first and receipts; copy should shift from technical safety to "quality and final call." |
| EarnBetter | https://earnbetter.com/ | 100% free, over 5 million jobs, unlimited professional and tailored docs, personalized matches, interview prep, tracker, external job support. | Bluey cannot beat free on perceived value. It should emphasize why paid automation is safer: exact receipts, verified emails, no unsupported claims, and preserved handoffs. |
| Sonara | https://www.sonara.ai/ | "We get to know you, find jobs for you, apply for you"; continuously scans millions of openings, daily best matches, 10x applications, reclaim hours. | Sonara's promise is emotionally simple. Bluey should simplify the first-run story without adopting unverified "10x" language. |
| WonsultingAI | https://www.wonsulting.com/wonsultingai | Guided journey and diagnosis: shows where progress is blocked, job search plan, resume score, ResumeAI, CoverLetterAI, NetworkAI, JobTrackerAI, InterviewAI, JobBoardAI, interview guarantee claims. | Bluey has the raw data to diagnose funnel blockers but does not yet present "what is stuck and what to do today." |

## Already Implemented In Bluey

- Review-first and auto-submit modes with thresholds and hard filters.
- Per-job resume tailoring, visible diffs, and provenance-protected claims.
- Application-specific receipts with exact resume, answer set, evidence, identity, and timestamps.
- Intervention Inbox that pauses instead of guessing on unknown required facts, CAPTCHA, 2FA, assessments, and sensitive questions.
- Local and cloud runner choice, with isolated browser profiles and takeover.
- Application email identities, verified addresses, inbox connections, aliases, and email OTP approval path.
- Career Tracks, separate match streams, daily limits, location policy, salary floor, job age, excluded companies, and one active company application control.
- Answer Memory reuse scoped to account, Career Track, or company.
- Transparent metering: included applications, overage, no double-charge on retries/handoffs.

## Better UX/Product Ideas Worth Adopting

1. "Today's work" home screen. Borrow WonsultingAI/Teal-style guidance: show three actions only, such as review 3 strong matches, answer 1 paused question, connect 1 inbox.
2. Control-first framing. Jobscan's "review and approve before every submission" is clearer than Bluey's current technically correct but dense "Approve application -> Choose runner" flow.
3. Confidence ladder. Let users start with "organize and tailor only," then "autofill with me," then "review-first queue," then "auto-submit high confidence." This reduces fear for inexperienced users.
4. Public compatibility matrix. Simplify wins trust by naming ATS coverage. Bluey should list supported automatic, handoff, and planned sites with plain statuses.
5. Job-search health metrics. Huntr/WonsultingAI-style metrics could show response rate, application age, stuck stages, interview conversion, stale follow-ups, and weak match patterns.
6. Company research packet. Perplexity/Comet highlights company research and networking. Bluey should add "Know before applying" with company summary, recent news, role risks, and possible warm contacts where public data allows.
7. First-run sample application. Show a safe preview application before asking users to trust automation: job, tailored resume diff, answers, runner options, receipt example.
8. Softer onboarding shortcuts. EarnBetter and Jobscan emphasize setup in minutes. Bluey's six steps are solid but could offer "import now, finish details when needed."
9. Human-readable dealbreakers. ApplyBlast-style salary/location/seniority/dealbreakers should be front-and-center in Matches and Settings.
10. Interview-prep handoff. AIApply/JobCopilot/WonsultingAI sell post-application support. Bluey can add lightweight prep packs from job/resume/application receipt without building live interview coaching.

## Missing Technical Capabilities Affecting UX

- Broader ATS adapters: iCIMS, Taleo, Avature, Workable, Jobvite, BambooHR, UKG, Oracle Recruiting, and custom employer forms beyond the current constrained semantic boundary.
- LinkedIn/Indeed automation remains handoff-only; this is safety-aligned but will look incomplete against extension competitors.
- No public browser-extension overlay on arbitrary job pages; user must operate inside Bluey Jobs/Bluey Browser.
- No live company research, recruiter/contact discovery, or networking CRM.
- No interview preparation, offer tracking, salary negotiation, or post-application coaching.
- No application outcome analytics beyond receipts/status evidence; funnel diagnosis is not yet productized.
- No visual ATS compatibility matrix or per-site automation confidence score.
- No resume keyword/match diagnostics as explicit as Teal/Jobscan/Huntr, even though match reasons and diffs exist.
- Production release gates remain external: provider credentials, Gmail/Outlook OAuth apps, browser takeover gateway, object storage, signed installers, and ATS certification.

## Public Claims Not Verifiable Without Account/Payment

- AIApply: exact current subscription and auto-apply credit pack prices, actual dashboard controls, quality of draft review, true volume limits, and success-rate/testimonial claims.
- ApplyBlast: pricing, credit model, monthly caps, exact supported sites, actual tailoring quality, and auto-apply safeguards.
- JobCopilot: real in-app Copilot training workflow, hiring-manager contact credits, exact expanded site behavior, and application quality.
- LoopCV: free-plan daily/monthly application volume, actual job-board coverage, dashboard quality, and whether auto-applied messages remain personalized.
- Simplify: exact real-world compatibility across 100+ portals and quality of autofill for complex/sensitive questions.
- Teal/Huntr/EarnBetter: AI generation quality, parsing accuracy, tracker automation, and actual effect on interviews.
- Jobscan Auto Apply: current pricing, exact ATS coverage, and whether every field is practically editable before submission.
- Sonara: pricing, matching quality, "millions" coverage, and 10x application outcome claims.
- WonsultingAI: interview guarantee conditions, money-back terms, and whether diagnostics are driven by real application data or user-entered status.
- Perplexity Comet: reliability of form fill, LinkedIn access behavior, and safe handling of personal data in real job-search sessions.

## Ideas Bluey Should Deliberately Avoid

- Avoid "spray and pray," "hundreds per day," or "apply while you sleep" positioning. It undermines Bluey's trust advantage.
- Avoid hiding application costs behind vague credits. Bluey's included applications plus overage is easier to understand.
- Avoid claiming guaranteed interviews, 80% improvement, or 10x outcomes unless backed by Bluey-owned cohort data.
- Avoid auto-answering sensitive questions, legal eligibility, relocation, sponsorship, disability, veteran status, or assessments.
- Avoid generic cover-letter spam as a default. Keep cover letters optional, role-specific, and reviewable.
- Avoid asking users for raw job-site passwords. Continue using isolated browser profiles and OAuth/provider handoffs.
- Avoid pretending LinkedIn/Indeed background automation is safe if policy or anti-bot behavior makes it owner-controlled.
- Avoid a "black box applied for you" timeline. Every submission should retain the exact materials and evidence.

## Prioritized UX Gap Analysis

### P0 - First-Session Trust And Momentum

Screen: Onboarding

Recommendation: Add a "Quick start" path that imports a resume, asks for desired role/location/salary/dealbreakers, then lands on a sample packet. Move detailed work history cleanup into progressive prompts when a fact is needed.

Why: Competitors win by promising "setup once" and "minutes." Bluey's six-step flow is trustworthy but may feel like homework to tired users.

### P0 - Application Review Clarity

Screen: Applications detail

Recommendation: Replace generic diff copy with actual job-specific reasons and a field-level review checklist: resume, cover letter if any, required answers, identity/email, site, cost, runner.

Why: Jobscan's strongest claim is "the final call is yours." Bluey has the underlying controls but should make approval feel more concrete.

### P0 - Automation Boundary Visibility

Screen: Matches detail and Browser

Recommendation: Show a site capability badge: "Auto-submit supported," "Review-first only," "Handoff required," or "Not supported yet." Link each badge to why.

Why: Simplify names coverage; Bluey should be transparent about where it is better and where it will pause.

### P1 - Today View

Screen: AppShell or new Jobs home

Recommendation: Add a default dashboard with "Ready to review," "Waiting on you," "Running now," "Follow up soon," and "Improve setup." Keep Matches as a deeper tab.

Why: Tired users need prioritization more than a dense list of options.

### P1 - Search Quality Controls

Screen: Settings and Matches

Recommendation: Promote dealbreakers into a visible policy summary: salary floor, remote/on-site, seniority, sponsorship, excluded companies, duplicate-company rule, max posting age.

Why: ApplyBlast/Sonara sell "we apply to the right jobs." Bluey can make "right" auditable.

### P1 - Job Search Metrics

Screen: Applications or new Insights

Recommendation: Add funnel cards: applied, employer viewed/auto-replied, recruiter response, interview, rejected/closed, no response after N days. Add "what to change" suggestions.

Why: WonsultingAI/Huntr/Teal make users feel organized; Bluey's receipts can power a better version.

### P1 - Compatibility And Plans

Screen: Settings plans and Browser

Recommendation: Add plan comparison rows for included applications, local/cloud runner, supported ATS automation, review-first, auto-submit, receipts, inboxes, and email identities.

Why: Current plan copy is compact but hides why Cloud is materially safer/more useful than competitors.

### P2 - Research And Networking

Screen: Matches detail

Recommendation: Add a "Before you apply" panel with company summary, why this role fits, risk flags, public recruiter/hiring-team hints where available, and suggested outreach copy.

Why: Perplexity/Comet broadens the job-search job beyond form filling.

### P2 - Interview Prep From Receipt

Screen: Submitted receipt

Recommendation: Add "Prepare for this interview" after submission: likely questions, STAR notes from the exact resume, company-specific talking points, and follow-up email.

Why: AIApply/JobCopilot/WonsultingAI sell the whole journey; Bluey can use submitted context without overbuilding.

## Copy Suggestions

- Onboarding headline: "Start with one resume. Bluey turns it into reviewed applications."
- Quick start CTA: "Find my first matches"
- Matches empty state: "No safe matches yet. Loosen a filter or paste a job you already like."
- Match detail badge: "Auto-submit supported after review" / "Handoff required on this site"
- Application review title: "Review exactly what will be sent"
- Approval CTA: "Approve this application"
- Runner CTA: "Run in Bluey Browser" and "Run in the background"
- Intervention banner: "Paused because Bluey will not guess"
- Answer Memory copy: "Use this answer only where it is true"
- Receipt title: "Proof of what was sent"
- Plan footnote: "You pay for completed applications, not retries or handoffs."
- Auto-submit setting: "Only submit when the match, site, and answers are all safe"
- Dealbreaker summary: "Bluey will skip jobs below this line"
- Browser setup: "Your job-site sessions stay separate from your everyday browser"

## Proposed First-Session Journey

1. Welcome: "What kind of help do you want today?" Choices: organize my search, tailor applications for review, apply in the background.
2. Import: user uploads resume. Bluey extracts basics locally and asks only for missing name/contact/location if needed.
3. Intent: one compact form for target role, target locations, remote preference, salary floor, sponsorship, and excluded companies.
4. Safety defaults: preselect review-first, factual resume mode, ask on unknown facts, one active application per company, max 10/day.
5. First results: show 5 matches with fit, compensation/freshness when known, and capability badges.
6. First packet: user opens one match and sees exact resume changes, required answers, application email, cost, and "why this is safe to send."
7. Trust moment: show a mock receipt preview before approval: exact resume, answers, timestamp, evidence, and how to pause/take over.
8. Action: user approves one application and chooses local browser or cloud if entitled.
9. After action: show "You are done for now" with the next two useful actions: connect inbox, answer missing fact, or review more matches.
10. Return loop: daily Today view says what happened, what needs input, and what Bluey recommends changing.

## Product Positioning Recommendation

Bluey should not position itself as the highest-volume auto-apply product. The stronger lane is:

"Bluey prepares and submits job applications with your final rules, your real facts, and proof of what happened."

This lets Bluey compete against volume tools by owning trust: reviewed packets, exact receipts, preserved handoffs, verified emails, and truthful answer memory.

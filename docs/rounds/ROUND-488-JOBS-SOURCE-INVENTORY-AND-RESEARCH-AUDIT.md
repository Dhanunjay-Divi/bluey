# Round 488 - Jobs Source Inventory And Research Audit

Date: 2026-07-11
Branch: `codex/bluey-jobs-20260710`
Scope: audit the cloned job-automation repositories and the downloaded AI/browser research archives so Bluey Jobs can reuse the strongest ideas without inheriting brittle or unsafe product shape.

## Source roots reviewed

Two external source roots were reviewed as read-only references:

- `/Users/uno/Downloads/cue-bluey-jobs/_refs/jobs-research`
- `/Users/uno/Downloads/job and AI browser`

The `_refs/jobs-research` root contains 14 cloned job-automation repositories. The `job and AI browser` root contains 10 ZIP archives focused on search, RAG, Perplexity-style answers, and AI browser/research UX. None of these sources were committed into Bluey in this round.

## Audit method

The scan covered repository manifests, package files, license files, tests, entrypoints, workflow files, browser automation modules, job-discovery modules, resume/profile modules, email/calendar surfaces, and security-sensitive code paths. The archive scan extracted ZIPs only into temporary directories and did not execute untrusted app code.

This round is a source-to-Bluey decision record. It is not a blanket approval to ship every upstream dependency or to copy upstream code without retaining notices.

## Repositories reviewed

The detailed per-repo commit and license ledger is in `jobs/THIRD_PARTY_PROVENANCE.md`. The practical reuse result is:

| Source | Best Bluey use | Decision |
| --- | --- | --- |
| `santifer/career-ops` | ATS job discovery, provider contracts, CI/SBOM discipline, defensive provider tests | Strongest code donor. Port selectively with notices and add Bluey rate limits, dedupe, SSRF, telemetry, and tenant boundaries. |
| `neonwatty/job-apply-plugin` | Common application fields, document field patterns, confirmation-oriented workflow | Use as MIT pattern source. Do not overclaim universal stop behavior; Bluey must implement its own unknown-required-field detector. |
| `bdcorps/easy-job-application-filler-extension` | Legacy field aliases and simple answer mapping | Use narrow field-alias patterns only. |
| `AkbarDevop/ai-job-agent` | Answer memory, approval gates, application status discipline | Port ideas into Bluey account/Career Track/company answer memory and intervention flow. |
| `proficientlyjobs/proficiently-claude-skills` | One artifact folder per job, JD mapping, review checklist | Product pattern only unless license file is added. |
| `MadsLorentzen/ai-job-search` | Search/review workflow, factual resume drafting patterns | Pattern source for reviewer and quality checks. |
| `suxrobGM/jobpilot` | Local data protection, profile ownership checks, queues, SSE | Use security and queue patterns; avoid autonomous unsafe behavior and unsigned updater patterns. |
| `leopu00/job-hunter-team` | Prompt-injection fences, keyring usage, SSRF tests | Use safety patterns after tightening DNS rebinding/subresource coverage. |
| `Rayyan9477/AutoApply-AI-Agentic-Browser-Automation-for-Job-Search` | Backend architecture and apply pipeline | User-authorized but no top-level license in clone. Treat as reference/clean-room until provenance is resolved. |
| `11844/Auto_Jobs_Applier_AIHawk` | Broad profile, filters, exclusions, daily volume, apply-once rules | Product requirements only because the clone has a source-available license. |
| `feder-cr/jobs_applier_ai_agent_aihawk` | AIHawk lineage, profile/run config | Research only due AGPL. |
| `GodsScion/Auto_job_applier_linkedIn` | Configuration breadth and browser workflow ideas | Research only due AGPL and LinkedIn-specific automation risk. |
| `Pickle-Pixel/ApplyPilot` | Staged discovery/scoring/tailoring/export/validation/retry pipeline | Product research only due AGPL. |
| `jaejaywoo/HireGPT` | Legacy Electron/local-service boundary | Research only. |

## Downloaded AI/browser archives reviewed

| Archive | SHA-256 prefix | Best Bluey use | Decision |
| --- | --- | --- | --- |
| `Vane-master.zip` | `012cad3b` | Typed model/tool contracts, mode-based research UX, query classification | Reimplement useful contracts and routing, not storage/execution. |
| `abbey-main.zip` | `d92fb07e` | Source ledger, evaluation flow, manifest invalidation, context budgeting | Clean-room architecture only. |
| `search_with_lepton-main.zip` | `e7de9603` | Provider normalization, sources-first answer UX, replayable search | Reimplement provider/replay ideas. |
| `rag-search-main.zip` | `e37c90f8` | Reranker interface and chunk/retrieve structure | Reimplement small interface patterns only. |
| `clarity-ai-main.zip` | `b6a42e5e` | Citation interaction and answer/source presentation | Use UX pattern only; source lineage needs notice care. |
| `perplexideez-main.zip` | `6a7714ff` | Perplexica-style search UX and source display | AGPL lineage: clean-room patterns only. |
| `Perplexity-Clone-Python-main.zip` | `d3055bfb` | Minimal Python answer/search demo | Research only. |
| `perplexity-ai-main.zip` | `815258e2` | Private endpoint automation anti-pattern | Do not use. |
| `llm-answer-engine-main.zip` | `1584e57b` | Answer-engine layout ideas | Research only. |
| `spy-search-main.zip` | `2565a072` | Search/archive exploration | Research only. |

The archive audit found 1,117 source files and roughly 105k lines of code across the ZIPs, but only a very small amount of executable test coverage. That is a strong signal to use these as design references, not production foundations.

## Research subsystem recommendation

Bluey Jobs should add a dedicated `jobs/research` subsystem rather than mixing web search into application automation. The subsystem should:

- Discover recent jobs and company pages through provider contracts.
- Fetch source pages with strict URL/DNS/redirect/content limits.
- Store a source ledger with content hashes and retrieval timestamps.
- Create match explanations with structured citations.
- Never modify hard filters, user-confirmed facts, or final application answers.
- Feed stronger context to ranking, resume tailoring, interview prep, and company follow-up.

The minimum data shape should include `source_id`, `claim_id`, `chunk_id`, `content_hash`, `retrieved_at`, `quote`, `offsets`, `provider`, and `job_id`. Every displayed claim should be traceable to a source or to a user-confirmed Career Profile fact.

## Resume/profile implications

The strongest common pattern across AIHawk, ApplyPilot, ai-job-agent, ai-job-search, and proficiently skills is that lazy users still need a complete baseline profile before automation is useful. Bluey should collect:

- Legal name, contact, location, work authorization, sponsorship, relocation, salary, and job-location policy.
- Employers, titles, start/end dates, responsibilities, achievements, stack, and metrics.
- Education, projects, certifications, licenses, languages, and publications.
- Role targets, excluded titles/companies, daily volume, seniority, remote/hybrid/onsite policy, and commute/relocation boundaries.
- Reusable answers scoped by account, Career Track, company, and question type.
- Multiple application identities, each with email, calendar, browser profile, resume defaults, and site-login state.

Bluey should not let a Data Engineering track reuse the SDE resume path for the same company by accident. Applications should freeze `career_track_id`, `application_identity_id`, `browser_profile_id`, `canonical_company_id`, `canonical_job_id`, and `resume_version_id` before metering or submission.

## Hard rejects

Do not carry these patterns into Bluey:

- CAPTCHA solving or fake identity flows.
- LinkedIn/Indeed private API reverse engineering.
- Background submission on sites where we cannot reliably preserve user control and receipts.
- Selenium selectors without typed semantic recovery.
- Plaintext cookies, local YAML as production state, or single-user file state.
- Process-global queue state for cloud browsers.
- Guessing legal, authorization, demographic, salary, or required factual answers.
- Auto-retrying after a click where submission outcome is unknown.

## Immediate Bluey build order from this audit

1. Finish identity-scoped browser profiles.
2. Add a submission ledger that distinguishes prepared, submit started, unknown, confirmed, rejected, and receipt captured.
3. Promote Career Track/company/application identity separation into DB constraints.
4. Make Answer Memory first-class with account, track, company, and question-scope precedence.
5. Complete Greenhouse and Lever adapters first, then Workday, Ashby, SmartRecruiters.
6. Add a source-ledger research module before broadening discovery providers.
7. Add dependency-notice generation before any public beta release.

## Safety note

During this audit a generated competitive-inspection asset folder contained a raw Chromium profile. That folder was excluded from the commit. Browser state, cookies, caches, and site history must never enter docs, logs, receipts, or source control.

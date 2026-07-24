# Bluey Jobs Source Provenance

This ledger records the repositories reviewed for Bluey Jobs, the exact commit
cloned on July 10, 2026, the license visible in that clone, and the resulting
reuse decision. Raw research clones live under the ignored
`_refs/jobs-research/` directory and are not shipped with Bluey.

The project owner explicitly authorized Bluey to reuse code from the supplied
repositories and stated that they participate in every supplied project. That
authorization is recorded here in addition to, not in place of, each
repository's published license and contributor notices. Third-party dependency
licenses remain independently binding.

## Supplied repositories

| Repository | Reviewed commit | License observed | Bluey decision |
| --- | --- | --- | --- |
| [`GodsScion/Auto_job_applier_linkedIn`](https://github.com/GodsScion/Auto_job_applier_linkedIn) | `8d74e8ccb85b` | AGPL-3.0 | Research only in this round: browser workflow and configuration breadth. |
| [`proficientlyjobs/proficiently-claude-skills`](https://github.com/proficientlyjobs/proficiently-claude-skills) | `9bc1f6fd7af5` | README says MIT; no top-level license file in clone | Product pattern: per-job artifact folders, JD mapping, consolidated review. No source file copied. |
| [`Rayyan9477/AutoApply-AI-Agentic-Browser-Automation-for-Job-Search`](https://github.com/Rayyan9477/AutoApply-AI-Agentic-Browser-Automation-for-Job-Search) | `960ddc32a2f0` | No top-level license file in clone | User-authorized reference. Treat as clean-room or permission-recorded source until contributor and third-party provenance is attached. |
| [`feder-cr/jobs_applier_ai_agent_aihawk`](https://github.com/feder-cr/jobs_applier_ai_agent_aihawk) | `ab4113d453ab` | AGPL-3.0 | Research only: profile and run configuration. |
| [`neonwatty/job-apply-plugin`](https://github.com/neonwatty/job-apply-plugin) | `4330f090ba94` | MIT | Adapted field-label matching, ATS form patterns, pre-submit confirmation, and sensitive-field confirmation patterns into `automation/src/form-intelligence.ts`. Bluey implements its own universal unknown-required-field intervention behavior. |
| [`Pickle-Pixel/ApplyPilot`](https://github.com/Pickle-Pixel/ApplyPilot) | `4a8d521f67f5` | AGPL-3.0 | Product research: staged discovery, scoring, tailoring, PDF, validation, and retry pipeline. |
| [`11844/Auto_Jobs_Applier_AIHawk`](https://github.com/11844/Auto_Jobs_Applier_AIHawk) | `1cecff373348` | Repository-specific proprietary/source-available license | Adapted product requirements only: profile breadth, filters, exclusions, one-company rules, and per-job artifacts. No source file copied in this round. |
| [`santifer/career-ops`](https://github.com/santifer/career-ops) | `01bf8b469ad5` | MIT | Adapted public ATS provider, host-pinning, bounded pagination, retry, normalization, exact dedupe, source-trust reasons, and description-fingerprint cross-listing signals into `automation/src/public-ats.ts` and `automation/src/job-source-intelligence.ts`. |
| [`jaejaywoo/HireGPT`](https://github.com/jaejaywoo/HireGPT) | `835e5f52f093` | MIT | Research only: legacy Electron and local-service boundary. |

The GitHub [`job-application` topic](https://github.com/topics/job-application)
was used as a discovery index. Bluey did not treat the topic page as a license
or clone every listed repository.

## Curated public job-list readers

Bluey includes an original bounded parser for four owner-requested public job
lists. No list repository source code, images, or branding is copied into the
product. The reader fetches only the allowlisted README, does not follow
redirects, caps both bytes and rows, ignores aggregator links, and retains the
original employer application URL as the candidate lead.

| Feed | Bluey source ID | Production meaning |
| --- | --- | --- |
| [`SimplifyJobs/New-Grad-Positions`](https://github.com/SimplifyJobs/New-Grad-Positions) | `feed-simplify-new-grad` | Candidate new-grad lead only. |
| [`PrepAIJobs/Summer2026-Internships`](https://github.com/PrepAIJobs/Summer2026-Internships) | `feed-prepai-internships` | Candidate internship lead only. |
| [`PrepAIJobs/New-Grad-2026`](https://github.com/PrepAIJobs/New-Grad-2026) | `feed-prepai-new-grad` | Candidate new-grad lead only. |
| [`zapplyjobs/New-Grad-Jobs-2027`](https://github.com/zapplyjobs/New-Grad-Jobs-2027) | `feed-zapply-new-grad` | Candidate new-grad lead only. |

The public lists are discovery indexes, not application truth. A candidate URL
must be canonicalized, deduplicated, fetched from the original employer or ATS,
verified open, rescored against the Career Track, and passed through current
server-owned eligibility before it may become an application. A list's license
does not establish rights in every upstream job posting. Bluey must honor
removals and source terms and must not retain a stale list row as an open job.

The reader uses `parse5` 7.3.0 (MIT, Ivan Nikulin) and its transitive `entities`
6.0.1 dependency (BSD-2-Clause, Felix Bohm). Their notices are recorded in
`automation/THIRD_PARTY_NOTICES.md`; dependency versions remain locked by
`jobs/package-lock.json`.

## Downloaded AI/browser archives

The following ZIP archives under `/Users/uno/Downloads/job and AI browser` were
reviewed as read-only research material. They are not shipped with Bluey.

| Archive | SHA-256 | Bluey decision |
| --- | --- | --- |
| `Vane-master.zip` | `012cad3ba1a7fb7883b67e334a2d9c0cb7777da6aa3ea001d37a9140bb0f2f80` | Reimplement useful typed model/tool contracts, role modes, query classification, and research UX patterns. |
| `abbey-main.zip` | `d92fb07e1c7624c1187eda9e458dc6726c99e8088ee21652f641f4ae39dbd294` | Clean-room source-ledger, quality-evaluation, context-budgeting, and manifest-invalidation patterns. |
| `search_with_lepton-main.zip` | `e7de9603d55e2028a56aa78b07ed39d33b353f24435ee58e782c51d9e0a5c7ef` | Reimplement provider normalization, source-first UX, and replayable search ideas. |
| `rag-search-main.zip` | `e37c90f8e94c537da72d5b6e504990f84804ba92772caf0ff1ee6d4c95df737d` | Reimplement reranker and chunk/retrieve interfaces only. |
| `clarity-ai-main.zip` | `b6a42e5efb1bdf1717b885c2c8bbfb1e5727d40ab1b63e23550e545b505a25da` | Citation interaction pattern only; verify inherited notice obligations before source reuse. |
| `perplexideez-main.zip` | `6a7714ffaeebb9713c36da0d1363f87d59ca45f458d088a113cb1dfa331000fd` | AGPL/Perplexica-lineage style research only; clean-room patterns. |
| `Perplexity-Clone-Python-main.zip` | `d3055bfbe9007e7057ff5f28b4684bf78e8ae3f429c19d3a54f020114506fbe7` | Minimal demo; research only. |
| `perplexity-ai-main.zip` | `815258e25e640aa03a8a2b1d77bab2b1a59c2a8ff38b27a0bd7d8b439233e307` | Reject private-endpoint/account automation; do not use. |
| `llm-answer-engine-main.zip` | `1584e57bc8675558a39e6f56ee3dfe9819116ea37ee830df098f0673f2b535e6` | Answer-engine layout research only. |
| `spy-search-main.zip` | `2565a072035a14dba312827ef96157212f23813bd3ffdc084be077208dfd1a6e` | Search/archive research only. |

## Additional topic candidates reviewed

| Repository | Reviewed commit | License observed | Bluey decision |
| --- | --- | --- | --- |
| [`MadsLorentzen/ai-job-search`](https://github.com/MadsLorentzen/ai-job-search) | `7e8df3581927` | MIT | Workflow and search research only. |
| [`AkbarDevop/ai-job-agent`](https://github.com/AkbarDevop/ai-job-agent) | `9ce47d29b5fc` | MIT | Adapted answer-memory precedence, review gates, and receipt/status ideas into typed Bluey form and receipt modules. |
| [`bdcorps/easy-job-application-filler-extension`](https://github.com/bdcorps/easy-job-application-filler-extension) | `16ea57dc7a95` | MIT | Adapted legacy field aliases and reusable-answer matching into `automation/src/form-intelligence.ts`. |
| [`leopu00/job-hunter-team`](https://github.com/leopu00/job-hunter-team) | `f2575c976f8a` | MIT | Multi-agent architecture research only. |
| [`suxrobGM/jobpilot`](https://github.com/suxrobGM/jobpilot) | `272b7aca424c` | MIT | Product and queue architecture research only. |

## External company-directory feed

Bluey may use the public company directory and manifest metadata published by
[`kalil0321/ats-scrapers`](https://github.com/kalil0321/ats-scrapers), now
published as Jobhive, to suggest employer career pages a user can explicitly
connect to a Career Track and to plan bounded shared candidate-feed ingestion.
The repository was reviewed at commit
`d825caefc8e97c3533efe1707b4daddfeed58706` and exposes an MIT code license
(copyright Kalil Bouzigues, 2026).

No Jobhive scraper, proxy, anti-bot, or browser-evasion component is shipped by
Bluey. Bluey reads the bounded manifest, validates pinned artifact URLs,
declared checksums, row counts, byte sizes, and schema, and builds bounded work
for one shared ingestion service. It must not download the multi-gigabyte
snapshot once per account. Every selected row is still revalidated against and
attributed to the original employer or ATS before ranking, preparation, or use.
The external dataset is therefore a candidate lead, never application truth or
permission to automate a submission.

Manifest reviewed on July 20, 2026:
`https://storage.stapply.ai/jobhive/v1/manifest.json`.

| ATS directory | Rows | SHA-256 declared by manifest |
| --- | ---: | --- |
| Greenhouse | 4,966 | `94307570bfe88a1b06bd888619652703441c5a4b7a710af39a3f95f617f428aa` |
| Lever | 2,113 | `e6ef68e92d65192027cdadd15fc0462bffee4548f4e2d2a5984a6ce955315ba5` |
| Ashby | 2,856 | `50d570e781768937b1bc7a6d8770cd2b8c4bc2f9a446ecbfc27ac995bf04b792` |
| SmartRecruiters | 2,214 | `ce0cddbe531bd2e6de97ff2c513d48ef2f103fe8ff065fd02eff326fb450ffba` |
| Workday | 2,604 | `034811ae293215fb60c21145e32d3e5a9ab49caba6080cb6639c02f60731b0b8` |

The MIT software license does not itself establish rights in every directory
record or upstream job posting. Production use remains subject to independent
dataset-rights review, source attribution, removal handling, and the terms of
each original source. The manifest reader does not activate production
ingestion by itself. Bluey never uses the dataset to bypass access controls.

## Bundled document fonts

The automation package ships the following font files for Unicode PDF
generation. `pdf-lib` embeds per-document subsets so generated files retain an
extractable text layer without carrying each complete font.

| Asset | Upstream revision | License | SHA-256 |
| --- | --- | --- | --- |
| `automation/assets/fonts/NotoSans-Regular.ttf` | [`notofonts/noto-fonts@ffebf8c1ee449e544955a7e813c54f9b73848eac`](https://github.com/notofonts/noto-fonts/tree/ffebf8c1ee449e544955a7e813c54f9b73848eac/hinted/ttf/NotoSans) | SIL Open Font License 1.1; copyright Google LLC, 2015-2021 | `b85c38ecea8a7cfb39c24e395a4007474fa5a4fc864f6ee33309eb4948d232d5` |
| `automation/assets/fonts/NotoSansSC-Regular.ttf` | [`notofonts/noto-cjk@f8d157532fbfaeda587e826d4cd5b21a49186f7c`](https://github.com/notofonts/noto-cjk/blob/f8d157532fbfaeda587e826d4cd5b21a49186f7c/Sans/Variable/TTF/Subset/NotoSansSC-VF.ttf) | SIL Open Font License 1.1; source SHA-256 `d68bafcb48a2707749396aa12bbbd833cb70401f3a9a689fd2902c7e0d295964` | `f7c076d935cbe9e0a4b8c3f559ae5db01522b9cfdd77af136444f6595be09c92` |
| `automation/assets/fonts/NotoSansKR-Regular.ttf` | [`notofonts/noto-cjk@f8d157532fbfaeda587e826d4cd5b21a49186f7c`](https://github.com/notofonts/noto-cjk/blob/f8d157532fbfaeda587e826d4cd5b21a49186f7c/Sans/Variable/TTF/Subset/NotoSansKR-VF.ttf) | SIL Open Font License 1.1; source SHA-256 `9e1d729e7e2b36f9ef439da102f8c134c10aabe46f1c843bf0aca5c043b86f76` | `272e1290201d9443b354a617da24424e2a8d71fb396d0816df98ca77d91cf915` |

The SC and KR files are deterministic weight-400 instances created with
FontTools 4.59.1 from those pinned variable sources. The name-table family,
subfamily, full-name, and PostScript records were normalized to `Regular`; no
glyph outlines or licensing metadata were removed.

The exact upstream licenses and notices accompany the assets under
`automation/assets/fonts/*-LICENSE.txt` and are included in automation package
artifacts.

## Shipped adaptation map

| Bluey file | Source lineage | Bluey changes |
| --- | --- | --- |
| `automation/src/public-ats.ts` | `career-ops` public provider modules | Rewritten as strict TypeScript contracts; five Bluey ATS source types; HTTPS-only target construction; exact host pinning; redirect rejection; response cap; bounded retry/backoff; normalization and account-filter input. |
| `automation/src/job-source-intelligence.ts` | `career-ops` `_trust-validator.mjs` and `fingerprint-core.mjs` at `01bf8b469ad5177a9c30230bc00509ead8e006c2` | Rewritten as typed advisory source-trust reasons and deterministic description-fingerprint cross-listing signals. Bluey keeps suspected listings separate and retains original-source revalidation plus server-owned eligibility. |
| `automation/src/form-intelligence.ts` | `job-apply-plugin`, `easy-job-application-filler-extension`, `ai-job-agent` | Rewritten around Bluey Career Profile facts, three-level answer memory, verification-aware Auto-submit, document packets, and typed Intervention Inbox records. |
| `automation/src/receipts.ts` | Per-job artifact and status patterns observed across `proficiently-claude-skills`, `ai-job-agent`, and AIHawk | Original typed immutable receipt bundle containing the exact job, resume version, final answers, confirmed claims, documents, events, screenshots, outcome, and deterministic SHA-256 fingerprint. |
| `automation/src/standard-adapters.ts` | ATS field and stop-condition research above | Original Bluey implementation of deterministic Workday, Greenhouse, Lever, Ashby, and SmartRecruiters execution; no upstream selectors or source file copied. |
| `automation/src/documents.ts` | Resume artifact workflow research above | Original Bluey ATS PDF materializer based on the frozen, job-specific resume record. |
| `runner/src/*` | Browser isolation patterns reviewed across the supplied repositories | Original Bluey cloud-runner implementation with per-identity serialization and AES-256-GCM profile snapshots. |

Required MIT copyright and permission notices are retained in
`THIRD_PARTY_NOTICES.md`. Package-manager lock files remain the source of truth
for runtime dependency versions; release builds must also generate dependency
notices from the JavaScript, Rust, Electron, Playwright, and container lock
files. The July 11 audit flagged dependency notice generation as a required
release gate, including transitive JavaScript packages and native browser/doc
tooling.

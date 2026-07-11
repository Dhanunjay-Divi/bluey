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
| [`Rayyan9477/AutoApply-AI-Agentic-Browser-Automation-for-Job-Search`](https://github.com/Rayyan9477/AutoApply-AI-Agentic-Browser-Automation-for-Job-Search) | `960ddc32a2f0` | No top-level license file in clone | Architecture research only. |
| [`feder-cr/jobs_applier_ai_agent_aihawk`](https://github.com/feder-cr/jobs_applier_ai_agent_aihawk) | `ab4113d453ab` | AGPL-3.0 | Research only: profile and run configuration. |
| [`neonwatty/job-apply-plugin`](https://github.com/neonwatty/job-apply-plugin) | `4330f090ba94` | MIT | Adapted field-label matching, ATS form patterns, and pause-on-unknown behavior into `automation/src/form-intelligence.ts`. |
| [`Pickle-Pixel/ApplyPilot`](https://github.com/Pickle-Pixel/ApplyPilot) | `4a8d521f67f5` | AGPL-3.0 | Product research: staged discovery, scoring, tailoring, PDF, validation, and retry pipeline. |
| [`11844/Auto_Jobs_Applier_AIHawk`](https://github.com/11844/Auto_Jobs_Applier_AIHawk) | `1cecff373348` | Repository-specific proprietary/source-available license | Adapted product requirements only: profile breadth, filters, exclusions, one-company rules, and per-job artifacts. No source file copied in this round. |
| [`santifer/career-ops`](https://github.com/santifer/career-ops) | `267dfb707987` | MIT | Adapted public ATS provider, host-pinning, bounded pagination, retry, normalization, dedupe, and stale-feed patterns into `automation/src/public-ats.ts`. |
| [`jaejaywoo/HireGPT`](https://github.com/jaejaywoo/HireGPT) | `835e5f52f093` | MIT | Research only: legacy Electron and local-service boundary. |

The GitHub [`job-application` topic](https://github.com/topics/job-application)
was used as a discovery index. Bluey did not treat the topic page as a license
or clone every listed repository.

## Additional topic candidates reviewed

| Repository | Reviewed commit | License observed | Bluey decision |
| --- | --- | --- | --- |
| [`MadsLorentzen/ai-job-search`](https://github.com/MadsLorentzen/ai-job-search) | `7e8df3581927` | MIT | Workflow and search research only. |
| [`AkbarDevop/ai-job-agent`](https://github.com/AkbarDevop/ai-job-agent) | `9ce47d29b5fc` | MIT | Adapted answer-memory precedence, review gates, and receipt/status ideas into typed Bluey form and receipt modules. |
| [`bdcorps/easy-job-application-filler-extension`](https://github.com/bdcorps/easy-job-application-filler-extension) | `16ea57dc7a95` | MIT | Adapted legacy field aliases and reusable-answer matching into `automation/src/form-intelligence.ts`. |
| [`leopu00/job-hunter-team`](https://github.com/leopu00/job-hunter-team) | `f2575c976f8a` | MIT | Multi-agent architecture research only. |
| [`suxrobGM/jobpilot`](https://github.com/suxrobGM/jobpilot) | `272b7aca424c` | MIT | Product and queue architecture research only. |

## Shipped adaptation map

| Bluey file | Source lineage | Bluey changes |
| --- | --- | --- |
| `automation/src/public-ats.ts` | `career-ops` public provider modules | Rewritten as strict TypeScript contracts; five Bluey ATS source types; HTTPS-only target construction; exact host pinning; redirect rejection; response cap; bounded retry/backoff; normalization and account-filter input. |
| `automation/src/form-intelligence.ts` | `job-apply-plugin`, `easy-job-application-filler-extension`, `ai-job-agent` | Rewritten around Bluey Career Profile facts, three-level answer memory, verification-aware Auto-submit, document packets, and typed Intervention Inbox records. |
| `automation/src/receipts.ts` | Per-job artifact and status patterns observed across `proficiently-claude-skills`, `ai-job-agent`, and AIHawk | Original typed immutable receipt bundle containing the exact job, resume version, final answers, confirmed claims, documents, events, screenshots, outcome, and deterministic SHA-256 fingerprint. |
| `automation/src/standard-adapters.ts` | ATS field and stop-condition research above | Original Bluey implementation of deterministic Workday, Greenhouse, Lever, Ashby, and SmartRecruiters execution; no upstream selectors or source file copied. |
| `automation/src/documents.ts` | Resume artifact workflow research above | Original Bluey ATS PDF materializer based on the frozen, job-specific resume record. |
| `runner/src/*` | Browser isolation patterns reviewed across the supplied repositories | Original Bluey cloud-runner implementation with per-identity serialization and AES-256-GCM profile snapshots. |

Required MIT copyright and permission notices are retained in
`THIRD_PARTY_NOTICES.md`. Package-manager lock files remain the source of truth
for runtime dependency versions; release builds must also generate dependency
notices from the JavaScript, Rust, Electron, Playwright, and container lock
files.

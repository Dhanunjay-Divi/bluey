# Jobs C2C and MCP current-source readiness audit

October 28, 2026. Read-only native-agent inventory on Phase638 source774/e959,
HEAD40949858. No implementation, tests, browser, network, provider, credentials or
deployment was performed by these audit assignments. Parent's full Rust15847 run
is separate evidence and remains live at this checkpoint. This is not a claim of
complete competitor parity, operational readiness or an autonomous launch.

## MCP: REST capabilities are not an MCP product

No Jobs MCP server, tool registry/transport or CLI entrypoint was found in the
inspected shipped Jobs/server/CLI sources and manifests. Round622 explicitly
defers MCP delegation; Round638 still lists it unfinished. Existing REST routes
provide useful implementation seams, not already-implemented MCP tools:

| Capability | Current source seam | MCP status |
| --- | --- | --- |
| Search/match | Discovery sources/catalog, matches/page/detail routes | Missing |
| Prepare | Applications creation, packet commit and resume versions | Missing |
| Approve | Packet approval, separate Track auto-submit authorize/revoke | Missing delegated approval protocol |
| Apply | Queue-run endpoint and separately gated runner/workflow execution | Missing MCP apply authority |
| Track/status | Tracks, workspace, applications, interventions, run events | Missing |
| Mail | Connections, sync/messages, exact communication draft/approval/cancel | Missing MCP surface; provider gates separate |

Route registration: `server/src/api/jobs.rs:114`. The discovery catalog in
`server/src/api/jobs_source_directory.rs:58` is a source directory, not a general
autonomous-search tool. `server/Cargo.toml:19` exposes the Jobs API binary; its two
integration-feature examples are private signed test fixtures, not MCP services.

Existing authentication/account deletion fences, beta/admission checks, onboarding,
Track authorization, eligibility and runner checks must remain server authority.
Mail write/sync/dispatch/reconciliation gates and exact revision/hash approval are
not delegated-agent credentials. See `server/src/api/mod.rs:422`,
`jobs_beta_access.rs:107`, `jobs.rs:1843`, `jobs_mailbox_oauth.rs:132`,
`jobs_communication_actions.rs:314` and the respective sync/dispatch modules.

Round612 (section near line673) plans audience-bound tokens, per-tool/per-Track
scopes, revocation and separate irreversible-action challenges. Those are design
contracts, not implemented MCP safeguards. Never expose worker signing keys or
turn a client-provided approval field into submit/mail authority.

Smallest successor acceptance: one real read-only `get_job_matches` MCP tool using
existing tenant-scoped matching, with initialize/list/call tested against two
isolated accounts. Prove authorized Track access and rejection of missing/revoked/
wrong-audience/scope/cross-account authority and unknown tools, with zero mutation.
Preparation/approval/apply/mail-write tools stay absent in that first slice. The
broader requested MCP workflow remains open; the read-only slice is not completion.

## C2C/W2: taxonomy exists; vendor workflows do not

Typed ATS ingestion accepts closed employment/engagement values and rejects
negated/mixed evidence (`jobs/automation/src/public-ats.ts:1207-1306`). Server
canonicalization binds those facts into posting evidence
(`server/src/db/jobs/discovery.rs:3226`, `:3568`). Portal category controls expose
full-time/contract and W2/C2C/1099/direct-hire; onboarding and Tracks persist them
(`JobCategoryChoices.tsx:1`, `Onboarding.tsx:435`, `SettingsView.tsx:682`).

Track/account eligibility rejects mismatches and unknown hard facts, rather than
guessing a job's arrangement (`candidate_policy.rs:274-327`, `eligibility.rs:1184-1227`).
Existing unit tests cover exclusive mismatch, unknown, negated and mixed evidence
(`candidate_policy.rs:829-912`, `public-ats.test.ts:76-179`, `:471-580`, `:648-678`).

Phase633 proved preference readback including W2/C2C, but its actual synthetic
posting was full_time+w2. That journey reached factual reprepare/local queue, not
a final browser session/receipt (`browser-api.integration.mjs:359-396`, `:551-585`;
`jobs-nonprod-browser-fixture.rs:381-407`; IMPL633). It does not prove an exclusive
C2C posting-to-runner-to-receipt journey. Current final-submit provider contracts
name only Greenhouse/Lever (`jobs/automation/src/contracts.ts:200-204`); actual
certification/runtime authority remains separately required. Normalized
employment/engagement can be retained in receipts, but no C2C receipt was produced
by this audit (`contracts.ts:91-105`, `receipts.ts:94-121`).

Existing communications are application-bound recruiter reply/calendar actions,
not C2C vendor chat. They require an existing application and exact mailbox message
for reply (`communication_actions.rs:367-412`, `:1196-1277`); the UI explicitly says
Replies and calendar actions (`ApplicationsView.tsx:1028-1064`). Provider writes
remain separately disabled/gated (`jobs/OPERATIONS.md:61-72`).

Requirement/vendor chains, right-to-represent, rate/term negotiation, W9/COI/MSA/SOW,
timesheets/invoices, contract signatures and C2C chat/outreach were not found as
production implementations in the inspected scope. Phase620D designs rights,
requirement/vendor facts, reviewed simulated outreach, provider-evidence chat and
human-reviewed legal/contract facts
(`PLAN-PHASE-620B-F-JOBS-OMNICHANNEL-OUTREACH-AND-AGENT-CONTROL-PLANE.md`,
lines899-1014/1736-1751).
`JOBS-V1-COMPLETION-LEDGER.md:40-43` already labels taxonomy implemented and recruiter
workflows successor; preserve that distinction in customer copy and release claims.

Next proof: three isolated synthetic lanes—full-time without an engagement hard
filter, contract+W2 and contract+C2C—through persisted Track policy, match/mismatch/
unknown decisions, factual packet, synthetic runner and receipt category readback.
Use distinct employers/accounts where needed to preserve apply-once constraints.
This would qualify general employer apply only, not the C2C vendor product.

First C2C product batch must separately implement Phase620D's no-egress
rights→requirement→match→review→simulated receipt plus provider-evidence chat
projection, with `attachment_refs=[]`. Full contracts/outreach, real provider
effects, legal commitments and launch remain open and require their own gates.

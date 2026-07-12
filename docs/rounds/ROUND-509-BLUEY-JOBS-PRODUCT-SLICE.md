# Round 509 - Bluey Jobs Product Slice

Date: 2026-07-10

## Branch and isolation

- Branch: `codex/bluey-jobs-20260710`
- Worktree: `/Users/uno/Downloads/cue-bluey-jobs`
- Existing Bluey host overlay, audio, meeting runtime, daemon, and native
  session behavior were not modified.
- This work is intentionally isolated for selective review and merge.

## Customer product

Added the `/jobs` product with responsive light and dark experiences for:

1. Matches
   - Career Track filters, query, match score, workplace filters, density
     control, pasted job links, restricted-site handoff, and match explanation.
2. Applications
   - Packet states, intervention inbox, job-specific resume preview, visible
     diff, PDF/DOCX export, runner selection, browser takeover, and receipts.
3. Resume
   - PDF/DOCX/TXT import, Career Profile editing, provenance summary, tailored
     version history, resume diff, and exports.
4. Browser
   - Local and cloud runner choices, active-run state, pause/resume, takeover,
     supported ATS presentation, and LinkedIn/Indeed handoff policy.
5. Settings
   - Career Track Agents, role and location rules, Factual/Enhance, Review
     first/Auto-submit, claim review, thresholds, integrations, and plans.

The first-run onboarding captures identity, work history, education, skills,
authorization/search preferences, location policy, salary, daily limit, and
application defaults before creating the first Career Track.

`/JobApply` now redirects to canonical `/jobs`.

## Jobs API and data

Added authenticated `/api/jobs` routes for:

- workspace, profile, facts, preferences, and Career Tracks;
- matches, applications, resume versions, and packet commit;
- browser sessions, interventions, integrations, entitlements, and run events.

Added tenant-scoped SQLite and PostgreSQL records for the same domains.
Important invariants are enforced in the persistence layer:

- canonical jobs deduplicate per account;
- each generated resume belongs to one canonical job;
- packet regeneration is idempotent;
- metering is once per account and canonical job;
- included allowance or overage balance deduction is atomic with its ledger;
- concurrent commits serialize before checking metering;
- restricted-site packets cannot enter background Auto-submit;
- Auto-submit also honors the account threshold and missing requirements.
- customer JSON payloads are authenticated-encrypted at rest with AES-256-GCM,
  while relational tenant keys remain available for safe queries.

The invited-beta entitlement admin route is
`PATCH /admin/jobs/entitlements/:account_id`. Customer subscription checkout
still needs the Jobs product catalog and recurring webhook mapping before paid
plans can be self-serve.

## Independent service

Added the `bluey-jobs-api` Rust binary. It exposes only Jobs routes, validates
the same Bluey bearer tokens, and uses the shared account/balance database.
Caddy routes `/api/jobs/*` to port 8081 while the existing Bluey API remains on
8080.

Release builds require `BLUEY_JOBS_BETA_ENABLED=1`; otherwise Jobs APIs return
404. Added a hardened systemd example and environment settings.

## Automation boundaries

Added a shared TypeScript package with:

- typed discovery and application-adapter contracts;
- Workday, Greenhouse, Lever, Ashby, and SmartRecruiters detection;
- local/cloud runner parity types;
- submission receipts, interventions, and validation issues;
- public-link and private-network policy checks;
- mandatory handoff for LinkedIn and Indeed.

Added an Electron/Chromium controller with account-isolated persistent profiles,
OS-encrypted Bluey tokens, protocol handling, and local run startup. Added
Temporal workflow/activity boundaries for cloud runs and interventions.

These are beta foundations, not a claim that external providers are live.
Public release still requires fixture-proven form adapters, packaged desktop
installers, a production browser-pool activity, and provider credentials.

## Integrations and plans

The interface does not fake Gmail, Outlook, or calendar authorization. Connect
opens invited-beta access instead of writing a false connected state. The API
accepts user-driven disconnects but rejects fabricated connected state.

Free, Pro, and Cloud entitlements, allowances, local/cloud flags, and $0.50
overage metering exist. Paid plan self-service remains a release gate until the
Square recurring product and webhook mapping is configured.

## Legal and routing

- Added Bluey Jobs terms and privacy sections.
- Added Jobs to the sitemap.
- Added Caddy SPA fallback for deep route refresh.
- Added Caddy routing for the independent Jobs API.
- Added source-provenance and architecture documents.

## Verification

Completed:

- `npm run typecheck`
- `npm run test`
- `npm run build`
- `npm audit --audit-level=high` - zero vulnerabilities
- `cargo check`
- `cargo check --bin bluey-jobs-api`
- `cargo test db::jobs --lib` - six focused tests passed, including application state transitions
- production route build into `web/jobs`
- route refresh response at `/jobs/matches?preview=1`
- desktop/mobile and light/dark screenshot review across all five views and
  major dialogs during implementation
- no horizontal overflow in the reviewed desktop/mobile routes

Local preview:

`http://127.0.0.1:5187/jobs/matches?preview=1`

## Release gates

Before public billing or automated submission is enabled:

1. Configure and verify Square recurring Jobs products and webhook entitlement
   changes.
2. Complete Gmail/Outlook OAuth token storage and revocation.
3. Run fixture and sandbox suites for all five deterministic ATS adapters.
4. Connect the Temporal worker to the production cloud browser pool.
5. Package, sign, and test Bluey Browser installers and account pairing.
6. Complete production envelope-key wrapping and rotation for cloud browser
   bundles.
7. Connect licensed discovery providers and OpenSearch ingestion.
8. Complete load, regional recovery, and tenant-isolation penetration tests.

The portal labels runner and provider surfaces as beta until these gates pass.

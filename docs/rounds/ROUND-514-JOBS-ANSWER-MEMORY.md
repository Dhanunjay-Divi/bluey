# Round 476: Jobs Answer Memory

Date: 2026-07-10

## Goal

Let a user answer a blocked application question once, resume the same run, and optionally reuse that confirmed answer without repeatedly interrupting local or cloud automation.

## Product Behavior

- Settings now includes a compact Answer Memory section with add, edit, and remove controls.
- The Intervention Inbox shows an inline answer control for unknown required questions and missing facts.
- `Remember this answer` is selected by default for ordinary application questions and not selected by default for sensitive questions.
- Users can scope a saved answer to all applications, one Career Track, or one company.
- Resolution priority is deterministic: company, then Career Track, then account.
- Submitting an answer advances the canonical application from `needs_input` to `queued` and resumes its browser run.
- CAPTCHA, account-owner verification, and assessments continue through their existing recovery paths rather than Answer Memory.

## Persistence And API

- Added encrypted, tenant-scoped `jobs_answer_memory` persistence for SQLite and PostgreSQL.
- Question lookup uses a keyed private hash; the question and answer remain inside the encrypted payload.
- Re-saving the same normalized question and scope updates one record instead of producing duplicates.
- Added authenticated CRUD endpoints at `/api/jobs/answers` and `/api/jobs/answers/:answer_id`.
- Intervention resolution accepts an answer, reuse decision, scope, and scope identifier.
- The answer used for the active application is kept in the encrypted intervention payload. It is not emitted into run-event logs.
- The Jobs workspace now returns Answer Memory so Settings and application workers share one source of truth.

## Automation Contract

- The form planner already resolves confirmed memory by company, Career Track, and account priority.
- The planner now accepts the API's `scope_id` shape directly as well as its existing internal `scopeId` shape.
- Sensitive demographic questions still pause instead of being silently answered from memory.

## Verification

- Rust tests cover encrypted storage, tenant isolation, idempotent upsert, and deletion.
- Automation tests cover priority and API-shaped memory records.
- Portal type checks, tests, and production build pass.
- The preview flow was exercised from Intervention Inbox through `Use answer & resume`; the application row and intervention count updated immediately.
- Settings add/edit/delete surfaces and the intervention control were reviewed in dark and light themes and at a 390px mobile viewport.
- Browser console checks reported no errors or warnings.
- No Bluey host overlay, meeting runtime, audio, or native session files were changed.

## Remaining Production Activation

- The live Electron and cloud browser packet loaders still need to fetch the workspace Answer Memory before calling the typed form planner. The API and planner contract are ready for that connection.
- Temporal browser workers and the first live ATS adapters still need deployment credentials, isolated browser infrastructure, and sandbox certification before cloud application runs can be generally enabled.
- Gmail/Outlook authorization, Jobs subscription checkout, production object storage/KMS, observability, and scale/security testing remain launch dependencies for the wider Jobs product.

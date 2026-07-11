# Round 479 - Jobs Automation Execution Pipeline

## Objective

Turn the Bluey Jobs beta from a portal and persistence foundation into one real
local/cloud execution path without touching the meeting overlay, audio,
transcription, or native session runtime.

## Implemented

- Added deterministic Workday, Greenhouse, Lever, Ashby, and SmartRecruiters
  adapters with shared prepare, fill, validate, multi-step submit, confirmation,
  missing-answer, CAPTCHA, 2FA, and assessment behavior.
- Added fixture coverage for every initial ATS and kept direct employer forms
  behind the existing constrained semantic boundary.
- Added a Playwright page bridge used by both local and cloud execution.
- Scoped Bluey Browser Chromium data by `(account_id, application_identity_id)`
  so separate application emails never share cookies.
- Added an authenticated web-to-desktop dispatch path. Jobs issues an encrypted,
  24-hour, application-scoped capability ticket; the custom protocol carries
  only that random ticket, never a reusable Bluey access token or packet.
- Local interventions preserve the visible Chromium page. After the owner
  handles CAPTCHA, 2FA, an assessment, or a new required answer, the controller
  resumes the same frozen application and sends its result back to Jobs.
- Added deterministic ATS PDF and cover-letter materialization from the frozen,
  job-specific resume version.
- Added local receipts, screenshots, document hashes, adapter version, selected
  application identity, and browser profile ID.
- Added a cloud browser service with per-identity serialization, encrypted
  AES-256-GCM profile snapshots, preserved intervention sessions, and exact
  receipt generation.
- Added encrypted, durable browser-step results and deterministic API run IDs.
  Temporal retries, network response loss, repeated queue clicks, and repeated
  intervention signals now converge on the same recorded step instead of
  clicking Submit or charging twice.
- Added DNS-level public-network enforcement for initial and redirected local
  and cloud browser navigation.
- Completed Answer Memory execution: company answers override Career Track
  answers, Career Track answers override account answers, and a direct answer
  on the current application wins over all saved values.
- Tightened form behavior for required radio groups and ambiguous submission
  results. A run records `submitted` only after explicit confirmation text or
  a confirmation URL; otherwise the preserved browser asks for review.
- Replaced the placeholder Temporal activity with a real browser-pool call.
- Added a workflow gateway, idempotent workflow start, 24-hour intervention
  wait, workflow signals, same-page resume, and final browser release.
- Closed the fast-response signal race and configured both Temporal gateway and
  worker for Temporal Cloud TLS/API-key authentication.
- Added a real authenticated queue endpoint. Queueing now freezes the account,
  application identity, job, tailored resume, answers, claims, and browser
  profile before starting Temporal.
- Added worker-only state, run-event, intervention, and final-receipt endpoints.
  Final submission stores resume and confirmation evidence before transitioning
  the application to `submitted`.
- Added Electron Builder and container manifests plus an operations runbook.
- Made production package builds clean and source-only, then verified the
  packaged macOS app starts from `dist/main.js`, contains the compiled shared
  automation package, bundles Playwright Chromium, and excludes Bluey source
  and test directories. A local Apple development signing pass also completed;
  public notarization remains an external release gate.

## Invariants

1. A run cannot change application email, resume version, canonical job, or
   browser profile after queueing.
2. One application identity has at most one active browser run at a time.
3. Unknown required facts and verification challenges pause instead of guessing.
4. A submitted state requires exact resume and confirmation evidence.
5. Local and cloud runners execute the same adapter package.
6. Existing Bluey overlay, audio, meeting, and native runtime files are untouched.
7. Browser activity cannot navigate to private, loopback, link-local, or
   credential-bearing application URLs.
8. A retried browser step returns its encrypted recorded result and never
   repeats a completed submit action.
9. A local launch URL contains a short-lived application capability, not a
   Bluey login token, application answers, or resume content.
10. Machine-local document and screenshot paths are removed before a receipt
    is stored on the web account.

## Verification

- `npm test` in `jobs`: 62 passing.
- `npm run typecheck` in `jobs`: passing.
- `npm run build` in `jobs`: passing.
- `npm run smoke --workspace @bluey/jobs-automation`: passing.
- `electron-builder --dir`: passing with packaged archive and Chromium checks.
- `cargo check --manifest-path server/Cargo.toml --bin bluey-jobs-api`: passing.
- `cargo test --manifest-path server/Cargo.toml db::jobs::tests --lib`: 16 passing.
- In-app Browser checks at desktop and `390x844`: dark/light themes pass, local
  run chooser passes, route reload passes, no horizontal overflow, and no
  console warnings or errors.

## External Launch Gates

The implementation is ready for deployment integration, but public automation
still requires real Temporal/provider credentials, R2/S3 receipt upload,
Gmail/Outlook OAuth applications, a browser-takeover streaming service, signed
desktop installers, and live ATS tenant certification. These cannot be safely
fabricated in source control and are itemized in `jobs/OPERATIONS.md`.

# Round 601 - Jobs Evidence Lifecycle and Exact Submit

**Date:** 2026-08-05
**Branch:** `feat/phase-601-jobs-evidence-lifecycle`
**Status:** Verified local source batch; no runtime deployment or flag change

## Outcome

Bluey now binds an authorized Greenhouse or Lever final submission to one exact
provider job, form target, ordered set of successful controls, and set of
content-addressed PDF bytes. The irreversible click also has protected evidence
capacity before it begins, and the resulting receipt, documents, and one to four
confirmation screenshots enter one durable, account-scoped lifecycle.

Cloud and local runners now establish their browser network guard while offline,
recover only an exact bound page with no surviving service worker, and keep a
submitted result staged until the server accepts the same token-and-fence replay.
A corrupt checkpoint blocks only its encrypted browser-profile scope.

## Exact Employer-Side Authority

The certified Greenhouse and Lever path now requires all of the following:

1. The current page, effective form action, provider job identifier, and later
   confirmation URL all resolve to the approved job.
2. The form uses one bounded `POST` target with `multipart/form-data` and
   `_self`, and its identity remains unchanged through activation.
3. An isolated Chromium world captures successful controls and file hashes
   without trusting page-realm DOM or `FormData` prototypes.
4. The Node runner verifies the content-addressed PDF files from disk, validates
   the browser-generated multipart structure, and hydrates only file bodies
   omitted by Chromium while preserving the boundary and headers byte-for-byte.
5. Unknown or conflicting hidden fields, method overrides, cross-type field
   overlap, extra post-fill traffic, and unrelated redirects fail closed.
6. Only causal main-frame navigation that remains bound to the same provider
   job may follow the single employer-facing request; Submitted still requires
   the provider's explicit confirmation URL.
7. The observed submit response must be a `2xx` status or one of the intended
   redirect statuses (`301`, `302`, `303`, `307`, or `308`), and a returned or
   ambiguous submit form or negative submission language defeats
   confirmation-looking text.

The server independently validates the proof schema, provider job binding,
ordered field and file evidence, frozen packet and document hashes, execution
authority, attempt capacity, and immutable evidence manifest.

## Evidence and Account Lifecycle

- Evidence bytes and object slots are reserved before a cloud or local runner
  can activate final submit. Unknown outcomes retain the reservation; definite
  non-submissions release it; a valid receipt consumes it atomically.
- The receipt bundle, exact submitted documents, and each of one to four
  confirmation screenshots receive immutable account-scoped object records and
  unique indexed names.
- Authenticated downloads re-read the object and reject missing, tampered,
  mis-scoped, wrong-media, incomplete, or wrong-resume evidence.
- Resume-source and encrypted browser-profile writes publish through the durable
  object ledger. Dialect-paired migrations adopt compatible legacy objects and
  fail startup on conflicting bindings.
- Eligible account deletion takes an exclusive lifecycle guard, persists a
  write fence, waits for live puts regardless of ledger age, purges the account
  object prefix, and only then removes database ownership. PostgreSQL uses a
  separate bounded pool plus a cross-replica advisory lock, so lifecycle guards
  cannot starve primary-pool finalization. Artifact storage and the effective
  audit store must both be available; a missing configuration or failed sweep
  returns `503 Service Unavailable` while preserving the account and fence.
- Irreversible submission states still block deletion until reconciled. Known
  cloud-runner sessions or leases also return `409 Conflict` before a deletion
  intent is created because verified runner-volume purge acknowledgements are a
  separate launch gate; the deletion fence rejects any later session or lease.

## Recovery Boundary

- Fresh and recovered browsers start offline. Bluey selects exactly one bound
  page, rejects extra HTTP pages and service workers, installs the provider
  guard, and only then enables network and navigates.
- Submitted results are durably staged with exact account, application,
  identity, session, run, request, lease-token, and fence bindings.
- A lost runner or persistence response replays the same server finish and
  promotes only the matching staged result; ambiguous storage errors use bounded
  exact retries.
- Browser-session and profile mutations are serialized, and checkpoint failure
  is isolated to the affected content-addressed profile scope.

## Verification

- Jobs TypeScript: 988 tests passed across automation (530), browser (151),
  runner (110), workflows (76), and portal (121).
- All five Jobs workspaces passed strict TypeScript checks and production builds;
  the automation package export smoke passed in Node.
- Server: 936 library tests, 98 signed HTTP end-to-end tests, and 6 auxiliary
  integration tests passed.
- Server formatting, full-feature strict Clippy, the Jobs API compile target,
  root-workspace formatting, Jobs privacy/schema/provenance guards, and
  `git diff --check` passed.
- A real local Chromium test proved the exact PDF bytes reached the fixture
  endpoint even after the page replaced `FormData` and an input prototype, and
  proved a delayed beacon was blocked.
- Exhaustive status matrices reject unintended `3xx` responses at browser,
  receipt, workflow, and server boundaries, and both certified adapters reject
  ambiguous post-submit controls or active, passive, contracted, and “not yet”
  negative outcomes even on confirmation-looking pages.
- The structural dialect guard covers all 8 paired Jobs tables and 13 required
  indexes, including deletion intents and submission-evidence capacity.

## Production Boundary

No deployment, service restart, production database migration, object-store
mutation, secret access, or feature-flag change is part of this round.
Greenhouse and Lever remain review-only and uncertified for public unattended
submission. Authorized tenant DOM certification, live PostgreSQL migration and
concurrency testing, real R2/S3 fault testing, and physical-device power-loss
durability remain external launch gates. Local durability evidence covers
process crashes; parent-directory sync is best effort where a platform does not
support it. Cloud Browser distribution also remains gated until a signed
multi-runner account-volume purge protocol exists and legacy runner volumes are
reconciled or securely wiped.

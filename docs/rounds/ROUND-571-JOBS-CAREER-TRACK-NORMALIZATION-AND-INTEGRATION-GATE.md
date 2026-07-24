# Round 571 - Jobs Career Track Normalization And Integration Gate

Date: 2026-07-24

## Objective

Reconcile the completed Bluey Jobs resume-import, Career Track, application
inbox, and portal work into one reviewable mainline release slice without
creating another side repository or worktree.

This round closes the next setup gate after Round 570. It does not enable
managed model generation, local-browser distribution, cloud-browser
distribution, or employer-facing submission.

## Career Track Corrections

### Relevant experience

New Career Tracks no longer bind every imported employment entry as relevant
experience. The onboarding payload leaves `relevant_employment_ids` empty so
the server can apply its role-family policy rather than counting unrelated
work.

The setup and Settings summaries now calculate role-scoped, non-overlapping
experience:

- overlapping jobs are counted once;
- only employment relevant to the selected role family contributes;
- the displayed search range follows the documented minus-one/plus-two-year
  policy;
- seniority remains an independent guard instead of being inferred only from
  elapsed months.

The portal shows this as an explanation of Bluey's policy. It does not ask the
user to choose an arbitrary daily pace or Auto threshold.

### Resume-import truth

The Round 570 narrative boundary and correction warnings remain part of this
slice:

- responsibility prose cannot become an employer or title;
- sentence-like employer/title values produce a review warning;
- imported content remains a draft until the user reviews it.

## Read-Only Application Inbox

Round 568 adds the durable Gmail/Outlook application inbox:

- provider OAuth uses PKCE and one-time account/provider-bound state;
- credentials, cursors, and normalized message content are encrypted at rest;
- synchronization uses database-backed leases and idempotent provider-message
  storage;
- messages are correlated to one exact application;
- acknowledgements become evidence;
- material updates become review interventions;
- no send permission is requested;
- no employer outcome is changed by keyword inference.

Mailbox synchronization now fails closed. The worker is off when
`BLUEY_JOBS_MAILBOX_SYNC_ENABLED` is absent and starts only when operations
sets it to `1`. The example environment and operations guide keep it disabled
until reviewed OAuth credentials, redirect URIs, encrypted token storage, and
worker monitoring are configured.

Calendar connection and outbound email remain unavailable. The
application-communications module is planning-only and is not invoked by a
runtime worker.

## Schema

The release adds two Postgres migrations:

1. `014_jobs_provider_connections.sql`
   - one-time OAuth state;
   - encrypted provider credentials linked to the existing account-scoped
     mailbox connection table.
2. `015_jobs_mailbox_sync.sql`
   - durable synchronization cursors and leases;
   - encrypted normalized provider messages;
   - account/application indexes.

Both migrations declare `-- Target: Postgres` so migration discovery and
schema-parity checks include them.

## Portal

Settings exposes compact read-only inbox controls and truthful status:

- connect Gmail or Outlook;
- reconnect when authorization needs attention;
- request an immediate sync;
- disconnect and remove provider credentials;
- review matched employer updates in Applications.

The portal never displays raw OAuth tokens, full provider identifiers, or
unbounded private email content.

Desktop and mobile visual checks covered onboarding and Settings in both
responsive layouts. The pages remained within their viewport with no
horizontal overflow.

## Verification

Completed gates:

```text
Portal focused resume and Career Track tests
  5 passed

Portal full suite
  75 passed

Portal typecheck and production build
  passed

Server full library suite
  777 passed

Mailbox fail-closed configuration regression
  passed

Server strict Clippy
  passed

Server formatting
  passed

Jobs automation
  184 passed

Jobs browser
  100 passed

Jobs runner
  50 passed

Jobs workflows
  51 passed

Jobs monorepo typecheck
  passed

git diff --check
  passed
```

The portal build retains the existing large-chunk warning. This round does not
introduce a new warning or ship source maps.

## Mainline Hygiene

All work was reconciled in the existing mainline worktree. No new clone,
copied repository, scratch source tree, or worktree was created.

The older dirty Jobs worktree remains untouched because it contains unrelated
experimental changes and is not safe to delete or merge wholesale. It is not a
release source. Mainline remains the only deployment source for this round.

Historical signed-release and sign-in round documents found in the main
worktree are retained as dated evidence; they do not alter current release
artifacts.

## Release Boundaries

The following flags remain off:

```text
BLUEY_JOBS_MODEL_GENERATION_ENABLED=0
BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED=0
BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED=0
BLUEY_JOBS_MAILBOX_SYNC_ENABLED=0
```

Before a production mailbox worker is enabled, operations must install and
verify provider credentials, exact callback URLs, provider approval, encrypted
token storage, monitoring, disconnect/deletion, and a bounded live canary.

No native Bluey artifact is rebuilt or republished by this web/server slice.

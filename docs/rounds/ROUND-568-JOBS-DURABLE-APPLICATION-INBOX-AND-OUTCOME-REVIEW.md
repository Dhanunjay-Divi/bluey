# Round 568 - Jobs Durable Application Inbox And Outcome Review

Date: 2026-07-23

## Objective

Give Bluey Jobs a durable, read-only application inbox that can connect a user's Gmail or Outlook account, correlate employer messages with existing applications, preserve evidence, and ask the user to review meaningful updates.

This round deliberately does not send email, infer a final application state without review, connect calendars, or enable unattended application submission.

## User Flow

1. The user opens Jobs Settings and selects `Connect inbox`.
2. Bluey starts a provider-specific OAuth flow with PKCE and a one-time expiring state.
3. The provider callback stores an encrypted provider credential and an account-scoped mailbox connection.
4. A durable worker leases due mailbox connections and reads recent inbox messages.
5. Bluey correlates each message with an existing application using the application identity, employer, role, and sender domain.
6. Acknowledgements are recorded as immutable evidence.
7. Interviews, assessments, offers, rejections, and requests for information create a review item.
8. The portal shows the connection status, last sync, recent matched updates, and a `Review` link into Applications.

The Settings page clearly states that inbox access is read-only and that Bluey never sends email from a connected inbox.

## Provider Scope

### Gmail

- OAuth scopes: `openid`, `email`, and `gmail.readonly`.
- Up to 200 recent inbox messages are fetched per sync.
- Initial ingestion is bounded to the previous 90 days.
- Full message payloads are normalized to bounded plain text before encrypted storage.

### Outlook

- OAuth scopes: `openid`, `email`, `offline_access`, `User.Read`, and `Mail.Read`.
- Microsoft Graph delta links provide an incremental cursor.
- Outlook currently provides `bodyPreview` for classification rather than fetching the entire body.

Neither provider grants send permission.

## Architecture

### OAuth

- `server/src/api/jobs_mailbox_oauth.rs`
- Public callback: `GET /api/jobs/oauth/:provider/callback`
- Authenticated start: `POST /api/jobs/mailbox-oauth/:provider/start`
- PKCE verifier and state are encrypted at rest.
- State is one-time, provider-bound, account-bound, and expires.
- The callback redirects to Jobs Settings with a minimized result code.

Required provider configuration:

```text
BLUEY_JOBS_GOOGLE_CLIENT_ID
BLUEY_JOBS_GOOGLE_CLIENT_SECRET
BLUEY_JOBS_MICROSOFT_CLIENT_ID
BLUEY_JOBS_MICROSOFT_CLIENT_SECRET
BLUEY_JOBS_MICROSOFT_TENANT_ID
```

`BLUEY_JOBS_MICROSOFT_TENANT_ID` defaults to `common`.

### Durable Sync

- `server/src/jobs_mailbox_sync/mod.rs`
- `server/src/jobs_mailbox_sync/providers.rs`
- `server/src/jobs_mailbox_sync/processing.rs`
- `server/src/db/jobs/mailbox_sync.rs`

The worker is controlled by:

```text
BLUEY_JOBS_MAILBOX_SYNC_ENABLED
BLUEY_JOBS_MAILBOX_SYNC_POLL_SECONDS
```

The worker defaults off and requires explicit
`BLUEY_JOBS_MAILBOX_SYNC_ENABLED=1` after the provider OAuth credentials,
reviewed redirect URIs, encrypted token storage, and worker monitoring are
configured. Shipping the connection and review UI does not start mailbox
polling by itself.

The worker:

- claims due connections with a database-backed lease;
- refreshes provider access tokens;
- stores rotated refresh tokens;
- ingests messages with stable provider-derived IDs;
- updates an incremental provider cursor;
- excludes disconnected or reauthorization-required connections from claims;
- marks authorization as needing reconnection after provider authorization failures;
- safely retries idempotent writes after lease expiry or process restart.

### Persistence

PostgreSQL migrations:

- `infra/postgres/server-runtime/014_jobs_provider_connections.sql`
- `infra/postgres/server-runtime/015_jobs_mailbox_sync.sql`

SQLite parity is provided by migration 0038 in `server/src/db/mod.rs`.

The data model stores:

- provider connection and encrypted OAuth credential;
- durable sync state, cursor, lease owner, and lease expiry;
- encrypted normalized message content;
- stable message-processing state;
- immutable application evidence;
- account-scoped review interventions.

Credentials and message content use the existing AES-256-GCM application encryption boundary.

## Correlation And Review

`server/src/jobs_mailbox_sync/processing.rs` scores candidate applications using:

- the frozen application identity email;
- employer name;
- job title;
- employer/source domain.

A message must meet the minimum score and separation from the runner-up. Ambiguous or unmatched messages become `needs_input`; they do not silently attach to an application.

Classified acknowledgements are evidence-only. Other material updates create an intervention. Bluey does not automatically transition an application to Interview, Rejected, or Offer.

The API returns minimized message summaries. It does not return provider message IDs, raw encrypted content, OAuth credentials, or cross-account existence details.

## Portal

Changed portal modules:

- `jobs/portal/src/App.tsx`
- `jobs/portal/src/api.ts`
- `jobs/portal/src/types.ts`
- `jobs/portal/src/components/ApplicationInboxSettings.tsx`
- `jobs/portal/src/views/SettingsView.tsx`
- `jobs/portal/src/styles.css`

The account-scoped list, sync, message-summary, and disconnect handlers are
isolated in `server/src/api/jobs_mailbox.rs`; the main Jobs router only binds
their existing public paths.

The Application inbox section now supports:

- Gmail and Outlook connection;
- plan-aware inbox count;
- connection health and reconnect state;
- manual sync;
- disconnect;
- recent matched employer updates;
- direct `Review` navigation for messages that need user input;
- an honest empty state;
- an explicit calendar-unavailable state.

No fake mailbox connection endpoint remains.

## Application Communication Planner

`jobs/automation/src/application-communications.ts` provides deterministic classification, application correlation, question extraction, and response-plan construction for future reviewed reply workflows.

This is a planning library only in this round. Its `draft` and `auto_send` output types are not wired to provider send APIs, and the production mailbox integration remains read-only.

## Security And Tenancy

- Jobs authentication and beta entitlement protect configuration and data routes.
- Jobs rate limits protect OAuth start, sync, list, and disconnect operations.
- The public OAuth callback has its own abuse limit.
- Provider credentials and normalized message bodies are encrypted.
- Database reads and writes are scoped by `account_id`.
- Cross-tenant identifiers return the same generic not-found response.
- API responses expose only the fields required for the portal.
- No raw email body, OAuth token, provider message identifier, or OTP is emitted to the portal or normal logs.

## Verification

Passed:

```text
cargo fmt --manifest-path server/Cargo.toml
cargo test --manifest-path server/Cargo.toml mailbox -- --nocapture
  15 passed
npm run build --workspace @bluey/jobs-automation
npm run test --workspace @bluey/jobs-automation -- application-communications.test.ts
  7 passed
npm run typecheck --workspace @bluey/jobs-automation
npm run test --workspace @bluey/jobs-portal
  75 passed
npm run typecheck --workspace @bluey/jobs-portal
npm run build --workspace @bluey/jobs-portal
cargo check --manifest-path server/Cargo.toml --lib --bins
git diff --check
```

The portal build retains its existing large-chunk warning; it does not fail the build.

Browser QA passed at desktop and mobile widths in light and dark themes. No browser errors or warnings were recorded.

Local QA captures:

- `/tmp/bluey-jobs-mailbox-settings-desktop.png`
- `/tmp/bluey-jobs-mailbox-section-desktop.png`
- `/tmp/bluey-jobs-mailbox-section-desktop-light.png`
- `/tmp/bluey-jobs-mailbox-section-mobile-final.png`

## Remaining Production Work

This round is complete as a durable read-only inbox and employer-update review slice. It is not the entire autonomous job-agent product.

Before enabling this feature for real accounts:

1. Configure approved Google and Microsoft OAuth applications and exact redirect URIs.
2. Run authorized live Gmail and Outlook connection, revocation, token-rotation, and provider-outage tests.
3. Add production monitoring for lease backlog, provider errors, reauthorization, correlation ambiguity, and intervention volume.
4. Define retention and deletion policy for encrypted message content.
5. Add calendar authorization and event handling as a separate reviewed feature.
6. Build a reviewed reply-drafting workflow before considering any send permission.

Universal discovery, resume generation, cloud browser execution, and unattended submission remain separate launch gates. No production deployment was performed in this round.

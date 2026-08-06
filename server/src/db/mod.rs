//! Database access layer.
//!
//! SQLite remains the default local/single-node backend. Postgres is the
//! paid-server destination and now has a real pool/migration boundary so
//! runtime adapter slices can move table-by-table without pretending an env
//! flag alone is a cutover.

use anyhow::{Context, Result};
use native_tls::{Certificate, TlsConnector};
use postgres_native_tls::MakeTlsConnector;
use r2d2::{ManageConnection, Pool, PooledConnection};
use r2d2_postgres::PostgresConnectionManager;
use r2d2_sqlite::SqliteConnectionManager;
use std::cell::Cell;
use std::ops::{Deref, DerefMut};
use std::path::Path;
use std::sync::Arc;
use tokio::runtime::{Handle, RuntimeFlavor};
use tokio::sync::Semaphore;

pub mod account_data;
pub mod accounts;
pub mod auth_tokens;
pub mod balance;
pub mod device_codes;
pub mod devices;
pub mod diagnostic_logs;
pub mod idempotency;
pub mod jobs;
pub mod jobs_generation;
pub mod jobs_generation_allowance;
pub mod jobs_provider_cost_holds;
mod jobs_tailoring;
pub mod legal_acceptances;
pub mod link_codes;
pub mod metrics;
pub mod object_uploads;
pub mod ops_audit;
pub mod refresh_tokens;
pub mod signup_otps;
pub mod stripe_auto_reload;
pub mod stt_accounting;
pub mod sync;
pub mod trial_abuse;
pub mod usage;
pub mod usage_reservations;
pub mod webhook_events;

pub type SqliteDbPool = Pool<SqliteConnectionManager>;
pub type PostgresDbPool = Pool<SafePostgresConnectionManager>;
pub type SqliteDbConn = PooledConnection<SqliteConnectionManager>;
pub type PostgresDbConn = PooledConnection<SafePostgresConnectionManager>;

const POSTGRES_PRIMARY_POOL_SIZE: u32 = 16;
const POSTGRES_LIFECYCLE_POOL_SIZE: u32 = 8;

/// PostgreSQL lifecycle locks deliberately use a pool separate from ordinary
/// database work. Account object guards span network I/O; retaining those
/// sessions in the primary pool could otherwise starve the finalization calls
/// needed to release the guards.
#[derive(Clone)]
pub struct PostgresPools {
    primary: PostgresDbPool,
    lifecycle: PostgresDbPool,
    lifecycle_slots: Arc<Semaphore>,
}

impl Deref for PostgresPools {
    type Target = PostgresDbPool;

    fn deref(&self) -> &Self::Target {
        &self.primary
    }
}

impl PostgresPools {
    pub(crate) fn lifecycle_pool(&self) -> PostgresDbPool {
        self.lifecycle.clone()
    }

    pub(crate) fn lifecycle_slots(&self) -> Arc<Semaphore> {
        Arc::clone(&self.lifecycle_slots)
    }
}

pub struct SafePostgresConnectionManager {
    inner: PostgresConnectionManager<MakeTlsConnector>,
}

pub struct SafePostgresClient {
    inner: Option<postgres::Client>,
    broken: bool,
}

impl SafePostgresConnectionManager {
    fn new(inner: PostgresConnectionManager<MakeTlsConnector>) -> Self {
        Self { inner }
    }
}

impl Deref for SafePostgresClient {
    type Target = postgres::Client;

    fn deref(&self) -> &Self::Target {
        self.inner
            .as_ref()
            .expect("safe postgres client missing inner client")
    }
}

impl DerefMut for SafePostgresClient {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner
            .as_mut()
            .expect("safe postgres client missing inner client")
    }
}

impl SafePostgresClient {
    pub(crate) fn mark_broken(&mut self) {
        self.broken = true;
    }
}

impl Drop for SafePostgresClient {
    fn drop(&mut self) {
        let Some(client) = self.inner.take() else {
            return;
        };

        if Handle::try_current().is_ok() {
            let _ = std::thread::Builder::new()
                .name("bluey-postgres-client-drop".to_string())
                .spawn(move || drop(client))
                .and_then(|handle| {
                    handle
                        .join()
                        .map_err(|_| std::io::Error::other("postgres client drop panicked"))
                });
        } else {
            drop(client);
        }
    }
}

impl ManageConnection for SafePostgresConnectionManager {
    type Connection = SafePostgresClient;
    type Error = postgres::Error;

    fn connect(&self) -> Result<Self::Connection, Self::Error> {
        self.inner.connect().map(|client| SafePostgresClient {
            inner: Some(client),
            broken: false,
        })
    }

    fn is_valid(&self, client: &mut Self::Connection) -> Result<(), Self::Error> {
        self.inner.is_valid(client)
    }

    fn has_broken(&self, client: &mut Self::Connection) -> bool {
        client.broken || self.inner.has_broken(client)
    }
}

#[derive(Clone)]
pub enum DbPool {
    Sqlite(SqliteDbPool),
    Postgres(PostgresPools),
}

impl DbPool {
    pub fn backend_name(&self) -> &'static str {
        match self {
            Self::Sqlite(_) => "sqlite",
            Self::Postgres(_) => "postgres",
        }
    }

    /// Existing SQLite adapter callers use this while they are being moved.
    /// In Postgres mode, reaching this method is an adapter coverage bug, not
    /// a silent fallback to local SQLite.
    pub fn get(&self) -> Result<SqliteDbConn> {
        match self {
            Self::Sqlite(pool) => pool.get().context("get sqlite conn"),
            Self::Postgres(_) => anyhow::bail!(
                "server DB adapter path still uses SQLite while BLUEY_SERVER_DB_BACKEND=postgres"
            ),
        }
    }

    pub fn get_pg(&self) -> Result<PostgresDbConn> {
        match self {
            Self::Postgres(pools) => {
                if in_tokio_multithread_runtime() && !in_db_blocking_context() {
                    anyhow::bail!(
                        "Postgres DB access must run inside db::run_blocking_db while on the Tokio runtime"
                    );
                }
                pools.primary.get().context("get postgres conn")
            }
            Self::Sqlite(_) => anyhow::bail!("postgres connection requested from sqlite backend"),
        }
    }
}

thread_local! {
    static DB_BLOCKING_CONTEXT_DEPTH: Cell<usize> = const { Cell::new(0) };
}

fn in_db_blocking_context() -> bool {
    DB_BLOCKING_CONTEXT_DEPTH.with(|depth| depth.get() > 0)
}

fn enter_db_blocking_context<R>(f: impl FnOnce() -> R) -> R {
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            DB_BLOCKING_CONTEXT_DEPTH.with(|depth| {
                depth.set(depth.get().saturating_sub(1));
            });
        }
    }

    DB_BLOCKING_CONTEXT_DEPTH.with(|depth| depth.set(depth.get() + 1));
    let _guard = Guard;
    f()
}

fn in_tokio_multithread_runtime() -> bool {
    Handle::try_current()
        .map(|handle| handle.runtime_flavor() == RuntimeFlavor::MultiThread)
        .unwrap_or(false)
}

/// Run a synchronous DB operation behind Tokio's blocking boundary.
///
/// The server DB adapter is intentionally still sync-shaped while SQLite and
/// Postgres parity stabilizes. Postgres calls must enter this helper before
/// touching `postgres::Client`; `DbPool::get_pg` enforces that at runtime so a
/// missed path fails loudly instead of blocking async worker threads.
pub fn run_blocking_db<R>(f: impl FnOnce() -> R) -> R {
    if in_db_blocking_context() {
        return f();
    }
    if in_tokio_multithread_runtime() {
        tokio::task::block_in_place(|| enter_db_blocking_context(f))
    } else {
        enter_db_blocking_context(f)
    }
}

/// Open or create the SQLite DB. Enables WAL + foreign keys.
pub fn open_pool(path: &Path) -> Result<DbPool> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let manager = SqliteConnectionManager::file(path).with_init(|conn| {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             PRAGMA synchronous = NORMAL;
             PRAGMA busy_timeout = 5000;",
        )
    });
    let pool = Pool::builder()
        .max_size(8)
        .build(manager)
        .context("build pool")?;
    Ok(DbPool::Sqlite(pool))
}

pub fn open_postgres_pool(database_url: &str) -> Result<DbPool> {
    let pg_config = database_url
        .parse::<postgres::Config>()
        .context("parse BLUEY_DATABASE_URL")?;
    let mut tls_builder = TlsConnector::builder();
    if let Ok(ca_cert_path) = std::env::var("BLUEY_POSTGRES_CA_CERT_PATH") {
        let ca_bytes = std::fs::read(&ca_cert_path)
            .with_context(|| format!("read BLUEY_POSTGRES_CA_CERT_PATH {ca_cert_path}"))?;
        let ca_cert = Certificate::from_pem(&ca_bytes)
            .or_else(|_| Certificate::from_der(&ca_bytes))
            .with_context(|| format!("parse postgres CA certificate {ca_cert_path}"))?;
        tls_builder.add_root_certificate(ca_cert);
    }
    if std::env::var("BLUEY_POSTGRES_ACCEPT_INVALID_CERTS")
        .map(|value| value == "1")
        .unwrap_or(false)
    {
        tls_builder.danger_accept_invalid_certs(true);
    }
    if std::env::var("BLUEY_POSTGRES_ACCEPT_INVALID_HOSTNAMES")
        .map(|value| value == "1")
        .unwrap_or(false)
    {
        tls_builder.danger_accept_invalid_hostnames(true);
    }
    let tls = tls_builder
        .build()
        .context("build postgres TLS connector")?;
    let primary_manager = SafePostgresConnectionManager::new(PostgresConnectionManager::new(
        pg_config.clone(),
        MakeTlsConnector::new(tls.clone()),
    ));
    let lifecycle_manager = SafePostgresConnectionManager::new(PostgresConnectionManager::new(
        pg_config,
        MakeTlsConnector::new(tls),
    ));
    let primary = Pool::builder()
        .max_size(POSTGRES_PRIMARY_POOL_SIZE)
        .build(primary_manager)
        .context("build postgres pool")?;
    let lifecycle = Pool::builder()
        .max_size(POSTGRES_LIFECYCLE_POOL_SIZE)
        .min_idle(Some(0))
        .build(lifecycle_manager)
        .context("build postgres lifecycle pool")?;
    Ok(DbPool::Postgres(PostgresPools {
        primary,
        lifecycle,
        lifecycle_slots: Arc::new(Semaphore::new(POSTGRES_LIFECYCLE_POOL_SIZE as usize)),
    }))
}

/// Migrations, run in order. Each one is idempotent (CREATE TABLE IF NOT
/// EXISTS, etc.) so safe to re-run on every startup.
const SQLITE_JOBS_GLOBAL_CANDIDATE_INDEX: &str =
    include_str!("../../../infra/sqlite/server-runtime/035_jobs_global_candidate_index.sql");
const SQLITE_JOBS_RESUME_SOURCE_ASSETS: &str =
    include_str!("../../../infra/sqlite/server-runtime/036_jobs_resume_source_assets.sql");
const SQLITE_JOBS_GLOBAL_INGESTION_QUARANTINE: &str =
    include_str!("../../../infra/sqlite/server-runtime/039_jobs_global_ingestion_quarantine.sql");
const SQLITE_JOBS_AUTO_SUBMIT_AUTHORIZATIONS: &str =
    include_str!("../../../infra/sqlite/server-runtime/040_jobs_auto_submit_authorizations.sql");
const SQLITE_JOBS_COMMUNICATION_ACTIONS: &str =
    include_str!("../../../infra/sqlite/server-runtime/041_jobs_communication_actions.sql");
const SQLITE_JOBS_BROWSER_PROFILE_SNAPSHOTS: &str =
    include_str!("../../../infra/sqlite/server-runtime/042_jobs_browser_profile_snapshots.sql");
const SQLITE_ACCOUNT_DELETION_INTENTS: &str =
    include_str!("../../../infra/sqlite/server-runtime/043_account_deletion_intents.sql");
const SQLITE_JOBS_SUBMISSION_EVIDENCE_RESERVATIONS: &str = include_str!(
    "../../../infra/sqlite/server-runtime/044_jobs_submission_evidence_reservations.sql"
);
const SQLITE_JOBS_ACCOUNT_OBJECT_UPLOAD_BACKFILL: &str = include_str!(
    "../../../infra/sqlite/server-runtime/045_jobs_account_object_upload_backfill.sql"
);
const SQLITE_JOBS_RUNNER_VOLUME_PURGE: &str =
    include_str!("../../../infra/sqlite/server-runtime/046_jobs_runner_volume_purge.sql");
const SQLITE_JOBS_BROWSER_RELEASE_AUTHORITY: &str =
    include_str!("../../../infra/sqlite/server-runtime/047_jobs_browser_release_authority.sql");

const MIGRATIONS: &[&str] = &[
    // 0001 — accounts: identity + auth + balance
    r#"
    CREATE TABLE IF NOT EXISTS accounts (
        id                          TEXT PRIMARY KEY,                  -- uuid
        email                       TEXT NOT NULL UNIQUE,
        password_hash               TEXT NOT NULL,                     -- bcrypt
        email_verified_at           DATETIME,
        created_at                  DATETIME NOT NULL DEFAULT (datetime('now')),
        last_login_at               DATETIME,
        balance_cents               INTEGER NOT NULL DEFAULT 0,
        trial_seconds_remaining     INTEGER NOT NULL DEFAULT 900,      -- 15 min free trial
        is_temporary                INTEGER NOT NULL DEFAULT 0,
        temporary_expires_at        DATETIME,
        auto_topup_enabled          INTEGER NOT NULL DEFAULT 0,
        auto_topup_threshold_cents  INTEGER NOT NULL DEFAULT 500,      -- $5
        auto_topup_amount_cents     INTEGER NOT NULL DEFAULT 1500,     -- $15
        stripe_customer_id          TEXT,
        stripe_payment_method_id    TEXT,
        is_admin                    INTEGER NOT NULL DEFAULT 0
    );
    CREATE INDEX IF NOT EXISTS idx_accounts_email ON accounts(email);
    "#,
    // 0002 — credit_batches: per-reload tracking for 1-year FIFO expiry
    r#"
    CREATE TABLE IF NOT EXISTS credit_batches (
        id                  TEXT PRIMARY KEY,
        account_id          TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        amount_cents        INTEGER NOT NULL,
        remaining_cents     INTEGER NOT NULL,
        purchased_at        DATETIME NOT NULL DEFAULT (datetime('now')),
        expires_at          DATETIME NOT NULL,
        stripe_charge_id    TEXT,
        expired_at          DATETIME
    );
    CREATE INDEX IF NOT EXISTS idx_credit_batches_account ON credit_batches(account_id, expires_at);
    "#,
    // 0003 — refresh_tokens: long-lived auth state
    r#"
    CREATE TABLE IF NOT EXISTS refresh_tokens (
        token_hash       TEXT PRIMARY KEY,             -- sha256 of the token
        account_id       TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        device_label     TEXT,
        device_id        TEXT,
        created_at       DATETIME NOT NULL DEFAULT (datetime('now')),
        last_used_at     DATETIME,
        expires_at       DATETIME NOT NULL,
        revoked_at       DATETIME
    );
    CREATE INDEX IF NOT EXISTS idx_refresh_tokens_account ON refresh_tokens(account_id);
    "#,
    // 0004 — device_codes: OAuth-style device flow opened by first `bluey on`
    r#"
    CREATE TABLE IF NOT EXISTS device_codes (
        device_code      TEXT PRIMARY KEY,
        user_code        TEXT NOT NULL UNIQUE,
        account_id       TEXT REFERENCES accounts(id) ON DELETE SET NULL,
        approved         INTEGER NOT NULL DEFAULT 0,
        device_id        TEXT,
        device_name      TEXT,
        platform         TEXT,
        arch             TEXT,
        app_version      TEXT,
        created_at       DATETIME NOT NULL DEFAULT (datetime('now')),
        expires_at       DATETIME NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_device_codes_user_code ON device_codes(user_code);
    "#,
    // 0005 — usage_events: per-request metering for billing + analytics
    r#"
    CREATE TABLE IF NOT EXISTS usage_events (
        id                    TEXT PRIMARY KEY,
        account_id            TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        request_id            TEXT NOT NULL,
        ts                    DATETIME NOT NULL DEFAULT (datetime('now')),
        origin                TEXT NOT NULL DEFAULT 'legacy_unverified',
        kind                  TEXT NOT NULL,            -- llm | embed | stt | vision
        task_type             TEXT,                      -- general | code | system_design | meeting | writing | vision
        lane                  TEXT,                      -- instant | balanced | deep | vision
        provider              TEXT,                      -- openai | anthropic | deepgram
        model                 TEXT,
        input_tokens          INTEGER NOT NULL DEFAULT 0,
        output_tokens         INTEGER NOT NULL DEFAULT 0,
        latency_ms            INTEGER NOT NULL DEFAULT 0,
        cost_cents_to_bluey   INTEGER NOT NULL DEFAULT 0,
        cost_cents_to_customer INTEGER NOT NULL DEFAULT 0,
        was_speculative       INTEGER NOT NULL DEFAULT 0,
        was_fallback          INTEGER NOT NULL DEFAULT 0,
        CHECK (origin IN ('server', 'client', 'legacy_unverified'))
    );
    CREATE INDEX IF NOT EXISTS idx_usage_events_account_ts ON usage_events(account_id, ts);
    CREATE INDEX IF NOT EXISTS idx_usage_events_request ON usage_events(request_id);
    "#,
    // 0006 — stripe_webhook_events: idempotency for webhook handling
    r#"
    CREATE TABLE IF NOT EXISTS stripe_webhook_events (
        event_id      TEXT PRIMARY KEY,                  -- Stripe's evt_*
        type          TEXT NOT NULL,
        received_at   DATETIME NOT NULL DEFAULT (datetime('now')),
        processed_at  DATETIME,
        body          TEXT NOT NULL                     -- raw JSON for audit
    );
    "#,
    // 0007 — request_idempotency: dedupe /router/complete retries.
    //
    // Stage 4 codex blocker S4.1: clients can retry after a timeout/lost
    // response and double-charge. We require a client-supplied
    // request_id and reserve (account_id, request_id) at entry. A retry
    // with the same id either returns the cached terminal response or
    // 409 Conflict if the original request is still in-flight.
    r#"
    CREATE TABLE IF NOT EXISTS request_idempotency (
        account_id    TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        request_id    TEXT NOT NULL,
        status        TEXT NOT NULL,            -- in_progress | complete | failed
        response_json TEXT,                      -- cached CompleteResponse for retries
        http_status   INTEGER,                   -- cached HTTP status
        created_at    DATETIME NOT NULL DEFAULT (datetime('now')),
        completed_at  DATETIME,
        PRIMARY KEY (account_id, request_id)
    );
    CREATE INDEX IF NOT EXISTS idx_request_idempotency_created
        ON request_idempotency(created_at);
    "#,
    // 0008 — unique index on stripe_charge_id for idempotent credit.
    // Codex Stage 6 S6.1: prevents duplicate-charge double-credit even
    // if webhook fires twice for the same payment.
    r#"
    CREATE UNIQUE INDEX IF NOT EXISTS idx_credit_batches_stripe_charge
        ON credit_batches(stripe_charge_id)
        WHERE stripe_charge_id IS NOT NULL;
    "#,
    // 0009 — usage_events idempotency. Codex Stage 7 S7.1.
    //
    // Daemon retries of /usage/event with the same request_id would
    // otherwise double-count cues + spend. UNIQUE constraint enforced
    // via partial index because we want to allow multiple kinds (llm,
    // embed, stt, vision) per request_id where they semantically
    // represent different physical events on the same logical request.
    //
    // INSERT OR IGNORE in usage::record handles the dedupe path
    // gracefully.
    r#"
    CREATE UNIQUE INDEX IF NOT EXISTS idx_usage_events_dedupe
        ON usage_events(account_id, request_id, kind);
    "#,
    // 0010 — email verification + password reset tokens (Stage 13).
    // Both tables follow the same shape as refresh_tokens: sha256-hashed
    // at rest, single-use, 24-hour expiry. consume() rotates the row.
    r#"
    CREATE TABLE IF NOT EXISTS email_verification_tokens (
        token_hash    TEXT PRIMARY KEY,
        account_id    TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        created_at    DATETIME NOT NULL DEFAULT (datetime('now')),
        expires_at    DATETIME NOT NULL,
        consumed_at   DATETIME
    );
    CREATE INDEX IF NOT EXISTS idx_email_verify_account ON email_verification_tokens(account_id);

    CREATE TABLE IF NOT EXISTS password_reset_tokens (
        token_hash    TEXT PRIMARY KEY,
        account_id    TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        created_at    DATETIME NOT NULL DEFAULT (datetime('now')),
        expires_at    DATETIME NOT NULL,
        consumed_at   DATETIME
    );
    CREATE INDEX IF NOT EXISTS idx_password_reset_account ON password_reset_tokens(account_id);
    "#,
    // 0011 — auth_link_codes: one-time codes for browser→app deep-link
    // handoff (Onboarding Option A). Codex Stage 18.
    //
    // Stores access+refresh tokens at rest (sha256-hashed PK).
    // 5-minute expiry. Single-use via atomic UPDATE...RETURNING.
    r#"
    CREATE TABLE IF NOT EXISTS auth_link_codes (
        code_hash      TEXT PRIMARY KEY,
        account_id     TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        access_token   TEXT NOT NULL,
        refresh_token  TEXT NOT NULL,
        created_at     DATETIME NOT NULL DEFAULT (datetime('now')),
        expires_at     DATETIME NOT NULL,
        consumed_at    DATETIME
    );
    CREATE INDEX IF NOT EXISTS idx_auth_link_codes_account
        ON auth_link_codes(account_id);
    "#,
    // 0012 — cloud session sync + RAG foundation.
    //
    // SQLite keeps the dev/alpha server simple. The schema intentionally
    // mirrors a future Postgres + pgvector layout: tenant key first,
    // stable client ids, JSON metadata, and embedding vectors stored as a
    // serialized array until pgvector lands.
    r#"
    CREATE TABLE IF NOT EXISTS cloud_sessions (
        account_id          TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        session_id          TEXT NOT NULL,
        title               TEXT NOT NULL,
        status              TEXT NOT NULL DEFAULT 'active',
        created_at_ms       INTEGER NOT NULL,
        updated_at_ms       INTEGER NOT NULL,
        last_active_at_ms   INTEGER,
        answer_style        TEXT,
        metadata_json       TEXT NOT NULL DEFAULT '{}',
        deleted_at_ms       INTEGER,
        PRIMARY KEY (account_id, session_id)
    );
    CREATE INDEX IF NOT EXISTS idx_cloud_sessions_account_updated
        ON cloud_sessions(account_id, updated_at_ms DESC);

    CREATE TABLE IF NOT EXISTS cloud_transcript_segments (
        account_id          TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        segment_id          TEXT NOT NULL,
        session_id          TEXT NOT NULL,
        speaker             TEXT NOT NULL,
        source              TEXT NOT NULL,
        text                TEXT NOT NULL,
        start_ms            INTEGER,
        end_ms              INTEGER,
        ts_ms               INTEGER NOT NULL,
        is_final            INTEGER NOT NULL DEFAULT 1,
        metadata_json       TEXT NOT NULL DEFAULT '{}',
        PRIMARY KEY (account_id, segment_id)
    );
    CREATE INDEX IF NOT EXISTS idx_cloud_transcript_session_ts
        ON cloud_transcript_segments(account_id, session_id, ts_ms);

    CREATE TABLE IF NOT EXISTS cloud_cue_responses (
        account_id              TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        response_id             TEXT NOT NULL,
        session_id              TEXT NOT NULL,
        kind                    TEXT NOT NULL,
        text                    TEXT NOT NULL,
        source_text             TEXT,
        ts_ms                   INTEGER NOT NULL,
        provider                TEXT,
        model                   TEXT,
        lane                    TEXT,
        task_type               TEXT,
        cost_cents              INTEGER,
        balance_cents_after     INTEGER,
        cost_label              TEXT,
        artifact_type           TEXT,
        artifact_body           TEXT,
        artifact_confidence     REAL,
        metadata_json           TEXT NOT NULL DEFAULT '{}',
        PRIMARY KEY (account_id, response_id)
    );
    CREATE INDEX IF NOT EXISTS idx_cloud_responses_session_ts
        ON cloud_cue_responses(account_id, session_id, ts_ms);

    CREATE TABLE IF NOT EXISTS cloud_context_artifacts (
        account_id          TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        artifact_id         TEXT NOT NULL,
        session_id          TEXT NOT NULL,
        kind                TEXT NOT NULL,
        title               TEXT NOT NULL,
        note                TEXT,
        source_uri          TEXT,
        content_hash        TEXT,
        text_preview        TEXT,
        created_at_ms       INTEGER NOT NULL,
        updated_at_ms       INTEGER NOT NULL,
        metadata_json       TEXT NOT NULL DEFAULT '{}',
        PRIMARY KEY (account_id, artifact_id)
    );
    CREATE INDEX IF NOT EXISTS idx_cloud_context_session
        ON cloud_context_artifacts(account_id, session_id);

    CREATE TABLE IF NOT EXISTS cloud_rag_chunks (
        account_id          TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        chunk_id            TEXT NOT NULL,
        session_id          TEXT,
        source_kind         TEXT NOT NULL,
        source_id           TEXT NOT NULL,
        chunk_index         INTEGER NOT NULL,
        text                TEXT NOT NULL,
        embedding_json      TEXT,
        embedding_model     TEXT,
        token_count         INTEGER,
        content_hash        TEXT,
        updated_at_ms       INTEGER NOT NULL,
        metadata_json       TEXT NOT NULL DEFAULT '{}',
        PRIMARY KEY (account_id, chunk_id)
    );
    CREATE INDEX IF NOT EXISTS idx_cloud_rag_account_source
        ON cloud_rag_chunks(account_id, source_kind, source_id);
    CREATE INDEX IF NOT EXISTS idx_cloud_rag_session
        ON cloud_rag_chunks(account_id, session_id);

    CREATE TABLE IF NOT EXISTS cloud_child_tombstones (
        account_id          TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        child_kind         TEXT NOT NULL,
        child_id           TEXT NOT NULL,
        session_id         TEXT NOT NULL,
        deleted_at_ms      INTEGER NOT NULL,
        PRIMARY KEY (account_id, child_kind, child_id)
    );
    CREATE INDEX IF NOT EXISTS idx_cloud_child_tombstones_session
        ON cloud_child_tombstones(account_id, session_id, deleted_at_ms);

    CREATE TABLE IF NOT EXISTS stt_sessions (
        session_token       TEXT PRIMARY KEY,
        account_id          TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        bluey_session_id    TEXT NOT NULL,
        provider            TEXT NOT NULL,
        model               TEXT NOT NULL,
        source              TEXT NOT NULL,
        mode                TEXT NOT NULL,
        max_seconds         INTEGER NOT NULL,
        created_at_ms       INTEGER NOT NULL,
        expires_at_ms       INTEGER NOT NULL,
        consumed_seconds    INTEGER NOT NULL DEFAULT 0,
        started_at_ms       INTEGER,
        ended_at_ms         INTEGER,
        relay_close_reason  TEXT
    );
    CREATE INDEX IF NOT EXISTS idx_stt_sessions_account_exp
        ON stt_sessions(account_id, expires_at_ms);
    "#,
    // 0013 — pending signup OTPs.
    //
    // Create-account now verifies email ownership before issuing account
    // tokens. The pending row stores a bcrypt password hash and a server-keyed
    // OTP hash for up to 10 minutes; successful confirmation creates the
    // account and deletes the pending row.
    r#"
    CREATE TABLE IF NOT EXISTS signup_otps (
        email           TEXT PRIMARY KEY,
        otp_hash        TEXT NOT NULL,
        password_hash   TEXT NOT NULL,
        account_id      TEXT REFERENCES accounts(id) ON DELETE CASCADE,
        attempts        INTEGER NOT NULL DEFAULT 0,
        created_at      DATETIME NOT NULL DEFAULT (datetime('now')),
        expires_at      DATETIME NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_signup_otps_expires_at
        ON signup_otps(expires_at);
    "#,
    // 0014 — trial grants + abuse ledger.
    //
    // The ledger intentionally stores hashed signals only. It lets us block
    // repeated trial abuse and inspect suspicious patterns without retaining
    // raw IP addresses, device identifiers, or user agents.
    r#"
    CREATE TABLE IF NOT EXISTS trial_grants (
        id                    TEXT PRIMARY KEY,
        account_id            TEXT REFERENCES accounts(id) ON DELETE SET NULL,
        email_hash            TEXT NOT NULL,
        email_domain_hash     TEXT,
        ip_hash               TEXT,
        device_hash           TEXT,
        user_agent_hash       TEXT,
        ip_user_agent_hash    TEXT,
        granted_seconds       INTEGER NOT NULL DEFAULT 900,
        decision              TEXT NOT NULL,
        reason                TEXT,
        created_at            DATETIME NOT NULL DEFAULT (datetime('now'))
    );
    CREATE INDEX IF NOT EXISTS idx_trial_grants_email
        ON trial_grants(email_hash, created_at);
    CREATE INDEX IF NOT EXISTS idx_trial_grants_ip
        ON trial_grants(ip_hash, created_at);
    CREATE INDEX IF NOT EXISTS idx_trial_grants_device
        ON trial_grants(device_hash, created_at);
    CREATE INDEX IF NOT EXISTS idx_trial_grants_ip_ua
        ON trial_grants(ip_user_agent_hash, created_at);

    CREATE TABLE IF NOT EXISTS trial_abuse_events (
        id                    TEXT PRIMARY KEY,
        account_id            TEXT REFERENCES accounts(id) ON DELETE SET NULL,
        email_hash            TEXT,
        email_domain_hash     TEXT,
        ip_hash               TEXT,
        device_hash           TEXT,
        user_agent_hash       TEXT,
        ip_user_agent_hash    TEXT,
        event_type            TEXT NOT NULL,
        severity              INTEGER NOT NULL DEFAULT 1,
        reason                TEXT,
        created_at            DATETIME NOT NULL DEFAULT (datetime('now'))
    );
    CREATE INDEX IF NOT EXISTS idx_trial_abuse_events_created
        ON trial_abuse_events(created_at);
    CREATE INDEX IF NOT EXISTS idx_trial_abuse_events_email
        ON trial_abuse_events(email_hash, created_at);
    CREATE INDEX IF NOT EXISTS idx_trial_abuse_events_ip
        ON trial_abuse_events(ip_hash, created_at);
    CREATE INDEX IF NOT EXISTS idx_trial_abuse_events_device
        ON trial_abuse_events(device_hash, created_at);
    "#,
    // 0015 — balance movement ledger.
    //
    // credit_batches remains the spendable FIFO ledger, while this table is
    // the audit/evidence ledger: every balance_cents movement records the
    // before/after value and the processor/request evidence that caused it.
    r#"
    CREATE TABLE IF NOT EXISTS balance_ledger_entries (
        id                    TEXT PRIMARY KEY,
        account_id            TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        event_type            TEXT NOT NULL,
        amount_cents          INTEGER NOT NULL,
        balance_cents_before  INTEGER NOT NULL,
        balance_cents_after   INTEGER NOT NULL,
        reason                TEXT,
        provider              TEXT,
        processor_payment_id  TEXT,
        source_id             TEXT,
        idempotency_key       TEXT,
        request_id            TEXT,
        metadata_json         TEXT NOT NULL DEFAULT '{}',
        created_at            DATETIME NOT NULL DEFAULT (datetime('now'))
    );
    CREATE INDEX IF NOT EXISTS idx_balance_ledger_account_created
        ON balance_ledger_entries(account_id, created_at);
    CREATE INDEX IF NOT EXISTS idx_balance_ledger_provider_payment
        ON balance_ledger_entries(provider, processor_payment_id);
    CREATE INDEX IF NOT EXISTS idx_balance_ledger_request
        ON balance_ledger_entries(account_id, request_id);
    "#,
    // 0016 - redacted ops audit events.
    //
    // This table intentionally avoids an account foreign key so delete/export
    // evidence survives account hard-delete without retaining account data.
    r#"
    CREATE TABLE IF NOT EXISTS ops_audit_events (
        id                    TEXT PRIMARY KEY,
        account_id_hash       TEXT,
        actor_account_id_hash TEXT,
        event_type            TEXT NOT NULL,
        status                TEXT NOT NULL,
        metadata_json         TEXT NOT NULL DEFAULT '{}',
        created_at            DATETIME NOT NULL DEFAULT (datetime('now'))
    );
    CREATE INDEX IF NOT EXISTS idx_ops_audit_created
        ON ops_audit_events(created_at);
    CREATE INDEX IF NOT EXISTS idx_ops_audit_event_created
        ON ops_audit_events(event_type, created_at);
    CREATE INDEX IF NOT EXISTS idx_ops_audit_account_created
        ON ops_audit_events(account_id_hash, created_at);
    "#,
    // 0017 - diagnostic log chunk index.
    //
    // Operational logs and support diagnostics should live in bounded local
    // files or private R2 objects, not as raw log bodies in the database. This
    // table is the Postgres/SQLite index that lets support find the right
    // durable object by account/session/kind without exposing transcripts,
    // screenshots, document text, or prompts in ordinary support payloads.
    r#"
    CREATE TABLE IF NOT EXISTS diagnostic_log_chunks (
        id              TEXT PRIMARY KEY,
        account_id      TEXT REFERENCES accounts(id) ON DELETE CASCADE,
        workspace_id    TEXT,
        session_id      TEXT,
        session_code    TEXT,
        kind            TEXT NOT NULL,
        storage         TEXT NOT NULL DEFAULT 'local',
        object_key      TEXT,
        local_path      TEXT,
        bytes           INTEGER NOT NULL DEFAULT 0,
        sha256          TEXT,
        created_at_ms   INTEGER NOT NULL,
        expires_at_ms   INTEGER NOT NULL,
        metadata_json   TEXT NOT NULL DEFAULT '{}'
    );
    CREATE INDEX IF NOT EXISTS idx_diagnostic_log_chunks_account_created
        ON diagnostic_log_chunks(account_id, created_at_ms DESC);
    CREATE INDEX IF NOT EXISTS idx_diagnostic_log_chunks_session
        ON diagnostic_log_chunks(account_id, session_id, created_at_ms DESC);
    CREATE INDEX IF NOT EXISTS idx_diagnostic_log_chunks_expires
        ON diagnostic_log_chunks(expires_at_ms);
    CREATE INDEX IF NOT EXISTS idx_diagnostic_log_chunks_kind_created
        ON diagnostic_log_chunks(kind, created_at_ms DESC);
    "#,
    // 0018 - stable account desktop devices.
    r#"
    CREATE TABLE IF NOT EXISTS account_devices (
        id                    TEXT PRIMARY KEY,
        account_id            TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        device_id             TEXT NOT NULL,
        device_name           TEXT NOT NULL,
        platform              TEXT NOT NULL,
        arch                  TEXT,
        app_version           TEXT,
        registered_at         DATETIME NOT NULL DEFAULT (datetime('now')),
        last_seen_at          DATETIME,
        last_heartbeat_at     DATETIME,
        revoked_at            DATETIME,
        UNIQUE(account_id, device_id)
    );
    CREATE INDEX IF NOT EXISTS idx_account_devices_account
        ON account_devices(account_id, revoked_at);
    CREATE INDEX IF NOT EXISTS idx_account_devices_account_device
        ON account_devices(account_id, device_id);
    "#,
    // 0019 - legal acceptance ledger.
    //
    // Product flows already require Terms/Privacy consent before signup,
    // trial conversion, and Try Us access. This ledger records the account,
    // purpose, policy versions, and hashed request signals so support/admin
    // can prove consent without retaining raw IP, device, or user-agent data.
    r#"
    CREATE TABLE IF NOT EXISTS legal_acceptances (
        id                    TEXT PRIMARY KEY,
        account_id            TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        purpose               TEXT NOT NULL,
        terms_version         TEXT NOT NULL,
        privacy_version       TEXT NOT NULL,
        terms_text_hash       TEXT NOT NULL DEFAULT '',
        privacy_text_hash     TEXT NOT NULL DEFAULT '',
        email_hash            TEXT,
        ip_hash               TEXT,
        user_agent_hash       TEXT,
        device_hash           TEXT,
        ip_user_agent_hash    TEXT,
        metadata_json         TEXT NOT NULL DEFAULT '{}',
        retention_expires_at  DATETIME,
        accepted_at           DATETIME NOT NULL DEFAULT (datetime('now')),
        created_at            DATETIME NOT NULL DEFAULT (datetime('now')),
        UNIQUE(account_id, purpose, terms_version, privacy_version)
    );
    "#,
    // 0020 - durable object upload metadata, quota accounting, and outbox.
    r#"
    CREATE TABLE IF NOT EXISTS object_uploads (
        id                  TEXT PRIMARY KEY,
        account_id          TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        object_kind         TEXT NOT NULL CHECK(object_kind IN ('artifact', 'session_audit')),
        logical_id          TEXT NOT NULL,
        session_id          TEXT,
        storage_scope       TEXT NOT NULL CHECK(storage_scope IN ('artifact', 'audit')),
        object_key          TEXT NOT NULL,
        size_bytes          INTEGER NOT NULL CHECK(size_bytes > 0),
        sha256              TEXT NOT NULL CHECK(length(sha256) = 64),
        content_type        TEXT NOT NULL,
        expires_at_ms       INTEGER NOT NULL,
        state               TEXT NOT NULL DEFAULT 'pending'
            CHECK(state IN ('pending', 'ready', 'delete_pending', 'deleted')),
        metadata_json       TEXT NOT NULL DEFAULT '{}',
        created_at_ms       INTEGER NOT NULL,
        updated_at_ms       INTEGER NOT NULL,
        uploaded_at_ms      INTEGER,
        deleted_at_ms       INTEGER,
        UNIQUE(account_id, object_kind, logical_id),
        UNIQUE(storage_scope, object_key)
    );
    CREATE INDEX IF NOT EXISTS idx_object_uploads_account_state
        ON object_uploads(account_id, state, created_at_ms);
    CREATE INDEX IF NOT EXISTS idx_object_uploads_session
        ON object_uploads(account_id, session_id, state);
    CREATE INDEX IF NOT EXISTS idx_object_uploads_cleanup
        ON object_uploads(storage_scope, state, expires_at_ms, updated_at_ms);

    CREATE TABLE IF NOT EXISTS object_upload_daily_usage (
        account_id          TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        day_start_ms        INTEGER NOT NULL,
        reserved_bytes      INTEGER NOT NULL DEFAULT 0 CHECK(reserved_bytes >= 0),
        reserved_objects    INTEGER NOT NULL DEFAULT 0 CHECK(reserved_objects >= 0),
        updated_at_ms       INTEGER NOT NULL,
        PRIMARY KEY(account_id, day_start_ms)
    );

    CREATE TABLE IF NOT EXISTS object_storage_outbox (
        id                  TEXT PRIMARY KEY,
        upload_id           TEXT NOT NULL REFERENCES object_uploads(id) ON DELETE CASCADE,
        account_id          TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        operation           TEXT NOT NULL CHECK(operation IN ('put', 'delete')),
        state               TEXT NOT NULL DEFAULT 'pending'
            CHECK(state IN ('pending', 'processing', 'retry', 'completed', 'abandoned')),
        attempt_count       INTEGER NOT NULL DEFAULT 0 CHECK(attempt_count >= 0),
        next_attempt_at_ms  INTEGER NOT NULL,
        last_error          TEXT,
        created_at_ms       INTEGER NOT NULL,
        updated_at_ms       INTEGER NOT NULL,
        completed_at_ms     INTEGER,
        UNIQUE(upload_id, operation)
    );
    CREATE INDEX IF NOT EXISTS idx_object_storage_outbox_due
        ON object_storage_outbox(operation, state, next_attempt_at_ms, updated_at_ms);
    CREATE INDEX IF NOT EXISTS idx_object_storage_outbox_account
        ON object_storage_outbox(account_id, operation, state);
    "#,
    // 0021 - atomic managed-usage reservations.
    r#"
    CREATE TABLE IF NOT EXISTS usage_reservations (
        account_id                    TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        request_id                    TEXT NOT NULL,
        kind                          TEXT NOT NULL,
        status                        TEXT NOT NULL CHECK(status IN ('reserved', 'settled', 'released')),
        attempt                       INTEGER NOT NULL DEFAULT 1 CHECK(attempt > 0),
        estimated_customer_cents      INTEGER NOT NULL CHECK(estimated_customer_cents >= 0),
        estimated_upstream_cents      INTEGER NOT NULL CHECK(estimated_upstream_cents >= 0),
        reserved_cents                INTEGER NOT NULL DEFAULT 0 CHECK(reserved_cents >= 0),
        actual_customer_cents         INTEGER NOT NULL DEFAULT 0 CHECK(actual_customer_cents >= 0),
        settled_cents                 INTEGER NOT NULL DEFAULT 0 CHECK(settled_cents >= 0),
        refunded_cents                INTEGER NOT NULL DEFAULT 0 CHECK(refunded_cents >= 0),
        reserved_trial_seconds        INTEGER NOT NULL DEFAULT 0 CHECK(reserved_trial_seconds >= 0),
        settled_trial_seconds         INTEGER NOT NULL DEFAULT 0 CHECK(settled_trial_seconds >= 0),
        refunded_trial_seconds        INTEGER NOT NULL DEFAULT 0 CHECK(refunded_trial_seconds >= 0),
        created_at_ms                 INTEGER NOT NULL,
        expires_at_ms                 INTEGER NOT NULL,
        settled_at_ms                 INTEGER,
        balance_cents_after           INTEGER,
        trial_seconds_remaining_after INTEGER,
        reservation_reason            TEXT NOT NULL,
        terminal_reason               TEXT,
        PRIMARY KEY(account_id, request_id)
    );
    CREATE INDEX IF NOT EXISTS idx_usage_reservations_status_expiry
        ON usage_reservations(status, expires_at_ms);
    CREATE INDEX IF NOT EXISTS idx_usage_reservations_account_expiry
        ON usage_reservations(account_id, status, expires_at_ms);
    "#,
    // 0022 - durable Stripe Auto Reload attempts.
    //
    // PaymentIntents are created unconfirmed, attached to one of these rows,
    // and only then confirmed. The active-account index is the cross-process
    // guard against opening two chargeable attempts for one account.
    r#"
    CREATE TABLE IF NOT EXISTS stripe_auto_reload_attempts (
        id                          TEXT PRIMARY KEY,
        account_id                  TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        amount_cents                INTEGER NOT NULL,
        currency                    TEXT NOT NULL DEFAULT 'usd',
        stripe_customer_id          TEXT NOT NULL,
        stripe_payment_method_id    TEXT NOT NULL,
        stripe_payment_intent_id    TEXT,
        stripe_charge_id            TEXT,
        create_idempotency_key      TEXT NOT NULL UNIQUE,
        confirm_idempotency_key     TEXT NOT NULL UNIQUE,
        status                      TEXT NOT NULL,
        failure_code                TEXT,
        last_event_id               TEXT,
        created_at                  DATETIME NOT NULL DEFAULT (datetime('now')),
        updated_at                  DATETIME NOT NULL DEFAULT (datetime('now')),
        payment_intent_created_at   DATETIME,
        charged_at                  DATETIME,
        credited_at                 DATETIME,
        failed_at                   DATETIME,
        reversed_at                 DATETIME,
        last_reconciled_at          DATETIME,
        CHECK (amount_cents > 0),
        CHECK (currency = 'usd'),
        CHECK (status IN (
            'reserved', 'requires_confirmation', 'processing',
            'reconciliation_required', 'succeeded', 'failed',
            'canceled', 'reversed'
        ))
    );
    CREATE UNIQUE INDEX IF NOT EXISTS idx_stripe_auto_reload_payment_intent
        ON stripe_auto_reload_attempts(stripe_payment_intent_id)
        WHERE stripe_payment_intent_id IS NOT NULL;
    CREATE UNIQUE INDEX IF NOT EXISTS idx_stripe_auto_reload_charge
        ON stripe_auto_reload_attempts(stripe_charge_id)
        WHERE stripe_charge_id IS NOT NULL;
    CREATE UNIQUE INDEX IF NOT EXISTS idx_stripe_auto_reload_active_account
        ON stripe_auto_reload_attempts(account_id)
        WHERE status IN (
            'reserved', 'requires_confirmation', 'processing',
            'reconciliation_required'
        );
    CREATE INDEX IF NOT EXISTS idx_stripe_auto_reload_account_created
        ON stripe_auto_reload_attempts(account_id, created_at);
    "#,
    // 0023 - Bluey Jobs customer workspace.
    //
    // Jobs is intentionally isolated from the meeting/session runtime. The
    // tenant key is present on every row, packet generation is job-specific,
    // and packet metering is idempotent per canonical job.
    r#"
    CREATE TABLE IF NOT EXISTS jobs_profiles (
        account_id            TEXT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
        profile_json          TEXT NOT NULL DEFAULT '{}',
        onboarding_step       INTEGER NOT NULL DEFAULT 0,
        onboarding_complete   INTEGER NOT NULL DEFAULT 0,
        created_at_ms         INTEGER NOT NULL,
        updated_at_ms         INTEGER NOT NULL
    );

    CREATE TABLE IF NOT EXISTS jobs_facts (
        id                    TEXT PRIMARY KEY,
        account_id            TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        category              TEXT NOT NULL,
        label                 TEXT NOT NULL,
        value_json            TEXT NOT NULL,
        source                TEXT NOT NULL,
        verification_status   TEXT NOT NULL DEFAULT 'unverified',
        confirmed_at_ms       INTEGER,
        confirmed_by          TEXT,
        schema_version        INTEGER NOT NULL DEFAULT 1,
        created_at_ms         INTEGER NOT NULL,
        updated_at_ms         INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_jobs_facts_account_category
        ON jobs_facts(account_id, category, updated_at_ms DESC);

    CREATE TABLE IF NOT EXISTS jobs_preferences (
        account_id            TEXT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
        preferences_json      TEXT NOT NULL DEFAULT '{}',
        updated_at_ms         INTEGER NOT NULL
    );

    CREATE TABLE IF NOT EXISTS jobs_tracks (
        id                    TEXT PRIMARY KEY,
        account_id            TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        track_json            TEXT NOT NULL,
        active                INTEGER NOT NULL DEFAULT 1,
        created_at_ms         INTEGER NOT NULL,
        updated_at_ms         INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_jobs_tracks_account
        ON jobs_tracks(account_id, active, updated_at_ms DESC);

    CREATE TABLE IF NOT EXISTS jobs_postings (
        id                    TEXT PRIMARY KEY,
        account_id            TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        canonical_key         TEXT NOT NULL,
        posting_json          TEXT NOT NULL,
        source                TEXT NOT NULL,
        canonical_url         TEXT,
        company               TEXT NOT NULL,
        title                 TEXT NOT NULL,
        location              TEXT,
        match_score           INTEGER NOT NULL DEFAULT 0,
        status                TEXT NOT NULL DEFAULT 'matched',
        created_at_ms         INTEGER NOT NULL,
        updated_at_ms         INTEGER NOT NULL,
        UNIQUE(account_id, canonical_key)
    );
    CREATE INDEX IF NOT EXISTS idx_jobs_postings_account_score
        ON jobs_postings(account_id, status, match_score DESC, updated_at_ms DESC);

    CREATE TABLE IF NOT EXISTS jobs_resume_versions (
        id                    TEXT PRIMARY KEY,
        account_id            TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        job_id                TEXT NOT NULL,
        version_no            INTEGER NOT NULL,
        mode                  TEXT NOT NULL,
        content_json          TEXT NOT NULL,
        diff_json             TEXT NOT NULL DEFAULT '{}',
        claim_ids_json        TEXT NOT NULL DEFAULT '[]',
        checksum              TEXT NOT NULL,
        created_at_ms         INTEGER NOT NULL,
        UNIQUE(account_id, job_id, checksum),
        FOREIGN KEY (job_id) REFERENCES jobs_postings(id) ON DELETE CASCADE
    );
    CREATE INDEX IF NOT EXISTS idx_jobs_resume_versions_job
        ON jobs_resume_versions(account_id, job_id, version_no DESC);

    CREATE TABLE IF NOT EXISTS jobs_applications (
        id                    TEXT PRIMARY KEY,
        account_id            TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        job_id                TEXT NOT NULL,
        resume_version_id     TEXT,
        state                 TEXT NOT NULL DEFAULT 'matched',
        application_json      TEXT NOT NULL DEFAULT '{}',
        created_at_ms         INTEGER NOT NULL,
        updated_at_ms         INTEGER NOT NULL,
        submitted_at_ms       INTEGER,
        UNIQUE(account_id, job_id),
        FOREIGN KEY (job_id) REFERENCES jobs_postings(id) ON DELETE CASCADE,
        FOREIGN KEY (resume_version_id) REFERENCES jobs_resume_versions(id) ON DELETE SET NULL
    );
    CREATE INDEX IF NOT EXISTS idx_jobs_applications_account_state
        ON jobs_applications(account_id, state, updated_at_ms DESC);

    CREATE TABLE IF NOT EXISTS jobs_browser_sessions (
        id                    TEXT PRIMARY KEY,
        account_id            TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        runner                TEXT NOT NULL,
        status                TEXT NOT NULL,
        session_json          TEXT NOT NULL DEFAULT '{}',
        created_at_ms         INTEGER NOT NULL,
        updated_at_ms         INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_jobs_browser_sessions_account
        ON jobs_browser_sessions(account_id, updated_at_ms DESC);

    CREATE TABLE IF NOT EXISTS jobs_interventions (
        id                    TEXT PRIMARY KEY,
        account_id            TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        application_id        TEXT,
        kind                  TEXT NOT NULL,
        status                TEXT NOT NULL DEFAULT 'open',
        intervention_json     TEXT NOT NULL DEFAULT '{}',
        created_at_ms         INTEGER NOT NULL,
        resolved_at_ms        INTEGER,
        FOREIGN KEY (application_id) REFERENCES jobs_applications(id) ON DELETE CASCADE
    );
    CREATE INDEX IF NOT EXISTS idx_jobs_interventions_account_status
        ON jobs_interventions(account_id, status, created_at_ms DESC);

    CREATE TABLE IF NOT EXISTS jobs_answer_memory (
        id                    TEXT PRIMARY KEY,
        account_id            TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        scope                 TEXT NOT NULL,
        scope_id              TEXT NOT NULL DEFAULT '',
        question_hash         TEXT NOT NULL,
        answer_json           TEXT NOT NULL DEFAULT '{}',
        created_at_ms         INTEGER NOT NULL,
        updated_at_ms         INTEGER NOT NULL,
        UNIQUE(account_id, scope, scope_id, question_hash)
    );
    CREATE INDEX IF NOT EXISTS idx_jobs_answer_memory_account
        ON jobs_answer_memory(account_id, updated_at_ms DESC);

    CREATE TABLE IF NOT EXISTS jobs_integrations (
        id                    TEXT PRIMARY KEY,
        account_id            TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        provider              TEXT NOT NULL,
        status                TEXT NOT NULL DEFAULT 'disconnected',
        integration_json      TEXT NOT NULL DEFAULT '{}',
        updated_at_ms         INTEGER NOT NULL,
        UNIQUE(account_id, provider)
    );

    CREATE TABLE IF NOT EXISTS jobs_entitlements (
        account_id            TEXT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
        plan                  TEXT NOT NULL DEFAULT 'free',
        track_limit           INTEGER NOT NULL DEFAULT 1,
        monthly_packet_limit  INTEGER NOT NULL DEFAULT 5,
        used_packets          INTEGER NOT NULL DEFAULT 0,
        period_start_ms       INTEGER NOT NULL,
        period_end_ms         INTEGER NOT NULL,
        local_browser         INTEGER NOT NULL DEFAULT 0,
        cloud_browser         INTEGER NOT NULL DEFAULT 0,
        updated_at_ms         INTEGER NOT NULL
    );

    CREATE TABLE IF NOT EXISTS jobs_run_events (
        id                    TEXT PRIMARY KEY,
        account_id            TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        run_id                TEXT NOT NULL,
        event_type            TEXT NOT NULL,
        event_json            TEXT NOT NULL DEFAULT '{}',
        created_at_ms         INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_jobs_run_events_account_run
        ON jobs_run_events(account_id, run_id, created_at_ms ASC);

    CREATE TABLE IF NOT EXISTS jobs_packet_metering (
        account_id            TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        job_id                TEXT NOT NULL,
        application_id        TEXT NOT NULL,
        metering_key          TEXT NOT NULL,
        included              INTEGER NOT NULL,
        amount_cents          INTEGER NOT NULL DEFAULT 0,
        created_at_ms         INTEGER NOT NULL,
        PRIMARY KEY (account_id, job_id),
        UNIQUE(account_id, metering_key),
        FOREIGN KEY (job_id) REFERENCES jobs_postings(id) ON DELETE CASCADE,
        FOREIGN KEY (application_id) REFERENCES jobs_applications(id) ON DELETE CASCADE
    );
    "#,
    // 0024 - Bluey Jobs application identities and multi-inbox connections.
    //
    // Login identity remains in accounts. Application addresses and provider
    // mailboxes are independently tenant-scoped, encrypted payloads.
    r#"
    CREATE TABLE IF NOT EXISTS jobs_application_identities (
        id                    TEXT PRIMARY KEY,
        account_id            TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        email_hash            TEXT NOT NULL UNIQUE,
        identity_json         TEXT NOT NULL,
        verification_status   TEXT NOT NULL DEFAULT 'pending',
        is_default            INTEGER NOT NULL DEFAULT 0,
        created_at_ms         INTEGER NOT NULL,
        updated_at_ms         INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_jobs_application_identities_account
        ON jobs_application_identities(account_id, is_default DESC, updated_at_ms DESC);

    CREATE TABLE IF NOT EXISTS jobs_identity_verifications (
        identity_id           TEXT PRIMARY KEY REFERENCES jobs_application_identities(id) ON DELETE CASCADE,
        account_id            TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        otp_hash              TEXT NOT NULL,
        attempts              INTEGER NOT NULL DEFAULT 0,
        expires_at_ms         INTEGER NOT NULL,
        created_at_ms         INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_jobs_identity_verifications_expiry
        ON jobs_identity_verifications(expires_at_ms);

    CREATE TABLE IF NOT EXISTS jobs_mailbox_connections (
        id                    TEXT PRIMARY KEY,
        account_id            TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        provider              TEXT NOT NULL,
        provider_subject_hash TEXT NOT NULL,
        status                TEXT NOT NULL DEFAULT 'pending',
        connection_json       TEXT NOT NULL,
        created_at_ms         INTEGER NOT NULL,
        updated_at_ms         INTEGER NOT NULL,
        UNIQUE(account_id, provider, provider_subject_hash)
    );
    CREATE INDEX IF NOT EXISTS idx_jobs_mailbox_connections_account
        ON jobs_mailbox_connections(account_id, status, updated_at_ms DESC);
    "#,
    // 0025 - immutable evidence attached to each Jobs application.
    //
    // Resume artifacts, submission confirmations, provider email events, and
    // interview calendar events share one tenant-scoped, idempotent ledger.
    r#"
    CREATE TABLE IF NOT EXISTS jobs_application_evidence (
        id                    TEXT PRIMARY KEY,
        account_id            TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        application_id        TEXT NOT NULL REFERENCES jobs_applications(id) ON DELETE CASCADE,
        kind                  TEXT NOT NULL,
        provider_event_hash   TEXT NOT NULL,
        evidence_json         TEXT NOT NULL,
        occurred_at_ms        INTEGER NOT NULL,
        created_at_ms         INTEGER NOT NULL,
        UNIQUE(account_id, provider_event_hash)
    );
    CREATE INDEX IF NOT EXISTS idx_jobs_application_evidence_application
        ON jobs_application_evidence(account_id, application_id, occurred_at_ms DESC);
    "#,
    // 0026 - capability-scoped local Bluey Browser launches.
    //
    // The website hands the desktop app a short-lived random ticket instead
    // of putting a reusable Bluey access token or application packet in a
    // custom-protocol URL. Packet and ticket secrets remain encrypted at rest.
    r#"
    CREATE TABLE IF NOT EXISTS jobs_local_run_tickets (
        id                    TEXT PRIMARY KEY,
        account_id            TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        application_id        TEXT NOT NULL REFERENCES jobs_applications(id) ON DELETE CASCADE,
        ticket_hash           TEXT NOT NULL UNIQUE,
        ticket_secret         TEXT NOT NULL,
        payload_json          TEXT NOT NULL,
        status                TEXT NOT NULL DEFAULT 'queued',
        expires_at_ms         INTEGER NOT NULL,
        created_at_ms         INTEGER NOT NULL,
        updated_at_ms         INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_jobs_local_run_tickets_expiry
        ON jobs_local_run_tickets(expires_at_ms, status);
    CREATE INDEX IF NOT EXISTS idx_jobs_local_run_tickets_account
        ON jobs_local_run_tickets(account_id, updated_at_ms DESC);
    "#,
    // 0027 - atomic Bluey Jobs application-attempt reservations.
    //
    // Packet preparation and metering are intentionally separate from an
    // employer-facing attempt. A reservation is created transactionally when
    // an application enters the browser queue.
    r#"
    CREATE TABLE IF NOT EXISTS jobs_attempt_reservations (
        id                    TEXT PRIMARY KEY,
        account_id            TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        application_id        TEXT NOT NULL REFERENCES jobs_applications(id) ON DELETE CASCADE,
        company_key           TEXT NOT NULL,
        period_key            TEXT NOT NULL,
        runner                TEXT NOT NULL DEFAULT 'unassigned',
        status                TEXT NOT NULL DEFAULT 'reserved',
        reserved_at_ms        INTEGER NOT NULL,
        updated_at_ms         INTEGER NOT NULL,
        UNIQUE(account_id, application_id)
    );
    CREATE INDEX IF NOT EXISTS idx_jobs_attempt_reservations_period
        ON jobs_attempt_reservations(account_id, period_key, status, reserved_at_ms);
    CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_attempt_reservations_active_company
        ON jobs_attempt_reservations(account_id, company_key)
        WHERE status IN ('reserved', 'running', 'side_effect_unknown', 'submitted');
    "#,
    // 0028 - durable Bluey Jobs discovery schedules, leases, health, and source membership.
    //
    // Discovery workers claim due sources with a short-lived lease. Successful
    // snapshots are replay-safe and keep enough membership state to close jobs
    // that disappear from a complete provider feed without trusting browser input.
    r#"
    CREATE TABLE IF NOT EXISTS jobs_discovery_sources (
        id                    TEXT PRIMARY KEY,
        account_id            TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        track_id              TEXT NOT NULL DEFAULT '',
        provider              TEXT NOT NULL,
        source_key            TEXT NOT NULL,
        source_json           TEXT NOT NULL,
        status                TEXT NOT NULL DEFAULT 'active',
        health                TEXT NOT NULL DEFAULT 'waiting',
        consecutive_failures  INTEGER NOT NULL DEFAULT 0,
        run_interval_ms       INTEGER NOT NULL DEFAULT 900000,
        next_run_at_ms        INTEGER NOT NULL,
        last_success_at_ms    INTEGER,
        last_failure_at_ms    INTEGER,
        last_error_code       TEXT,
        lease_owner           TEXT,
        lease_token           TEXT,
        lease_expires_at_ms   INTEGER,
        created_at_ms         INTEGER NOT NULL,
        updated_at_ms         INTEGER NOT NULL,
        UNIQUE(account_id, provider, source_key, track_id)
    );
    CREATE INDEX IF NOT EXISTS idx_jobs_discovery_sources_due
        ON jobs_discovery_sources(status, health, next_run_at_ms, lease_expires_at_ms);

    CREATE TABLE IF NOT EXISTS jobs_discovery_runs (
        id                    TEXT PRIMARY KEY,
        account_id            TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        source_id             TEXT NOT NULL REFERENCES jobs_discovery_sources(id) ON DELETE CASCADE,
        replay_key            TEXT NOT NULL,
        status                TEXT NOT NULL DEFAULT 'running',
        discovered_count      INTEGER NOT NULL DEFAULT 0,
        upserted_count        INTEGER NOT NULL DEFAULT 0,
        closed_count          INTEGER NOT NULL DEFAULT 0,
        error_code            TEXT,
        snapshot_hash         TEXT,
        started_at_ms         INTEGER NOT NULL,
        completed_at_ms       INTEGER,
        UNIQUE(source_id, replay_key)
    );
    CREATE INDEX IF NOT EXISTS idx_jobs_discovery_runs_source
        ON jobs_discovery_runs(source_id, started_at_ms DESC);

    CREATE TABLE IF NOT EXISTS jobs_discovery_memberships (
        source_id             TEXT NOT NULL REFERENCES jobs_discovery_sources(id) ON DELETE CASCADE,
        account_id            TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        external_id           TEXT NOT NULL,
        canonical_key         TEXT NOT NULL,
        job_id                TEXT NOT NULL REFERENCES jobs_postings(id) ON DELETE CASCADE,
        content_hash          TEXT NOT NULL,
        first_seen_at_ms      INTEGER NOT NULL,
        last_seen_at_ms       INTEGER NOT NULL,
        last_seen_run_id      TEXT NOT NULL,
        availability_status   TEXT NOT NULL DEFAULT 'active',
        missing_count         INTEGER NOT NULL DEFAULT 0,
        missing_since_at_ms   INTEGER,
        PRIMARY KEY(source_id, external_id)
    );
    CREATE INDEX IF NOT EXISTS idx_jobs_discovery_memberships_job
        ON jobs_discovery_memberships(account_id, job_id);
    "#,
    // 0029 - durable, fenced execution leases for cloud browser runs.
    //
    // Only prepared leases may expire or rotate. The partial unique indexes
    // make browser-profile and application ownership authoritative in the DB
    // across runner replicas; click_started remains active until reconciled.
    r#"
    CREATE TABLE IF NOT EXISTS jobs_execution_leases (
        run_id                TEXT PRIMARY KEY,
        account_id            TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        application_id        TEXT NOT NULL REFERENCES jobs_applications(id) ON DELETE CASCADE,
        browser_profile_id    TEXT NOT NULL,
        owner_id              TEXT NOT NULL,
        lease_token_sha256    TEXT NOT NULL,
        fence                 INTEGER NOT NULL CHECK(fence > 0),
        phase                 TEXT NOT NULL CHECK(phase IN (
            'prepared', 'click_started', 'submitted', 'failed',
            'side_effect_unknown', 'released'
        )),
        lease_expires_at_ms   INTEGER NOT NULL,
        created_at_ms         INTEGER NOT NULL,
        updated_at_ms         INTEGER NOT NULL,
        finished_at_ms        INTEGER
    );
    CREATE INDEX IF NOT EXISTS idx_jobs_execution_leases_binding
        ON jobs_execution_leases(account_id, application_id, run_id);
    CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_execution_leases_active_application
        ON jobs_execution_leases(application_id)
        WHERE phase IN ('prepared', 'click_started');
    CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_execution_leases_active_profile
        ON jobs_execution_leases(browser_profile_id)
        WHERE phase IN ('prepared', 'click_started');

    CREATE TABLE IF NOT EXISTS jobs_local_run_resume_actions (
        id                    TEXT PRIMARY KEY,
        run_id                TEXT NOT NULL REFERENCES jobs_local_run_tickets(id) ON DELETE CASCADE,
        account_id            TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        application_id        TEXT NOT NULL REFERENCES jobs_applications(id) ON DELETE CASCADE,
        intervention_id       TEXT NOT NULL UNIQUE REFERENCES jobs_interventions(id) ON DELETE CASCADE,
        action                TEXT NOT NULL CHECK(action = 'approve_submission'),
        status                TEXT NOT NULL CHECK(status IN ('approved', 'consumed')),
        expires_at_ms         INTEGER NOT NULL,
        created_at_ms         INTEGER NOT NULL,
        consumed_at_ms        INTEGER
    );
    CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_local_resume_actions_active_run
        ON jobs_local_run_resume_actions(run_id)
        WHERE status = 'approved';
    CREATE INDEX IF NOT EXISTS idx_jobs_local_resume_actions_application
        ON jobs_local_run_resume_actions(account_id, application_id, created_at_ms DESC);
    "#,
    // 0030 - append-only candidate feedback, support issues, and outcomes.
    //
    // The encrypted payload keeps optional notes private while relational
    // ownership columns make tenant and target validation explicit.
    r#"
    CREATE TABLE IF NOT EXISTS jobs_candidate_events (
        id                    TEXT PRIMARY KEY,
        account_id            TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        event_type            TEXT NOT NULL CHECK(event_type IN (
            'match_feedback', 'application_issue', 'application_outcome'
        )),
        job_id                TEXT REFERENCES jobs_postings(id) ON DELETE CASCADE,
        application_id        TEXT REFERENCES jobs_applications(id) ON DELETE CASCADE,
        status                TEXT NOT NULL,
        event_json            TEXT NOT NULL,
        created_at_ms         INTEGER NOT NULL,
        updated_at_ms         INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_jobs_candidate_events_account
        ON jobs_candidate_events(account_id, event_type, created_at_ms DESC);
    CREATE INDEX IF NOT EXISTS idx_jobs_candidate_events_job
        ON jobs_candidate_events(account_id, job_id, created_at_ms DESC);
    CREATE INDEX IF NOT EXISTS idx_jobs_candidate_events_application
        ON jobs_candidate_events(account_id, application_id, created_at_ms DESC);
    "#,
    // 0031 - idempotent, Jobs-owned model generation cache.
    //
    // Packet generation is included in the Jobs allowance, so these rows
    // retain provider provenance and Bluey's upstream cost without charging
    // the general chat balance. The generation key is a hash of the tenant,
    // job snapshot, profile truth, mode, and schema version.
    r#"
    CREATE TABLE IF NOT EXISTS jobs_resume_generations (
        id                    TEXT PRIMARY KEY,
        account_id            TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        job_id                TEXT NOT NULL,
        generation_key        TEXT NOT NULL,
        reservation_token     TEXT NOT NULL,
        status                TEXT NOT NULL DEFAULT 'reserved',
        output_json           TEXT,
        provider              TEXT,
        model                 TEXT,
        input_tokens          INTEGER NOT NULL DEFAULT 0,
        output_tokens         INTEGER NOT NULL DEFAULT 0,
        cost_cents_to_bluey   INTEGER NOT NULL DEFAULT 0,
        failure_code          TEXT,
        created_at_ms         INTEGER NOT NULL,
        updated_at_ms         INTEGER NOT NULL,
        UNIQUE(account_id, generation_key),
        FOREIGN KEY (job_id) REFERENCES jobs_postings(id) ON DELETE CASCADE,
        CHECK (status IN ('reserved', 'completed', 'failed'))
    );
    CREATE INDEX IF NOT EXISTS idx_jobs_resume_generations_job
        ON jobs_resume_generations(account_id, job_id, updated_at_ms DESC);
    "#,
    // 0032 - pre-dispatch Jobs packet allowance reservation.
    //
    // Managed resume generation consumes the Jobs packet allowance rather
    // than general chat credit. A job-scoped row fences duplicate generation
    // attempts and is converted into ordinary packet metering on commit.
    r#"
    CREATE TABLE IF NOT EXISTS jobs_generation_allowance_reservations (
        account_id            TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        job_id                TEXT NOT NULL REFERENCES jobs_postings(id) ON DELETE CASCADE,
        generation_key        TEXT NOT NULL,
        reservation_token     TEXT NOT NULL,
        status                TEXT NOT NULL DEFAULT 'reserved',
        period_start_ms       INTEGER NOT NULL,
        application_id        TEXT REFERENCES jobs_applications(id) ON DELETE SET NULL,
        created_at_ms         INTEGER NOT NULL,
        updated_at_ms         INTEGER NOT NULL,
        PRIMARY KEY (account_id, job_id),
        UNIQUE(account_id, generation_key),
        CHECK (status IN ('reserved', 'released', 'committed'))
    );
    CREATE INDEX IF NOT EXISTS idx_jobs_generation_allowance_status
        ON jobs_generation_allowance_reservations(account_id, status, updated_at_ms DESC);

    -- Anonymous conservative spend carried across the authority cutover.
    -- This table deliberately has no account, request, provider, or model key.
    CREATE TABLE IF NOT EXISTS usage_cutover_spend_baseline (
        occurred_at          DATETIME NOT NULL,
        cost_cents           INTEGER NOT NULL,
        CHECK (cost_cents > 0 AND cost_cents <= 100000000)
    );
    CREATE INDEX IF NOT EXISTS idx_usage_cutover_spend_baseline_time
        ON usage_cutover_spend_baseline(occurred_at);

    CREATE TABLE IF NOT EXISTS jobs_provider_cost_holds (
        request_scope_hash    TEXT PRIMARY KEY,
        account_scope_hash    TEXT NOT NULL,
        generation_scope_hash TEXT NOT NULL,
        root_scope_hash       TEXT NOT NULL,
        reservation_token     TEXT NOT NULL,
        provider              TEXT NOT NULL,
        model                 TEXT NOT NULL,
        projected_cost_cents  INTEGER NOT NULL,
        settled_cost_cents    INTEGER NOT NULL DEFAULT 0,
        usage_provenance      TEXT NOT NULL DEFAULT 'missing',
        status                TEXT NOT NULL DEFAULT 'held',
        created_at_ms         INTEGER NOT NULL,
        updated_at_ms         INTEGER NOT NULL,
        CHECK (status IN ('held', 'settled', 'released')),
        CHECK (usage_provenance IN ('exact', 'estimated', 'missing')),
        CHECK (projected_cost_cents > 0 AND projected_cost_cents <= 100000000),
        CHECK (settled_cost_cents >= 0 AND settled_cost_cents <= 100000000)
    );
    CREATE INDEX IF NOT EXISTS idx_jobs_provider_cost_holds_generation
        ON jobs_provider_cost_holds(account_scope_hash, generation_scope_hash, status, updated_at_ms DESC);
    CREATE INDEX IF NOT EXISTS idx_jobs_provider_cost_holds_root
        ON jobs_provider_cost_holds(account_scope_hash, root_scope_hash, status, updated_at_ms DESC);
    CREATE INDEX IF NOT EXISTS idx_jobs_provider_cost_holds_global
        ON jobs_provider_cost_holds(status, updated_at_ms DESC);
    "#,
    // 0033 - a provider board has one authoritative Career Track binding per
    // account. The application preflight is advisory; this uniqueness is the
    // last-line concurrency authority.
    r#"
    CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_discovery_sources_board_owner
        ON jobs_discovery_sources(account_id, provider, source_key);
    "#,
    // 0034 - immutable candidate evidence revisions and claim-level resume
    // provenance. Packet generation and runner claims fence against these
    // records so a changed profile, Track, identity, or confirmed fact cannot
    // silently reach an employer.
    r#"
    CREATE TABLE IF NOT EXISTS jobs_profile_evidence_revisions (
        id                    TEXT PRIMARY KEY,
        account_id            TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        career_track_id       TEXT NOT NULL REFERENCES jobs_tracks(id) ON DELETE CASCADE,
        revision_no           INTEGER NOT NULL,
        content_hash          TEXT NOT NULL,
        snapshot_json         TEXT NOT NULL,
        created_at_ms         INTEGER NOT NULL,
        UNIQUE(account_id, career_track_id, revision_no),
        UNIQUE(account_id, career_track_id, content_hash)
    );
    CREATE INDEX IF NOT EXISTS idx_jobs_evidence_revisions_track
        ON jobs_profile_evidence_revisions(account_id, career_track_id, revision_no DESC);

    CREATE TABLE IF NOT EXISTS jobs_resume_claim_evidence (
        id                    TEXT PRIMARY KEY,
        account_id            TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        resume_version_id     TEXT NOT NULL REFERENCES jobs_resume_versions(id) ON DELETE CASCADE,
        claim_id              TEXT NOT NULL,
        evidence_revision_id  TEXT NOT NULL REFERENCES jobs_profile_evidence_revisions(id) ON DELETE RESTRICT,
        source_ids_json       TEXT NOT NULL,
        claim_json            TEXT NOT NULL,
        created_at_ms         INTEGER NOT NULL,
        UNIQUE(account_id, resume_version_id, claim_id)
    );
    CREATE INDEX IF NOT EXISTS idx_jobs_claim_evidence_revision
        ON jobs_resume_claim_evidence(account_id, evidence_revision_id, created_at_ms DESC);
    "#,
    // 0035 - shared candidate-feed staging. Large third-party/public candidate
    // datasets are ingested once and materialized into bounded account views.
    SQLITE_JOBS_GLOBAL_CANDIDATE_INDEX,
    // 0036 - source resume bytes and exact-template capability metadata.
    SQLITE_JOBS_RESUME_SOURCE_ASSETS,
    // 0037 - server-only OAuth state and encrypted provider credentials.
    r#"
    CREATE TABLE IF NOT EXISTS jobs_oauth_states (
        state_hash       TEXT PRIMARY KEY,
        account_id       TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        provider         TEXT NOT NULL,
        state_json       TEXT NOT NULL,
        expires_at_ms    INTEGER NOT NULL,
        created_at_ms    INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_jobs_oauth_states_expiry
        ON jobs_oauth_states(expires_at_ms);

    CREATE TABLE IF NOT EXISTS jobs_provider_credentials (
        connection_id         TEXT PRIMARY KEY REFERENCES jobs_mailbox_connections(id) ON DELETE CASCADE,
        account_id            TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        provider              TEXT NOT NULL,
        provider_subject_hash TEXT NOT NULL,
        credential_json       TEXT NOT NULL,
        created_at_ms         INTEGER NOT NULL,
        updated_at_ms         INTEGER NOT NULL,
        UNIQUE(account_id, provider, provider_subject_hash)
    );
    CREATE INDEX IF NOT EXISTS idx_jobs_provider_credentials_account
        ON jobs_provider_credentials(account_id, provider, updated_at_ms DESC);
    "#,
    // 0038 - durable provider cursors, restart-safe leases, and encrypted
    // mailbox messages for application outcome and intervention processing.
    r#"
    CREATE TABLE IF NOT EXISTS jobs_provider_sync_state (
        connection_id       TEXT PRIMARY KEY REFERENCES jobs_mailbox_connections(id) ON DELETE CASCADE,
        account_id          TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        provider            TEXT NOT NULL,
        sync_json           TEXT NOT NULL,
        next_sync_at_ms     INTEGER NOT NULL,
        last_synced_at_ms   INTEGER,
        lease_owner         TEXT,
        lease_expires_at_ms INTEGER,
        created_at_ms       INTEGER NOT NULL,
        updated_at_ms       INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_jobs_provider_sync_due
        ON jobs_provider_sync_state(next_sync_at_ms, lease_expires_at_ms);
    CREATE INDEX IF NOT EXISTS idx_jobs_provider_sync_account
        ON jobs_provider_sync_state(account_id, provider, updated_at_ms DESC);

    CREATE TABLE IF NOT EXISTS jobs_provider_messages (
        id                    TEXT PRIMARY KEY,
        account_id            TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        connection_id         TEXT NOT NULL REFERENCES jobs_mailbox_connections(id) ON DELETE CASCADE,
        provider              TEXT NOT NULL,
        provider_message_hash TEXT NOT NULL,
        application_id        TEXT REFERENCES jobs_applications(id) ON DELETE SET NULL,
        processing_status     TEXT NOT NULL,
        message_json          TEXT NOT NULL,
        received_at_ms        INTEGER NOT NULL,
        processed_at_ms       INTEGER,
        created_at_ms         INTEGER NOT NULL,
        updated_at_ms         INTEGER NOT NULL,
        UNIQUE(account_id, provider, provider_message_hash)
    );
    CREATE INDEX IF NOT EXISTS idx_jobs_provider_messages_account
        ON jobs_provider_messages(account_id, received_at_ms DESC);
    CREATE INDEX IF NOT EXISTS idx_jobs_provider_messages_application
        ON jobs_provider_messages(account_id, application_id, received_at_ms DESC);
    CREATE INDEX IF NOT EXISTS idx_jobs_provider_messages_status
        ON jobs_provider_messages(account_id, processing_status, updated_at_ms DESC);
    "#,
    // 0039 - bounded semantic-row quarantine evidence for global discovery.
    SQLITE_JOBS_GLOBAL_INGESTION_QUARANTINE,
    // 0040 - Track-scoped, revisioned user authority for unattended
    // application submission.
    SQLITE_JOBS_AUTO_SUBMIT_AUTHORIZATIONS,
    // 0041 - approval-gated, replay-safe email and calendar actions.
    SQLITE_JOBS_COMMUNICATION_ACTIONS,
    // 0042 - durable encrypted Browser profile snapshot generations.
    SQLITE_JOBS_BROWSER_PROFILE_SNAPSHOTS,
    // 0043 - durable account-deletion write fence and upload drain status.
    SQLITE_ACCOUNT_DELETION_INTENTS,
    // 0044 - protected evidence capacity reserved before employer Submit.
    SQLITE_JOBS_SUBMISSION_EVIDENCE_RESERVATIONS,
    // 0045 - adopt existing source resumes and Browser profiles into the
    // account-scoped object lifecycle.
    SQLITE_JOBS_ACCOUNT_OBJECT_UPLOAD_BACKFILL,
    // 0046 - signed managed-runner volume identity, purge fan-out, and
    // pseudonymous restore tombstones.
    SQLITE_JOBS_RUNNER_VOLUME_PURGE,
    // 0047 - signed local Browser release and claim authority.
    SQLITE_JOBS_BROWSER_RELEASE_AUTHORITY,
];

pub fn run_migrations(pool: &DbPool) -> Result<()> {
    match pool {
        DbPool::Sqlite(_) => run_sqlite_migrations(pool),
        DbPool::Postgres(_) => run_postgres_migrations(pool),
    }
}

fn run_sqlite_migrations(pool: &DbPool) -> Result<()> {
    let conn = pool.get().context("get conn")?;
    for (i, sql) in MIGRATIONS.iter().enumerate() {
        conn.execute_batch(sql)
            .with_context(|| format!("migration {} failed", i + 1))?;
    }
    // Keep historical SQLite migrations immutable. Additive columns used by
    // the global-candidate cold-storage lifecycle are applied after replay.
    ensure_column(
        &conn,
        "jobs_global_candidates",
        "content_hash",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        &conn,
        "jobs_global_candidates",
        "archive_state",
        "TEXT NOT NULL DEFAULT 'hot'",
    )?;
    ensure_column(
        &conn,
        "jobs_global_candidates",
        "archive_storage_key",
        "TEXT",
    )?;
    ensure_column(&conn, "jobs_global_candidates", "archive_sha256", "TEXT")?;
    ensure_column(
        &conn,
        "jobs_global_candidates",
        "archive_size_bytes",
        "INTEGER",
    )?;
    ensure_column(&conn, "jobs_global_candidates", "archived_at_ms", "INTEGER")?;
    ensure_column(
        &conn,
        "jobs_global_candidates",
        "archive_attempt_count",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(
        &conn,
        "jobs_global_candidates",
        "archive_next_attempt_at_ms",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(
        &conn,
        "jobs_global_candidates",
        "archive_lease_owner",
        "TEXT",
    )?;
    ensure_column(
        &conn,
        "jobs_global_candidates",
        "archive_lease_expires_at_ms",
        "INTEGER",
    )?;
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_jobs_global_candidates_archive_due
             ON jobs_global_candidates(
                 archive_state, availability_status, updated_at_ms,
                 archive_next_attempt_at_ms
             );",
    )?;
    ensure_column(
        &conn,
        "jobs_global_ingestion_runs",
        "rejected_rows",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(
        &conn,
        "jobs_global_ingestion_runs",
        "rejection_summary_json",
        "TEXT NOT NULL DEFAULT '{}'",
    )?;
    ensure_column(&conn, "stt_sessions", "started_at_ms", "INTEGER")?;
    ensure_column(&conn, "stt_sessions", "ended_at_ms", "INTEGER")?;
    ensure_column(&conn, "stt_sessions", "relay_close_reason", "TEXT")?;
    ensure_column(&conn, "device_codes", "device_id", "TEXT")?;
    ensure_column(&conn, "device_codes", "device_name", "TEXT")?;
    ensure_column(&conn, "device_codes", "platform", "TEXT")?;
    ensure_column(&conn, "device_codes", "arch", "TEXT")?;
    ensure_column(&conn, "device_codes", "app_version", "TEXT")?;
    // 0033 - explicit trust provenance for provider-attempt settlement.
    // SQLite has no broadly supported `ADD COLUMN IF NOT EXISTS`, so use the
    // same schema-inspection helper as earlier additive migrations. This keeps
    // startup/replay idempotent for both pre-0033 and already-upgraded files.
    ensure_column(
        &conn,
        "jobs_provider_cost_holds",
        "usage_provenance",
        "TEXT NOT NULL DEFAULT 'missing' CHECK (usage_provenance IN ('exact', 'estimated', 'missing'))",
    )?;
    ensure_column(
        &conn,
        "usage_events",
        "origin",
        "TEXT NOT NULL DEFAULT 'legacy_unverified'",
    )?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS bluey_data_migrations (
             name TEXT PRIMARY KEY,
             applied_at DATETIME NOT NULL DEFAULT (datetime('now'))
         );",
    )?;
    let usage_origin_cutover_applied: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM bluey_data_migrations
          WHERE name = 'usage-origin-authority-cutover-v1')",
        [],
        |row| row.get(0),
    )?;
    let usage_cutover_spend_baseline_applied: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM bluey_data_migrations
          WHERE name = 'usage-cutover-spend-baseline-v1')",
        [],
        |row| row.get(0),
    )?;
    if !usage_origin_cutover_applied && !usage_cutover_spend_baseline_applied {
        conn.execute_batch(
            "BEGIN IMMEDIATE;
             INSERT INTO usage_cutover_spend_baseline(occurred_at, cost_cents)
             SELECT ts, MIN(MAX(cost_cents_to_bluey, 0), 100000000)
               FROM usage_events
              WHERE cost_cents_to_bluey > 0
                AND NOT EXISTS (
                    SELECT 1 FROM bluey_data_migrations
                     WHERE name = 'usage-cutover-spend-baseline-v1'
                );
             INSERT OR IGNORE INTO bluey_data_migrations(name)
              VALUES ('usage-cutover-spend-baseline-v1');
             UPDATE usage_events
            SET origin = 'legacy_unverified'
          WHERE NOT EXISTS (
              SELECT 1 FROM bluey_data_migrations
               WHERE name = 'usage-origin-authority-cutover-v1'
          );
             INSERT OR IGNORE INTO bluey_data_migrations(name) VALUES
              ('usage-origin-authority-cutover-v1'),
              ('usage-origin-taxonomy-repair-v1');
             COMMIT;",
        )?;
    } else {
        if !usage_cutover_spend_baseline_applied {
            // A database that already crossed authority using an earlier
            // build no longer has trustworthy row provenance. Snapshot every
            // positive cost conservatively; temporary double counting is
            // safer than resetting the rolling cap.
            conn.execute_batch(
                "BEGIN IMMEDIATE;
                 INSERT INTO usage_cutover_spend_baseline(occurred_at, cost_cents)
                 SELECT ts, MIN(MAX(cost_cents_to_bluey, 0), 100000000)
                   FROM usage_events
                  WHERE cost_cents_to_bluey > 0
                    AND NOT EXISTS (
                        SELECT 1 FROM bluey_data_migrations
                         WHERE name = 'usage-cutover-spend-baseline-v1'
                    );
                 INSERT OR IGNORE INTO bluey_data_migrations(name)
                  VALUES ('usage-cutover-spend-baseline-v1');
                 COMMIT;",
            )?;
        }
        if !usage_origin_cutover_applied {
            conn.execute_batch(
                "BEGIN IMMEDIATE;
                 UPDATE usage_events
                    SET origin = 'legacy_unverified'
                  WHERE NOT EXISTS (
                      SELECT 1 FROM bluey_data_migrations
                       WHERE name = 'usage-origin-authority-cutover-v1'
                  );
                 INSERT OR IGNORE INTO bluey_data_migrations(name) VALUES
                  ('usage-origin-authority-cutover-v1'),
                  ('usage-origin-taxonomy-repair-v1');
                 COMMIT;",
            )?;
        }
    }
    let usage_origin_taxonomy_repair_applied: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM bluey_data_migrations
          WHERE name = 'usage-origin-taxonomy-repair-v1')",
        [],
        |row| row.get(0),
    )?;
    if !usage_origin_taxonomy_repair_applied {
        conn.execute_batch(
            "BEGIN IMMEDIATE;
             UPDATE usage_events
                SET origin = 'legacy_unverified'
              WHERE (origin IS NULL
                 OR origin NOT IN ('server', 'client', 'legacy_unverified'))
                AND NOT EXISTS (
                    SELECT 1 FROM bluey_data_migrations
                     WHERE name = 'usage-origin-taxonomy-repair-v1'
                );
             INSERT OR IGNORE INTO bluey_data_migrations(name)
              VALUES ('usage-origin-taxonomy-repair-v1');
             COMMIT;",
        )?;
    }
    let usage_reservation_repair_applied: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM bluey_data_migrations
          WHERE name = 'usage-reservations-settled-at-repair-v1')",
        [],
        |row| row.get(0),
    )?;
    if !usage_reservation_repair_applied {
        conn.execute_batch(
            "BEGIN IMMEDIATE;
             UPDATE usage_reservations
                SET settled_at_ms = created_at_ms
              WHERE status = 'settled' AND settled_at_ms IS NULL
                AND NOT EXISTS (
                    SELECT 1 FROM bluey_data_migrations
                     WHERE name = 'usage-reservations-settled-at-repair-v1'
                );
             INSERT OR IGNORE INTO bluey_data_migrations(name)
              VALUES ('usage-reservations-settled-at-repair-v1');
             COMMIT;",
        )?;
    }
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_usage_events_server_ts
             ON usage_events(ts) WHERE origin = 'server';
         CREATE INDEX IF NOT EXISTS idx_usage_events_server_identity
             ON usage_events(account_id, request_id, kind)
             WHERE origin = 'server';
         CREATE TRIGGER IF NOT EXISTS trg_usage_events_origin_insert
         BEFORE INSERT ON usage_events
         WHEN NEW.origin IS NULL
           OR NEW.origin NOT IN ('server', 'client', 'legacy_unverified')
         BEGIN
             SELECT RAISE(ABORT, 'invalid usage event origin');
         END;
         CREATE TRIGGER IF NOT EXISTS trg_usage_events_origin_update
         BEFORE UPDATE OF origin ON usage_events
         WHEN NEW.origin IS NULL
           OR NEW.origin NOT IN ('server', 'client', 'legacy_unverified')
         BEGIN
             SELECT RAISE(ABORT, 'invalid usage event origin');
         END;
         CREATE INDEX IF NOT EXISTS idx_usage_reservations_settled_exposure
             ON usage_reservations(settled_at_ms)
             WHERE status = 'settled';",
    )?;
    ensure_column(&conn, "refresh_tokens", "device_id", "TEXT")?;
    ensure_column(
        &conn,
        "accounts",
        "reserved_cents",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(&conn, "accounts", "square_customer_id", "TEXT")?;
    ensure_column(&conn, "accounts", "square_card_id", "TEXT")?;
    ensure_column(&conn, "accounts", "square_card_brand", "TEXT")?;
    ensure_column(&conn, "accounts", "square_card_last4", "TEXT")?;
    ensure_column(
        &conn,
        "accounts",
        "billing_restricted",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(&conn, "accounts", "billing_restriction_reason", "TEXT")?;
    ensure_column(&conn, "accounts", "billing_restricted_at", "DATETIME")?;
    ensure_column(
        &conn,
        "accounts",
        "is_temporary",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(&conn, "accounts", "temporary_expires_at", "DATETIME")?;
    ensure_column(
        &conn,
        "signup_otps",
        "account_id",
        "TEXT REFERENCES accounts(id) ON DELETE CASCADE",
    )?;
    ensure_column(
        &conn,
        "jobs_resume_generations",
        "reservation_token",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    conn.execute(
        "UPDATE jobs_resume_generations SET reservation_token = id \
         WHERE reservation_token = ''",
        [],
    )?;
    ensure_column(
        &conn,
        "stt_sessions",
        "reserved_cents",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(
        &conn,
        "stt_sessions",
        "settled_cents",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(
        &conn,
        "stt_sessions",
        "refunded_cents",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(
        &conn,
        "stt_sessions",
        "reserved_trial_seconds",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(
        &conn,
        "stt_sessions",
        "settled_trial_seconds",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(
        &conn,
        "stt_sessions",
        "refunded_trial_seconds",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(&conn, "trial_grants", "email_domain_hash", "TEXT")?;
    ensure_column(&conn, "trial_abuse_events", "email_domain_hash", "TEXT")?;
    ensure_column(
        &conn,
        "legal_acceptances",
        "terms_text_hash",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        &conn,
        "legal_acceptances",
        "privacy_text_hash",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(&conn, "legal_acceptances", "email_hash", "TEXT")?;
    ensure_column(&conn, "legal_acceptances", "ip_user_agent_hash", "TEXT")?;
    ensure_column(
        &conn,
        "legal_acceptances",
        "retention_expires_at",
        "DATETIME",
    )?;
    ensure_column(
        &conn,
        "cloud_context_artifacts",
        "updated_at_ms",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    conn.execute(
        "UPDATE cloud_context_artifacts
         SET updated_at_ms = created_at_ms
         WHERE updated_at_ms < created_at_ms",
        [],
    )?;
    ensure_column(
        &conn,
        "legal_acceptances",
        "accepted_at",
        // SQLite rejects non-constant defaults such as datetime('now') when
        // ALTER TABLE adds a column. New writes populate this explicitly.
        "DATETIME",
    )?;
    conn.execute_batch(
        r#"
        CREATE INDEX IF NOT EXISTS idx_trial_grants_email_domain
            ON trial_grants(email_domain_hash, created_at);
        CREATE INDEX IF NOT EXISTS idx_trial_abuse_events_email_domain
            ON trial_abuse_events(email_domain_hash, created_at);
        CREATE INDEX IF NOT EXISTS idx_legal_acceptances_account_created
            ON legal_acceptances(account_id, created_at);
        CREATE INDEX IF NOT EXISTS idx_legal_acceptances_purpose_created
            ON legal_acceptances(purpose, created_at);
        CREATE INDEX IF NOT EXISTS idx_legal_acceptances_email_created
            ON legal_acceptances(email_hash, created_at);
        "#,
    )?;
    tracing::info!(
        backend = pool.backend_name(),
        count = MIGRATIONS.len(),
        "migrations applied"
    );
    Ok(())
}

const POSTGRES_RUNTIME_SCHEMA: &str =
    include_str!("../../../infra/postgres/server-runtime/001_server_runtime_compat.sql");
const POSTGRES_USAGE_RESERVATIONS: &str =
    include_str!("../../../infra/postgres/server-runtime/002_usage_reservations.sql");
const POSTGRES_OBJECT_UPLOAD_CONTROLS: &str =
    include_str!("../../../infra/postgres/server-runtime/003_object_upload_controls.sql");
const POSTGRES_STRIPE_AUTO_RELOAD: &str =
    include_str!("../../../infra/postgres/server-runtime/004_stripe_auto_reload.sql");
const POSTGRES_JOBS_CANDIDATE_EVENTS: &str =
    include_str!("../../../infra/postgres/server-runtime/005_jobs_candidate_events.sql");
const POSTGRES_CONTEXT_ARTIFACT_REVISIONS: &str =
    include_str!("../../../infra/postgres/server-runtime/006_context_artifact_revisions.sql");
const POSTGRES_JOBS_RESUME_GENERATIONS: &str =
    include_str!("../../../infra/postgres/server-runtime/007_jobs_resume_generations.sql");
const POSTGRES_JOBS_GENERATION_ALLOWANCE: &str =
    include_str!("../../../infra/postgres/server-runtime/008_jobs_generation_allowance.sql");
const POSTGRES_JOBS_DISCOVERY_BOARD_OWNER: &str =
    include_str!("../../../infra/postgres/server-runtime/009_jobs_discovery_board_owner.sql");
const POSTGRES_PROVIDER_USAGE_PROVENANCE: &str =
    include_str!("../../../infra/postgres/server-runtime/010_provider_usage_provenance.sql");
const POSTGRES_JOBS_CANDIDATE_EVIDENCE: &str =
    include_str!("../../../infra/postgres/server-runtime/011_jobs_candidate_evidence.sql");
const POSTGRES_JOBS_GLOBAL_CANDIDATE_INDEX: &str =
    include_str!("../../../infra/postgres/server-runtime/012_jobs_global_candidate_index.sql");
const POSTGRES_JOBS_RESUME_SOURCE_ASSETS: &str =
    include_str!("../../../infra/postgres/server-runtime/013_jobs_resume_source_assets.sql");
const POSTGRES_JOBS_PROVIDER_CONNECTIONS: &str =
    include_str!("../../../infra/postgres/server-runtime/014_jobs_provider_connections.sql");
const POSTGRES_JOBS_MAILBOX_SYNC: &str =
    include_str!("../../../infra/postgres/server-runtime/015_jobs_mailbox_sync.sql");
const POSTGRES_JOBS_GLOBAL_INGESTION_QUARANTINE: &str =
    include_str!("../../../infra/postgres/server-runtime/016_jobs_global_ingestion_quarantine.sql");
const POSTGRES_JOBS_GLOBAL_CANDIDATE_ARCHIVE: &str =
    include_str!("../../../infra/postgres/server-runtime/017_jobs_global_candidate_archive.sql");
const POSTGRES_JOBS_AUTO_SUBMIT_AUTHORIZATIONS: &str =
    include_str!("../../../infra/postgres/server-runtime/018_jobs_auto_submit_authorizations.sql");
const POSTGRES_JOBS_COMMUNICATION_ACTIONS: &str =
    include_str!("../../../infra/postgres/server-runtime/019_jobs_communication_actions.sql");
const POSTGRES_JOBS_BROWSER_PROFILE_SNAPSHOTS: &str =
    include_str!("../../../infra/postgres/server-runtime/020_jobs_browser_profile_snapshots.sql");
pub const ACCOUNT_DELETION_INTENTS_MIGRATION_ID: &str = "021_account_deletion_intents.sql";
const POSTGRES_ACCOUNT_DELETION_INTENTS: &str =
    include_str!("../../../infra/postgres/server-runtime/021_account_deletion_intents.sql");
pub const JOBS_SUBMISSION_EVIDENCE_RESERVATIONS_MIGRATION_ID: &str =
    "022_jobs_submission_evidence_reservations.sql";
const POSTGRES_JOBS_SUBMISSION_EVIDENCE_RESERVATIONS: &str = include_str!(
    "../../../infra/postgres/server-runtime/022_jobs_submission_evidence_reservations.sql"
);
pub const JOBS_ACCOUNT_OBJECT_UPLOAD_BACKFILL_MIGRATION_ID: &str =
    "023_jobs_account_object_upload_backfill.sql";
const POSTGRES_JOBS_ACCOUNT_OBJECT_UPLOAD_BACKFILL: &str = include_str!(
    "../../../infra/postgres/server-runtime/023_jobs_account_object_upload_backfill.sql"
);
pub const JOBS_RUNNER_VOLUME_PURGE_MIGRATION_ID: &str = "024_jobs_runner_volume_purge.sql";
const POSTGRES_JOBS_RUNNER_VOLUME_PURGE: &str =
    include_str!("../../../infra/postgres/server-runtime/024_jobs_runner_volume_purge.sql");
pub const JOBS_BROWSER_RELEASE_AUTHORITY_MIGRATION_ID: &str =
    "025_jobs_browser_release_authority.sql";
const POSTGRES_JOBS_BROWSER_RELEASE_AUTHORITY: &str =
    include_str!("../../../infra/postgres/server-runtime/025_jobs_browser_release_authority.sql");
const POSTGRES_MIGRATIONS: &[(&str, &str)] = &[
    ("001_server_runtime_compat.sql", POSTGRES_RUNTIME_SCHEMA),
    ("002_usage_reservations.sql", POSTGRES_USAGE_RESERVATIONS),
    (
        "003_object_upload_controls.sql",
        POSTGRES_OBJECT_UPLOAD_CONTROLS,
    ),
    ("004_stripe_auto_reload.sql", POSTGRES_STRIPE_AUTO_RELOAD),
];
const POSTGRES_JOBS_SCHEMA: &str =
    include_str!("../../../infra/postgres/server-runtime/002_jobs.sql");
const POSTGRES_POST_JOBS_MIGRATIONS: &[(&str, &str)] = &[
    (
        "005_jobs_candidate_events.sql",
        POSTGRES_JOBS_CANDIDATE_EVENTS,
    ),
    (
        "007_jobs_resume_generations.sql",
        POSTGRES_JOBS_RESUME_GENERATIONS,
    ),
    (
        "008_jobs_generation_allowance.sql",
        POSTGRES_JOBS_GENERATION_ALLOWANCE,
    ),
    (
        "009_jobs_discovery_board_owner.sql",
        POSTGRES_JOBS_DISCOVERY_BOARD_OWNER,
    ),
    (
        "010_provider_usage_provenance.sql",
        POSTGRES_PROVIDER_USAGE_PROVENANCE,
    ),
    (
        "011_jobs_candidate_evidence.sql",
        POSTGRES_JOBS_CANDIDATE_EVIDENCE,
    ),
    (
        "012_jobs_global_candidate_index.sql",
        POSTGRES_JOBS_GLOBAL_CANDIDATE_INDEX,
    ),
    (
        "013_jobs_resume_source_assets.sql",
        POSTGRES_JOBS_RESUME_SOURCE_ASSETS,
    ),
    (
        "014_jobs_provider_connections.sql",
        POSTGRES_JOBS_PROVIDER_CONNECTIONS,
    ),
    ("015_jobs_mailbox_sync.sql", POSTGRES_JOBS_MAILBOX_SYNC),
    (
        "016_jobs_global_ingestion_quarantine.sql",
        POSTGRES_JOBS_GLOBAL_INGESTION_QUARANTINE,
    ),
    (
        "017_jobs_global_candidate_archive.sql",
        POSTGRES_JOBS_GLOBAL_CANDIDATE_ARCHIVE,
    ),
    (
        "018_jobs_auto_submit_authorizations.sql",
        POSTGRES_JOBS_AUTO_SUBMIT_AUTHORIZATIONS,
    ),
    (
        "019_jobs_communication_actions.sql",
        POSTGRES_JOBS_COMMUNICATION_ACTIONS,
    ),
    (
        "020_jobs_browser_profile_snapshots.sql",
        POSTGRES_JOBS_BROWSER_PROFILE_SNAPSHOTS,
    ),
    (
        ACCOUNT_DELETION_INTENTS_MIGRATION_ID,
        POSTGRES_ACCOUNT_DELETION_INTENTS,
    ),
    (
        JOBS_SUBMISSION_EVIDENCE_RESERVATIONS_MIGRATION_ID,
        POSTGRES_JOBS_SUBMISSION_EVIDENCE_RESERVATIONS,
    ),
    (
        JOBS_ACCOUNT_OBJECT_UPLOAD_BACKFILL_MIGRATION_ID,
        POSTGRES_JOBS_ACCOUNT_OBJECT_UPLOAD_BACKFILL,
    ),
    (
        JOBS_RUNNER_VOLUME_PURGE_MIGRATION_ID,
        POSTGRES_JOBS_RUNNER_VOLUME_PURGE,
    ),
    (
        JOBS_BROWSER_RELEASE_AUTHORITY_MIGRATION_ID,
        POSTGRES_JOBS_BROWSER_RELEASE_AUTHORITY,
    ),
];

fn run_postgres_migrations(pool: &DbPool) -> Result<()> {
    run_blocking_db(|| run_postgres_migrations_inner(pool))
}

fn run_postgres_migrations_inner(pool: &DbPool) -> Result<()> {
    let mut conn = pool.get_pg()?;
    conn.batch_execute(
        "CREATE TABLE IF NOT EXISTS bluey_schema_migrations (
            version TEXT PRIMARY KEY,
            applied_at TIMESTAMPTZ NOT NULL DEFAULT now()
        );",
    )
    .context("ensure postgres migration ledger")?;

    for (version, sql) in POSTGRES_MIGRATIONS {
        let already = conn
            .query_opt(
                "SELECT 1 FROM bluey_schema_migrations WHERE version = $1",
                &[version],
            )
            .with_context(|| format!("check postgres migration ledger for {version}"))?
            .is_some();

        // Runtime migrations are idempotent and intentionally self-healing.
        // Replaying CREATE IF NOT EXISTS statements also repairs schema drift
        // without mutating already-recorded application data.
        conn.batch_execute(sql)
            .with_context(|| format!("apply postgres migration {version}"))?;
        if !already {
            conn.execute(
                "INSERT INTO bluey_schema_migrations(version) VALUES ($1)
                 ON CONFLICT (version) DO NOTHING",
                &[version],
            )
            .with_context(|| format!("record postgres migration {version}"))?;
        }
    }

    conn.batch_execute(POSTGRES_JOBS_SCHEMA)
        .context("apply postgres Jobs schema")?;
    conn.execute(
        "INSERT INTO bluey_schema_migrations(version) VALUES ($1)
         ON CONFLICT (version) DO NOTHING",
        &[&"002_jobs.sql"],
    )
    .context("record postgres Jobs migration")?;
    for (version, sql) in POSTGRES_POST_JOBS_MIGRATIONS {
        conn.batch_execute(sql)
            .with_context(|| format!("apply post-Jobs postgres migration {version}"))?;
        conn.execute(
            "INSERT INTO bluey_schema_migrations(version) VALUES ($1)
             ON CONFLICT (version) DO NOTHING",
            &[version],
        )
        .with_context(|| format!("record post-Jobs postgres migration {version}"))?;
    }
    conn.batch_execute(POSTGRES_CONTEXT_ARTIFACT_REVISIONS)
        .context("apply postgres context artifact revision migration")?;
    conn.execute(
        "INSERT INTO bluey_schema_migrations(version) VALUES ($1)
         ON CONFLICT (version) DO NOTHING",
        &[&"006_context_artifact_revisions.sql"],
    )
    .context("record postgres context artifact revision migration")?;

    let vector_ready = conn
        .query_opt("SELECT 1 FROM pg_extension WHERE extname = 'vector'", &[])
        .context("check pgvector extension")?
        .is_some();
    if !vector_ready {
        anyhow::bail!("pgvector extension missing after postgres migrations");
    }
    let rag_ready = conn
        .query_one(
            "SELECT udt_name
               FROM information_schema.columns
              WHERE table_schema = 'public'
                AND table_name = 'cloud_rag_chunks'
                AND column_name = 'embedding'",
            &[],
        )
        .context("check cloud_rag_chunks.embedding")?;
    let embedding_type: String = rag_ready.get(0);
    if embedding_type != "vector" {
        anyhow::bail!("cloud_rag_chunks.embedding is {embedding_type}, expected vector");
    }

    tracing::info!(
        backend = pool.backend_name(),
        migrations = POSTGRES_MIGRATIONS.len() + POSTGRES_POST_JOBS_MIGRATIONS.len(),
        "migrations applied"
    );
    Ok(())
}

fn ensure_column(
    conn: &rusqlite::Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<()> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let exists = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(|row| row.ok())
        .any(|name| name == column);
    if !exists {
        conn.execute_batch(&format!(
            "ALTER TABLE {table} ADD COLUMN {column} {definition};"
        ))?;
    }
    Ok(())
}

#[cfg(test)]
mod blocking_boundary_tests {
    use super::{in_db_blocking_context, run_blocking_db};

    #[test]
    fn run_blocking_db_marks_sync_context() {
        assert!(!in_db_blocking_context());
        assert!(run_blocking_db(in_db_blocking_context));
        assert!(!in_db_blocking_context());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn run_blocking_db_marks_tokio_context() {
        assert!(!in_db_blocking_context());
        assert!(run_blocking_db(in_db_blocking_context));
        assert!(!in_db_blocking_context());
    }
}

#[cfg(test)]
mod sqlite_migration_replay_tests {
    use super::{ensure_column, open_pool, run_migrations, SQLITE_JOBS_BROWSER_RELEASE_AUTHORITY};

    #[test]
    fn browser_release_authority_uses_immutable_history_and_a_mutable_explicit_head() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", false).unwrap();
        conn.execute_batch(SQLITE_JOBS_BROWSER_RELEASE_AUTHORITY)
            .unwrap();

        for table in [
            "jobs_browser_release_signature_sets",
            "jobs_browser_release_signatures",
            "jobs_browser_release_trust_policies",
            "jobs_browser_release_trust_keys",
            "jobs_browser_release_manifests",
            "jobs_browser_release_artifacts",
            "jobs_browser_release_activations",
            "jobs_browser_release_rollbacks",
            "jobs_browser_release_revocations",
            "jobs_browser_release_channel_transitions",
            "jobs_browser_release_channel_heads",
            "jobs_browser_account_channel_assignments",
            "jobs_local_run_release_bindings",
            "jobs_local_run_claim_replays",
        ] {
            let exists: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master \
                      WHERE type = 'table' AND name = ?1",
                    rusqlite::params![table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(exists, 1, "missing Browser release table {table}");
        }

        let signature_set_sha256 = "a".repeat(64);
        conn.execute(
            "INSERT INTO jobs_browser_release_signature_sets (\
               signature_set_sha256, signature_set_id, trust_generation, role,\
               target_audience, target_sha256, signed_at_ms, signature_count,\
               canonical_signature_set_base64url, recorded_by, recorded_at_ms\
             ) VALUES (?1, 'signature-set-test', 1, 'release',\
               'bluey-jobs-browser-release-manifest-v1', ?2, 1, 1, 'YQ',\
               'migration-test', 1)",
            rusqlite::params![signature_set_sha256, "b".repeat(64)],
        )
        .unwrap();
        let immutable = conn
            .execute(
                "UPDATE jobs_browser_release_signature_sets SET recorded_at_ms = 2 \
                  WHERE signature_set_sha256 = ?1",
                rusqlite::params![signature_set_sha256],
            )
            .unwrap_err();
        assert!(
            format!("{immutable:?}").contains("signature set is immutable"),
            "unexpected immutable-row error: {immutable:?}"
        );

        let subject_sha256 = "f".repeat(64);
        let insert_revocation = |revocation_sha256: &str,
                                 revocation_id: &str,
                                 revocation_generation: i64,
                                 subject_id: &str| {
            conn.execute(
                "INSERT INTO jobs_browser_release_revocations (\
                       revocation_sha256, revocation_id, revocation_generation,\
                       trust_generation, subject_kind, subject_id, subject_sha256,\
                       reason_ref, canonical_revocation_base64url,\
                       authorization_signature_set_sha256, issued_at_ms, recorded_by,\
                       recorded_at_ms\
                     ) VALUES (?1, ?2, ?3, 1, 'artifact', ?4, ?5,\
                       'migration-test', 'YQ', ?6, 1, 'migration-test', 1)",
                rusqlite::params![
                    revocation_sha256,
                    revocation_id,
                    revocation_generation,
                    subject_id,
                    subject_sha256,
                    signature_set_sha256,
                ],
            )
        };
        insert_revocation(&"1".repeat(64), "revocation-a", 1, "artifact-a").unwrap();
        insert_revocation(&"2".repeat(64), "revocation-b", 2, "artifact-b").unwrap();
        let duplicate_exact_subject =
            insert_revocation(&"3".repeat(64), "revocation-c", 3, "artifact-a").unwrap_err();
        assert!(
            format!("{duplicate_exact_subject:?}").contains("UNIQUE constraint failed"),
            "unexpected exact-subject uniqueness error: {duplicate_exact_subject:?}"
        );

        conn.execute(
            "INSERT INTO jobs_browser_release_channel_heads (\
               channel, head_revision, current_transition_sha256,\
               current_activation_sha256, current_manifest_sha256,\
               current_trust_generation, current_channel_sequence, updated_at_ms\
             ) VALUES ('internal', 1, ?1, ?2, ?3, 1, 1, 1)",
            rusqlite::params!["c".repeat(64), "d".repeat(64), "e".repeat(64)],
        )
        .unwrap();
        assert_eq!(
            conn.execute(
                "UPDATE jobs_browser_release_channel_heads SET updated_at_ms = 2 \
                  WHERE channel = 'internal' AND head_revision = 1",
                [],
            )
            .unwrap(),
            1,
            "the explicit channel head must be CAS-mutable"
        );
        let undeletable = conn
            .execute(
                "DELETE FROM jobs_browser_release_channel_heads WHERE channel = 'internal'",
                [],
            )
            .unwrap_err();
        assert!(
            format!("{undeletable:?}").contains("channel head cannot be deleted"),
            "unexpected channel-head deletion error: {undeletable:?}"
        );
    }

    #[test]
    fn global_ingestion_quarantine_columns_upgrade_in_place() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE jobs_global_ingestion_runs (
                id TEXT PRIMARY KEY,
                source_id TEXT NOT NULL
             );
             INSERT INTO jobs_global_ingestion_runs (id, source_id)
             VALUES ('run-legacy', 'source-legacy');",
        )
        .unwrap();

        ensure_column(
            &conn,
            "jobs_global_ingestion_runs",
            "rejected_rows",
            "INTEGER NOT NULL DEFAULT 0",
        )
        .unwrap();
        ensure_column(
            &conn,
            "jobs_global_ingestion_runs",
            "rejection_summary_json",
            "TEXT NOT NULL DEFAULT '{}'",
        )
        .unwrap();

        let upgraded: (i64, String) = conn
            .query_row(
                "SELECT rejected_rows, rejection_summary_json
                   FROM jobs_global_ingestion_runs
                  WHERE id = 'run-legacy'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(upgraded, (0, "{}".to_string()));
    }

    #[test]
    fn data_repairs_do_not_update_rows_after_markers_exist() {
        let path = std::env::temp_dir().join(format!(
            "bluey-migration-replay-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let pool = open_pool(&path).unwrap();
        run_migrations(&pool).unwrap();
        let account =
            crate::db::accounts::Account::create(&pool, "migration-replay@bluey.test", "hash")
                .unwrap();
        let conn = pool.get().unwrap();
        conn.execute(
            "INSERT INTO usage_reservations
                (account_id, request_id, kind, status, attempt,
                 estimated_customer_cents, estimated_upstream_cents,
                 created_at_ms, expires_at_ms, settled_at_ms,
                 reservation_reason)
             VALUES (?1, 'replay', 'llm', 'settled', 1, 0, 0,
                     1000, 2000, NULL, 'replay-test')",
            rusqlite::params![account.id],
        )
        .unwrap();
        conn.execute_batch(
            "CREATE TABLE migration_update_audit (table_name TEXT NOT NULL);
             CREATE TRIGGER audit_usage_event_repair
             AFTER UPDATE ON usage_events BEGIN
               INSERT INTO migration_update_audit VALUES ('usage_events');
             END;
             CREATE TRIGGER audit_usage_reservation_repair
             AFTER UPDATE ON usage_reservations BEGIN
               INSERT INTO migration_update_audit VALUES ('usage_reservations');
             END;",
        )
        .unwrap();
        drop(conn);

        run_migrations(&pool).unwrap();

        let conn = pool.get().unwrap();
        let updates: i64 = conn
            .query_row("SELECT COUNT(*) FROM migration_update_audit", [], |row| {
                row.get(0)
            })
            .unwrap();
        let markers: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM bluey_data_migrations WHERE name IN (
                    'usage-cutover-spend-baseline-v1',
                    'usage-origin-authority-cutover-v1',
                    'usage-origin-taxonomy-repair-v1',
                    'usage-reservations-settled-at-repair-v1'
                 )",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let settled_at: Option<i64> = conn
            .query_row(
                "SELECT settled_at_ms FROM usage_reservations
                  WHERE account_id = ?1 AND request_id = 'replay'",
                rusqlite::params![account.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(updates, 0);
        assert_eq!(markers, 4);
        assert_eq!(settled_at, None);
    }

    #[test]
    fn cutover_spend_baseline_is_anonymous_replay_safe_bounded_and_conservative() {
        let path = std::env::temp_dir().join(format!(
            "bluey-cutover-baseline-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let pool = open_pool(&path).unwrap();
        run_migrations(&pool).unwrap();
        let account =
            crate::db::accounts::Account::create(&pool, "cutover-baseline@bluey.test", "hash")
                .unwrap();
        let conn = pool.get().unwrap();
        conn.execute(
            "DELETE FROM bluey_data_migrations WHERE name IN (
                'usage-cutover-spend-baseline-v1',
                'usage-origin-authority-cutover-v1',
                'usage-origin-taxonomy-repair-v1'
             )",
            [],
        )
        .unwrap();
        for (request_id, cost, age) in [
            ("recent", 7_i64, "now"),
            ("hostile", i64::MAX, "now"),
            ("negative", -5, "now"),
            ("expired", 11, "-3 days"),
        ] {
            conn.execute(
                "INSERT INTO usage_events (
                    id, account_id, request_id, origin, kind,
                    cost_cents_to_bluey, ts
                 ) VALUES (?1, ?2, ?3, 'server', 'llm', ?4,
                           CASE WHEN ?5 = 'now' THEN datetime('now')
                                ELSE datetime('now', ?5) END)",
                rusqlite::params![
                    uuid::Uuid::new_v4().to_string(),
                    account.id,
                    request_id,
                    cost,
                    age,
                ],
            )
            .unwrap();
        }
        drop(conn);

        run_migrations(&pool).unwrap();
        run_migrations(&pool).unwrap();
        let conn = pool.get().unwrap();
        let costs: Vec<i64> = conn
            .prepare("SELECT cost_cents FROM usage_cutover_spend_baseline ORDER BY cost_cents")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(costs, vec![7, 11, 100_000_000]);
        let columns: Vec<String> = conn
            .prepare("PRAGMA table_info(usage_cutover_spend_baseline)")
            .unwrap()
            .query_map([], |row| row.get(1))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(columns, vec!["occurred_at", "cost_cents"]);
        conn.execute(
            "DELETE FROM accounts WHERE id = ?1",
            rusqlite::params![account.id],
        )
        .unwrap();
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM usage_cutover_spend_baseline",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
            3,
            "privacy deletion must not erase anonymous rolling spend truth"
        );
        drop(conn);

        let next =
            crate::db::accounts::Account::create(&pool, "cutover-baseline-next@bluey.test", "hash")
                .unwrap();
        assert_eq!(
            crate::db::jobs_provider_cost_holds::reserve(
                &pool,
                &next.id,
                "router:after-cutover:llm",
                "after-cutover-token",
                "after-cutover:attempt:0",
                "openai",
                "gpt-5.4-mini",
                1,
                100,
                crate::config::UpstreamSpendGuard {
                    limit_cents: 100_000_007,
                    window_hours: 24,
                },
            )
            .unwrap(),
            crate::db::jobs_provider_cost_holds::CostHoldReservation::GlobalLimit
        );

        pool.get()
            .unwrap()
            .execute(
                "UPDATE usage_cutover_spend_baseline
                    SET occurred_at = datetime('now', '-3 days')",
                [],
            )
            .unwrap();
        assert!(matches!(
            crate::db::jobs_provider_cost_holds::reserve(
                &pool,
                &next.id,
                "router:after-expiry:llm",
                "after-expiry-token",
                "after-expiry:attempt:0",
                "openai",
                "gpt-5.4-mini",
                1,
                100,
                crate::config::UpstreamSpendGuard {
                    limit_cents: 1,
                    window_hours: 24,
                },
            )
            .unwrap(),
            crate::db::jobs_provider_cost_holds::CostHoldReservation::Held { .. }
        ));
        assert_eq!(
            pool.get()
                .unwrap()
                .query_row(
                    "SELECT COUNT(*) FROM usage_cutover_spend_baseline",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            3,
            "rows outside a short admission window remain for fixed retention"
        );
        pool.get()
            .unwrap()
            .execute(
                "UPDATE usage_cutover_spend_baseline
                    SET occurred_at = datetime('now', '-32 days')",
                [],
            )
            .unwrap();
        let cleanup = crate::db::jobs_provider_cost_holds::prune_expired_spend_truth(&pool)
            .expect("fixed-retention cleanup");
        assert_eq!(cleanup.cutover_baseline_rows_deleted, 3);
        assert_eq!(
            pool.get()
                .unwrap()
                .query_row(
                    "SELECT COUNT(*) FROM usage_cutover_spend_baseline",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            0,
            "rows older than the fixed maximum window plus grace are deleted"
        );
    }

    #[test]
    fn jobs_account_objects_are_backfilled_into_the_durable_ledger() {
        let path = std::env::temp_dir().join(format!(
            "bluey-jobs-object-backfill-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let pool = open_pool(&path).unwrap();
        run_migrations(&pool).unwrap();
        let account =
            crate::db::accounts::Account::create(&pool, "jobs-object-backfill@bluey.test", "hash")
                .unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        pool.get()
            .unwrap()
            .execute_batch(&format!(
                "INSERT INTO jobs_resume_source_assets (
                    id, account_id, file_name, media_type, file_type, storage_key,
                    sha256, size_bytes, page_count, template_status, created_at_ms, updated_at_ms
                 ) VALUES (
                    'resume-backfill', '{account_id}', 'Resume.pdf', 'application/pdf', 'pdf',
                    'accounts/{account_id}/jobs/resume-backfill.pdf', '{resume_sha}', 128, 2,
                    'converted_layout', {now}, {now}
                 );
                 INSERT INTO jobs_browser_profile_snapshots (
                    account_id, browser_profile_id, generation, object_key, sha256,
                    size_bytes, envelope_version, writer_run_id, writer_fence, updated_at_ms
                 ) VALUES (
                    '{account_id}', 'profile-backfill', 4,
                    'accounts/{account_id}/jobs/profile-backfill.enc', '{profile_sha}',
                    256, 2, 'run-backfill', 7, {now}
                 );",
                account_id = account.id,
                resume_sha = "a".repeat(64),
                profile_sha = "b".repeat(64),
            ))
            .unwrap();

        run_migrations(&pool).unwrap();
        run_migrations(&pool).unwrap();
        let conn = pool.get().unwrap();
        let rows: Vec<(String, String, String)> = conn
            .prepare(
                "SELECT logical_id, state, metadata_json
                   FROM object_uploads
                  WHERE account_id = ?1 AND logical_id LIKE 'jobs-%'
                  ORDER BY logical_id",
            )
            .unwrap()
            .query_map(rusqlite::params![account.id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows[0].0,
            format!(
                "jobs-browser-profile:profile-backfill:4:2:{}",
                "b".repeat(64)
            )
        );
        assert_eq!(rows[0].1, "ready");
        assert!(rows[0].2.contains("jobs_browser_profile_snapshot"));
        assert_eq!(rows[1].0, "jobs-resume-source:resume-backfill");
        assert_eq!(rows[1].1, "ready");
        assert!(rows[1].2.contains("jobs_resume_source"));
        let outbox_counts: (i64, i64) = conn
            .query_row(
                "SELECT COUNT(*),
                        SUM(CASE
                              WHEN operation = 'put' AND state = 'completed'
                               AND attempt_count >= 1 AND last_error IS NULL
                               AND completed_at_ms IS NOT NULL
                               AND next_attempt_at_ms = completed_at_ms
                               AND updated_at_ms = completed_at_ms
                              THEN 1 ELSE 0
                            END)
                   FROM object_storage_outbox WHERE account_id = ?1",
                rusqlite::params![account.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            outbox_counts,
            (2, 2),
            "adopted objects need terminal PUT history without runnable PUT work"
        );
    }

    #[test]
    fn jobs_account_object_backfill_replays_over_current_writer_metadata() {
        let path = std::env::temp_dir().join(format!(
            "bluey-jobs-current-object-replay-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let pool = open_pool(&path).unwrap();
        run_migrations(&pool).unwrap();
        let account = crate::db::accounts::Account::create(
            &pool,
            "jobs-current-object-replay@bluey.test",
            "hash",
        )
        .unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        let resume_id = "88f64a61-45e5-4d3e-9ad4-a615dfc854f5";
        let resume_sha = "c".repeat(64);
        let requested_profile_sha = "d".repeat(64);
        let resume_key = format!("accounts/{}/jobs/{resume_id}/{resume_sha}.pdf", account.id);
        let resume_metadata = serde_json::json!({
            "artifact_class": "jobs_resume_source",
            "jobs_resume_source_asset_id": resume_id,
            "request_id": resume_id,
            "profile_mode": "replace",
            "base_profile_sha256": null,
            "requested_profile_sha256": requested_profile_sha,
            "replaces_source_asset_id": null,
            "file_name": "Current Resume.pdf",
            "file_type": "pdf",
            "media_type": "application/pdf",
            "page_count": 3,
            "retention_policy": "account_lifetime_until_deletion",
        });
        assert_eq!(resume_metadata.as_object().unwrap().len(), 12);
        let limits = crate::object_storage::UploadLimits {
            max_object_bytes: 1_024,
            max_account_bytes: 4_096,
            max_daily_bytes: 4_096,
            max_account_objects: 10,
        };
        let resume_input = crate::db::object_uploads::NewObjectUpload {
            account_id: account.id.clone(),
            object_kind: crate::db::object_uploads::ObjectKind::Artifact,
            logical_id: format!("jobs-resume-source:{resume_id}"),
            session_id: None,
            storage_scope: crate::db::object_uploads::StorageScope::Artifact,
            object_key: resume_key.clone(),
            size_bytes: 128,
            sha256: resume_sha.clone(),
            content_type: "application/pdf".to_string(),
            expires_at_ms: i64::MAX,
            metadata_json: resume_metadata.clone(),
            now_ms: now,
            limits,
        };
        let resume_reservation =
            crate::db::object_uploads::reserve_account_object_upload(&pool, &resume_input).unwrap();
        let ready_resume = crate::db::object_uploads::mark_upload_ready(
            &pool,
            &resume_reservation.upload.id,
            now + 1,
        )
        .unwrap();
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO jobs_resume_source_assets (
                    id, account_id, file_name, media_type, file_type, storage_key,
                    sha256, size_bytes, page_count, template_status, created_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, 'Current Resume.pdf', 'application/pdf', 'pdf', ?3,
                           ?4, 128, 3, 'converted_layout', ?5, ?6)",
                rusqlite::params![resume_id, account.id, resume_key, resume_sha, now, now + 1],
            )
            .unwrap();

        let browser_profile_id = "profile-current";
        let browser_sha = "e".repeat(64);
        let browser_key = format!(
            "accounts/{}/jobs/browser/{browser_profile_id}/4/{browser_sha}.enc",
            account.id
        );
        let browser_metadata = serde_json::json!({
            "artifact_class": "jobs_browser_profile_snapshot",
            "jobs_browser_profile_id": browser_profile_id,
            "jobs_application_id": "application-current",
            "jobs_run_id": "run-current",
            "generation": 4,
            "expected_generation": 3,
            "writer_fence": 7,
            "envelope_version": 2,
            "retention_policy": "account_lifetime_until_deletion",
        });
        assert_eq!(browser_metadata.as_object().unwrap().len(), 9);
        let browser_input = crate::db::object_uploads::NewObjectUpload {
            account_id: account.id.clone(),
            object_kind: crate::db::object_uploads::ObjectKind::Artifact,
            logical_id: format!("jobs-browser-profile:{browser_profile_id}:4:2:{browser_sha}"),
            session_id: None,
            storage_scope: crate::db::object_uploads::StorageScope::Artifact,
            object_key: browser_key.clone(),
            size_bytes: 256,
            sha256: browser_sha.clone(),
            content_type: crate::db::jobs::BROWSER_PROFILE_SNAPSHOT_CONTENT_TYPE.to_string(),
            expires_at_ms: i64::MAX,
            metadata_json: browser_metadata.clone(),
            now_ms: now + 2,
            limits,
        };
        let browser_reservation =
            crate::db::object_uploads::reserve_account_object_upload(&pool, &browser_input)
                .unwrap();
        let ready_browser = crate::db::object_uploads::mark_upload_ready(
            &pool,
            &browser_reservation.upload.id,
            now + 3,
        )
        .unwrap();
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO jobs_browser_profile_snapshots (
                    account_id, browser_profile_id, generation, object_key, sha256,
                    size_bytes, envelope_version, writer_run_id, writer_fence, updated_at_ms
                 ) VALUES (?1, ?2, 4, ?3, ?4, 256, 2, 'run-current', 7, ?5)",
                rusqlite::params![
                    account.id,
                    browser_profile_id,
                    browser_key,
                    browser_sha,
                    now + 3
                ],
            )
            .unwrap();

        run_migrations(&pool).expect("current writer metadata must survive migration replay");

        let stored_resume = crate::db::object_uploads::artifact_upload(
            &pool,
            &account.id,
            &resume_input.logical_id,
        )
        .unwrap()
        .expect("current resume ledger row");
        let stored_browser = crate::db::object_uploads::artifact_upload(
            &pool,
            &account.id,
            &browser_input.logical_id,
        )
        .unwrap()
        .expect("current browser ledger row");
        assert_eq!(stored_resume, ready_resume);
        assert_eq!(stored_browser, ready_browser);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&stored_resume.metadata_json).unwrap(),
            resume_metadata
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&stored_browser.metadata_json).unwrap(),
            browser_metadata
        );

        let conn = pool.get().unwrap();
        let resume_pointer: (String, String, String, i64, Option<i64>, i64) = conn
            .query_row(
                "SELECT id, storage_key, sha256, size_bytes, page_count, updated_at_ms
                   FROM jobs_resume_source_assets WHERE account_id = ?1",
                rusqlite::params![account.id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(
            resume_pointer,
            (
                resume_id.to_string(),
                resume_key,
                resume_sha,
                128,
                Some(3),
                now + 1,
            )
        );
        let browser_pointer: (String, i64, String, String, i64, i64, String, i64, i64) = conn
            .query_row(
                "SELECT browser_profile_id, generation, object_key, sha256, size_bytes,
                        envelope_version, writer_run_id, writer_fence, updated_at_ms
                   FROM jobs_browser_profile_snapshots WHERE account_id = ?1",
                rusqlite::params![account.id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                        row.get(7)?,
                        row.get(8)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(
            browser_pointer,
            (
                browser_profile_id.to_string(),
                4,
                browser_key,
                browser_sha,
                256,
                2,
                "run-current".to_string(),
                7,
                now + 3,
            )
        );
        let outboxes = conn
            .prepare(
                "SELECT id, upload_id, account_id, operation, state, attempt_count,
                        next_attempt_at_ms, last_error, created_at_ms, updated_at_ms,
                        completed_at_ms
                   FROM object_storage_outbox
                  WHERE account_id = ?1
                  ORDER BY upload_id, operation",
            )
            .unwrap()
            .query_map(rusqlite::params![account.id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, i64>(8)?,
                    row.get::<_, i64>(9)?,
                    row.get::<_, Option<i64>>(10)?,
                ))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let mut expected_outboxes = vec![
            (
                format!("{}:put", ready_resume.id),
                ready_resume.id,
                account.id.clone(),
                "put".to_string(),
                "completed".to_string(),
                1,
                now + 1,
                None,
                now,
                now + 1,
                Some(now + 1),
            ),
            (
                format!("{}:put", ready_browser.id),
                ready_browser.id,
                account.id.clone(),
                "put".to_string(),
                "completed".to_string(),
                1,
                now + 3,
                None,
                now + 2,
                now + 3,
                Some(now + 3),
            ),
        ];
        expected_outboxes.sort_by(|left, right| left.1.cmp(&right.1));
        assert_eq!(outboxes, expected_outboxes);
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM object_uploads WHERE account_id = ?1",
                rusqlite::params![account.id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
            2,
            "migration replay must not add legacy-shaped duplicates"
        );
    }

    #[test]
    fn jobs_account_object_backfill_rejects_a_mismatched_logical_replay() {
        let path = std::env::temp_dir().join(format!(
            "bluey-jobs-object-backfill-conflict-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let pool = open_pool(&path).unwrap();
        run_migrations(&pool).unwrap();
        let account = crate::db::accounts::Account::create(
            &pool,
            "jobs-object-backfill-conflict@bluey.test",
            "hash",
        )
        .unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        pool.get()
            .unwrap()
            .execute_batch(&format!(
                "INSERT INTO jobs_resume_source_assets (
                    id, account_id, file_name, media_type, file_type, storage_key,
                    sha256, size_bytes, page_count, template_status, created_at_ms, updated_at_ms
                 ) VALUES (
                    'resume-conflict', '{account_id}', 'Resume.pdf', 'application/pdf', 'pdf',
                    'accounts/{account_id}/jobs/resume-conflict.pdf', '{sha}', 128, 2,
                    'converted_layout', {now}, {now}
                 );
                 INSERT INTO object_uploads (
                    id, account_id, object_kind, logical_id, session_id, storage_scope,
                    object_key, size_bytes, sha256, content_type, expires_at_ms, state,
                    metadata_json, created_at_ms, updated_at_ms, uploaded_at_ms
                 ) VALUES (
                    'preexisting-resume-conflict', '{account_id}', 'artifact',
                    'jobs-resume-source:resume-conflict', NULL, 'artifact',
                    'accounts/{account_id}/jobs/wrong-object.pdf', 128, '{sha}',
                    'application/pdf', 9223372036854775807, 'ready',
                    json_object(
                      'artifact_class', 'jobs_resume_source',
                      'jobs_resume_source_asset_id', 'resume-conflict',
                      'file_name', 'Resume.pdf',
                      'file_type', 'pdf',
                      'media_type', 'application/pdf',
                      'page_count', 2,
                      'retention_policy', 'account_lifetime_until_deletion'
                    ),
                    {now}, {now}, {now}
                 );",
                account_id = account.id,
                sha = "c".repeat(64),
            ))
            .unwrap();

        let error = run_migrations(&pool)
            .expect_err("a logical replay with a different object key must fail closed");
        assert!(
            format!("{error:#}").contains("CHECK constraint failed"),
            "unexpected migration error: {error:#}"
        );
    }

    #[test]
    fn jobs_account_object_backfill_rejects_a_mismatched_put_replay() {
        let path = std::env::temp_dir().join(format!(
            "bluey-jobs-object-backfill-put-conflict-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let pool = open_pool(&path).unwrap();
        run_migrations(&pool).unwrap();
        let account = crate::db::accounts::Account::create(
            &pool,
            "jobs-object-backfill-put-conflict@bluey.test",
            "hash",
        )
        .unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        pool.get()
            .unwrap()
            .execute_batch(&format!(
                "INSERT INTO jobs_resume_source_assets (
                    id, account_id, file_name, media_type, file_type, storage_key,
                    sha256, size_bytes, page_count, template_status, created_at_ms, updated_at_ms
                 ) VALUES (
                    'resume-put-conflict', '{account_id}', 'Resume.pdf', 'application/pdf', 'pdf',
                    'accounts/{account_id}/jobs/resume-put-conflict.pdf', '{sha}', 128, 2,
                    'converted_layout', {now}, {now}
                 );",
                account_id = account.id,
                sha = "d".repeat(64),
            ))
            .unwrap();
        run_migrations(&pool).unwrap();
        pool.get()
            .unwrap()
            .execute(
                "UPDATE object_storage_outbox
                    SET state = 'retry', last_error = 'preexisting drift',
                        completed_at_ms = NULL
                  WHERE account_id = ?1 AND operation = 'put'",
                rusqlite::params![account.id],
            )
            .unwrap();

        let error = run_migrations(&pool)
            .expect_err("a terminal PUT replay with different state must fail closed");
        assert!(
            format!("{error:#}").contains("CHECK constraint failed"),
            "unexpected migration error: {error:#}"
        );
    }
}

#[cfg(test)]
mod postgres_migration_tests {
    use super::{
        ACCOUNT_DELETION_INTENTS_MIGRATION_ID, JOBS_ACCOUNT_OBJECT_UPLOAD_BACKFILL_MIGRATION_ID,
        JOBS_BROWSER_RELEASE_AUTHORITY_MIGRATION_ID, JOBS_RUNNER_VOLUME_PURGE_MIGRATION_ID,
        JOBS_SUBMISSION_EVIDENCE_RESERVATIONS_MIGRATION_ID, POSTGRES_ACCOUNT_DELETION_INTENTS,
        POSTGRES_CONTEXT_ARTIFACT_REVISIONS, POSTGRES_JOBS_ACCOUNT_OBJECT_UPLOAD_BACKFILL,
        POSTGRES_JOBS_BROWSER_RELEASE_AUTHORITY, POSTGRES_JOBS_GLOBAL_CANDIDATE_INDEX,
        POSTGRES_JOBS_RUNNER_VOLUME_PURGE, POSTGRES_JOBS_SCHEMA,
        POSTGRES_JOBS_SUBMISSION_EVIDENCE_RESERVATIONS, POSTGRES_MIGRATIONS,
        POSTGRES_POST_JOBS_MIGRATIONS, SQLITE_ACCOUNT_DELETION_INTENTS,
        SQLITE_JOBS_ACCOUNT_OBJECT_UPLOAD_BACKFILL, SQLITE_JOBS_AUTO_SUBMIT_AUTHORIZATIONS,
        SQLITE_JOBS_BROWSER_RELEASE_AUTHORITY, SQLITE_JOBS_GLOBAL_CANDIDATE_INDEX,
        SQLITE_JOBS_RUNNER_VOLUME_PURGE, SQLITE_JOBS_SUBMISSION_EVIDENCE_RESERVATIONS,
    };

    #[test]
    fn embedded_postgres_migrations_are_operator_discoverable() {
        let migrations = POSTGRES_MIGRATIONS
            .iter()
            .copied()
            .chain(std::iter::once(("002_jobs.sql", POSTGRES_JOBS_SCHEMA)))
            .chain(POSTGRES_POST_JOBS_MIGRATIONS.iter().copied())
            .chain(std::iter::once((
                "006_context_artifact_revisions.sql",
                POSTGRES_CONTEXT_ARTIFACT_REVISIONS,
            )));

        for (version, sql) in migrations {
            assert!(
                sql.lines()
                    .take(12)
                    .any(|line| line.to_ascii_lowercase().contains("target: postgres")),
                "{version} would be skipped by scripts/bluey-postgres-migrate.sh"
            );
        }
    }

    #[test]
    fn candidate_events_are_part_of_runtime_postgres_migrations() {
        let (_, sql) = POSTGRES_POST_JOBS_MIGRATIONS
            .iter()
            .find(|(version, _)| *version == "005_jobs_candidate_events.sql")
            .expect("candidate event migration must run before Jobs routes are served");

        assert!(sql.contains("CREATE TABLE IF NOT EXISTS jobs_candidate_events"));
    }

    #[test]
    fn resume_generations_are_part_of_runtime_postgres_migrations() {
        let (_, sql) = POSTGRES_POST_JOBS_MIGRATIONS
            .iter()
            .find(|(version, _)| *version == "007_jobs_resume_generations.sql")
            .expect("resume generation migration must run before Jobs routes are served");

        assert!(sql.contains("CREATE TABLE IF NOT EXISTS jobs_resume_generations"));
        assert!(sql.contains("UNIQUE(account_id, generation_key)"));
        assert!(sql.contains("reservation_token TEXT NOT NULL"));
        assert!(sql.contains("created_at_ms BIGINT"));
    }

    #[test]
    fn discovery_board_owner_is_a_unique_post_jobs_migration() {
        let (_, sql) = POSTGRES_POST_JOBS_MIGRATIONS
            .iter()
            .find(|(version, _)| *version == "009_jobs_discovery_board_owner.sql")
            .expect("discovery board ownership must be enforced before Jobs routes are served");
        let normalized = sql.split_whitespace().collect::<Vec<_>>().join(" ");

        assert!(normalized.contains(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_discovery_sources_board_owner ON jobs_discovery_sources(account_id, provider, source_key);"
        ));
    }

    #[test]
    fn spend_cutover_migration_captures_anonymous_clamped_baseline_once() {
        let (_, sql) = POSTGRES_POST_JOBS_MIGRATIONS
            .iter()
            .find(|(version, _)| *version == "008_jobs_generation_allowance.sql")
            .expect("spend cutover migration must run before paid routes are served");

        assert!(sql.contains("CREATE TABLE IF NOT EXISTS usage_cutover_spend_baseline"));
        assert!(sql.contains("usage-cutover-spend-baseline-v1"));
        assert!(sql.contains("LOCK TABLE usage_events IN SHARE ROW EXCLUSIVE MODE"));
        assert_eq!(
            sql.matches("LOCK TABLE usage_events IN SHARE ROW EXCLUSIVE MODE")
                .count(),
            1
        );
        assert!(sql.contains(
            "WHERE name = 'usage-origin-taxonomy-repair-v1'\n  ) THEN\n    LOCK TABLE usage_events"
        ));
        assert!(sql.contains("LEAST(GREATEST(cost_cents_to_bluey, 0), 100000000)"));
        assert!(!sql.contains("usage_cutover_spend_baseline (\n  account_id"));
    }

    #[test]
    fn provider_usage_provenance_is_a_validated_post_jobs_migration() {
        let (_, sql) = POSTGRES_POST_JOBS_MIGRATIONS
            .iter()
            .find(|(version, _)| *version == "010_provider_usage_provenance.sql")
            .expect("provider provenance migration must run before paid routes are served");

        assert!(sql.contains("ADD COLUMN IF NOT EXISTS usage_provenance"));
        assert!(sql.contains("'exact', 'estimated', 'missing'"));
        assert!(sql.contains("NOT VALID"));
        assert!(sql.contains("VALIDATE CONSTRAINT jobs_provider_cost_holds_usage_provenance"));
    }

    #[test]
    fn candidate_evidence_is_an_immutable_post_jobs_migration() {
        let (_, sql) = POSTGRES_POST_JOBS_MIGRATIONS
            .iter()
            .find(|(version, _)| *version == "011_jobs_candidate_evidence.sql")
            .expect("candidate evidence migration must run before Jobs routes are served");

        assert!(sql.contains("CREATE TABLE IF NOT EXISTS jobs_profile_evidence_revisions"));
        assert!(sql.contains("UNIQUE(account_id, career_track_id, revision_no)"));
        assert!(sql.contains("UNIQUE(account_id, career_track_id, content_hash)"));
        assert!(sql.contains("CREATE TABLE IF NOT EXISTS jobs_resume_claim_evidence"));
        assert!(sql.contains("UNIQUE(account_id, resume_version_id, claim_id)"));
        assert!(sql.contains("evidence_revision_id TEXT NOT NULL REFERENCES jobs_profile_evidence_revisions(id) ON DELETE RESTRICT"));
    }

    #[test]
    fn global_candidate_index_is_a_shared_replay_safe_migration() {
        let (_, sql) = POSTGRES_POST_JOBS_MIGRATIONS
            .iter()
            .find(|(version, _)| *version == "012_jobs_global_candidate_index.sql")
            .expect("global candidate index must exist before its worker starts");

        for required in [
            "CREATE TABLE IF NOT EXISTS jobs_global_discovery_sources",
            "CREATE TABLE IF NOT EXISTS jobs_global_ingestion_runs",
            "CREATE TABLE IF NOT EXISTS jobs_global_candidates",
            "CREATE TABLE IF NOT EXISTS jobs_global_candidate_memberships",
            "CREATE TABLE IF NOT EXISTS jobs_global_ingestion_batches",
            "CREATE TABLE IF NOT EXISTS jobs_global_candidate_materializations",
            "CREATE TABLE IF NOT EXISTS jobs_global_materialization_state",
            "UNIQUE(source_id, replay_key)",
            "PRIMARY KEY(run_id, batch_index)",
        ] {
            assert!(sql.contains(required), "missing {required}");
            assert!(
                SQLITE_JOBS_GLOBAL_CANDIDATE_INDEX.contains(required),
                "SQLite migration missing {required}"
            );
        }
    }

    #[test]
    fn global_ingestion_quarantine_is_persisted_for_fresh_and_upgraded_postgres() {
        for sql in [
            POSTGRES_JOBS_GLOBAL_CANDIDATE_INDEX,
            POSTGRES_POST_JOBS_MIGRATIONS
                .iter()
                .find(|(version, _)| *version == "016_jobs_global_ingestion_quarantine.sql")
                .map(|(_, sql)| *sql)
                .expect("global ingestion quarantine migration must run before its worker starts"),
        ] {
            assert!(sql.contains("rejected_rows"));
            assert!(sql.contains("rejection_summary_json"));
        }
        assert!(POSTGRES_JOBS_GLOBAL_CANDIDATE_INDEX
            .contains("rejection_summary_json TEXT NOT NULL DEFAULT '{}'"));
    }

    #[test]
    fn auto_submit_authorizations_are_track_scoped_and_single_active_revision() {
        let (_, sql) = POSTGRES_POST_JOBS_MIGRATIONS
            .iter()
            .find(|(version, _)| *version == "018_jobs_auto_submit_authorizations.sql")
            .expect("Auto-submit authorization must exist before Jobs routes are served");

        for required in [
            "CREATE TABLE IF NOT EXISTS jobs_auto_submit_authorizations",
            "UNIQUE(account_id, career_track_id, revision_no)",
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_auto_submit_authorizations_active",
            "WHERE revoked_at_ms IS NULL",
        ] {
            assert!(sql.contains(required), "missing {required}");
            assert!(
                SQLITE_JOBS_AUTO_SUBMIT_AUTHORIZATIONS.contains(required),
                "SQLite migration missing {required}"
            );
        }
    }

    #[test]
    fn account_deletion_intents_are_runtime_migrated_with_dialect_parity() {
        let (version, postgres_sql) = POSTGRES_POST_JOBS_MIGRATIONS
            .iter()
            .find(|(version, _)| *version == ACCOUNT_DELETION_INTENTS_MIGRATION_ID)
            .expect("account-deletion intent must exist before account deletion is served");

        assert_eq!(*version, "021_account_deletion_intents.sql");
        for required in [
            "CREATE TABLE IF NOT EXISTS account_deletion_intents",
            "account_id",
            "PRIMARY KEY",
            "REFERENCES accounts(id) ON DELETE CASCADE",
            "requested_at_ms",
            "last_checked_at_ms",
            "fresh_upload_cutoff_ms",
            "fresh_in_flight_puts",
        ] {
            assert!(
                postgres_sql.contains(required),
                "PostgreSQL missing {required}"
            );
            assert!(
                SQLITE_ACCOUNT_DELETION_INTENTS.contains(required),
                "SQLite missing {required}"
            );
        }
        assert_eq!(*postgres_sql, POSTGRES_ACCOUNT_DELETION_INTENTS);
    }

    #[test]
    fn submission_evidence_capacity_is_runtime_migrated_with_dialect_parity() {
        let (version, postgres_sql) = POSTGRES_POST_JOBS_MIGRATIONS
            .iter()
            .find(|(version, _)| *version == JOBS_SUBMISSION_EVIDENCE_RESERVATIONS_MIGRATION_ID)
            .expect("submission evidence capacity must exist before Jobs execution is served");

        assert_eq!(*version, "022_jobs_submission_evidence_reservations.sql");
        for required in [
            "CREATE TABLE IF NOT EXISTS jobs_submission_evidence_capacity",
            "PRIMARY KEY(account_id, application_id, run_id)",
            "REFERENCES accounts(id) ON DELETE CASCADE",
            "REFERENCES jobs_applications(id) ON DELETE CASCADE",
            "reserved_bytes",
            "reserved_objects",
            "consumed_bytes",
            "consumed_objects",
            "expires_at_ms",
            "state",
            "WHERE state = 'active'",
        ] {
            assert!(
                postgres_sql.contains(required),
                "PostgreSQL missing {required}"
            );
            assert!(
                SQLITE_JOBS_SUBMISSION_EVIDENCE_RESERVATIONS.contains(required),
                "SQLite missing {required}"
            );
        }
        assert_eq!(
            *postgres_sql,
            POSTGRES_JOBS_SUBMISSION_EVIDENCE_RESERVATIONS
        );
    }

    #[test]
    fn jobs_account_objects_are_backfilled_with_dialect_parity() {
        let (version, postgres_sql) = POSTGRES_POST_JOBS_MIGRATIONS
            .iter()
            .find(|(version, _)| *version == JOBS_ACCOUNT_OBJECT_UPLOAD_BACKFILL_MIGRATION_ID)
            .expect("Jobs account objects must join the ledger before object writers start");

        assert_eq!(*version, "023_jobs_account_object_upload_backfill.sql");
        for required in [
            "INTO object_uploads",
            "jobs_resume_source_assets",
            "jobs_browser_profile_snapshots",
            "jobs-resume-source:",
            "jobs-browser-profile:",
            "generation || ':' ||",
            "envelope_version || ':' || lower(sha256)",
            "jobs_resume_source",
            "jobs_browser_profile_snapshot",
            "account_lifetime_until_deletion",
            "9223372036854775807",
            "'ready'",
            "ON CONFLICT (account_id, object_kind, logical_id) DO NOTHING",
            "INTO object_storage_outbox",
            "'put', 'completed', 1",
            "ON CONFLICT (upload_id, operation) DO NOTHING",
            "attempt_count >= 1",
            "completed_at_ms",
        ] {
            assert!(
                postgres_sql.contains(required),
                "PostgreSQL missing {required}"
            );
            assert!(
                SQLITE_JOBS_ACCOUNT_OBJECT_UPLOAD_BACKFILL.contains(required),
                "SQLite missing {required}"
            );
        }
        assert_eq!(*postgres_sql, POSTGRES_JOBS_ACCOUNT_OBJECT_UPLOAD_BACKFILL);
        assert!(!postgres_sql.contains("ON CONFLICT DO NOTHING"));
        assert!(!SQLITE_JOBS_ACCOUNT_OBJECT_UPLOAD_BACKFILL.contains("INSERT OR IGNORE"));
        assert!(postgres_sql
            .contains("RAISE EXCEPTION 'Jobs account-object backfill validation failed'"));
        assert!(SQLITE_JOBS_ACCOUNT_OBJECT_UPLOAD_BACKFILL
            .contains("CREATE TEMP TABLE jobs_account_object_backfill_validation"));

        let postgres_compact = postgres_sql
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        for current_shape_clause in [
            "'request_id', source.id",
            "upload.metadata_json::jsonb ->> 'profile_mode' IN ( 'replace', 'merge_source' )",
            "upload.metadata_json::jsonb ->> 'base_profile_sha256' ~ '^[0-9a-f]{64}$'",
            "upload.metadata_json::jsonb ->> 'requested_profile_sha256' ~ '^[0-9a-f]{64}$'",
            "'replaces_source_asset_id', upload.metadata_json::jsonb -> 'replaces_source_asset_id'",
            "'jobs_application_id', upload.metadata_json::jsonb -> 'jobs_application_id'",
            "'jobs_run_id', snapshot.writer_run_id",
            "'expected_generation', snapshot.generation - 1",
            "'writer_fence', snapshot.writer_fence",
            "outbox.id = upload.id || ':put'",
            "outbox.account_id = upload.account_id",
            "outbox.next_attempt_at_ms = outbox.completed_at_ms",
            "outbox.updated_at_ms = outbox.completed_at_ms",
            "outbox.created_at_ms <= outbox.completed_at_ms",
        ] {
            assert!(
                postgres_compact.contains(current_shape_clause),
                "PostgreSQL current-shape validation missing {current_shape_clause}"
            );
        }

        let sqlite_compact = SQLITE_JOBS_ACCOUNT_OBJECT_UPLOAD_BACKFILL
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        for current_shape_clause in [
            "(SELECT COUNT(*) FROM json_each(upload.metadata_json)) = 12",
            "json_extract(upload.metadata_json, '$.request_id') = source.id",
            "json_extract(upload.metadata_json, '$.profile_mode') IN ( 'replace', 'merge_source' )",
            "(SELECT COUNT(*) FROM json_each(upload.metadata_json)) = 9",
            "json_extract(upload.metadata_json, '$.jobs_run_id') = snapshot.writer_run_id",
            "json_extract(upload.metadata_json, '$.writer_fence') = snapshot.writer_fence",
            "json_extract(upload.metadata_json, '$.expected_generation') = snapshot.generation - 1",
        ] {
            assert!(
                sqlite_compact.contains(current_shape_clause),
                "SQLite current-shape validation missing {current_shape_clause}"
            );
        }

        let maximum_browser_profile_logical_id_bytes = "jobs-browser-profile:".len()
            + 240
            + 1
            + i64::MAX.to_string().len()
            + 1
            + i64::MAX.to_string().len()
            + 1
            + 64;
        assert!(
            maximum_browser_profile_logical_id_bytes <= 384,
            "the maximum accepted browser-profile candidate identity must fit the ledger"
        );
    }

    #[test]
    fn browser_release_authority_is_runtime_migrated_with_dialect_parity() {
        let (version, postgres_sql) = POSTGRES_POST_JOBS_MIGRATIONS
            .iter()
            .find(|(version, _)| *version == JOBS_BROWSER_RELEASE_AUTHORITY_MIGRATION_ID)
            .expect("Browser release authority must exist before local tickets can be claimed");

        assert_eq!(*version, "025_jobs_browser_release_authority.sql");
        for required in [
            "CREATE TABLE IF NOT EXISTS jobs_browser_release_signature_sets",
            "CREATE TABLE IF NOT EXISTS jobs_browser_release_signatures",
            "CREATE TABLE IF NOT EXISTS jobs_browser_release_trust_policies",
            "CREATE TABLE IF NOT EXISTS jobs_browser_release_trust_keys",
            "CREATE TABLE IF NOT EXISTS jobs_browser_release_manifests",
            "CREATE TABLE IF NOT EXISTS jobs_browser_release_artifacts",
            "CREATE TABLE IF NOT EXISTS jobs_browser_release_activations",
            "CREATE TABLE IF NOT EXISTS jobs_browser_release_rollbacks",
            "CREATE TABLE IF NOT EXISTS jobs_browser_release_revocations",
            "CREATE TABLE IF NOT EXISTS jobs_browser_release_channel_transitions",
            "CREATE TABLE IF NOT EXISTS jobs_browser_release_channel_heads",
            "CREATE TABLE IF NOT EXISTS jobs_browser_account_channel_assignments",
            "CREATE TABLE IF NOT EXISTS jobs_local_run_release_bindings",
            "CREATE TABLE IF NOT EXISTS jobs_local_run_claim_replays",
            "canonical_signature_set_base64url",
            "canonical_policy_base64url",
            "authorization_signature_set_sha256",
            "target_audience",
            "root_threshold",
            "release_threshold",
            "promotion_threshold",
            "incident_threshold",
            "app_version",
            "artifact_filename",
            "manifest_signature_set_sha256",
            "activation_authorization_signature_set_sha256",
            "trust_policy_sha256",
            "channel_head_revision",
            "channel_transition_sha256",
            "transition_kind",
            "rollback_authority_sha256",
            "current_transition_sha256",
            "recorded_by",
            "UNIQUE(subject_kind, subject_id, subject_sha256)",
            "subject_kind, subject_id, subject_sha256,",
            "account_channel_assignment_sha256",
            "build_descriptor_sha256",
            "claim_nonce_sha256",
            "claim_request_sha256",
            "claim_response_sha256",
            "claim_response_secret",
            "trg_jobs_browser_release_rollbacks_no_update",
            "trg_jobs_browser_release_rollbacks_no_delete",
            "trg_jobs_browser_release_revocations_no_update",
            "trg_jobs_browser_release_revocations_no_delete",
            "trg_jobs_browser_release_signature_sets_no_update",
            "trg_jobs_browser_release_signature_sets_no_delete",
            "trg_jobs_browser_release_trust_policies_no_update",
            "trg_jobs_browser_release_trust_policies_no_delete",
            "trg_jobs_browser_release_channel_transitions_no_update",
            "trg_jobs_browser_release_channel_transitions_no_delete",
            "trg_jobs_browser_release_channel_heads_no_delete",
            "trg_jobs_local_run_release_bindings_no_update",
            "trg_jobs_local_run_claim_replays_no_update",
            "Imported activations and rollback targets remain inert",
        ] {
            assert!(
                postgres_sql.contains(required),
                "PostgreSQL Browser release migration missing {required}"
            );
            assert!(
                SQLITE_JOBS_BROWSER_RELEASE_AUTHORITY.contains(required),
                "SQLite Browser release migration missing {required}"
            );
        }
        assert!(postgres_sql
            .contains("artifact_count               BIGINT NOT NULL CHECK(artifact_count = 5)"));
        assert!(SQLITE_JOBS_BROWSER_RELEASE_AUTHORITY
            .contains("artifact_count               INTEGER NOT NULL CHECK(artifact_count = 5)"));
        for exact_subject in [
            "'signing-key'",
            "'build-descriptor'",
            "'manifest'",
            "'release'",
            "'artifact'",
        ] {
            assert!(postgres_sql.contains(exact_subject));
            assert!(SQLITE_JOBS_BROWSER_RELEASE_AUTHORITY.contains(exact_subject));
        }
        assert!(!postgres_sql.contains("'artifact', 'activation'"));
        assert!(!postgres_sql.contains("accepted_server_release_ids_sha256"));
        assert!(!postgres_sql.contains("artifact_set_sha256"));
        assert!(!postgres_sql.contains("idx_jobs_browser_release_activations_current"));
        assert!(!postgres_sql.contains("trg_jobs_browser_release_channel_heads_no_update"));
        assert_eq!(
            postgres_sql
                .lines()
                .filter(|line| line.trim_start().starts_with("recorded_by "))
                .count(),
            7
        );
        assert_eq!(
            SQLITE_JOBS_BROWSER_RELEASE_AUTHORITY
                .lines()
                .filter(|line| line.trim_start().starts_with("recorded_by "))
                .count(),
            7
        );
        assert_eq!(*postgres_sql, POSTGRES_JOBS_BROWSER_RELEASE_AUTHORITY);
    }

    #[test]
    fn runner_volume_purge_is_runtime_migrated_with_dialect_parity() {
        let (version, postgres_sql) = POSTGRES_POST_JOBS_MIGRATIONS
            .iter()
            .find(|(version, _)| *version == JOBS_RUNNER_VOLUME_PURGE_MIGRATION_ID)
            .expect("runner-volume purge schema must exist before a runner can enroll");

        assert_eq!(*version, "024_jobs_runner_volume_purge.sql");
        for required in [
            "CREATE TABLE IF NOT EXISTS jobs_runner_volume_fleet_state",
            "CREATE TABLE IF NOT EXISTS jobs_runner_legacy_inventory_authorities",
            "CREATE TABLE IF NOT EXISTS jobs_runner_volume_admission_grants",
            "CREATE TABLE IF NOT EXISTS jobs_runner_volumes",
            "CREATE TABLE IF NOT EXISTS jobs_runner_volume_keys",
            "CREATE TABLE IF NOT EXISTS jobs_runner_volume_authority_uses",
            "CREATE TABLE IF NOT EXISTS jobs_runner_volume_storage_attestations",
            "CREATE TABLE IF NOT EXISTS jobs_execution_lease_volume_bindings",
            "CREATE TABLE IF NOT EXISTS jobs_runner_account_subjects",
            "CREATE TABLE IF NOT EXISTS jobs_runner_volume_residencies",
            "CREATE TABLE IF NOT EXISTS jobs_runner_purge_requests",
            "CREATE TABLE IF NOT EXISTS jobs_runner_purge_targets",
            "CREATE TABLE IF NOT EXISTS jobs_runner_purge_tombstones",
            "CREATE TABLE IF NOT EXISTS jobs_runner_purge_enforcements",
            "CREATE TABLE IF NOT EXISTS jobs_runner_volume_destructions",
            "idx_jobs_runner_volume_grants_expiry",
            "idx_jobs_runner_volumes_worker_status",
            "idx_jobs_runner_volume_authority_uses_consumed",
            "idx_jobs_runner_volume_storage_attestations_latest",
            "idx_jobs_execution_lease_volume_bindings_volume",
            "idx_jobs_runner_residencies_subject_state",
            "idx_jobs_runner_residencies_volume_state",
            "idx_jobs_runner_purge_requests_account_state",
            "idx_jobs_runner_purge_requests_deletion_attempt",
            "idx_jobs_runner_purge_targets_volume_state",
            "idx_jobs_runner_purge_targets_request_state",
            "idx_jobs_runner_purge_tombstones_request",
            "idx_jobs_runner_purge_enforcements_volume_state",
            "idx_jobs_runner_volume_destructions_volume",
            "REFERENCES accounts(id) ON DELETE SET NULL",
            "public_key_base64url",
            "enrollment_epoch",
            "issued_fleet_generation",
            "required_tombstone_generation",
            "reconciled_tombstone_generation",
            "destruction_generation",
            "legacy_reconciliation_generation",
            "storage_attestation_generation",
            "storage_attestation_count",
            "storage_attestation_set_sha256",
            "legacy_inventory_state",
            "legacy_inventory_generation",
            "legacy_inventory_reconciliation_id",
            "legacy_inventory_authority_id",
            "legacy_inventory_authority_sha256",
            "legacy_inventory_root_count",
            "legacy_inventory_root_set_sha256",
            "cutover_enrollment_generation",
            "cutover_purge_generation",
            "cutover_tombstone_generation",
            "cutover_destruction_generation",
            "cutover_legacy_reconciliation_generation",
            "cutover_storage_attestation_generation",
            "cutover_storage_attestation_count",
            "cutover_storage_attestation_set_sha256",
            "cutover_legacy_inventory_generation",
            "cutover_legacy_inventory_reconciliation_id",
            "cutover_legacy_inventory_authority_id",
            "cutover_legacy_inventory_authority_sha256",
            "cutover_non_destroyed_volume_count",
            "cutover_destruction_count",
            "cutover_unresolved_legacy_volume_count",
            "purge_subject_sha256",
            "volume_key_fingerprint",
            "command_sha256",
            "ack_signature",
            "legacy_unresolved_count",
            "resolved_target_count",
            "target_set_sha256",
            "snapshot_inventory_sha256",
            "runner legacy inventory authority is append-only",
            "runner volume storage attestation is append-only",
            "indefinite_managed_restore_safety",
        ] {
            assert!(
                postgres_sql.contains(required),
                "PostgreSQL runner purge migration missing {required}"
            );
            assert!(
                SQLITE_JOBS_RUNNER_VOLUME_PURGE.contains(required),
                "SQLite runner purge migration missing {required}"
            );
        }
        let postgres_compact = postgres_sql
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let sqlite_compact = SQLITE_JOBS_RUNNER_VOLUME_PURGE
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        assert!(postgres_compact
            .contains("purge_generation BIGINT NOT NULL DEFAULT 0 CHECK(purge_generation >= 0)"));
        assert!(sqlite_compact
            .contains("purge_generation INTEGER NOT NULL DEFAULT 0 CHECK(purge_generation >= 0)"));
        assert!(postgres_compact.contains(
            "destruction_generation BIGINT NOT NULL DEFAULT 0 CHECK(destruction_generation >= 0)"
        ));
        assert!(sqlite_compact.contains(
            "destruction_generation INTEGER NOT NULL DEFAULT 0 CHECK(destruction_generation >= 0)"
        ));
        assert!(postgres_compact.contains(
            "legacy_reconciliation_generation BIGINT NOT NULL DEFAULT 0 CHECK(legacy_reconciliation_generation >= 0)"
        ));
        assert!(sqlite_compact.contains(
            "legacy_reconciliation_generation INTEGER NOT NULL DEFAULT 0 CHECK(legacy_reconciliation_generation >= 0)"
        ));
        assert!(postgres_compact.contains(
            "legacy_unresolved SMALLINT NOT NULL DEFAULT 0 CHECK(legacy_unresolved IN (0, 1))"
        ));
        assert!(postgres_compact.contains(
            "tombstone_generation BIGINT NOT NULL UNIQUE CHECK(tombstone_generation >= 1)"
        ));
        assert!(sqlite_compact.contains(
            "tombstone_generation INTEGER NOT NULL UNIQUE CHECK(tombstone_generation >= 1)"
        ));
        assert_eq!(*postgres_sql, POSTGRES_JOBS_RUNNER_VOLUME_PURGE);
    }
}

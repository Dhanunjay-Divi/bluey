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
use tokio::runtime::{Handle, RuntimeFlavor};

pub mod account_data;
pub mod accounts;
pub mod auth_tokens;
pub mod balance;
pub mod device_codes;
pub mod devices;
pub mod diagnostic_logs;
pub mod idempotency;
pub mod jobs;
pub mod link_codes;
pub mod metrics;
pub mod ops_audit;
pub mod refresh_tokens;
pub mod signup_otps;
pub mod stt_accounting;
pub mod sync;
pub mod trial_abuse;
pub mod usage;
pub mod webhook_events;

pub type SqliteDbPool = Pool<SqliteConnectionManager>;
pub type PostgresDbPool = Pool<SafePostgresConnectionManager>;
pub type SqliteDbConn = PooledConnection<SqliteConnectionManager>;
pub type PostgresDbConn = PooledConnection<SafePostgresConnectionManager>;

pub struct SafePostgresConnectionManager {
    inner: PostgresConnectionManager<MakeTlsConnector>,
}

pub struct SafePostgresClient {
    inner: Option<postgres::Client>,
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
        })
    }

    fn is_valid(&self, client: &mut Self::Connection) -> Result<(), Self::Error> {
        self.inner.is_valid(client)
    }

    fn has_broken(&self, client: &mut Self::Connection) -> bool {
        self.inner.has_broken(client)
    }
}

#[derive(Clone)]
pub enum DbPool {
    Sqlite(SqliteDbPool),
    Postgres(PostgresDbPool),
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
            Self::Postgres(pool) => {
                if in_tokio_multithread_runtime() && !in_db_blocking_context() {
                    anyhow::bail!(
                        "Postgres DB access must run inside db::run_blocking_db while on the Tokio runtime"
                    );
                }
                pool.get().context("get postgres conn")
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
    let manager = SafePostgresConnectionManager::new(PostgresConnectionManager::new(
        pg_config,
        MakeTlsConnector::new(tls),
    ));
    let pool = Pool::builder()
        .max_size(16)
        .build(manager)
        .context("build postgres pool")?;
    Ok(DbPool::Postgres(pool))
}

/// Migrations, run in order. Each one is idempotent (CREATE TABLE IF NOT
/// EXISTS, etc.) so safe to re-run on every startup.
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
        was_fallback          INTEGER NOT NULL DEFAULT 0
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
    // 0019 - Bluey Jobs customer workspace.
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
    // 0020 - Bluey Jobs application identities and multi-inbox connections.
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
    // 0021 - immutable evidence attached to each Jobs application.
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
    // 0022 - capability-scoped local Bluey Browser launches.
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
    ensure_column(&conn, "stt_sessions", "started_at_ms", "INTEGER")?;
    ensure_column(&conn, "stt_sessions", "ended_at_ms", "INTEGER")?;
    ensure_column(&conn, "stt_sessions", "relay_close_reason", "TEXT")?;
    ensure_column(&conn, "device_codes", "device_id", "TEXT")?;
    ensure_column(&conn, "device_codes", "device_name", "TEXT")?;
    ensure_column(&conn, "device_codes", "platform", "TEXT")?;
    ensure_column(&conn, "device_codes", "arch", "TEXT")?;
    ensure_column(&conn, "device_codes", "app_version", "TEXT")?;
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
    conn.execute_batch(
        r#"
        CREATE INDEX IF NOT EXISTS idx_trial_grants_email_domain
            ON trial_grants(email_domain_hash, created_at);
        CREATE INDEX IF NOT EXISTS idx_trial_abuse_events_email_domain
            ON trial_abuse_events(email_domain_hash, created_at);
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
const POSTGRES_JOBS_SCHEMA: &str =
    include_str!("../../../infra/postgres/server-runtime/002_jobs.sql");

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

    let version = "001_server_runtime_compat.sql";
    let already = conn
        .query_opt(
            "SELECT 1 FROM bluey_schema_migrations WHERE version = $1",
            &[&version],
        )
        .context("check postgres migration ledger")?
        .is_some();
    if !already {
        conn.batch_execute(POSTGRES_RUNTIME_SCHEMA)
            .context("apply postgres runtime compatibility schema")?;
        conn.execute(
            "INSERT INTO bluey_schema_migrations(version) VALUES ($1)
             ON CONFLICT (version) DO NOTHING",
            &[&version],
        )
        .context("record postgres migration")?;
    } else {
        // The schema is idempotent and should stay self-healing as columns
        // are added before the full adapter lands.
        conn.batch_execute(POSTGRES_RUNTIME_SCHEMA)
            .context("refresh postgres runtime compatibility schema")?;
    }

    conn.batch_execute(POSTGRES_JOBS_SCHEMA)
        .context("apply postgres Jobs schema")?;
    conn.execute(
        "INSERT INTO bluey_schema_migrations(version) VALUES ($1)
         ON CONFLICT (version) DO NOTHING",
        &[&"002_jobs.sql"],
    )
    .context("record postgres Jobs migration")?;

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
        migration = version,
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

//! Database access layer. SQLite via r2d2 connection pool.
//!
//! Schema migrations are inline strings (no external migrator) — small,
//! linear, easy to read. Each migration is idempotent and runs on every
//! server startup; new migrations append to MIGRATIONS.

use anyhow::{Context, Result};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use std::path::Path;

pub mod accounts;
pub mod auth_tokens;
pub mod balance;
pub mod device_codes;
pub mod idempotency;
pub mod link_codes;
pub mod refresh_tokens;
pub mod signup_otps;
pub mod sync;
pub mod usage;

pub type DbPool = Pool<SqliteConnectionManager>;

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
    Ok(pool)
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
        trial_seconds_remaining     INTEGER NOT NULL DEFAULT 600,      -- 10 min free trial
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
        attempts        INTEGER NOT NULL DEFAULT 0,
        created_at      DATETIME NOT NULL DEFAULT (datetime('now')),
        expires_at      DATETIME NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_signup_otps_expires_at
        ON signup_otps(expires_at);
    "#,
];

pub fn run_migrations(pool: &DbPool) -> Result<()> {
    let conn = pool.get().context("get conn")?;
    for (i, sql) in MIGRATIONS.iter().enumerate() {
        conn.execute_batch(sql)
            .with_context(|| format!("migration {} failed", i + 1))?;
    }
    ensure_column(&conn, "stt_sessions", "started_at_ms", "INTEGER")?;
    ensure_column(&conn, "stt_sessions", "ended_at_ms", "INTEGER")?;
    ensure_column(&conn, "stt_sessions", "relay_close_reason", "TEXT")?;
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
    tracing::info!(count = MIGRATIONS.len(), "migrations applied");
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

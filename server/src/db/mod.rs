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
pub mod balance;
pub mod idempotency;
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
        auto_topup_enabled          INTEGER NOT NULL DEFAULT 1,
        auto_topup_threshold_cents  INTEGER NOT NULL DEFAULT 500,      -- $5
        auto_topup_amount_cents     INTEGER NOT NULL DEFAULT 3000,     -- $30
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
    // 0004 — device_codes: OAuth-style device flow (bluey login)
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
        lane                  TEXT,                      -- instant | balanced | deep | vision | local
        provider              TEXT,                      -- openai | anthropic | ollama | deepgram
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
];

pub fn run_migrations(pool: &DbPool) -> Result<()> {
    let conn = pool.get().context("get conn")?;
    for (i, sql) in MIGRATIONS.iter().enumerate() {
        conn.execute_batch(sql)
            .with_context(|| format!("migration {} failed", i + 1))?;
    }
    tracing::info!(count = MIGRATIONS.len(), "migrations applied");
    Ok(())
}

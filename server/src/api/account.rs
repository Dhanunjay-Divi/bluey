//! Account endpoints — real implementations.

use axum::{extract::State, http::StatusCode, Extension, Json};
use serde::Serialize;

use super::AppState;
use crate::auth::AuthedAccount;

#[derive(Serialize)]
pub struct AccountMe {
    pub id: String,
    pub email: String,
    pub balance_cents: i64,
    pub trial_seconds_remaining: i64,
    pub auto_topup_enabled: bool,
    pub auto_topup_threshold_cents: i64,
    pub auto_topup_amount_cents: i64,
}

pub async fn me(Extension(AuthedAccount(account)): Extension<AuthedAccount>) -> Json<AccountMe> {
    Json(AccountMe {
        id: account.id,
        email: account.email,
        balance_cents: account.balance_cents,
        trial_seconds_remaining: account.trial_seconds_remaining,
        auto_topup_enabled: account.auto_topup_enabled,
        auto_topup_threshold_cents: account.auto_topup_threshold_cents,
        auto_topup_amount_cents: account.auto_topup_amount_cents,
    })
}

#[derive(Serialize)]
pub struct UsageWindow {
    pub period_days: i64,
    pub total_cues: i64,
    pub total_cents_spent: i64,
    pub mix: Vec<MixEntry>,
    pub tier_label: String,
    pub projected_days_remaining: f64,
}

#[derive(Serialize)]
pub struct MixEntry {
    pub task_type: String,
    pub count: i64,
    pub cost_cents: i64,
    pub percent: f64,
}

const PERIOD_DAYS: i64 = 7;

pub async fn usage(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
) -> Result<Json<UsageWindow>, StatusCode> {
    let conn = state
        .pool
        .get()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    // Codex Stage 7 S7.2: use SQLite datetime() consistently. Stored
    // ts is "YYYY-MM-DD HH:MM:SS" via datetime('now') default; we
    // compute the cutoff the same way to avoid mixed-format text
    // comparison breaking around boundaries.
    let _ = PERIOD_DAYS; // (referenced in datetime literal below)

    let (total_cues, total_cents_spent): (i64, i64) = conn
        .query_row(
            "SELECT COUNT(*), COALESCE(SUM(cost_cents_to_customer), 0)
             FROM usage_events
             WHERE account_id = ?1 AND ts >= datetime('now', '-7 days')",
            rusqlite::params![&account.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Per-task-type breakdown. Uses lane as the bucket when task_type
    // is NULL because v0.1 client-side classifier metadata isn't always
    // forwarded into usage_events. Coalesce to "general" for legibility.
    let mut stmt = conn
        .prepare(
            "SELECT COALESCE(task_type, lane, 'general') AS bucket,
                    COUNT(*) AS cnt,
                    COALESCE(SUM(cost_cents_to_customer), 0) AS cost
             FROM usage_events
             WHERE account_id = ?1 AND ts >= datetime('now', '-7 days')
             GROUP BY bucket
             ORDER BY cost DESC",
        )
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mix_raw: Vec<(String, i64, i64)> = stmt
        .query_map(rusqlite::params![&account.id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .filter_map(|r| r.ok())
        .collect();

    let mix: Vec<MixEntry> = mix_raw
        .into_iter()
        .map(|(t, count, cost)| {
            let percent = if total_cues > 0 {
                100.0 * (count as f64) / (total_cues as f64)
            } else {
                0.0
            };
            MixEntry {
                task_type: t,
                count,
                cost_cents: cost,
                percent,
            }
        })
        .collect();

    // Tier classification + projection.
    let avg_cost_per_cue_cents = if total_cues > 0 {
        (total_cents_spent as f64) / (total_cues as f64)
    } else {
        2.2 // typical-tier default ~2.2 cents/cue
    };
    let cues_per_30 = if avg_cost_per_cue_cents > 0.0 {
        3000.0 / avg_cost_per_cue_cents
    } else {
        f64::INFINITY
    };
    let tier_label = if cues_per_30 >= 2200.0 {
        "Light"
    } else if cues_per_30 >= 1100.0 {
        "Typical tech"
    } else {
        "Heavy"
    };

    // Projected days remaining at current burn rate.
    let cues_per_day = (total_cues as f64) / (PERIOD_DAYS as f64);
    let avg_cents_per_day = cues_per_day * avg_cost_per_cue_cents;
    let projected_days_remaining = if avg_cents_per_day > 0.01 {
        (account.balance_cents as f64) / avg_cents_per_day
    } else {
        365.0 // no usage yet → cap at 1 year (credit-validity boundary)
    };

    Ok(Json(UsageWindow {
        period_days: PERIOD_DAYS,
        total_cues,
        total_cents_spent,
        mix,
        tier_label: tier_label.to_string(),
        projected_days_remaining: (projected_days_remaining * 10.0).round() / 10.0,
    }))
}

//! Bluey CLI cloud-account commands: `bluey usage`, `bluey credits`.
//!
//! These talk to bluey-server via cue-cloud-client, using the auth
//! token stored in keyring after `bluey login`. They print
//! customer-facing summaries to stdout matching the format documented
//! in `docs/PRICING-MODEL.md` Section 4.3.

use anyhow::{Context, Result};
use cue_cloud_client::{AccountMe, CloudClient, UsageWindow};

/// Print the user's current balance + auto-top-up status + tier
/// projection.
pub async fn show_usage(client: &CloudClient) -> Result<()> {
    let me: AccountMe = client
        .auth_get("/account/me")
        .await
        .context("/account/me")?;
    let usage: UsageWindow = client
        .auth_get("/account/usage")
        .await
        .context("/account/usage")?;

    let balance_dollars = me.balance_cents as f64 / 100.0;
    let topup_dollars = me.auto_topup_amount_cents as f64 / 100.0;
    let topup_threshold = me.auto_topup_threshold_cents as f64 / 100.0;
    let topup = if me.auto_topup_enabled {
        format!("ON, ${topup_dollars:.0} at <${topup_threshold:.0}")
    } else {
        "OFF".to_string()
    };

    println!();
    println!("Balance         ${balance_dollars:.2}      (auto top-up: {topup})");
    println!(
        "Last 7 days     {} cues, ${:.2} spent",
        usage.total_cues,
        usage.total_cents_spent as f64 / 100.0,
    );
    println!("Tier            {} user", usage.tier_label);
    println!(
        "Projection      ${:.2} lasts ~{:.0} days at your current rate",
        balance_dollars, usage.projected_days_remaining,
    );

    if me.trial_seconds_remaining > 0 {
        let mins = (me.trial_seconds_remaining as f64 / 60.0).round();
        println!();
        println!("Free trial      {mins:.0} minutes remaining");
    }

    if !usage.mix.is_empty() {
        println!();
        println!("Tier comparison:");
        println!("  Light       ~3,000 cues per $30 (~3 months)");
        println!("  Typical     ~1,380 cues per $30 (~5 weeks)");
        println!("  Heavy         ~825 cues per $30 (~10 days)");
        println!();
        println!("Last 7 days breakdown:");
        for entry in &usage.mix {
            println!(
                "  {:<22} {:>4} ({:>4.1}%)  ${:.2}",
                entry.task_type,
                entry.count,
                entry.percent,
                entry.cost_cents as f64 / 100.0,
            );
        }
    }

    println!();
    println!("Credits expire 1 year from purchase. Run `bluey credits` for batch-by-batch dates.");
    Ok(())
}

/// Print per-batch credit expiration dates. (Stub: server endpoint for
/// per-batch listing isn't built yet; will be added when needed. For
/// now we print the headline reminder.)
pub async fn show_credits(client: &CloudClient) -> Result<()> {
    let me: AccountMe = client
        .auth_get("/account/me")
        .await
        .context("/account/me")?;
    println!();
    println!("Balance: ${:.2}", me.balance_cents as f64 / 100.0,);
    println!();
    println!("Per-batch expiration listing is coming in a future release.");
    println!("For now: every $30 reload stays active for 1 year from its purchase date.");
    Ok(())
}

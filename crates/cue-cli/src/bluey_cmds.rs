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

    // Codex Stage 10 (S8.3 nit): tier numbers fetched from server.
    let tiers = fetch_pricing_tiers(client).await;

    if !usage.mix.is_empty() {
        println!();
        println!("Tier comparison:");
        if let Some(t) = tiers.as_ref() {
            for tier in &t.tiers {
                let dollars = t.reload_amount_cents as f64 / 100.0;
                println!(
                    "  {:<11} ~{:>5} cues per ${:.0} ({})",
                    tier.label, tier.cues_per_reload, dollars, tier.typical_duration_label,
                );
            }
        } else {
            // Fallback to canonical defaults if /pricing/tiers unreachable.
            println!("  Light       ~3,000 cues per $30 (~3 months)");
            println!("  Typical     ~1,380 cues per $30 (~5 weeks)");
            println!("  Heavy         ~825 cues per $30 (~10 days)");
        }
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
    println!("Credits expire 1 year from purchase. Per-batch expiration listing is coming in a future release.");
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

async fn fetch_pricing_tiers(client: &CloudClient) -> Option<cue_cloud_client::PricingTiers> {
    // /pricing/tiers is public (no auth). Use the unauthenticated GET path.
    match client
        .public_get::<cue_cloud_client::PricingTiers>("/pricing/tiers")
        .await
    {
        Ok(t) => Some(t),
        Err(e) => {
            eprintln!("could not fetch /pricing/tiers: {e}");
            None
        }
    }
}

/// Codex Stage 16: bluey logout — clear the keyring tokens.
pub async fn logout(client: &CloudClient) -> Result<()> {
    if client.current_tokens().is_none() {
        println!("Already logged out.");
        return Ok(());
    }
    client.clear_tokens()?;
    println!("Bluey account logged out (keyring cleared).");
    Ok(())
}

/// Codex Stage 16: bluey portal — open Stripe Customer Portal in browser.
pub async fn portal(client: &CloudClient) -> Result<()> {
    #[derive(serde::Deserialize)]
    struct PortalResponse {
        portal_url: String,
    }
    let resp: PortalResponse = client
        .auth_post("/billing/portal", &serde_json::json!({}))
        .await
        .context("/billing/portal")?;
    println!("Opening Stripe Customer Portal:");
    println!("  {}", resp.portal_url);
    if let Err(e) = webbrowser::open(&resp.portal_url) {
        eprintln!("(could not open browser: {e}; copy the URL above)");
    }
    Ok(())
}

/// Codex Stage 16: bluey export — download account data as JSON.
pub async fn export_data(client: &CloudClient) -> Result<()> {
    let bundle: serde_json::Value = client
        .auth_get("/account/export")
        .await
        .context("/account/export")?;
    let pretty = serde_json::to_string_pretty(&bundle)?;
    let filename = format!(
        "bluey-export-{}.json",
        chrono::Utc::now().format("%Y%m%d-%H%M%S")
    );
    std::fs::write(&filename, pretty)?;
    println!("Exported account data to {filename}");
    Ok(())
}

/// Codex Stage 16: bluey delete-account. REQUIRES interactive confirmation.
pub async fn delete_account(client: &CloudClient, force: bool) -> Result<()> {
    if !force {
        println!();
        println!("⚠  This will PERMANENTLY DELETE your Bluey account.");
        println!("   - All credit batches forfeited (1-year validity does NOT apply on delete).");
        println!("   - All usage history removed.");
        println!("   - Refund eligibility check via support@bluey.dev BEFORE deletion if you have unused credits.");
        println!();
        println!("Type DELETE to confirm:");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if input.trim() != "DELETE" {
            println!("Aborted.");
            return Ok(());
        }
    }
    #[derive(serde::Deserialize)]
    struct DeleteAck {
        deleted: bool,
        deleted_at: String,
    }
    let ack: DeleteAck = client
        .auth_post("/account/delete", &serde_json::json!({}))
        .await
        .context("/account/delete")?;
    if ack.deleted {
        let _ = client.clear_tokens();
        println!("Account deleted at {}.", ack.deleted_at);
        println!("Local keyring tokens cleared.");
    }
    Ok(())
}

//! Bluey CLI cloud-account commands: `bluey usage`, `bluey credits`.
//!
//! These talk to bluey-server via cue-cloud-client, using the auth
//! token stored in the local account profile after sign-in. They print
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
    let topup = auto_topup_status_label(&me);

    println!();
    println!("Balance         ${balance_dollars:.2}      (auto top-up: {topup})");
    println!(
        "Last 7 days     {} cues, ${:.2} spent",
        usage.total_cues,
        usage.total_cents_spent as f64 / 100.0,
    );
    println!("Tier            {} user", usage.tier_label);
    let projection = if usage.projection_label.trim().is_empty() {
        format!(
            "${balance_dollars:.2} lasts ~{:.0} days at your current rate",
            usage.projected_days_remaining
        )
    } else {
        usage.projection_label.clone()
    };
    println!("Projection      {projection}");

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

fn auto_topup_status_label(me: &AccountMe) -> String {
    if me.billing_restricted.unwrap_or(false) {
        return me
            .billing_restriction_reason
            .as_deref()
            .filter(|reason| !reason.trim().is_empty())
            .map(|reason| format!("PAUSED, {reason}"))
            .unwrap_or_else(|| "PAUSED".to_string());
    }
    if !me.auto_topup_enabled {
        return "OFF".to_string();
    }

    let topup_dollars = me.auto_topup_amount_cents as f64 / 100.0;
    let topup_threshold = me.auto_topup_threshold_cents as f64 / 100.0;
    let ready = me.auto_topup_available.unwrap_or(true);
    if ready {
        if let Some(label) = me
            .saved_payment_method_label
            .as_deref()
            .filter(|label| !label.trim().is_empty())
        {
            return format!("ON, ${topup_dollars:.0} at <${topup_threshold:.0} ({label})");
        }
        return format!("ON, ${topup_dollars:.0} at <${topup_threshold:.0}");
    }

    let reason = me
        .auto_topup_unavailable_reason
        .as_deref()
        .map(str::trim)
        .filter(|reason| !reason.is_empty())
        .unwrap_or("save a card first");
    format!("ON, setup needed: {reason}")
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

#[cfg(test)]
mod tests {
    use super::*;

    fn account_me(auto_topup_enabled: bool) -> AccountMe {
        AccountMe {
            id: "acct_test".to_string(),
            email: "test@example.com".to_string(),
            balance_cents: 482,
            trial_seconds_remaining: 0,
            auto_topup_enabled,
            auto_topup_threshold_cents: 500,
            auto_topup_amount_cents: 3000,
            billing_provider: Some("square".to_string()),
            auto_topup_available: None,
            auto_topup_unavailable_reason: None,
            saved_payment_method_label: None,
            square_environment: Some("production".to_string()),
            billing_restricted: Some(false),
            billing_restriction_reason: None,
        }
    }

    #[test]
    fn auto_topup_label_shows_ready_saved_card() {
        let mut me = account_me(true);
        me.auto_topup_available = Some(true);
        me.saved_payment_method_label = Some("Visa ending 4242".to_string());

        assert_eq!(
            auto_topup_status_label(&me),
            "ON, $30 at <$5 (Visa ending 4242)"
        );
    }

    #[test]
    fn auto_topup_label_shows_setup_needed_when_enabled_without_card() {
        let mut me = account_me(true);
        me.auto_topup_available = Some(false);
        me.auto_topup_unavailable_reason =
            Some("Save a card for Auto Reload before turning this on.".to_string());

        assert_eq!(
            auto_topup_status_label(&me),
            "ON, setup needed: Save a card for Auto Reload before turning this on."
        );
    }

    #[test]
    fn auto_topup_label_shows_paused_for_restricted_billing() {
        let mut me = account_me(true);
        me.billing_restricted = Some(true);
        me.billing_restriction_reason =
            Some("Billing is paused while this account is under review.".to_string());

        assert_eq!(
            auto_topup_status_label(&me),
            "PAUSED, Billing is paused while this account is under review."
        );
    }
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

/// Codex Stage 16: bluey logout — clear local account tokens.
pub async fn logout(client: &CloudClient) -> Result<()> {
    if client.current_tokens().is_none() {
        println!("Already logged out.");
        return Ok(());
    }
    client.clear_tokens()?;
    println!("Bluey account logged out.");
    Ok(())
}

/// Codex Stage 16: bluey portal — open the provider-backed billing page in browser.
pub async fn portal(client: &CloudClient) -> Result<()> {
    #[derive(serde::Deserialize)]
    struct PortalResponse {
        portal_url: String,
    }
    let resp: PortalResponse = client
        .auth_post("/billing/portal", &serde_json::json!({}))
        .await
        .context("/billing/portal")?;
    println!("Opening Bluey billing page:");
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
        println!("   - Any unused Bluey credits are lost and cannot be used after deletion.");
        println!("   - All usage history removed.");
        println!("   - Refund eligibility check via support@bluey.sh BEFORE deletion if you have unused credits.");
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
        .auth_post(
            "/account/delete",
            &serde_json::json!({
                "confirm_text": "DELETE",
                "accept_data_loss": true,
                "accept_credit_loss": true
            }),
        )
        .await
        .context("/account/delete")?;
    if ack.deleted {
        let _ = client.clear_tokens();
        println!("Account deleted at {}.", ack.deleted_at);
        println!("Local account tokens cleared.");
    }
    Ok(())
}

//! Bluey CLI cloud-account commands: `bluey usage`, `bluey credits`.
//!
//! These talk to bluey-server via cue-cloud-client, using the auth
//! token stored in the local account profile after sign-in. They print
//! customer-facing summaries to stdout matching the format documented
//! in `docs/PRICING-MODEL.md` Section 4.3.

use anyhow::{bail, Context, Result};
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
#[derive(serde::Deserialize)]
struct DeleteAccountResponse {
    deleted: bool,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    deleted_at: Option<String>,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    retry_after_ms: Option<u64>,
}

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
    let response = client
        .auth_post_raw(
            "/account/delete",
            &serde_json::json!({
                "confirm_text": "DELETE",
                "accept_data_loss": true,
                "accept_credit_loss": true
            }),
        )
        .await
        .context("/account/delete")?;
    let status = response.status();
    let ack: DeleteAccountResponse = response
        .json()
        .await
        .context("parse /account/delete response")?;
    let verified_deleted = status == reqwest::StatusCode::OK
        && ack.deleted
        && ack.state.as_deref() == Some("deleted")
        && ack
            .deleted_at
            .as_deref()
            .is_some_and(|value| !value.is_empty());
    if verified_deleted {
        let _ = client.clear_tokens();
        println!(
            "Account deleted at {}.",
            ack.deleted_at
                .as_deref()
                .unwrap_or("the server-confirmed time")
        );
        println!("Local account tokens cleared.");
    } else if status == reqwest::StatusCode::ACCEPTED && !ack.deleted {
        println!(
            "{}",
            ack.note.as_deref().unwrap_or(
                "Account deletion is securely pending runner-volume and storage cleanup."
            )
        );
        if let Some(retry_after_ms) = ack.retry_after_ms {
            println!(
                "Retry after about {} seconds.",
                retry_after_ms.div_ceil(1_000)
            );
        }
        if let Some(state) = ack.state.as_deref() {
            println!("Deletion state: {state}.");
        }
        println!("Local account tokens were kept so deletion can be checked again.");
    } else {
        bail!(
            "Account deletion returned an inconsistent completion response (HTTP {status}); local account tokens were kept."
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cue_cloud_client::{
        client::ClientConfig,
        tokens::{MemoryStore, Tokens},
    };
    use std::{sync::Arc, time::Duration};
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    fn deletion_client(origin: String) -> (CloudClient, Tokens) {
        let tokens = Tokens {
            access: "access-token".to_string(),
            refresh: "refresh-token".to_string(),
            email: "owner@example.test".to_string(),
        };
        let store = Arc::new(MemoryStore::new());
        let client = CloudClient::new(
            ClientConfig {
                base_url: origin,
                user_agent: "bluey-delete-test".to_string(),
                timeout: Duration::from_secs(5),
                trace_id: None,
            },
            store,
        )
        .unwrap();
        client.save_tokens(tokens.clone()).unwrap();
        (client, tokens)
    }

    #[test]
    fn pending_account_deletion_response_never_looks_deleted() {
        let response: DeleteAccountResponse = serde_json::from_value(serde_json::json!({
            "deleted": false,
            "state": "pending_runner_volume_purge",
            "retry_after_ms": 5_000,
            "note": "Runner cleanup is pending."
        }))
        .unwrap();
        assert!(!response.deleted);
        assert_eq!(response.deleted_at, None);
        assert_eq!(
            response.state.as_deref(),
            Some("pending_runner_volume_purge")
        );
        assert_eq!(response.retry_after_ms, Some(5_000));
    }

    #[tokio::test]
    async fn pending_account_delete_http_response_keeps_tokens() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/account/delete"))
            .respond_with(ResponseTemplate::new(202).set_body_json(serde_json::json!({
                "deleted": false,
                "state": "pending_runner_volume_purge",
                "retry_after_ms": 5_000,
                "note": "Runner cleanup is pending."
            })))
            .expect(1)
            .mount(&server)
            .await;
        let (client, expected_tokens) = deletion_client(server.uri());

        delete_account(&client, true).await.unwrap();

        assert_eq!(client.current_tokens(), Some(expected_tokens));
    }

    #[tokio::test]
    async fn completed_account_delete_http_response_clears_tokens() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/account/delete"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "deleted": true,
                "state": "deleted",
                "deleted_at": "2026-08-05T12:00:00Z",
                "object_count_deleted": 0,
                "note": "Deleted."
            })))
            .expect(1)
            .mount(&server)
            .await;
        let (client, _) = deletion_client(server.uri());

        delete_account(&client, true).await.unwrap();

        assert_eq!(client.current_tokens(), None);
    }

    #[tokio::test]
    async fn accepted_delete_claim_cannot_clear_tokens() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/account/delete"))
            .respond_with(ResponseTemplate::new(202).set_body_json(serde_json::json!({
                "deleted": true,
                "state": "deleted",
                "deleted_at": "2026-08-05T12:00:00Z"
            })))
            .expect(1)
            .mount(&server)
            .await;
        let (client, expected_tokens) = deletion_client(server.uri());

        let error = delete_account(&client, true)
            .await
            .expect_err("202 must never prove hard deletion");

        assert!(error
            .to_string()
            .contains("inconsistent completion response"));
        assert_eq!(client.current_tokens(), Some(expected_tokens));
    }

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

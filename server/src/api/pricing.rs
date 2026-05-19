//! /pricing/tiers — server-owned tier numbers.
//!
//! Codex Stage 10 (S8.3 nit): the CLI bluey usage table previously
//! hard-coded "Light ~3,000 cues per $30" etc. Those numbers belong
//! on the server so future rebalancing doesn't require a CLI release.
//!
//! Public endpoint (no auth). The numbers are the canonical source of
//! truth; PRICING-MODEL.md should defer to this endpoint.

use axum::Json;
use serde::Serialize;

#[derive(Serialize)]
pub struct PricingTiers {
    pub reload_amount_cents: i64,
    pub minimum_cue_cents: i64,
    pub tiers: Vec<Tier>,
    pub markup_percent: MarkupPercent,
    pub snapshot_date: &'static str,
}

#[derive(Serialize)]
pub struct Tier {
    pub name: &'static str,
    pub label: &'static str,
    pub cues_per_reload: i64,
    pub typical_duration_label: &'static str,
}

#[derive(Serialize)]
pub struct MarkupPercent {
    pub easy: u32,
    pub medium: u32,
    pub deep: u32,
    pub vision: u32,
}

pub async fn get_tiers() -> Json<PricingTiers> {
    Json(PricingTiers {
        reload_amount_cents: 3000,
        minimum_cue_cents: 1,
        snapshot_date: "2026-05-19",
        markup_percent: MarkupPercent {
            easy: 200,
            medium: 200,
            deep: 150,
            vision: 150,
        },
        tiers: vec![
            Tier {
                name: "light",
                label: "Light",
                cues_per_reload: 3000,
                typical_duration_label: "~3 months",
            },
            Tier {
                name: "typical",
                label: "Typical tech",
                cues_per_reload: 1380,
                typical_duration_label: "~5 weeks",
            },
            Tier {
                name: "heavy",
                label: "Heavy",
                cues_per_reload: 825,
                typical_duration_label: "~10 days",
            },
        ],
    })
}

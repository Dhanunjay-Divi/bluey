//! Per-model pricing table + markup tiers.
//!
//! Source of truth lives here AND in `docs/PRICING-MODEL.md`. Any
//! change to either must be reflected in both. Last reconciled
//! 2026-06-20 against PRICING-MODEL.md and MODEL-ROUTING.md.
//!
//! ## Unit semantics
//!
//! `upstream_in_microcents_per_1m` is the upstream provider's price
//! in MICROCENTS per 1 million tokens. 1 cent = 10,000 microcents.
//! So a list price of $3 per 1M tokens (Anthropic Claude Sonnet)
//! is 300 cents/1M = 3,000,000 microcents/1M.
//!
//! This integer-microcent representation lets us compute fractional
//! cents without floats. The cost function rounds final customer-cents
//! UP to the nearest cent so we never undercharge by sub-cent fractions.

#[derive(Debug, Clone, Copy)]
pub struct ModelPricing {
    pub provider: &'static str,
    pub model: &'static str,
    /// Bluey upstream cost per 1M input tokens (microcents).
    pub upstream_in_microcents_per_1m: i64,
    /// Bluey upstream cost per 1M output tokens (microcents).
    pub upstream_out_microcents_per_1m: i64,
    /// Markup applied. 200 = 200% markup → customer pays 3× upstream.
    pub markup_percent: i64,
}

const MICROCENTS_PER_CENT: i64 = 10_000;

pub const PRICING: &[ModelPricing] = &[
    ModelPricing {
        // OpenAI gpt-5.4-mini: $0.75/1M in, $4.50/1M out (list 2026-06).
        provider: "openai",
        model: "gpt-5.4-mini",
        upstream_in_microcents_per_1m: 750_000,
        upstream_out_microcents_per_1m: 4_500_000,
        markup_percent: 200,
    },
    ModelPricing {
        // OpenAI gpt-5.5 (flagship/vision-capable): $5/1M in, $30/1M out.
        provider: "openai",
        model: "gpt-5.5",
        upstream_in_microcents_per_1m: 5_000_000,
        upstream_out_microcents_per_1m: 30_000_000,
        markup_percent: 150,
    },
    ModelPricing {
        // Anthropic Claude Sonnet 4.6: $3/1M in, $15/1M out.
        provider: "anthropic",
        model: "claude-sonnet-4-6",
        upstream_in_microcents_per_1m: 3_000_000,
        upstream_out_microcents_per_1m: 15_000_000,
        markup_percent: 200,
    },
    ModelPricing {
        // Anthropic Claude Opus 4.8: $5/1M in, $25/1M out.
        provider: "anthropic",
        model: "claude-opus-4-8",
        upstream_in_microcents_per_1m: 5_000_000,
        upstream_out_microcents_per_1m: 25_000_000,
        markup_percent: 150,
    },
    ModelPricing {
        // Anthropic Claude Haiku 4.5: $1/1M in, $5/1M out.
        provider: "anthropic",
        model: "claude-haiku-4-5-20251001",
        upstream_in_microcents_per_1m: 1_000_000,
        upstream_out_microcents_per_1m: 5_000_000,
        markup_percent: 200,
    },
    ModelPricing {
        // Gemini 3.1 Pro preview: use the conservative high-context tier
        // ($0.90/1M in, $5.40/1M out) until live prompt sizes are measured.
        provider: "gemini",
        model: "gemini-3.1-pro-preview",
        upstream_in_microcents_per_1m: 900_000,
        upstream_out_microcents_per_1m: 5_400_000,
        markup_percent: 150,
    },
    ModelPricing {
        // Gemini 3 Flash preview: $0.50/1M in, $3/1M out.
        provider: "gemini",
        model: "gemini-3-flash-preview",
        upstream_in_microcents_per_1m: 500_000,
        upstream_out_microcents_per_1m: 3_000_000,
        markup_percent: 200,
    },
    ModelPricing {
        // Gemini 3.1 Flash-Lite: $0.25/1M in, $1.50/1M out.
        provider: "gemini",
        model: "gemini-3.1-flash-lite",
        upstream_in_microcents_per_1m: 250_000,
        upstream_out_microcents_per_1m: 1_500_000,
        markup_percent: 200,
    },
    ModelPricing {
        // Ollama (local fallback): no upstream cost, no markup.
        provider: "ollama",
        model: "llama3.1",
        upstream_in_microcents_per_1m: 0,
        upstream_out_microcents_per_1m: 0,
        markup_percent: 0,
    },
    ModelPricing {
        // OpenAI text-embedding-3-small: $0.02 per 1M input tokens.
        // $0.02 = 2 cents = 20_000 microcents. No output cost.
        provider: "openai",
        model: "text-embedding-3-small",
        upstream_in_microcents_per_1m: 20_000,
        upstream_out_microcents_per_1m: 0,
        markup_percent: 200,
    },
    ModelPricing {
        // Deepgram nova-3 accuracy-first streaming: $0.0092/minute =
        // 0.92 cents/minute = 9_200 microcents/minute.
        // With 1 minute = 60 seconds, microcents per 1M seconds =
        // 9_200 * 1_000_000 / 60 = 153_333_333 (rounded down).
        // We use seconds as the input_tokens unit so the existing
        // pricing::compute_cost flow works unchanged.
        provider: "deepgram",
        model: "nova-3",
        upstream_in_microcents_per_1m: 153_333_333,
        upstream_out_microcents_per_1m: 0,
        markup_percent: 200,
    },
    ModelPricing {
        // OpenAI gpt-4o-mini-transcribe: $0.003/minute = 0.3 cents/minute =
        // 3_000 microcents/minute. With 1 minute = 60 seconds, microcents
        // per 1M seconds = 3_000 * 1_000_000 / 60 = 50_000_000.
        provider: "openai",
        model: "gpt-4o-mini-transcribe",
        upstream_in_microcents_per_1m: 50_000_000,
        upstream_out_microcents_per_1m: 0,
        markup_percent: 200,
    },
];

pub fn lookup(provider: &str, model: &str) -> Option<&'static ModelPricing> {
    PRICING
        .iter()
        .find(|p| p.provider == provider && p.model == model)
}

/// Compute the cost in cents (rounded up to the nearest cent).
/// Returns (bluey_cost_cents, customer_cost_cents).
pub fn compute_cost(pricing: &ModelPricing, input_tokens: i64, output_tokens: i64) -> (i64, i64) {
    let bluey_microcents = pricing.upstream_in_microcents_per_1m * input_tokens / 1_000_000
        + pricing.upstream_out_microcents_per_1m * output_tokens / 1_000_000;
    let customer_microcents = bluey_microcents * (100 + pricing.markup_percent) / 100;

    let bluey_cents = (bluey_microcents + MICROCENTS_PER_CENT - 1) / MICROCENTS_PER_CENT;
    let customer_cents = (customer_microcents + MICROCENTS_PER_CENT - 1) / MICROCENTS_PER_CENT;
    (bluey_cents.max(0), customer_cents.max(0))
}

/// Estimate cost upper bound for the entry check.
/// `max_output_tokens` is the request's `max_tokens`. We add 10% safety
/// margin so a slight overrun in actual completion length stays inside
/// the entry-check envelope.
pub fn estimate_cost_ceiling(
    pricing: &ModelPricing,
    input_tokens: i64,
    max_output_tokens: i64,
) -> i64 {
    let (_, customer_cents) = compute_cost(pricing, input_tokens, max_output_tokens);
    customer_cents + (customer_cents / 10).max(1)
}

pub fn estimate_bluey_cost_ceiling(
    pricing: &ModelPricing,
    input_tokens: i64,
    max_output_tokens: i64,
) -> i64 {
    let (bluey_cents, _) = compute_cost(pricing, input_tokens, max_output_tokens);
    bluey_cents + (bluey_cents / 10).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_known_model() {
        assert_eq!(
            lookup("openai", "gpt-5.4-mini").unwrap().markup_percent,
            200
        );
        assert_eq!(lookup("openai", "gpt-5.5").unwrap().markup_percent, 150);
        assert_eq!(
            lookup("anthropic", "claude-sonnet-4-6")
                .unwrap()
                .markup_percent,
            200
        );
        assert_eq!(
            lookup("anthropic", "claude-opus-4-8")
                .unwrap()
                .markup_percent,
            150
        );
        assert_eq!(
            lookup("anthropic", "claude-haiku-4-5-20251001")
                .unwrap()
                .markup_percent,
            200
        );
        assert_eq!(
            lookup("gemini", "gemini-3.1-pro-preview")
                .unwrap()
                .markup_percent,
            150
        );
        assert_eq!(
            lookup("gemini", "gemini-3.1-flash-lite")
                .unwrap()
                .markup_percent,
            200
        );
    }

    #[test]
    fn unknown_model_returns_none() {
        assert!(lookup("openai", "gpt-nonexistent").is_none());
    }

    #[test]
    fn easy_question_under_one_cent() {
        // 150 in / 100 out on gpt-5.4-mini.
        // raw: 150*750_000/1M + 100*4_500_000/1M = 112.5 + 450 = 562.5 microcents
        // 200% markup: 1687.5 microcents = 0.1688 cents → ceil 1 cent
        let p = lookup("openai", "gpt-5.4-mini").unwrap();
        let (bluey, customer) = compute_cost(p, 150, 100);
        assert_eq!(bluey, 1); // ceil 0.0563 cents → 1
        assert_eq!(customer, 1); // ceil 0.1688 cents → 1
                                 // Note: the fractional headline number from PRICING-MODEL.md
                                 // section 2 describes the *fractional* cost; the per-request
                                 // billing rounds up to 1 cent because that's the unit of currency.
                                 // For aggregation/reporting we use microcents internally.
    }

    #[test]
    fn medium_code_q_charges_few_cents() {
        // 800 in / 600 out on claude-sonnet-4-6
        // raw: 800*3_000_000/1M + 600*15_000_000/1M = 2400 + 9000 = 11400 microcents
        // raw cents: 11400 / 10000 = 1.14 cents → ceil 2
        // 200% markup: 11400 * 3 / 1 = 34200 microcents = 3.42 cents → ceil 4
        let p = lookup("anthropic", "claude-sonnet-4-6").unwrap();
        let (bluey, customer) = compute_cost(p, 800, 600);
        assert_eq!(bluey, 2);
        assert_eq!(customer, 4);
    }

    #[test]
    fn deep_question_charges_tens_of_cents() {
        // 1500 in / 1000 out on claude-opus-4-8.
        // raw: 1500*5M/1M + 1000*25M/1M = 7500 + 25000 = 32500 microcents
        // raw cents: 3.25 → ceil 4
        // 150% markup: 32500 * 2.5 = 81250 microcents = 8.125 cents → ceil 9
        let p = lookup("anthropic", "claude-opus-4-8").unwrap();
        let (bluey, customer) = compute_cost(p, 1500, 1000);
        assert_eq!(bluey, 4);
        assert_eq!(customer, 9);
    }

    #[test]
    fn ceiling_includes_safety_margin() {
        // Easy question: customer=1 cent. Ceiling = 1 + max(1/10, 1) = 1+1 = 2 cents.
        let p = lookup("openai", "gpt-5.4-mini").unwrap();
        assert_eq!(estimate_cost_ceiling(p, 150, 100), 2);

        // Medium question: customer=4 cents. Ceiling = 4 + max(4/10, 1) = 4+1 = 5 cents.
        let p = lookup("anthropic", "claude-sonnet-4-6").unwrap();
        assert_eq!(estimate_cost_ceiling(p, 800, 600), 5);
    }

    #[test]
    fn bluey_ceiling_uses_upstream_cost_not_markup() {
        let p = lookup("anthropic", "claude-sonnet-4-6").unwrap();
        assert_eq!(estimate_bluey_cost_ceiling(p, 800, 600), 3);
    }

    #[test]
    fn local_ollama_costs_nothing() {
        let p = lookup("ollama", "llama3.1").unwrap();
        let (bluey, customer) = compute_cost(p, 1000, 1000);
        assert_eq!(bluey, 0);
        assert_eq!(customer, 0);
    }

    #[test]
    fn embed_pricing_dollar_to_microcents_conversion() {
        // OpenAI text-embedding-3-small list price is $0.02 per 1M input tokens.
        // $0.02 = 0.02 * 100 cents/dollar = 2 cents = 2 * 10_000 microcents = 20_000 microcents/1M.
        let pricing = lookup("openai", "text-embedding-3-small").unwrap();
        assert_eq!(pricing.upstream_in_microcents_per_1m, 20_000);
        assert_eq!(pricing.upstream_out_microcents_per_1m, 0);
    }

    #[test]
    fn deepgram_pricing_dollar_per_minute_to_microcents_per_1m_seconds() {
        // Deepgram nova-3 accuracy-first streaming: $0.0092/minute =
        // 0.92 cents/minute = 9_200 microcents/minute.
        // microcents per second = 9_200 / 60 ≈ 153.333
        // microcents per 1M seconds = 153.333 * 1_000_000 ≈ 153_333_333.
        let pricing = lookup("deepgram", "nova-3").unwrap();
        assert_eq!(pricing.upstream_in_microcents_per_1m, 153_333_333);
        assert_eq!(pricing.markup_percent, 200);
    }

    #[test]
    fn deepgram_two_hour_dual_source_uses_200_percent_markup() {
        let pricing = lookup("deepgram", "nova-3").unwrap();
        let two_hours_two_sources_seconds = 2 * 60 * 60 * 2;
        let (bluey, customer) = compute_cost(pricing, two_hours_two_sources_seconds, 0);
        assert_eq!(bluey, 221);
        assert_eq!(customer, 663);
    }

    #[test]
    fn openai_transcribe_pricing_dollar_per_minute_to_microcents_per_1m_seconds() {
        // OpenAI gpt-4o-mini-transcribe: $0.003/minute = 0.3 cents/minute =
        // 3_000 microcents/minute. Microcents per 1M seconds =
        // 3_000 * 1_000_000 / 60 = 50_000_000.
        let pricing = lookup("openai", "gpt-4o-mini-transcribe").unwrap();
        assert_eq!(pricing.upstream_in_microcents_per_1m, 50_000_000);
        assert_eq!(pricing.markup_percent, 200);
    }

    #[test]
    fn embed_compute_cost_reasonable_for_1k_tokens() {
        let pricing = lookup("openai", "text-embedding-3-small").unwrap();
        // 1000 input tokens. Upstream cost: 1000 * 20_000 / 1_000_000 = 20 microcents = 0.002 cents.
        // Customer pays markup 200% = 3x = 0.006 cents. Rounds UP to 1 cent (whole-cent floor).
        let (bluey, customer) = compute_cost(pricing, 1_000, 0);
        assert_eq!(bluey, 1); // both bluey and customer round UP to 1c minimum
        assert_eq!(customer, 1); // customer pays 1c minimum (S4.3 floor)
    }
}

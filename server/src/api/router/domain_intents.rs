
fn looks_like_system_design_question(normalized: &str) -> bool {
    let explicit_known_design = contains_any(
        normalized,
        &[
            "system design",
            "design url shortener",
            "design a url shortener",
            "design an url shortener",
            "design link shortener",
            "design a link shortener",
            "design link shortening",
            "design a link shortening",
            "design short link service",
            "design a short link service",
            "design tinyurl",
            "design bitly",
            "build url shortener",
            "build a url shortener",
            "build an url shortener",
            "build link shortener",
            "build a link shortener",
            "build short link service",
            "build a short link service",
            "create url shortener",
            "create a url shortener",
            "create an url shortener",
            "create link shortener",
            "create a link shortener",
            "create short link service",
            "create a short link service",
            "design a rate limiter",
            "design rate limiter",
            "design notification system",
            "design a notification system",
            "design chat app",
            "design a chat app",
            "design messaging app",
            "design a messaging app",
            "design news feed",
            "design a news feed",
            "design pastebin",
            "design a cache",
            "design cache",
            "design distributed",
            "design a system",
            "design an app",
            "design the architecture",
            "high level design",
            "low level design",
            "architecture for",
        ],
    );
    let design_frame = contains_any(
        normalized,
        &[
            "design a ",
            "design an ",
            "design the ",
            "how would you design",
            "how do you design",
            "architect a ",
            "architect an ",
            "propose an architecture",
        ],
    );
    let design_target = contains_any(
        normalized,
        &[
            "system",
            "platform",
            "service",
            "application",
            " app",
            "store",
            "processor",
            "pipeline",
            "url shortener",
            "link shortener",
            "link shortening",
            "rate limiter",
            "messaging",
            "monitoring",
            "feature store",
            "payment processing",
            "payment gateway",
            "rag platform",
            "search engine",
            "notification",
            "news feed",
        ],
    );
    let scaling_frame = contains_any(
        normalized,
        &[
            "how would you scale",
            "how do you scale",
            "scale this system",
        ],
    );

    explicit_known_design || (design_frame && design_target) || scaling_frame
}

fn looks_like_system_design_followup_question(normalized: &str) -> bool {
    if normalized.trim().is_empty() {
        return false;
    }
    let followup_signal = contains_any(
        normalized,
        &[
            "this design",
            "that design",
            "same design",
            "above design",
            "previous design",
            "current design",
            "this architecture",
            "that architecture",
            "same architecture",
            "above architecture",
            "previous architecture",
            "current architecture",
            "what about",
            "what observability",
            "what monitoring",
            "which observability",
            "which metrics",
            "how about",
            "what happens if",
            "what if",
            "times out",
            "after charging",
            "hot partition",
            "servers fail",
            "users reconnect",
            "where would",
            "when would",
            "can we",
            "should we",
            "why did",
            "why do",
            "why use",
            "why would",
            "explain",
            "walk me through",
            "continue",
            "keep going",
            "go on",
            "next section",
            "next part",
            "expand",
            "elaborate",
            "add ",
            "include ",
            "cover ",
            "extend ",
        ],
    );
    if !followup_signal {
        return false;
    }
    contains_any(
        normalized,
        &[
            "design",
            "architecture",
            "requirement",
            "api",
            "gateway",
            "service",
            "services",
            "endpoint",
            "token",
            "counter",
            "counters",
            "redis",
            "data model",
            "database",
            "schema",
            "cache",
            "queue",
            "worker",
            "workers",
            "event",
            "stream",
            "ordering",
            "order",
            "latency",
            "timeout",
            "provider",
            "state transition",
            "throughput",
            "scale",
            "scaling",
            "shard",
            "partition",
            "replica",
            "region",
            "availability",
            "consistency",
            "tradeoff",
            "failure",
            "fallback",
            "retry",
            "observability",
            "metrics",
            "logs",
            "security",
            "auth",
            "rate limit",
        ],
    ) || contains_any(
        normalized,
        &[
            "continue",
            "keep going",
            "go on",
            "next section",
            "next part",
        ],
    )
}

fn looks_like_system_design_canvas_followup_question(normalized: &str) -> bool {
    contains_any(
        normalized,
        &[
            "continue",
            "keep going",
            "go on",
            "next section",
            "next part",
            "expand",
            "elaborate",
            "add ",
            "include ",
            "cover ",
            "extend ",
            "append ",
            "update ",
            "change the design",
            "redesign",
            "fill in",
            "what about",
            "how about",
            "failure mode",
            "failure modes",
            "tradeoff",
            "tradeoffs",
            "scaling",
            "scale",
            "data model",
            "api design",
            "observability",
            "security",
            "rate limiting",
            "rollout",
            "hot partition",
        ],
    )
}

fn looks_like_diagram_request(normalized: &str) -> bool {
    contains_any(
        normalized,
        &[
            "diagram",
            "flowchart",
            "sequence diagram",
            "architecture diagram",
            "data flow",
            "pictorial",
            "visual representation",
            "visualize",
            "visualise",
            "draw ",
            "draw a",
            "draw the",
            "block diagram",
            "box diagram",
        ],
    ) && !contains_any(
        normalized,
        &[
            "screenshot",
            "screen capture",
            "image shows",
            "attached image",
        ],
    )
}

fn looks_like_public_lookup_phrase(normalized: &str, word_count: usize) -> bool {
    (2..=8).contains(&word_count)
        && contains_any(
            normalized,
            &[
                "ranch",
                "restaurant",
                "hotel",
                "venue",
                "company",
                "startup",
                "school",
                "university",
                "college",
                "hospital",
                "clinic",
                "park",
                "trail",
                "museum",
                "airport",
                "product",
                "pricing",
                "stock",
                "weather",
            ],
        )
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn contains_token_phrase(haystack: &str, needle: &str) -> bool {
    let haystack_tokens = haystack
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    let needle_tokens = needle
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();

    !needle_tokens.is_empty()
        && haystack_tokens
            .windows(needle_tokens.len())
            .any(|window| window == needle_tokens.as_slice())
}

fn contains_any_token_phrase(haystack: &str, needles: &[&str]) -> bool {
    needles
        .iter()
        .any(|needle| contains_token_phrase(haystack, needle))
}

fn looks_like_contextual_payment_tooling_request(normalized: &str) -> bool {
    let tokens = normalized
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    let Some(action_index) = tokens
        .iter()
        .position(|token| matches!(*token, "design" | "build" | "create" | "architect"))
    else {
        return false;
    };
    if action_index > 3 {
        return false;
    }

    let mut target_index = action_index + 1;
    while target_index < tokens.len() && matches!(tokens[target_index], "a" | "an" | "the") {
        target_index += 1;
    }
    let tooling_targets = [
        "observability",
        "monitoring",
        "analytics",
        "reporting",
        "notification",
        "fraud",
    ];
    if tokens
        .get(target_index)
        .is_some_and(|token| tooling_targets.contains(token))
    {
        return true;
    }

    if tokens
        .get(target_index)
        .is_some_and(|token| matches!(*token, "payment" | "payments" | "payout"))
    {
        target_index += 1;
        while target_index < tokens.len() && matches!(tokens[target_index], "a" | "an" | "the") {
            target_index += 1;
        }
        return tokens
            .get(target_index)
            .is_some_and(|token| tooling_targets.contains(token));
    }

    false
}

fn looks_like_payment_domain(normalized: &str) -> bool {
    let checkout_payment_related = contains_token_phrase(normalized, "checkout")
        && contains_any(
            normalized,
            &[
                "ecommerce",
                "e commerce",
                "shopping cart",
                "customer order",
                "online order",
                "purchase",
                "merchant",
                "card authorization",
                "card payment",
                "billing",
                "commerce",
            ],
        );
    let has_card_domain = normalized.split_whitespace().any(|token| token == "card");
    let has_merchant_domain = normalized
        .split_whitespace()
        .any(|token| token == "merchant");
    let has_authorization_or_refund_operation = normalized.split_whitespace().any(|token| {
        token.starts_with("authoriz")
            || token.starts_with("authoris")
            || token.starts_with("refund")
    });
    let has_capture_operation = normalized
        .split_whitespace()
        .any(|token| token.starts_with("captur"));
    let card_or_merchant_payment_related = (has_card_domain
        && (has_authorization_or_refund_operation
            || (has_capture_operation
                && contains_any(normalized, &["payment", "transaction", "settlement"]))))
        || (has_merchant_domain
            && (has_authorization_or_refund_operation || has_capture_operation));

    checkout_payment_related
        || card_or_merchant_payment_related
        || contains_any_token_phrase(
            normalized,
            &[
                "payment",
                "payments",
                "charged the card",
                "charging the card",
                "card charge",
                "card processor",
                "money movement",
                "money transfer",
                "payout",
                "disbursement",
            ],
        )
}

fn looks_like_payment_money_effect_domain(normalized: &str) -> bool {
    let contextual_tooling_request = looks_like_contextual_payment_tooling_request(normalized);
    if contextual_tooling_request {
        return false;
    }

    looks_like_payment_domain(normalized)
        && contains_any_token_phrase(
            normalized,
            &[
                "payment processing",
                "payment processor",
                "payments processor",
                "process payment",
                "process payments",
                "payment request",
                "payment timeout",
                "payments platform",
                "payment platform",
                "payments system",
                "payment system",
                "payments service",
                "payment service",
                "payment gateway",
                "card processor",
                "paid order",
                "checkout",
                "authorization",
                "authorisation",
                "capture",
                "refund",
                "charged the card",
                "charging the card",
                "card charge",
                "money movement",
                "money transfer",
                "payout",
                "disbursement",
                "settlement",
            ],
        )
}

/// Narrowly identifies the ambiguous, post-dispatch payment-timeout follow-up
/// that has a one-paragraph interview-style output contract. Keep this shared
/// between prompt construction and visible-output shaping so a provider cannot
/// append coaching material after satisfying that contract.
fn is_post_dispatch_payment_timeout_question(
    normalized_question: &str,
    payment_money_effect_domain: bool,
) -> bool {
    payment_money_effect_domain
        && contains_any(
            normalized_question,
            &["timeout", "times out", "timed out", "ambiguous outcome"],
        )
        && contains_any(
            normalized_question,
            &[
                "after charging",
                "after the charge",
                "after dispatch",
                "after submission",
                "provider times out",
                "ambiguous outcome",
                "outcome is unknown",
            ],
        )
        && !contains_any(
            normalized_question,
            &["before dispatch", "before submission", "before sending"],
        )
}

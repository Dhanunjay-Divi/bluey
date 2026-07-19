
fn prompt_with_web_context(
    system: &str,
    user: &str,
    sources: &[CompleteSource],
) -> (String, String) {
    if sources.is_empty() {
        return (system.to_string(), user.to_string());
    }

    let mut context = String::from("Managed web search results selected by Bluey server:\n");
    for source in sources {
        context.push_str(&format!(
            "\n[{}] {}\nURL: {}\nSnippet: {}\n",
            source.id,
            source.title,
            source.url.as_deref().unwrap_or("not provided"),
            source.snippet.as_deref().unwrap_or("not provided")
        ));
    }

    let system = format!(
        "{system}\n\n{context}\nUse these web results only when they directly answer the user's question. If they are weak or irrelevant, say that clearly."
    );
    (system, user.to_string())
}

const DEFAULT_WEB_SEARCH_BUDGET_MS: u64 = 1_200;
const DEFAULT_WEB_SEARCH_MAX_RESULTS: usize = 3;
const MAX_WEB_SEARCH_RESULTS: usize = 5;
const MAX_WEB_SEARCHES_PER_ANSWER: i64 = 3;
const MAX_WEB_SEARCH_QUERY_CHARS: usize = 160;
const DEFAULT_WEB_SEARCH_CUSTOMER_COST_CENTS: i64 = 2;
const DEFAULT_TRIAL_WEB_SEARCHES_PER_DAY: i64 = 5;
const DEFAULT_WEB_SEARCH_ACCOUNT_HOURLY_LIMIT: i64 = 120;
const DEFAULT_WEB_SEARCH_BURST_LIMIT: i64 = 12;
const DEFAULT_WEB_SEARCH_BURST_WINDOW_SECS: u64 = 60;
const DEFAULT_WEB_SEARCH_REPEAT_WINDOW_SECS: u64 = 600;
const WEB_SEARCH_USAGE_KIND: &str = "web_search";
const WEB_SEARCH_TASK_TYPE: &str = "web_search";
const WEB_SEARCH_USAGE_MODEL: &str = "managed-web-search";

#[derive(Debug, Clone)]
struct WebSearchConfig {
    provider: String,
    endpoint: String,
    api_key: Option<String>,
    max_results: usize,
    budget: std::time::Duration,
    customer_cost_cents: i64,
    bluey_cost_cents: i64,
}

#[derive(Debug, Clone, Default)]
struct WebSearchOutcome {
    sources: Vec<CompleteSource>,
    attempted: bool,
    searches_used: i64,
    provider: Option<String>,
    latency_ms: i64,
    customer_cost_cents: i64,
    bluey_cost_cents: i64,
    skipped_reason: Option<&'static str>,
    provider_accounting_pending: bool,
}

fn web_search_config() -> Option<WebSearchConfig> {
    if env_flag_is_false("BLUEY_WEB_SEARCH_ENABLED") {
        return None;
    }
    let provider = std::env::var("BLUEY_WEB_SEARCH_PROVIDER")
        .unwrap_or_else(|_| "generic".to_string())
        .trim()
        .to_ascii_lowercase();
    let api_key = std::env::var("BLUEY_WEB_SEARCH_API_KEY")
        .ok()
        .or_else(|| std::env::var("TAVILY_API_KEY").ok())
        .or_else(|| std::env::var("BRAVE_SEARCH_API_KEY").ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let endpoint = std::env::var("BLUEY_WEB_SEARCH_ENDPOINT")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| match provider.as_str() {
            "tavily" => Some("https://api.tavily.com/search".to_string()),
            "brave" => Some("https://api.search.brave.com/res/v1/web/search".to_string()),
            _ => None,
        })?;
    if api_key.is_none() && !env_flag_is_true("BLUEY_WEB_SEARCH_ALLOW_NO_KEY") {
        return None;
    }
    let max_results = std::env::var("BLUEY_WEB_SEARCH_MAX_RESULTS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(DEFAULT_WEB_SEARCH_MAX_RESULTS)
        .clamp(1, MAX_WEB_SEARCH_RESULTS);
    let budget_ms = std::env::var("BLUEY_WEB_SEARCH_BUDGET_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_WEB_SEARCH_BUDGET_MS);
    let customer_cost_cents = web_search_env_i64(
        "BLUEY_WEB_SEARCH_CUSTOMER_COST_CENTS",
        DEFAULT_WEB_SEARCH_CUSTOMER_COST_CENTS,
        0,
        100,
    );
    let configured_bluey_cost = std::env::var("BLUEY_WEB_SEARCH_BLUEY_COST_CENTS").ok();
    let bluey_cost_cents = configured_web_search_bluey_cost(configured_bluey_cost.as_deref())?;

    Some(WebSearchConfig {
        provider,
        endpoint,
        api_key,
        max_results,
        budget: Duration::from_millis(budget_ms),
        customer_cost_cents,
        bluey_cost_cents,
    })
}

fn configured_web_search_bluey_cost(value: Option<&str>) -> Option<i64> {
    value?
        .trim()
        .parse::<i64>()
        .ok()
        .filter(|cost| (1..=100).contains(cost))
}

fn web_search_env_i64(key: &str, default: i64, min: i64, max: i64) -> i64 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.trim().parse::<i64>().ok())
        .unwrap_or(default)
        .clamp(min, max)
}

fn env_flag_is_true(key: &str) -> bool {
    std::env::var(key)
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn env_flag_is_false(key: &str) -> bool {
    std::env::var(key)
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "0" | "false" | "no" | "off"
            )
        })
        .unwrap_or(false)
}

async fn completion_web_search_budgeted(
    pool: &crate::db::DbPool,
    upstream_spend_guard: Option<crate::config::UpstreamSpendGuard>,
    account: &Account,
    request_id: &str,
    query_text: &str,
    plan: &AnswerPlan,
) -> WebSearchOutcome {
    if !plan.needs_web_search {
        return WebSearchOutcome::default();
    }
    let Some(config) = web_search_config() else {
        tracing::debug!(
            account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
            request_id,
            "managed web search skipped; provider not configured"
        );
        return WebSearchOutcome {
            attempted: true,
            skipped_reason: Some("provider_not_configured"),
            ..Default::default()
        };
    };
    let Some(query) = sanitized_web_search_query(query_text) else {
        tracing::warn!(
            account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
            request_id,
            "managed web search skipped; query was empty or sensitive after sanitization"
        );
        return WebSearchOutcome {
            attempted: true,
            provider: Some(config.provider.clone()),
            skipped_reason: Some("query_sanitized_empty_or_sensitive"),
            ..Default::default()
        };
    };
    if let Some(searches_used) = trial_web_searches_used_today(pool, account, request_id) {
        let trial_limit = web_search_env_i64(
            "BLUEY_TRIAL_WEB_SEARCHES_PER_DAY",
            DEFAULT_TRIAL_WEB_SEARCHES_PER_DAY,
            0,
            100,
        );
        if account.trial_seconds_remaining > 0 && searches_used >= trial_limit {
            tracing::warn!(
                account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                request_id,
                searches_used,
                trial_limit,
                "managed web search skipped; trial daily search quota reached"
            );
            return WebSearchOutcome {
                attempted: true,
                provider: Some(config.provider.clone()),
                skipped_reason: Some("trial_web_search_quota_reached"),
                ..Default::default()
            };
        }
    }
    if account.trial_seconds_remaining <= 0 && config.customer_cost_cents > 0 {
        match balance::can_afford(pool, &account.id, config.customer_cost_cents) {
            Ok(true) => {}
            Ok(false) => {
                tracing::warn!(
                    account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                    request_id,
                    search_cost_cents = config.customer_cost_cents,
                    "managed web search skipped; account needs credits"
                );
                return WebSearchOutcome {
                    attempted: true,
                    provider: Some(config.provider.clone()),
                    skipped_reason: Some("insufficient_credits"),
                    ..Default::default()
                };
            }
            Err(error) => {
                tracing::warn!(
                    account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                    request_id,
                    error = %error,
                    "managed web search skipped; credit preflight failed"
                );
                return WebSearchOutcome {
                    attempted: true,
                    provider: Some(config.provider.clone()),
                    skipped_reason: Some("credit_check_unavailable"),
                    ..Default::default()
                };
            }
        }
    }
    let hourly_limit = web_search_env_i64(
        "BLUEY_WEB_SEARCH_ACCOUNT_HOURLY_LIMIT",
        DEFAULT_WEB_SEARCH_ACCOUNT_HOURLY_LIMIT,
        0,
        10_000,
    );
    if hourly_limit > 0 {
        match usage::count_task_events_in_window(pool, &account.id, WEB_SEARCH_TASK_TYPE, 1) {
            Ok(searches_used) if searches_used >= hourly_limit => {
                tracing::warn!(
                    account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                    request_id,
                    searches_used,
                    hourly_limit,
                    "managed web search skipped; hourly safety rail reached"
                );
                return WebSearchOutcome {
                    attempted: true,
                    provider: Some(config.provider.clone()),
                    skipped_reason: Some("account_search_cooldown"),
                    ..Default::default()
                };
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(
                    account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                    request_id,
                    error = %error,
                    "managed web search hourly safety lookup failed; allowing request"
                );
            }
        }
    }
    let burst_limit = web_search_env_i64(
        "BLUEY_WEB_SEARCH_BURST_LIMIT",
        DEFAULT_WEB_SEARCH_BURST_LIMIT,
        0,
        1_000,
    );
    let burst_window = Duration::from_secs(web_search_env_i64(
        "BLUEY_WEB_SEARCH_BURST_WINDOW_SECS",
        DEFAULT_WEB_SEARCH_BURST_WINDOW_SECS as i64,
        0,
        3_600,
    ) as u64);
    if !allow_web_search_burst(&account.id, burst_window, burst_limit) {
        tracing::warn!(
            account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
            request_id,
            burst_limit,
            burst_window_secs = burst_window.as_secs(),
            "managed web search skipped; short-window safety rail reached"
        );
        return WebSearchOutcome {
            attempted: true,
            provider: Some(config.provider.clone()),
            skipped_reason: Some("account_search_cooldown"),
            ..Default::default()
        };
    }
    let repeat_window = Duration::from_secs(web_search_env_i64(
        "BLUEY_WEB_SEARCH_REPEAT_WINDOW_SECS",
        DEFAULT_WEB_SEARCH_REPEAT_WINDOW_SECS as i64,
        0,
        86_400,
    ) as u64);
    if !allow_web_search_repeat(&account.id, &query, repeat_window) {
        tracing::warn!(
            account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
            request_id,
            repeat_window_secs = repeat_window.as_secs(),
            "managed web search skipped; repeated identical query inside guard window"
        );
        return WebSearchOutcome {
            attempted: true,
            provider: Some(config.provider.clone()),
            skipped_reason: Some("repeated_query_guard"),
            ..Default::default()
        };
    }
    let provider = config.provider.clone();
    let budget = config.budget;
    let started = Instant::now();
    let attempt_request_id = format!("{request_id}:web-search-attempt");
    let mut cost_guard = match provider_cost_guard::reserve(
        pool,
        upstream_spend_guard,
        &account.id,
        &format!("router:{request_id}:web-search"),
        &attempt_request_id,
        &provider,
        WEB_SEARCH_USAGE_MODEL,
        config.bluey_cost_cents,
        "web_search_attempt",
        WEB_SEARCH_TASK_TYPE,
    ) {
        Ok(provider_cost_guard::Admission::Held(guard)) => guard,
        Ok(provider_cost_guard::Admission::Unconfigured) => {
            return WebSearchOutcome {
                attempted: true,
                provider: Some(provider),
                skipped_reason: Some("upstream_cost_unconfigured"),
                ..Default::default()
            };
        }
        Ok(provider_cost_guard::Admission::GlobalLimit) | Err(_) => {
            return WebSearchOutcome {
                attempted: true,
                provider: Some(provider),
                skipped_reason: Some("upstream_spend_guard"),
                ..Default::default()
            };
        }
    };
    match tokio::time::timeout(budget, perform_web_search(&config, &query)).await {
        Ok(Ok(sources)) => {
            let latency_ms = started.elapsed().as_millis().min(i64::MAX as u128) as i64;
            let searches_used = 1_i64.min(MAX_WEB_SEARCHES_PER_ANSWER);
            let actual_cost = config.bluey_cost_cents.saturating_mul(searches_used);
            let event = UsageEvent {
                request_id: attempt_request_id,
                kind: "web_search_attempt".to_string(),
                task_type: Some(WEB_SEARCH_TASK_TYPE.to_string()),
                lane: Some("web_search".to_string()),
                provider: Some(provider.clone()),
                model: Some(WEB_SEARCH_USAGE_MODEL.to_string()),
                input_tokens: searches_used,
                output_tokens: sources.len().try_into().unwrap_or(i64::MAX),
                latency_ms,
                cost_cents_to_bluey: actual_cost,
                cost_cents_to_customer: 0,
                was_speculative: false,
                was_fallback: false,
            };
            if settle_provider_attempt_before_customer(
                pool,
                &account.id,
                request_id,
                &mut cost_guard,
                event,
                actual_cost,
                pricing::UsageProvenance::Exact,
            )
            .is_err()
            {
                return WebSearchOutcome {
                    attempted: true,
                    provider: Some(provider),
                    latency_ms,
                    skipped_reason: Some("provider_accounting_pending"),
                    provider_accounting_pending: true,
                    ..Default::default()
                };
            }
            tracing::debug!(
                account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                request_id,
                provider = %provider,
                source_count = sources.len(),
                searches_used,
                cost_cents_to_customer = config.customer_cost_cents,
                "managed web search completed"
            );
            WebSearchOutcome {
                sources,
                attempted: true,
                searches_used,
                provider: Some(provider),
                latency_ms,
                customer_cost_cents: config.customer_cost_cents.saturating_mul(searches_used),
                bluey_cost_cents: config.bluey_cost_cents.saturating_mul(searches_used),
                skipped_reason: None,
                provider_accounting_pending: false,
            }
        }
        Ok(Err(error)) => {
            if let Err(settle_error) = cost_guard.settle_conservative() {
                tracing::error!(request_id, error = %settle_error, "failed to terminalize web search provider error exposure");
                return WebSearchOutcome {
                    attempted: true,
                    provider: Some(provider),
                    skipped_reason: Some("provider_accounting_pending"),
                    provider_accounting_pending: true,
                    ..Default::default()
                };
            }
            tracing::warn!(
                account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                request_id,
                provider = %provider,
                error = %error,
                "managed web search failed; continuing without web context"
            );
            WebSearchOutcome {
                attempted: true,
                provider: Some(provider),
                latency_ms: started.elapsed().as_millis().min(i64::MAX as u128) as i64,
                skipped_reason: Some("provider_error"),
                ..Default::default()
            }
        }
        Err(_) => {
            if let Err(error) = cost_guard.settle_conservative() {
                tracing::error!(request_id, error = %error, "failed to terminalize web search timeout exposure");
                return WebSearchOutcome {
                    attempted: true,
                    provider: Some(provider),
                    skipped_reason: Some("provider_accounting_pending"),
                    provider_accounting_pending: true,
                    ..Default::default()
                };
            }
            tracing::warn!(
                account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                request_id,
                provider = %provider,
                budget_ms = budget.as_millis() as u64,
                "managed web search exceeded budget; continuing without web context"
            );
            WebSearchOutcome {
                attempted: true,
                provider: Some(provider),
                latency_ms: started.elapsed().as_millis().min(i64::MAX as u128) as i64,
                skipped_reason: Some("provider_timeout"),
                ..Default::default()
            }
        }
    }
}

fn looks_like_contextual_code_generation_followup(normalized: &str) -> bool {
    if looks_like_algorithmic_challenge_prompt(normalized) {
        return false;
    }

    contains_any(
        normalized,
        &[
            "i want the code",
            "give me code",
            "give me python code",
            "give me java code",
            "can you give me code",
            "can you give me python code",
            "can you give me java code",
            "python code",
            "java code",
            "full code",
            "complete code",
            "same code",
            "code for the same",
            "solution for the same",
        ],
    )
}

fn trial_web_searches_used_today(
    pool: &crate::db::DbPool,
    account: &Account,
    request_id: &str,
) -> Option<i64> {
    if account.trial_seconds_remaining <= 0 {
        return Some(0);
    }
    match usage::count_task_events_in_window(pool, &account.id, WEB_SEARCH_TASK_TYPE, 24) {
        Ok(count) => Some(count),
        Err(error) => {
            tracing::warn!(
                account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                request_id,
                error = %error,
                "managed web search quota lookup failed; allowing request"
            );
            None
        }
    }
}

fn allow_web_search_burst(account_id: &str, window: Duration, limit: i64) -> bool {
    if window.is_zero() || limit <= 0 {
        return true;
    }
    let now = Instant::now();
    let key = web_search_account_guard_key(account_id);
    let guard = WEB_SEARCH_BURST_GUARD.get_or_init(|| Mutex::new(HashMap::new()));
    let Ok(mut entries) = guard.lock() else {
        return true;
    };
    let timestamps = entries.entry(key).or_insert_with(Vec::new);
    timestamps.retain(|seen_at| now.duration_since(*seen_at) <= window);
    if timestamps.len() >= limit as usize {
        return false;
    }
    timestamps.push(now);
    true
}

fn allow_web_search_repeat(account_id: &str, query: &str, window: Duration) -> bool {
    if window.is_zero() {
        return true;
    }
    let now = Instant::now();
    let key = web_search_repeat_guard_key(account_id, query);
    let guard = WEB_SEARCH_REPEAT_GUARD.get_or_init(|| Mutex::new(HashMap::new()));
    let Ok(mut entries) = guard.lock() else {
        return true;
    };
    entries.retain(|_, seen_at| now.duration_since(*seen_at) <= window);
    if entries.contains_key(&key) {
        return false;
    }
    entries.insert(key, now);
    true
}

fn web_search_account_guard_key(account_id: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    account_id.hash(&mut hasher);
    hasher.finish()
}

fn web_search_repeat_guard_key(account_id: &str, query: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    account_id.hash(&mut hasher);
    collapse_spaces(query)
        .to_ascii_lowercase()
        .hash(&mut hasher);
    hasher.finish()
}

static WEB_SEARCH_REPEAT_GUARD: OnceLock<Mutex<HashMap<u64, Instant>>> = OnceLock::new();
static WEB_SEARCH_BURST_GUARD: OnceLock<Mutex<HashMap<u64, Vec<Instant>>>> = OnceLock::new();

async fn perform_web_search(
    config: &WebSearchConfig,
    query: &str,
) -> anyhow::Result<Vec<CompleteSource>> {
    let client = reqwest::Client::builder()
        .timeout(config.budget)
        .redirect(reqwest::redirect::Policy::limited(2))
        .build()?;

    let response = match config.provider.as_str() {
        "brave" => {
            let params = [
                ("q", query.to_string()),
                ("count", config.max_results.to_string()),
                ("safesearch", "moderate".to_string()),
            ];
            let mut request = client.get(&config.endpoint).query(&params);
            if let Some(api_key) = config.api_key.as_deref() {
                request = request.header("X-Subscription-Token", api_key);
            }
            request.send().await?
        }
        "tavily" => {
            let mut body = serde_json::json!({
                "query": query,
                "max_results": config.max_results,
                "search_depth": "basic",
                "include_answer": false,
                "include_raw_content": false,
            });
            if let Some(api_key) = config.api_key.as_deref() {
                body["api_key"] = serde_json::Value::String(api_key.to_string());
            }
            client.post(&config.endpoint).json(&body).send().await?
        }
        _ => {
            let mut request = client.post(&config.endpoint).json(&serde_json::json!({
                "query": query,
                "max_results": config.max_results,
            }));
            if let Some(api_key) = config.api_key.as_deref() {
                request = request.bearer_auth(api_key);
            }
            request.send().await?
        }
    };

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!(
            "search provider returned HTTP {status}: {}",
            truncate_chars(&body, 320)
        );
    }
    let parsed: serde_json::Value = serde_json::from_str(&body)?;
    Ok(sources_from_search_response(
        &config.provider,
        &parsed,
        config.max_results,
    ))
}

fn sources_from_search_response(
    provider: &str,
    value: &serde_json::Value,
    max_results: usize,
) -> Vec<CompleteSource> {
    let results = if provider == "brave" {
        value.pointer("/web/results")
    } else {
        value
            .get("results")
            .or_else(|| value.get("items"))
            .or_else(|| value.pointer("/web/results"))
    }
    .and_then(|value| value.as_array())
    .cloned()
    .unwrap_or_default();

    let mut sources = Vec::new();
    for result in results {
        if sources.len() >= max_results {
            break;
        }
        let title = result
            .get("title")
            .or_else(|| result.get("name"))
            .and_then(|value| value.as_str())
            .map(|value| truncate_chars(value.trim(), 120))
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "Web result".to_string());
        let raw_url = result
            .get("url")
            .or_else(|| result.get("link"))
            .and_then(|value| value.as_str())
            .map(|value| value.trim().to_string());
        if raw_url
            .as_deref()
            .is_some_and(|value| !is_safe_public_web_url(value))
        {
            continue;
        }
        let url = raw_url.filter(|value| is_safe_public_web_url(value));
        let snippet = result
            .get("snippet")
            .or_else(|| result.get("description"))
            .or_else(|| result.get("content"))
            .and_then(|value| value.as_str())
            .map(|value| truncate_chars(value.trim(), 450))
            .filter(|value| !value.is_empty());
        if url.is_none() && snippet.is_none() {
            continue;
        }
        sources.push(CompleteSource {
            id: format!("W{}", sources.len() + 1),
            title,
            url,
            snippet,
            source_type: Some("web".to_string()),
        });
    }
    sources
}

fn is_safe_public_web_url(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://")) {
        return false;
    }
    ![
        "localhost",
        "127.0.0.1",
        "0.0.0.0",
        "[::1]",
        "10.",
        "192.168.",
        "172.16.",
        "172.17.",
        "172.18.",
        "172.19.",
        "172.20.",
        "172.21.",
        "172.22.",
        "172.23.",
        "172.24.",
        "172.25.",
        "172.26.",
        "172.27.",
        "172.28.",
        "172.29.",
        "172.30.",
        "172.31.",
    ]
    .iter()
    .any(|blocked| lower.contains(blocked))
}

fn sanitized_web_search_query(user_text: &str) -> Option<String> {
    let question = extract_search_question(user_text);
    let normalized = question.replace(['\n', '\r', '\t'], " ");
    let lower = normalized.to_ascii_lowercase();
    if normalized.contains('@')
        || normalized.contains("```")
        || contains_any(
            &lower,
            &[
                "password",
                "api key",
                "apikey",
                "secret key",
                "client secret",
                "token",
                "bearer ",
                "ssn",
                "social security",
            ],
        )
    {
        return None;
    }
    let mut cleaned = String::new();
    for word in normalized.split_whitespace() {
        let lower_word = word.to_ascii_lowercase();
        if lower_word.starts_with("http://") || lower_word.starts_with("https://") {
            continue;
        }
        if word.len() > 40 && word.chars().filter(|ch| ch.is_ascii_alphanumeric()).count() > 32 {
            continue;
        }
        if !cleaned.is_empty() {
            cleaned.push(' ');
        }
        for ch in word.chars() {
            if ch.is_ascii_alphanumeric()
                || ch.is_ascii_whitespace()
                || matches!(
                    ch,
                    '\'' | '"' | '-' | '_' | '.' | ',' | '?' | '&' | '/' | '(' | ')'
                )
            {
                cleaned.push(ch);
            }
        }
    }
    let cleaned = collapse_spaces(&cleaned);
    if cleaned.chars().count() < 4 {
        return None;
    }
    Some(truncate_chars(&cleaned, MAX_WEB_SEARCH_QUERY_CHARS))
}

fn extract_search_question(user_text: &str) -> String {
    let text = user_text.trim();
    if text.starts_with("Question:") {
        return split_question_and_planning_context(text)
            .0
            .trim()
            .to_string();
    }
    text.to_string()
}

fn extract_planning_context(user_text: &str) -> String {
    let text = user_text.trim();
    if !text.starts_with("Question:") {
        return String::new();
    }
    split_question_and_planning_context(text)
        .1
        .trim()
        .to_string()
}

fn extract_previous_system_design_answer(user_text: &str) -> Option<&str> {
    let text = user_text.trim();
    if !text.starts_with("Question:") {
        return None;
    }
    let (_, context) = split_question_and_planning_context(text);
    const PREFIX: &str = "\n\nSession context:\nPrevious system design answer:\n";
    let answer_with_following_context = context.strip_prefix(PREFIX)?;
    let answer_end = [
        "\n\n[",
        "\n\nSession context:",
        "\n\nScreen context:",
        "\n\nDocument context:",
        "\n\nAttached",
    ]
    .iter()
    .filter_map(|marker| answer_with_following_context.find(marker))
    .min()
    .unwrap_or(answer_with_following_context.len());
    let answer = answer_with_following_context[..answer_end].trim();
    (!answer.is_empty()).then_some(answer)
}

fn split_question_and_planning_context(text: &str) -> (&str, &str) {
    let rest = text.strip_prefix("Question:").unwrap_or(text);
    let markers = [
        "\n\nSession context:",
        "\n\nScreen context:",
        "\n\nAttached",
        "\n\nDocument context:",
    ];
    let Some(index) = markers.iter().filter_map(|marker| rest.find(marker)).min() else {
        return (rest, "");
    };
    (&rest[..index], &rest[index..])
}

fn looks_like_generic_screen_capture_prompt(normalized: &str) -> bool {
    contains_any(
        normalized,
        &[
            "answer using the attached screen capture",
            "answer using attached screen capture",
            "answer using the attached screen context",
            "answer using attached screen context",
        ],
    )
}

fn planning_context_has_document_signal(normalized_context: &str) -> bool {
    contains_any(
        normalized_context,
        &[
            "[document",
            "[file",
            "kind: document",
            "(document)",
            ".pdf",
            ".docx",
            ".xlsx",
            ".csv",
            "attached document",
            "document context",
            "resume",
            "job description",
        ],
    )
}

fn collapse_spaces(text: &str) -> String {
    let mut out = String::new();
    let mut last_space = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !last_space {
                out.push(' ');
                last_space = true;
            }
        } else {
            out.push(ch);
            last_space = false;
        }
    }
    out.trim().to_string()
}

fn retrieval_status_events(
    plan: &AnswerPlan,
    rag_count: usize,
    web_search: &WebSearchOutcome,
) -> Vec<Event> {
    retrieval_status_entries(plan, rag_count, web_search)
        .into_iter()
        .map(|(stage, message)| {
            Event::default().event("status").data(
                serde_json::json!({
                    "type": "status",
                    "stage": stage,
                    "message": message,
                })
                .to_string(),
            )
        })
        .collect()
}

fn retrieval_status_entries(
    plan: &AnswerPlan,
    _rag_count: usize,
    web_search: &WebSearchOutcome,
) -> Vec<(String, String)> {
    let mut statuses: Vec<(String, String)> = Vec::new();
    if plan.needs_screen {
        statuses.push((
            "reading_screen".to_string(),
            "Reading screen context...".to_string(),
        ));
    }
    if plan.needs_docs {
        statuses.push((
            "reading_docs".to_string(),
            "Reading attached documents...".to_string(),
        ));
    }
    if plan.needs_memory {
        statuses.push((
            "using_memory".to_string(),
            "Using relevant conversation context...".to_string(),
        ));
    }
    if web_search.attempted && web_search.skipped_reason.is_none() {
        statuses.push(("searching_web".to_string(), "Searching web...".to_string()));
    }
    if !web_search.sources.is_empty() {
        statuses.push((
            "reading_web_sources".to_string(),
            format!("Reading {} sources...", web_search.sources.len()),
        ));
    }
    if web_search.searches_used > 0 {
        statuses.push((
            "web_search_used".to_string(),
            web_search_usage_label(web_search.searches_used, web_search.sources.len()),
        ));
    } else if let Some(reason) = web_search.skipped_reason {
        statuses.push((
            "web_search_skipped".to_string(),
            web_search_skipped_label(reason).to_string(),
        ));
    }

    statuses
}

fn web_search_usage_label(searches_used: i64, source_count: usize) -> String {
    format!(
        "Web search used: {} {}, {} {}",
        searches_used,
        pluralize(searches_used, "search", "searches"),
        source_count,
        pluralize(source_count as i64, "source", "sources")
    )
}

fn web_search_skipped_label(reason: &str) -> &'static str {
    match reason {
        "provider_not_configured" => "Web search is not configured yet.",
        "query_sanitized_empty_or_sensitive" => "Web search skipped for private or unsafe text.",
        "trial_web_search_quota_reached" => "Trial web search limit reached today.",
        "repeated_query_guard" => "Web search paused briefly for this repeated question.",
        "account_search_cooldown" => "Web search paused briefly. Try again soon.",
        "insufficient_credits" => "Add credits to use web search.",
        "credit_check_unavailable" => "Web search is temporarily unavailable.",
        "provider_timeout" => "Web search timed out.",
        "provider_error" => "Web search provider failed.",
        _ => "Web search skipped.",
    }
}

fn pluralize(count: i64, singular: &'static str, plural: &'static str) -> &'static str {
    if count == 1 {
        singular
    } else {
        plural
    }
}

fn sources_sse_event(sources: &[CompleteSource]) -> Option<Event> {
    if sources.is_empty() {
        return None;
    }
    Some(
        Event::default().event("sources").data(
            serde_json::json!({
                "type": "sources",
                "sources": sources,
            })
            .to_string(),
        ),
    )
}

fn web_search_usage_event(request_id: &str, outcome: &WebSearchOutcome) -> Option<UsageEvent> {
    if outcome.searches_used <= 0 {
        return None;
    }
    Some(UsageEvent {
        request_id: format!("{request_id}:web-search"),
        kind: WEB_SEARCH_USAGE_KIND.to_string(),
        task_type: Some(WEB_SEARCH_TASK_TYPE.to_string()),
        lane: Some("web_search".to_string()),
        provider: outcome.provider.clone(),
        model: Some(WEB_SEARCH_USAGE_MODEL.to_string()),
        input_tokens: outcome.searches_used,
        output_tokens: outcome.sources.len() as i64,
        latency_ms: outcome.latency_ms,
        // The pre-dispatch web-search attempt hold is the sole upstream-cost
        // authority. This row allocates only the customer charge.
        cost_cents_to_bluey: 0,
        cost_cents_to_customer: outcome.customer_cost_cents,
        was_speculative: false,
        was_fallback: false,
    })
}

fn managed_completion_settlement_events(
    request_id: &str,
    primary_event: UsageEvent,
    llm_customer_cost_cents: i64,
    outcome: &WebSearchOutcome,
) -> Vec<usage_reservations::SettlementUsageEvent> {
    let mut events = vec![usage_reservations::SettlementUsageEvent {
        event: primary_event,
        customer_cost_cents: llm_customer_cost_cents,
    }];
    if let Some(web_event) = web_search_usage_event(request_id, outcome) {
        events.push(usage_reservations::SettlementUsageEvent {
            event: web_event,
            customer_cost_cents: outcome.customer_cost_cents,
        });
    }
    events
}

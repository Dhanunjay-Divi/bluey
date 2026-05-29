//! Provider/key health ledger for managed routing.
//!
//! Provider token buckets answer "how much traffic may this provider family
//! receive?" This ledger answers the more operational question: "which exact
//! provider/model/key is cooling down after an upstream 429?" Keeping that
//! state lets realtime paid requests route around a throttled allocation
//! instead of making the customer wait.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tokio::sync::Mutex;

use crate::config::UpstreamKeyCandidate;
use crate::rate_limit::CapacityDenied;

#[derive(Clone)]
pub struct ProviderHealth {
    local: Arc<Mutex<HashMap<String, Instant>>>,
    redis: Option<RedisHealth>,
    max_cooldown_secs: u64,
    counters: Arc<ProviderHealthCounters>,
}

#[derive(Clone)]
struct RedisHealth {
    client: redis::Client,
    namespace: Arc<str>,
}

#[derive(Default)]
struct ProviderHealthCounters {
    cooldowns_total: AtomicU64,
    all_keys_cooling_total: AtomicU64,
    redis_errors_total: AtomicU64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProviderHealthSnapshot {
    pub cooldowns_total: u64,
    pub all_keys_cooling_total: u64,
    pub redis_errors_total: u64,
}

impl Default for ProviderHealth {
    fn default() -> Self {
        Self {
            local: Arc::new(Mutex::new(HashMap::new())),
            redis: redis_health_from_env(),
            max_cooldown_secs: env_u64("BLUEY_PROVIDER_MAX_COOLDOWN_SECS").unwrap_or(300),
            counters: Arc::new(ProviderHealthCounters::default()),
        }
    }
}

impl ProviderHealth {
    pub fn snapshot(&self) -> ProviderHealthSnapshot {
        ProviderHealthSnapshot {
            cooldowns_total: self.counters.cooldowns_total.load(Ordering::Relaxed),
            all_keys_cooling_total: self
                .counters
                .all_keys_cooling_total
                .load(Ordering::Relaxed),
            redis_errors_total: self.counters.redis_errors_total.load(Ordering::Relaxed),
        }
    }

    pub async fn choose_key(
        &self,
        provider: &str,
        model: &str,
        candidates: &[UpstreamKeyCandidate],
    ) -> Result<UpstreamKeyCandidate, CapacityDenied> {
        let mut shortest_retry: Option<u64> = None;
        for candidate in candidates {
            match self
                .cooldown_remaining(provider, model, &candidate.fingerprint)
                .await
            {
                Some(retry_after_secs) => {
                    shortest_retry = Some(
                        shortest_retry
                            .map(|current| current.min(retry_after_secs))
                            .unwrap_or(retry_after_secs),
                    );
                }
                None => return Ok(candidate.clone()),
            }
        }
        self.counters
            .all_keys_cooling_total
            .fetch_add(1, Ordering::Relaxed);
        Err(CapacityDenied {
            retry_after_secs: shortest_retry.unwrap_or(1),
            reason: "provider_key_cooling_down",
        })
    }

    pub async fn record_cooldown(
        &self,
        provider: &str,
        model: &str,
        key_fingerprint: &str,
        retry_after_secs: u64,
    ) -> u64 {
        let cooldown_secs = retry_after_secs
            .max(1)
            .min(self.max_cooldown_secs.max(1));
        let key = health_key(provider, model, key_fingerprint);
        let expires_at = Instant::now() + Duration::from_secs(cooldown_secs);
        self.local.lock().await.insert(key.clone(), expires_at);
        self.counters
            .cooldowns_total
            .fetch_add(1, Ordering::Relaxed);

        if let Some(redis) = &self.redis {
            if let Err(error) = redis.record(&key, cooldown_secs).await {
                self.counters
                    .redis_errors_total
                    .fetch_add(1, Ordering::Relaxed);
                tracing::warn!(
                    provider,
                    model,
                    key_fingerprint,
                    error = %error,
                    "redis provider-health cooldown write failed; local cooldown remains active"
                );
            }
        }

        cooldown_secs
    }

    async fn cooldown_remaining(
        &self,
        provider: &str,
        model: &str,
        key_fingerprint: &str,
    ) -> Option<u64> {
        let key = health_key(provider, model, key_fingerprint);
        if let Some(redis) = &self.redis {
            match redis.remaining(&key).await {
                Ok(Some(seconds)) => return Some(seconds),
                Ok(None) => {}
                Err(error) => {
                    self.counters
                        .redis_errors_total
                        .fetch_add(1, Ordering::Relaxed);
                    tracing::warn!(
                        provider,
                        model,
                        key_fingerprint,
                        error = %error,
                        "redis provider-health read failed; falling back to local ledger"
                    );
                }
            }
        }

        let mut local = self.local.lock().await;
        let expires_at = local.get(&key).copied()?;
        let now = Instant::now();
        if expires_at <= now {
            local.remove(&key);
            return None;
        }
        Some((expires_at - now).as_secs().max(1))
    }
}

impl RedisHealth {
    async fn remaining(&self, key: &str) -> anyhow::Result<Option<u64>> {
        let redis_key = format!("{}:provider_health:{key}", self.namespace);
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let ttl: i64 = redis::cmd("TTL")
            .arg(redis_key)
            .query_async(&mut conn)
            .await?;
        if ttl > 0 {
            Ok(Some(ttl as u64))
        } else {
            Ok(None)
        }
    }

    async fn record(&self, key: &str, cooldown_secs: u64) -> anyhow::Result<()> {
        let redis_key = format!("{}:provider_health:{key}", self.namespace);
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        redis::cmd("SET")
            .arg(redis_key)
            .arg("cooldown")
            .arg("EX")
            .arg(cooldown_secs.max(1))
            .query_async::<()>(&mut conn)
            .await?;
        Ok(())
    }
}

fn redis_health_from_env() -> Option<RedisHealth> {
    let url = std::env::var("BLUEY_REDIS_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())?;
    match redis::Client::open(url.as_str()) {
        Ok(client) => Some(RedisHealth {
            client,
            namespace: Arc::from(
                std::env::var("BLUEY_REDIS_NAMESPACE")
                    .ok()
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| "bluey".to_string()),
            ),
        }),
        Err(error) => {
            tracing::error!(
                error = %error,
                "invalid BLUEY_REDIS_URL; using local provider-health ledger"
            );
            None
        }
    }
}

fn health_key(provider: &str, model: &str, key_fingerprint: &str) -> String {
    format!("{provider}:{model}:{key_fingerprint}")
}

fn env_u64(name: &str) -> Option<u64> {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|value| *value > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(secret: &str, fingerprint: &str) -> UpstreamKeyCandidate {
        UpstreamKeyCandidate {
            secret: secret.to_string(),
            fingerprint: fingerprint.to_string(),
        }
    }

    #[tokio::test]
    async fn choose_key_skips_cooling_candidate() {
        let health = ProviderHealth::default();
        let candidates = vec![candidate("key-a", "fp-a"), candidate("key-b", "fp-b")];

        health.record_cooldown("openai", "gpt-4o-mini", "fp-a", 60).await;

        let selected = health
            .choose_key("openai", "gpt-4o-mini", &candidates)
            .await
            .unwrap();
        assert_eq!(selected.secret, "key-b");
    }

    #[tokio::test]
    async fn choose_key_reports_retry_when_all_candidates_are_cooling() {
        let health = ProviderHealth::default();
        let candidates = vec![candidate("key-a", "fp-a"), candidate("key-b", "fp-b")];

        health.record_cooldown("openai", "gpt-4o-mini", "fp-a", 60).await;
        health.record_cooldown("openai", "gpt-4o-mini", "fp-b", 30).await;

        let denied = health
            .choose_key("openai", "gpt-4o-mini", &candidates)
            .await
            .unwrap_err();
        assert_eq!(denied.reason, "provider_key_cooling_down");
        assert!((1..=30).contains(&denied.retry_after_secs));
    }

    #[tokio::test]
    async fn snapshot_counts_cooldowns_and_all_keys_cooling() {
        let health = ProviderHealth::default();
        let candidates = vec![candidate("key-a", "fp-a")];

        health
            .record_cooldown("openai", "gpt-4o-mini", "fp-a", 60)
            .await;
        let _ = health
            .choose_key("openai", "gpt-4o-mini", &candidates)
            .await
            .unwrap_err();

        let snapshot = health.snapshot();
        assert_eq!(snapshot.cooldowns_total, 1);
        assert_eq!(snapshot.all_keys_cooling_total, 1);
    }
}

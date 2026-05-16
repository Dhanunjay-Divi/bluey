//! Generic token-bucket rate limiter.
//!
//! Designed for future use with LLM/STT API rate-limiting. Not yet wired to
//! any caller — available as a utility for any subsystem that needs
//! non-blocking or async rate-gating.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

/// Fixed-point scale: we store tokens * SCALE to avoid floating point atomics.
const SCALE: u64 = 1_000_000;

/// A lock-free token-bucket rate limiter.
pub struct RateLimiter {
    capacity: u32,
    refill_per_sec_scaled: u64,
    tokens_scaled: AtomicU64,
    /// Epoch instant — all timing is relative to this.
    epoch: Instant,
    last_refill_ms: AtomicU64,
}

impl RateLimiter {
    /// Create a new rate limiter with given capacity and refill rate.
    /// Starts full.
    pub fn new(capacity: u32, refill_per_sec: f64) -> Self {
        Self {
            capacity,
            refill_per_sec_scaled: (refill_per_sec * SCALE as f64) as u64,
            tokens_scaled: AtomicU64::new(capacity as u64 * SCALE),
            epoch: Instant::now(),
            last_refill_ms: AtomicU64::new(0),
        }
    }

    /// Try to acquire `n` tokens without blocking. Returns `true` if acquired.
    pub fn try_acquire(&self, n: u32) -> bool {
        self.refill();
        let needed = n as u64 * SCALE;
        loop {
            let current = self.tokens_scaled.load(Ordering::Acquire);
            if current < needed {
                return false;
            }
            match self.tokens_scaled.compare_exchange_weak(
                current,
                current - needed,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return true,
                Err(_) => continue,
            }
        }
    }

    /// Async acquire: sleeps until `n` tokens are available, then consumes them.
    pub async fn acquire(&self, n: u32) {
        loop {
            if self.try_acquire(n) {
                return;
            }
            let needed = n as u64 * SCALE;
            let current = self.tokens_scaled.load(Ordering::Acquire);
            let deficit = needed.saturating_sub(current);
            let wait_ms = if self.refill_per_sec_scaled > 0 {
                (deficit * 1000) / self.refill_per_sec_scaled
            } else {
                100
            };
            tokio::time::sleep(tokio::time::Duration::from_millis(wait_ms.max(1))).await;
        }
    }

    /// Number of tokens currently available (approximate).
    pub fn available(&self) -> u32 {
        self.refill();
        (self.tokens_scaled.load(Ordering::Acquire) / SCALE) as u32
    }

    fn refill(&self) {
        let now_ms = self.epoch.elapsed().as_millis() as u64;
        let prev_ms = self.last_refill_ms.load(Ordering::Acquire);
        if now_ms <= prev_ms {
            return;
        }
        if self
            .last_refill_ms
            .compare_exchange(prev_ms, now_ms, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        let elapsed_ms = now_ms - prev_ms;
        let add = (self.refill_per_sec_scaled * elapsed_ms) / 1000;
        if add == 0 {
            return;
        }
        let cap_scaled = self.capacity as u64 * SCALE;
        loop {
            let current = self.tokens_scaled.load(Ordering::Acquire);
            let new_val = (current + add).min(cap_scaled);
            match self.tokens_scaled.compare_exchange_weak(
                current,
                new_val,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(_) => continue,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn capacity_respected() {
        let rl = RateLimiter::new(5, 1.0);
        for _ in 0..5 {
            assert!(rl.try_acquire(1));
        }
        assert!(!rl.try_acquire(1));
    }

    #[test]
    fn multi_token_acquire() {
        let rl = RateLimiter::new(10, 100.0);
        assert!(rl.try_acquire(10));
        assert!(!rl.try_acquire(1));
    }

    #[tokio::test]
    async fn refill_over_time() {
        let rl = RateLimiter::new(2, 100.0);
        assert!(rl.try_acquire(2));
        assert!(!rl.try_acquire(1));
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        assert!(rl.try_acquire(1), "should have refilled after 50ms");
    }

    #[tokio::test]
    async fn acquire_blocks_until_available() {
        let rl = Arc::new(RateLimiter::new(1, 100.0));
        assert!(rl.try_acquire(1));
        let rl2 = rl.clone();
        let start = Instant::now();
        rl2.acquire(1).await;
        let elapsed = start.elapsed();
        assert!(elapsed.as_millis() >= 5, "should have waited for refill");
    }

    #[test]
    fn concurrent_try_acquire() {
        let rl = Arc::new(RateLimiter::new(100, 0.0));
        let mut handles = vec![];
        for _ in 0..10 {
            let rl = rl.clone();
            handles.push(std::thread::spawn(move || {
                let mut count = 0u32;
                for _ in 0..20 {
                    if rl.try_acquire(1) {
                        count += 1;
                    }
                }
                count
            }));
        }
        let total: u32 = handles.into_iter().map(|h| h.join().unwrap()).sum();
        assert_eq!(total, 100, "exactly capacity tokens should be granted");
    }

    #[test]
    fn available_reports_correctly() {
        let rl = RateLimiter::new(10, 0.0);
        assert_eq!(rl.available(), 10);
        rl.try_acquire(3);
        assert_eq!(rl.available(), 7);
    }

    #[tokio::test]
    async fn exhausted_bucket_retries() {
        let rl = Arc::new(RateLimiter::new(1, 50.0));
        assert!(rl.try_acquire(1));
        let rl2 = rl.clone();
        tokio::time::timeout(tokio::time::Duration::from_millis(200), rl2.acquire(1))
            .await
            .expect("acquire should complete within 200ms");
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{mpsc, Arc, Barrier};

    use super::*;
    use crate::db::{self, accounts::Account, balance, idempotency};

    fn temp_pool() -> DbPool {
        let path = std::env::temp_dir().join(format!(
            "bluey-usage-reservations-{}.db",
            uuid::Uuid::new_v4()
        ));
        let pool = db::open_pool(&path).unwrap();
        db::run_migrations(&pool).unwrap();
        pool
    }

    fn create_paid_account(pool: &DbPool, email: &str, cents: i64) -> String {
        let account = Account::create(pool, email, "hash").unwrap();
        pool.get()
            .unwrap()
            .execute(
                "UPDATE accounts SET trial_seconds_remaining = 0 WHERE id = ?1",
                params![&account.id],
            )
            .unwrap();
        balance::credit_internal(pool, &account.id, cents, "usage-reservation-test").unwrap();
        account.id
    }

    fn input<'a>(
        account_id: &'a str,
        request_id: &'a str,
        estimated_customer_cents: i64,
        created_at_ms: i64,
        expires_at_ms: i64,
    ) -> ReserveUsageInput<'a> {
        ReserveUsageInput {
            account_id,
            request_id,
            kind: "llm",
            reason: "llm_test",
            estimated_customer_cents,
            estimated_upstream_cents: estimated_customer_cents / 2,
            upstream_spend_guard: None,
            created_at_ms,
            expires_at_ms,
        }
    }

    fn account_money(pool: &DbPool, account_id: &str) -> (i64, i64) {
        pool.get()
            .unwrap()
            .query_row(
                "SELECT balance_cents, reserved_cents FROM accounts WHERE id = ?1",
                params![account_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap()
    }

    #[test]
    fn concurrent_paid_reservations_cannot_overspend() {
        let pool = Arc::new(temp_pool());
        let account_id = create_paid_account(&pool, "usage-concurrency@example.com", 100);
        let barrier = Arc::new(Barrier::new(3));
        let mut workers = Vec::new();

        for request_id in ["concurrent-1", "concurrent-2"] {
            let pool = Arc::clone(&pool);
            let account_id = account_id.clone();
            let barrier = Arc::clone(&barrier);
            workers.push(std::thread::spawn(move || {
                barrier.wait();
                reserve(&pool, input(&account_id, request_id, 75, 1_000, 61_000))
            }));
        }
        barrier.wait();

        let results: Vec<_> = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|result| matches!(result, Err(UsageReservationError::InsufficientBalance)))
                .count(),
            1
        );
        assert_eq!(account_money(&pool, &account_id), (25, 75));
    }

    #[test]
    fn application_clock_skew_cannot_hide_a_fresh_upstream_reservation() {
        let pool = temp_pool();
        let account_id = create_paid_account(&pool, "usage-clock-skew@example.com", 100);
        let guard = UpstreamSpendGuard {
            limit_cents: 5,
            window_hours: 24,
        };
        let mut past_clock = input(
            &account_id,
            "past-clock",
            10,
            -4_000_000_000,
            -3_999_940_000,
        );
        past_clock.estimated_upstream_cents = 4;
        past_clock.upstream_spend_guard = Some(guard);
        let first = reserve(&pool, past_clock).unwrap();

        let (created_at_ms, expires_at_ms): (i64, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT created_at_ms, expires_at_ms FROM usage_reservations
                  WHERE account_id = ?1 AND request_id = 'past-clock'",
                params![account_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert!(created_at_ms > 1_000_000_000_000);
        assert_eq!(expires_at_ms - created_at_ms, 60_000);
        assert_eq!(first.expires_at_ms, expires_at_ms);

        let mut future_clock = input(
            &account_id,
            "future-clock",
            10,
            4_000_000_000_000,
            4_000_000_060_000,
        );
        future_clock.estimated_upstream_cents = 2;
        future_clock.upstream_spend_guard = Some(guard);
        assert!(matches!(
            reserve(&pool, future_clock),
            Err(UsageReservationError::UpstreamSpendLimit)
        ));
    }

    #[test]
    fn anonymous_cutover_baseline_blocks_window_but_uses_fixed_retention() {
        let pool = temp_pool();
        let account_id = create_paid_account(&pool, "usage-cutover@example.com", 100);
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO usage_cutover_spend_baseline(occurred_at, cost_cents)
                 VALUES (datetime('now'), 7)",
                [],
            )
            .unwrap();

        let now = crate::db::jobs::now_ms();
        let guard = UpstreamSpendGuard {
            limit_cents: 10,
            window_hours: 24,
        };
        let mut blocked = input(
            &account_id,
            "cutover-baseline-blocked",
            8,
            now,
            now.saturating_add(60_000),
        );
        blocked.estimated_upstream_cents = 4;
        blocked.upstream_spend_guard = Some(guard);
        assert!(matches!(
            reserve(&pool, blocked),
            Err(UsageReservationError::UpstreamSpendLimit)
        ));
        assert_eq!(account_money(&pool, &account_id), (100, 0));

        // The row leaves this 24-hour admission window, but physical retention
        // remains fixed at the maximum configurable window plus grace.
        pool.get()
            .unwrap()
            .execute(
                "UPDATE usage_cutover_spend_baseline
                    SET occurred_at = datetime('now', '-3 days')",
                [],
            )
            .unwrap();
        let mut admitted = input(
            &account_id,
            "cutover-baseline-expired",
            8,
            now.saturating_add(1),
            now.saturating_add(60_001),
        );
        admitted.estimated_upstream_cents = 4;
        admitted.upstream_spend_guard = Some(guard);
        reserve(&pool, admitted).unwrap();
        assert_eq!(account_money(&pool, &account_id), (92, 8));
        assert_eq!(
            pool.get()
                .unwrap()
                .query_row(
                    "SELECT COUNT(*) FROM usage_cutover_spend_baseline",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            1
        );
        pool.get()
            .unwrap()
            .execute(
                "UPDATE usage_cutover_spend_baseline
                    SET occurred_at = datetime('now', '-32 days')",
                [],
            )
            .unwrap();
        let cleanup =
            crate::db::jobs_provider_cost_holds::prune_expired_spend_truth(&pool).unwrap();
        assert_eq!(cleanup.cutover_baseline_rows_deleted, 1);
    }

    #[test]
    fn trial_reservation_admits_only_one_concurrent_request() {
        let pool = temp_pool();
        let account = Account::create(&pool, "usage-trial@example.com", "hash").unwrap();
        let initial_trial = account.trial_seconds_remaining;
        balance::credit_internal(&pool, &account.id, 100, "trial-serialization-test").unwrap();

        let first = reserve(&pool, input(&account.id, "trial-1", 50, 1_000, 61_000)).unwrap();
        assert_eq!(first.reserved_trial_seconds, initial_trial);
        assert!(first.is_trial());

        let second = reserve(&pool, input(&account.id, "trial-2", 50, 1_000, 61_000)).unwrap_err();
        assert!(matches!(second, UsageReservationError::InProgress));
        release(&pool, &account.id, "trial-1", "test_release", 2_000).unwrap();

        let retried = reserve(&pool, input(&account.id, "trial-2", 50, 3_000, 63_000)).unwrap();
        assert_eq!(retried.reserved_trial_seconds, initial_trial);
    }

    #[test]
    fn released_request_id_can_retry_and_settlement_is_idempotent() {
        let pool = temp_pool();
        let account_id = create_paid_account(&pool, "usage-retry@example.com", 100);

        let first = reserve(&pool, input(&account_id, "retry-1", 60, 1_000, 61_000)).unwrap();
        assert_eq!(first.attempt, 1);
        assert_eq!(account_money(&pool, &account_id), (40, 60));

        let released = release(&pool, &account_id, "retry-1", "upstream_error", 2_000).unwrap();
        assert_eq!(released.refunded_cents, 60);
        assert_eq!(account_money(&pool, &account_id), (100, 0));

        let second = reserve(&pool, input(&account_id, "retry-1", 60, 3_000, 63_000)).unwrap();
        assert_eq!(second.attempt, 2);
        assert_eq!(account_money(&pool, &account_id), (40, 60));

        let settled = settle(&pool, &account_id, "retry-1", 20, 1_500, "completed", 5_000).unwrap();
        assert_eq!(settled.charged_customer_cents, 20);
        assert_eq!(settled.refunded_cents, 40);
        assert_eq!(account_money(&pool, &account_id), (80, 0));

        let replay = settle(&pool, &account_id, "retry-1", 20, 1_500, "completed", 6_000).unwrap();
        assert_eq!(replay, settled);
        assert_eq!(account_money(&pool, &account_id), (80, 0));

        let batch_remaining: i64 = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT remaining_cents FROM credit_batches WHERE account_id = ?1",
                params![account_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(batch_remaining, 80);
    }

    #[test]
    fn ttl_reconciliation_refunds_and_reopens_request_id() {
        let pool = temp_pool();
        let account_id = create_paid_account(&pool, "usage-ttl@example.com", 100);
        assert_eq!(
            idempotency::reserve(&pool, &account_id, "ttl-1").unwrap(),
            idempotency::ReserveOutcome::FreshReservation
        );
        let reservation = reserve(&pool, input(&account_id, "ttl-1", 70, 1_000, 2_000)).unwrap();
        assert_eq!(account_money(&pool, &account_id), (30, 70));

        // Caller-provided future time cannot expire a DB-authoritative hold.
        assert_eq!(
            reconcile_expired_for_account(&pool, &account_id, i64::MAX).unwrap(),
            0
        );
        assert!(reservation_expiry_delay_ms(
            &pool,
            &account_id,
            "ttl-1",
            reservation.attempt,
            reservation.expires_at_ms
        )
        .unwrap()
        .is_some_and(|delay| delay > 0));
        pool.get()
            .unwrap()
            .execute(
                "UPDATE usage_reservations
                    SET expires_at_ms = CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER) - 1
                  WHERE account_id = ?1 AND request_id = 'ttl-1'",
                params![account_id],
            )
            .unwrap();
        // Caller-provided past time cannot keep an expired hold alive.
        assert_eq!(
            reconcile_expired_for_account(&pool, &account_id, i64::MIN).unwrap(),
            1
        );
        assert_eq!(
            reconcile_expired_for_account(&pool, &account_id, i64::MAX).unwrap(),
            0
        );
        assert_eq!(account_money(&pool, &account_id), (100, 0));
        assert_eq!(
            idempotency::reserve(&pool, &account_id, "ttl-1").unwrap(),
            idempotency::ReserveOutcome::FreshReservation
        );

        let (status, reason): (String, String) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT status, terminal_reason
                   FROM usage_reservations
                  WHERE account_id = ?1 AND request_id = ?2",
                params![account_id, "ttl-1"],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(status, STATUS_RELEASED);
        assert_eq!(reason, "ttl_expired");
    }

    #[test]
    fn concurrent_startup_janitors_refund_an_expired_hold_exactly_once() {
        let pool = Arc::new(temp_pool());
        let account_id = create_paid_account(&pool, "usage-startup-janitor@example.com", 100);
        reserve(
            &pool,
            input(&account_id, "restart-expired", 70, 1_000, 61_000),
        )
        .unwrap();
        pool.get()
            .unwrap()
            .execute(
                "UPDATE usage_reservations
                    SET expires_at_ms = CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER) - 1
                  WHERE account_id = ?1 AND request_id = 'restart-expired'",
                params![account_id],
            )
            .unwrap();

        let barrier = Arc::new(Barrier::new(3));
        let workers = (0..2)
            .map(|_| {
                let pool = Arc::clone(&pool);
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    reconcile_expired_usage_reservations(&pool).unwrap()
                })
            })
            .collect::<Vec<_>>();
        barrier.wait();
        let released: usize = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .sum();
        assert_eq!(released, 1);
        assert_eq!(account_money(&pool, &account_id), (100, 0));
    }

    #[test]
    fn stale_janitor_selection_cannot_refund_a_replacement_attempt() {
        let pool = Arc::new(temp_pool());
        let account_id = create_paid_account(&pool, "usage-stale-janitor@example.com", 100);
        let first = reserve(
            &pool,
            input(&account_id, "reused-request", 70, 1_000, 61_000),
        )
        .unwrap();
        pool.get()
            .unwrap()
            .execute(
                "UPDATE usage_reservations
                    SET expires_at_ms = CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER) - 1
                  WHERE account_id = ?1 AND request_id = 'reused-request'",
                params![account_id],
            )
            .unwrap();

        let (selected_tx, selected_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        let worker_pool = Arc::clone(&pool);
        let worker = std::thread::spawn(move || {
            let key = expired_reservation_keys(&worker_pool, 1)
                .unwrap()
                .into_iter()
                .next()
                .expect("expired attempt selected");
            selected_tx.send(key.clone()).unwrap();
            resume_rx.recv().unwrap();
            release_expired_attempt(
                &worker_pool,
                &key.account_id,
                &key.request_id,
                key.attempt,
                key.expires_at_ms,
            )
            .unwrap()
        });

        let stale_key = selected_rx.recv().unwrap();
        assert_eq!(stale_key.attempt, first.attempt);
        release(
            &pool,
            &account_id,
            "reused-request",
            "concurrent_release",
            0,
        )
        .unwrap();
        let replacement = reserve(
            &pool,
            input(&account_id, "reused-request", 70, 3_000, 63_000),
        )
        .unwrap();
        assert_eq!(replacement.attempt, first.attempt + 1);
        assert_eq!(account_money(&pool, &account_id), (30, 70));

        resume_tx.send(()).unwrap();
        assert!(
            !worker.join().unwrap(),
            "stale janitor fence must no-op after request-id replacement"
        );
        assert_eq!(account_money(&pool, &account_id), (30, 70));
        let (status, attempt, expires_at_ms): (String, i64, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT status, attempt, expires_at_ms
                   FROM usage_reservations
                  WHERE account_id = ?1 AND request_id = 'reused-request'",
                params![account_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(status, STATUS_RESERVED);
        assert_eq!(attempt, replacement.attempt);
        assert_eq!(expires_at_ms, replacement.expires_at_ms);
        assert_eq!(
            reservation_expiry_delay_ms(
                &pool,
                &account_id,
                "reused-request",
                stale_key.attempt,
                stale_key.expires_at_ms,
            )
            .unwrap(),
            None,
            "the stale per-request delayed task must be fenced too"
        );
    }

    #[test]
    fn expired_trial_hold_cannot_block_next_endpoint_after_restart() {
        let pool = temp_pool();
        let account = Account::create(&pool, "usage-restart-trial@example.com", "hash").unwrap();
        let trial_seconds = account.trial_seconds_remaining;
        let first = reserve(
            &pool,
            input(&account.id, "stale-endpoint", 50, 1_000, 61_000),
        )
        .unwrap();
        assert_eq!(first.reserved_trial_seconds, trial_seconds);
        pool.get()
            .unwrap()
            .execute(
                "UPDATE usage_reservations
                    SET expires_at_ms = CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER) - 1
                  WHERE account_id = ?1 AND request_id = 'stale-endpoint'",
                params![account.id],
            )
            .unwrap();

        // `reserve` performs DB-clock reconciliation itself, so embed/STT/LLM
        // callers all recover even if their prior delayed task died.
        let next = reserve(
            &pool,
            input(&account.id, "next-endpoint", 50, 9_000, 69_000),
        )
        .unwrap();
        assert_eq!(next.reserved_trial_seconds, trial_seconds);
    }

    #[test]
    fn postgres_completion_settlement_cache_atomic_when_owned() {
        const CASE: &str =
            "db::usage_reservations::tests::postgres_completion_settlement_cache_atomic_when_owned";
        let Some(url) = db::owned_postgres_test::owned_url(CASE) else {
            return;
        };
        let pool = db::open_postgres_pool(&url).unwrap();
        db::run_migrations(&pool).unwrap();
        let account = Account::create(
            &pool,
            "completion-cache-atomic@example.com",
            "synthetic-hash",
        )
        .unwrap();
        pool.get_pg()
            .unwrap()
            .execute(
                "UPDATE accounts SET trial_seconds_remaining = 0 WHERE id = $1",
                &[&account.id],
            )
            .unwrap();
        assert!(
            balance::credit_internal(&pool, &account.id, 100, "completion-cache-atomic-test",)
                .unwrap()
        );
        let request_id = "completion-cache-atomic-request";
        assert_eq!(
            idempotency::reserve(&pool, &account.id, request_id).unwrap(),
            idempotency::ReserveOutcome::FreshReservation
        );
        reserve(&pool, input(&account.id, request_id, 40, 1_000, 61_000)).unwrap();
        let components = [SettlementUsageEvent {
            event: UsageEvent {
                request_id: request_id.to_string(),
                kind: "llm".to_string(),
                task_type: None,
                lane: Some("instant".to_string()),
                provider: Some("openai".to_string()),
                model: Some("gpt-5.4-mini".to_string()),
                input_tokens: 2,
                output_tokens: 1,
                latency_ms: 5,
                cost_cents_to_bluey: 0,
                cost_cents_to_customer: 20,
                was_speculative: false,
                was_fallback: false,
            },
            customer_cost_cents: 20,
        }];
        let encode = |settled: &SettledUsage| {
            serde_json::to_string(&serde_json::json!({
                "balance_cents": settled.balance_cents_after,
                "charged_cents": settled.charged_customer_cents,
                "request_id": request_id,
            }))
        };
        let snapshot = || {
            let mut conn = pool.get_pg().unwrap();
            let row = conn
                .query_one(
                    "SELECT a.balance_cents, a.reserved_cents, r.status,
                            (SELECT COUNT(*)::bigint FROM usage_events
                              WHERE account_id = $1 AND request_id = $2),
                            (SELECT COUNT(*)::bigint FROM balance_ledger_entries
                              WHERE account_id = $1),
                            (SELECT COALESCE(SUM(remaining_cents), 0)::bigint
                               FROM credit_batches WHERE account_id = $1),
                            i.status, i.response_json,
                            COALESCE(i.http_status = 200, FALSE),
                            i.completed_at IS NOT NULL,
                            CASE WHEN i.completed_at IS NULL THEN NULL
                                 ELSE i.completed_at::text END
                       FROM accounts a
                       JOIN usage_reservations r ON r.account_id = a.id
                       JOIN request_idempotency i ON i.account_id = a.id
                      WHERE a.id = $1 AND r.request_id = $2 AND i.request_id = $2",
                    &[&account.id, &request_id],
                )
                .unwrap();
            (
                row.get::<_, i64>(0),
                row.get::<_, i64>(1),
                row.get::<_, String>(2),
                row.get::<_, i64>(3),
                row.get::<_, i64>(4),
                row.get::<_, i64>(5),
                row.get::<_, String>(6),
                row.get::<_, Option<String>>(7),
                row.get::<_, bool>(8),
                row.get::<_, bool>(9),
                row.get::<_, Option<String>>(10),
            )
        };
        let reserved = snapshot();
        assert_eq!(
            (&reserved.0, &reserved.1, reserved.2.as_str()),
            (&60, &40, "reserved")
        );
        assert_eq!(reserved.3, 0);
        assert_eq!(reserved.5, 100);
        assert_eq!(reserved.6, "in_progress");
        assert_eq!(
            (reserved.7.as_ref(), reserved.8, reserved.9),
            (None, false, false)
        );

        pool.get_pg()
            .unwrap()
            .batch_execute(
                "CREATE FUNCTION bluey_test_reject_completion_cache() RETURNS trigger
                     LANGUAGE plpgsql AS $$
                   BEGIN
                     RAISE EXCEPTION 'synthetic completion cache refusal';
                   END
                   $$;
                 CREATE TRIGGER bluey_test_reject_completion_cache
                   BEFORE UPDATE ON request_idempotency
                   FOR EACH ROW WHEN (NEW.status = 'complete')
                   EXECUTE FUNCTION bluey_test_reject_completion_cache();",
            )
            .unwrap();
        let injected = settle_with_events_and_completion(
            &pool,
            &account.id,
            request_id,
            20,
            1_000,
            "completed",
            2_000,
            &components,
            &encode,
        )
        .unwrap_err();
        let UsageReservationError::Db(injected) = injected else {
            panic!("expected injected database failure");
        };
        let sqlstate = injected.chain().find_map(|cause| {
            cause
                .downcast_ref::<postgres::Error>()
                .and_then(postgres::Error::code)
                .map(|code| code.code().to_string())
        });
        assert_eq!(sqlstate.as_deref(), Some("P0001"));
        assert_eq!(snapshot(), reserved);
        pool.get_pg()
            .unwrap()
            .batch_execute(
                "DROP TRIGGER bluey_test_reject_completion_cache ON request_idempotency;
                 DROP FUNCTION bluey_test_reject_completion_cache();",
            )
            .unwrap();

        let settled = settle_with_events_and_completion(
            &pool,
            &account.id,
            request_id,
            20,
            1_000,
            "completed",
            3_000,
            &components,
            &encode,
        )
        .unwrap();
        assert_eq!(settled.charged_customer_cents, 20);
        let expected_response = encode(&settled).unwrap();
        let complete = snapshot();
        assert_eq!(
            (&complete.0, &complete.1, complete.2.as_str()),
            (&80, &0, "settled")
        );
        assert_eq!(complete.3, 1);
        assert_eq!(complete.4, reserved.4 + 1);
        assert_eq!(complete.5, 80);
        assert_eq!(complete.6, "complete");
        assert_eq!(complete.7.as_deref(), Some(expected_response.as_str()));
        assert!(complete.8 && complete.9 && complete.10.is_some());

        assert_eq!(
            settle_with_events_and_completion(
                &pool,
                &account.id,
                request_id,
                20,
                1_000,
                "completed",
                4_000,
                &components,
                &encode,
            )
            .unwrap(),
            settled
        );
        assert_eq!(snapshot(), complete);

        pool.get_pg()
            .unwrap()
            .execute(
                "UPDATE request_idempotency SET response_json = $3
                  WHERE account_id = $1 AND request_id = $2",
                &[&account.id, &request_id, &"{\"wrong\":true}"],
            )
            .unwrap();
        let mismatched = snapshot();
        assert!(matches!(
            settle_with_events_and_completion(
                &pool,
                &account.id,
                request_id,
                20,
                1_000,
                "completed",
                5_000,
                &components,
                &encode,
            ),
            Err(UsageReservationError::InvalidReservation)
        ));
        assert_eq!(snapshot(), mismatched);
        assert_eq!(
            (&mismatched.0, &mismatched.1, mismatched.2.as_str()),
            (&80, &0, "settled")
        );
        assert_eq!((mismatched.3, mismatched.4), (complete.3, complete.4));
        println!("owned_postgres_completion_settlement_cache_atomic_complete");
    }
}

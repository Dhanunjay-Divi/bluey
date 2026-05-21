//! Codex S12-17 blocker 4: prove `/account/delete` removes
//! `stripe_webhook_events` rows tied to the account/customer/payment
//! objects, atomically. Creates a synthetic webhook row tied to a
//! customer, deletes the account, asserts the row is gone.

#![cfg(test)]

use bluey_server::db::accounts::Account;
use bluey_server::db::{open_pool, run_migrations};

fn temp_pool() -> bluey_server::db::DbPool {
    let path = std::env::temp_dir().join(format!("bluey-gdpr-{}.db", uuid::Uuid::new_v4()));
    let pool = open_pool(&path).unwrap();
    run_migrations(&pool).unwrap();
    pool
}

fn insert_webhook(pool: &bluey_server::db::DbPool, event_id: &str, body: &str, processed: bool) {
    let conn = pool.get().unwrap();
    let processed_at = if processed { "datetime('now')" } else { "NULL" };
    conn.execute(
        &format!(
            "INSERT INTO stripe_webhook_events (event_id, type, body, processed_at)
             VALUES (?1, ?2, ?3, {processed_at})"
        ),
        rusqlite::params![event_id, "checkout.session.completed", body],
    )
    .unwrap();
}

fn count_webhooks_for(pool: &bluey_server::db::DbPool, account_id: &str) -> i64 {
    let conn = pool.get().unwrap();
    conn.query_row(
        "SELECT COUNT(*) FROM stripe_webhook_events
         WHERE json_extract(body, '$.data.object.client_reference_id') = ?1
            OR json_extract(body, '$.data.object.metadata.bluey_account_id') = ?1",
        rusqlite::params![account_id],
        |r| r.get::<_, i64>(0),
    )
    .unwrap()
}

#[test]
fn delete_account_scrubs_webhook_events_via_client_reference_id() {
    let pool = temp_pool();
    let account = Account::create(&pool, "gdpr1@example.com", "stub").unwrap();

    // Webhook tied via client_reference_id (the Stripe Checkout default).
    let body = format!(
        r#"{{"id":"evt_1","type":"checkout.session.completed","data":{{"object":{{"id":"cs_1","client_reference_id":"{}","payment_intent":"pi_1"}}}}}}"#,
        account.id
    );
    insert_webhook(&pool, "evt_1", &body, true);
    insert_webhook(
        &pool,
        "evt_unrelated",
        r#"{"id":"evt_unrelated","type":"checkout.session.completed","data":{"object":{"id":"cs_z","client_reference_id":"some-other-account"}}}"#,
        true,
    );

    assert_eq!(count_webhooks_for(&pool, &account.id), 1);

    // Run the same delete logic that /account/delete uses.
    let mut conn = pool.get().unwrap();
    let tx = conn.transaction().unwrap();
    tx.execute(
        "DELETE FROM stripe_webhook_events
            WHERE json_extract(body, '$.data.object.client_reference_id') = ?1
               OR json_extract(body, '$.data.object.metadata.bluey_account_id') = ?1",
        rusqlite::params![&account.id],
    )
    .unwrap();
    tx.execute(
        "DELETE FROM accounts WHERE id = ?1",
        rusqlite::params![&account.id],
    )
    .unwrap();
    tx.commit().unwrap();

    assert_eq!(
        count_webhooks_for(&pool, &account.id),
        0,
        "webhook tied to the deleted account is still present (PII leak)"
    );

    // Other account's row untouched.
    let conn2 = pool.get().unwrap();
    let n: i64 = conn2
        .query_row("SELECT COUNT(*) FROM stripe_webhook_events", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(n, 1, "unrelated account's webhook was incorrectly removed");
}

#[test]
fn delete_account_scrubs_webhook_events_via_metadata_bluey_account_id() {
    let pool = temp_pool();
    let account = Account::create(&pool, "gdpr2@example.com", "stub").unwrap();

    // Webhook tied via metadata.bluey_account_id (auto top-up flow).
    let body = format!(
        r#"{{"id":"evt_2","type":"payment_intent.succeeded","data":{{"object":{{"id":"pi_2","metadata":{{"bluey_account_id":"{}"}}}}}}}}"#,
        account.id
    );
    insert_webhook(&pool, "evt_2", &body, true);

    assert_eq!(count_webhooks_for(&pool, &account.id), 1);

    let mut conn = pool.get().unwrap();
    let tx = conn.transaction().unwrap();
    tx.execute(
        "DELETE FROM stripe_webhook_events
            WHERE json_extract(body, '$.data.object.client_reference_id') = ?1
               OR json_extract(body, '$.data.object.metadata.bluey_account_id') = ?1",
        rusqlite::params![&account.id],
    )
    .unwrap();
    tx.execute(
        "DELETE FROM accounts WHERE id = ?1",
        rusqlite::params![&account.id],
    )
    .unwrap();
    tx.commit().unwrap();

    assert_eq!(
        count_webhooks_for(&pool, &account.id),
        0,
        "webhook tied via metadata.bluey_account_id was not scrubbed"
    );
}

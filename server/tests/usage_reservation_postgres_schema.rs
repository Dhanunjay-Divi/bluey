const COMPAT_SCHEMA: &str =
    include_str!("../../infra/postgres/server-runtime/001_server_runtime_compat.sql");
const RESERVATION_SOURCE: &str = include_str!("../src/db/usage_reservations.rs");

#[test]
fn usage_reservation_flags_match_the_postgres_compatibility_schema() {
    let schema = COMPAT_SCHEMA
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    assert!(
        schema.contains("billing_restricted INTEGER NOT NULL DEFAULT 0"),
        "the compatibility schema contract changed; update reservation decoding deliberately"
    );
    assert!(
        RESERVATION_SOURCE.contains("try_get::<_, i32>(index)"),
        "Postgres INTEGER flags must be decoded as i32"
    );
    assert_eq!(
        RESERVATION_SOURCE
            .matches("billing_restricted: postgres_integer_flag(&row, 2)?")
            .count(),
        2,
        "both Postgres account snapshots must use the INTEGER flag decoder"
    );
    assert!(
        RESERVATION_SOURCE.contains("AND billing_restricted = 0"),
        "Postgres reservation updates must compare the INTEGER flag with zero"
    );
    assert!(
        !RESERVATION_SOURCE.contains("billing_restricted = FALSE"),
        "the compatibility schema does not use a SQL BOOLEAN for billing_restricted"
    );
}

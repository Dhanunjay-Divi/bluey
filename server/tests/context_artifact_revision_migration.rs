const MIGRATION: &str =
    include_str!("../../infra/postgres/server-runtime/006_context_artifact_revisions.sql");

#[test]
fn migration_is_postgres_only_and_keeps_the_rollback_default() {
    assert!(
        MIGRATION
            .lines()
            .take(12)
            .any(|line| line.trim().eq_ignore_ascii_case("-- Target: Postgres only")),
        "migration must be selected only by the Postgres runner"
    );

    let normalized = MIGRATION.split_whitespace().collect::<Vec<_>>().join(" ");
    let add_default = normalized
        .find("ADD COLUMN IF NOT EXISTS updated_at_ms BIGINT DEFAULT 0")
        .expect("new installs must create updated_at_ms with a default");
    let repair_default = normalized
        .find("ALTER COLUMN updated_at_ms SET DEFAULT 0")
        .expect("reruns must repair an existing column without a default");
    let backfill = normalized
        .find("SET updated_at_ms = created_at_ms")
        .expect("existing artifacts must be backfilled");
    let not_null = normalized
        .find("ALTER COLUMN updated_at_ms SET NOT NULL")
        .expect("the revision column must become non-null");

    assert!(add_default < repair_default);
    assert!(repair_default < backfill);
    assert!(backfill < not_null);
    assert!(
        !normalized.contains("DROP DEFAULT"),
        "older binaries omit updated_at_ms, so the default must remain"
    );
}

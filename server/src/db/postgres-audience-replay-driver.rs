// Phase657 integrated source candidate; full integrated verification is pending.
// Schema/transaction logic here is not signature, serving, or effect authority.
const PHASE657_REPLAY_SETUP: &str = include_str!("postgres-audience-replay-setup-legacy.sql");
const PHASE657_REPLAY_VALIDATE: &str = include_str!("postgres-audience-replay-validate-legacy.sql");

fn phase657_replay_catalog_shape(
    tx: &mut postgres::Transaction<'_>,
) -> Result<SignatureAudienceShape> {
    tx.batch_execute(PHASE657_REPLAY_VALIDATE)
        .context("validate PostgreSQL replay audience")?;
    let rows = tx.query(
        "SELECT state FROM pg_temp.phase657_audience_guard_result",
        &[],
    )?;
    anyhow::ensure!(rows.len() == 1, "invalid replay audience result");
    let tag: String = rows[0].try_get(0)?;
    match tag.as_str() {
        "legacy6" => Ok(SignatureAudienceShape::Six),
        "legacy8" => Ok(SignatureAudienceShape::Eight),
        "legacy10" => Ok(SignatureAudienceShape::Ten),
        "legacy13" => Ok(SignatureAudienceShape::Thirteen),
        "successor14" => Ok(SignatureAudienceShape::Fourteen),
        "release16" => Ok(SignatureAudienceShape::Sixteen),
        _ => anyhow::bail!("invalid replay audience result"),
    }
}

fn execute_postgres_audience_replay(conn: &mut postgres::Client, version: &str) -> Result<()> {
    let command = match version {
        JOBS_SIGNED_JOB_INTEGRITY_AUTHORITY_MIGRATION_ID => {
            SelectedSignatureAudienceMigration::Replay036
        }
        JOBS_ORIGINAL_SOURCE_VERIFIER_STARTUP_AUTHORITY_MIGRATION_ID => {
            SelectedSignatureAudienceMigration::Replay040
        }
        JOBS_PROVISIONED_RUNTIME_STARTUP_MIGRATION_ID => {
            SelectedSignatureAudienceMigration::Replay045
        }
        _ => anyhow::bail!("unsupported PostgreSQL audience replay migration"),
    };
    let canonical = canonical_migration_ledger();
    execute_selected_signature_audience_migration(conn, command, &canonical)
}

// The replay setup and validation helpers above are intentionally retained for
// the shared primitive. Canonical replay selects only the closed 036/040/045
// command mapping and canonical ledger; it has no separate transaction or SQL
// engine. A returned error requires the checkout owner to discard the client.

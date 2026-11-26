// UNREGISTERED, UNRUN operator/server shared draft for a selected migration
// ledger. It depends on the separately reviewed parser V2, catalog V2,
// legacy replay guard/adapter, and 049 forward SQL being staged privately in
// `db/mod.rs`; it is not an operator entrypoint or production authority.
//
// A caller supplies only a parsed `MigrationLedgerName` and a closed command.
// The ledger may be the canonical startup ledger or an operator-selected
// existing ledger; it cannot select a schema, table SQL, migration body, or a
// fault mode.

const PHASE657_SELECTED_LEDGER_FORWARD_049: &str =
    include_str!("postgres-audience-shared-forward.sql");
const PHASE657_CANONICAL_LEDGER: &str = "bluey_schema_migrations";
const PHASE657_OLD_ATS_LINEAGE: &str = "045_jobs_ats_storage_fences.sql";
const PHASE657_049: &str = "049_jobs_process_attachment_signature_audience.sql";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SelectedSignatureAudienceMigration {
    Replay036,
    Replay040,
    Replay045,
    Forward049,
    Forward052,
}

impl SelectedSignatureAudienceMigration {
    fn version(self) -> &'static str {
        match self {
            Self::Replay036 => JOBS_SIGNED_JOB_INTEGRITY_AUTHORITY_MIGRATION_ID,
            Self::Replay040 => JOBS_ORIGINAL_SOURCE_VERIFIER_STARTUP_AUTHORITY_MIGRATION_ID,
            Self::Replay045 => JOBS_PROVISIONED_RUNTIME_STARTUP_MIGRATION_ID,
            Self::Forward049 => PHASE657_049,
            Self::Forward052 => JOBS_MANAGED_CLOUD_RELEASE_V4_AUDIENCES_MIGRATION_ID,
        }
    }
}

fn canonical_migration_ledger() -> MigrationLedgerName {
    MigrationLedgerName::parse(PHASE657_CANONICAL_LEDGER)
        .expect("static canonical migration ledger is syntactically valid")
}

fn public_migration_ledger_exists(
    tx: &mut postgres::Transaction<'_>,
    ledger: &MigrationLedgerName,
) -> Result<bool> {
    tx.query_one(
        "SELECT to_regclass($1) IS NOT NULL",
        &[&ledger.qualified_public_identifier()],
    )
    .map_err(|_| anyhow::anyhow!("migration ledger inspection unavailable"))?
    .try_get(0)
    .map_err(|_| anyhow::anyhow!("migration ledger inspection unavailable"))
}

fn deterministic_existing_ledgers(
    tx: &mut postgres::Transaction<'_>,
    selected: &MigrationLedgerName,
) -> Result<Vec<MigrationLedgerName>> {
    // The selected ledger is mandatory. The canonical ledger is independently
    // inspected in this transaction and added only if it already exists.
    let canonical = canonical_migration_ledger();
    let mut ledgers = vec![selected.clone()];
    if canonical.as_str() != selected.as_str() && public_migration_ledger_exists(tx, &canonical)? {
        ledgers.push(canonical);
    }
    ledgers.sort_by(|left, right| left.as_str().cmp(right.as_str()));
    ledgers.dedup_by(|left, right| left.as_str() == right.as_str());
    Ok(ledgers)
}

fn require_initialized_signature_schema(tx: &mut postgres::Transaction<'_>) -> Result<()> {
    let initialized: bool = tx
        .query_one(
            "SELECT to_regclass('public.jobs_managed_cloud_signature_sets') IS NOT NULL",
            &[],
        )
        .map_err(|_| anyhow::anyhow!("signature audience schema inspection unavailable"))?
        .try_get(0)
        .map_err(|_| anyhow::anyhow!("signature audience schema inspection unavailable"))?;
    anyhow::ensure!(
        initialized,
        "signature audience migration requires initialized schema"
    );
    Ok(())
}

fn require_existing_migration_ledger(
    tx: &mut postgres::Transaction<'_>,
    ledger: &MigrationLedgerName,
) -> Result<()> {
    let exists = public_migration_ledger_exists(tx, ledger)?;
    anyhow::ensure!(exists, "migration ledger bootstrap is required");
    Ok(())
}

fn ledger_has_version(
    tx: &mut postgres::Transaction<'_>,
    ledger: &MigrationLedgerName,
    version: &str,
) -> Result<bool> {
    tx.query_one(
        &format!(
            "SELECT EXISTS(SELECT 1 FROM {} WHERE version=$1)",
            ledger.qualified_public_identifier()
        ),
        &[&version],
    )
    .map_err(|_| anyhow::anyhow!("migration ledger inspection unavailable"))?
    .try_get(0)
    .map_err(|_| anyhow::anyhow!("migration ledger inspection unavailable"))
}

fn refuse_old_ats_lineage(
    tx: &mut postgres::Transaction<'_>,
    ledgers: &[MigrationLedgerName],
) -> Result<()> {
    for ledger in ledgers {
        anyhow::ensure!(
            !ledger_has_version(tx, ledger, PHASE657_OLD_ATS_LINEAGE)?,
            "unreleased ATS migration lineage requires explicit upgrade review"
        );
    }
    Ok(())
}

fn history_shape_allowed(version: &str, shape: SignatureAudienceShape) -> bool {
    use SignatureAudienceShape::{Eight, Fourteen, Sixteen, Ten, Thirteen};
    match version {
        JOBS_SIGNED_JOB_INTEGRITY_AUTHORITY_MIGRATION_ID => {
            matches!(shape, Eight | Ten | Thirteen | Fourteen | Sixteen)
        }
        JOBS_ORIGINAL_SOURCE_VERIFIER_STARTUP_AUTHORITY_MIGRATION_ID => {
            matches!(shape, Ten | Thirteen | Fourteen | Sixteen)
        }
        JOBS_PROVISIONED_RUNTIME_STARTUP_MIGRATION_ID => {
            matches!(shape, Thirteen | Fourteen | Sixteen)
        }
        PHASE657_049 => matches!(shape, Fourteen | Sixteen),
        _ => false,
    }
}

fn validate_ledger_history_shape(
    tx: &mut postgres::Transaction<'_>,
    ledgers: &[MigrationLedgerName],
    shape: SignatureAudienceShape,
) -> Result<()> {
    let canonical = canonical_migration_ledger();
    let has_v4 = ledgers.contains(&canonical)
        && ledger_has_version(
            tx,
            &canonical,
            JOBS_MANAGED_CLOUD_RELEASE_V4_AUDIENCES_MIGRATION_ID,
        )?;
    anyhow::ensure!(
        has_v4 == (shape == SignatureAudienceShape::Sixteen),
        "PostgreSQL audience 052 canonical history/state mismatch"
    );
    for ledger in ledgers {
        anyhow::ensure!(
            ledger == &canonical
                || !ledger_has_version(
                    tx,
                    ledger,
                    JOBS_MANAGED_CLOUD_RELEASE_V4_AUDIENCES_MIGRATION_ID
                )?,
            "PostgreSQL audience 052 requires the canonical ledger"
        );
        // Preserve the existing 049 error for callers and diagnostics.
        anyhow::ensure!(
            !ledger_has_version(tx, ledger, PHASE657_049)?
                || history_shape_allowed(PHASE657_049, shape),
            "PostgreSQL audience 049 history/state mismatch"
        );
    }
    for ledger in ledgers {
        for version in [
            JOBS_SIGNED_JOB_INTEGRITY_AUTHORITY_MIGRATION_ID,
            JOBS_ORIGINAL_SOURCE_VERIFIER_STARTUP_AUTHORITY_MIGRATION_ID,
            JOBS_PROVISIONED_RUNTIME_STARTUP_MIGRATION_ID,
        ] {
            anyhow::ensure!(
                !ledger_has_version(tx, ledger, version)? || history_shape_allowed(version, shape),
                "PostgreSQL audience legacy history/state mismatch"
            );
        }
    }
    Ok(())
}

fn replay_shape_expected(
    command: SelectedSignatureAudienceMigration,
    before: SignatureAudienceShape,
) -> Result<SignatureAudienceShape> {
    use SelectedSignatureAudienceMigration::{
        Forward049, Forward052, Replay036, Replay040, Replay045,
    };
    use SignatureAudienceShape::{Eight, Fourteen, Six, Sixteen, Ten, Thirteen};

    match (command, before) {
        (Replay036, Six) => Ok(Eight),
        (Replay036, shape) => Ok(shape),
        (Replay040, Eight | Ten) => Ok(Ten),
        (Replay040, Thirteen) => Ok(Thirteen),
        (Replay040, Fourteen | Sixteen) => Ok(before),
        (Replay045, Ten | Thirteen) => Ok(Thirteen),
        (Replay045, Fourteen | Sixteen) => Ok(before),
        (Forward049, Thirteen | Fourteen) => Ok(Fourteen),
        (Forward049, Sixteen) => Ok(Sixteen),
        (Forward052, Fourteen | Sixteen) => Ok(Sixteen),
        _ => anyhow::bail!("unsupported PostgreSQL audience replay transition"),
    }
}

fn compiled_body(
    tx: &mut postgres::Transaction<'_>,
    command: SelectedSignatureAudienceMigration,
    before: SignatureAudienceShape,
    selected: &MigrationLedgerName,
) -> Result<()> {
    let version = command.version();
    let sql = match command {
        SelectedSignatureAudienceMigration::Replay036 => {
            POSTGRES_JOBS_SIGNED_JOB_INTEGRITY_AUTHORITY
        }
        SelectedSignatureAudienceMigration::Replay040 => {
            POSTGRES_JOBS_ORIGINAL_SOURCE_VERIFIER_STARTUP_AUTHORITY
        }
        SelectedSignatureAudienceMigration::Replay045 => POSTGRES_JOBS_PROVISIONED_RUNTIME_STARTUP,
        SelectedSignatureAudienceMigration::Forward049 => PHASE657_SELECTED_LEDGER_FORWARD_049,
        SelectedSignatureAudienceMigration::Forward052 => {
            guarded_release_v4_audience_body(POSTGRES_JOBS_MANAGED_CLOUD_RELEASE_V4_AUDIENCES)?
        }
    };

    tx.batch_execute("SET LOCAL search_path = public, pg_temp;")
        .context("set controlled PostgreSQL audience replay search path")?;
    match command {
        SelectedSignatureAudienceMigration::Replay036 => tx
            .batch_execute(sql)
            .context("apply compiled PostgreSQL audience 036")?,
        SelectedSignatureAudienceMigration::Replay040
        | SelectedSignatureAudienceMigration::Replay045 => {
            let adapted = adapt_postgres_audience_replay(version, sql, before)?;
            if command == SelectedSignatureAudienceMigration::Replay045 {
                tx.query_one(
                    "SELECT pg_catalog.set_config('bluey.migration_table', $1, true)",
                    &[&selected.as_str()],
                )
                .context("set validated selected migration ledger GUC")?;
                tx.batch_execute(postgres_provisioned_runtime_body(&adapted)?)
                    .context("apply compiled PostgreSQL audience 045")?;
            } else {
                tx.batch_execute(&adapted)
                    .context("apply compiled PostgreSQL audience 040")?;
            }
        }
        SelectedSignatureAudienceMigration::Forward049 => {
            if before == SignatureAudienceShape::Thirteen {
                tx.batch_execute(sql)
                    .context("apply compiled PostgreSQL audience 049")?;
            }
        }
        SelectedSignatureAudienceMigration::Forward052 => {
            if before == SignatureAudienceShape::Fourteen {
                tx.batch_execute(sql)
                    .context("apply compiled PostgreSQL audience 052")?;
            }
        }
    }
    Ok(())
}

fn write_selected_ledger(
    tx: &mut postgres::Transaction<'_>,
    selected: &MigrationLedgerName,
    version: &str,
) -> Result<()> {
    tx.execute(
        &format!(
            "INSERT INTO {}(version) VALUES ($1) ON CONFLICT (version) DO NOTHING",
            selected.qualified_public_identifier()
        ),
        &[&version],
    )
    .map_err(|_| anyhow::anyhow!("selected migration ledger write unavailable"))?;
    Ok(())
}

/// Applies one finite signature-audience migration body and records it only in
/// the validated selected public ledger. On any returned error, the checkout
/// owner must discard the connection; a one-shot CLI closes it. In particular,
/// a commit error has unknown outcome and is neither success nor rollback.
fn execute_selected_signature_audience_migration(
    conn: &mut postgres::Client,
    command: SelectedSignatureAudienceMigration,
    selected: &MigrationLedgerName,
) -> Result<()> {
    anyhow::ensure!(
        command != SelectedSignatureAudienceMigration::Forward052
            || selected == &canonical_migration_ledger(),
        "PostgreSQL audience 052 requires the canonical ledger"
    );
    let mut tx = conn
        .transaction()
        .context("begin selected PostgreSQL audience migration")?;
    let applied = (|| -> Result<()> {
        require_initialized_signature_schema(&mut tx)?;
        // This guard takes the signature-table ACCESS EXCLUSIVE lock. Its temp
        // references are established once; catalog ledger references follow in
        // the same still-locked transaction.
        tx.batch_execute(PHASE657_REPLAY_SETUP)
            .context("set up locked PostgreSQL audience replay guard")?;
        setup_migration_ledger_reference(&mut tx)?;

        let ledgers = deterministic_existing_ledgers(&mut tx, selected)?;
        for ledger in &ledgers {
            require_existing_migration_ledger(&mut tx, ledger)?;
            validate_existing_public_migration_ledger(&mut tx, ledger)?;
        }
        refuse_old_ats_lineage(&mut tx, &ledgers)?;

        let before = phase657_replay_catalog_shape(&mut tx)?;
        validate_ledger_history_shape(&mut tx, &ledgers, before)?;

        let expected = replay_shape_expected(command, before)?;
        compiled_body(&mut tx, command, before, selected)?;
        anyhow::ensure!(
            phase657_replay_catalog_shape(&mut tx)? == expected,
            "PostgreSQL audience replay postcondition mismatch"
        );
        write_selected_ledger(&mut tx, selected, command.version())?;
        if command == SelectedSignatureAudienceMigration::Forward052 {
            validate_ledger_history_shape(&mut tx, &ledgers, expected)?;
        }
        Ok(())
    })();
    if let Err(error) = applied {
        return match tx.rollback() {
            Ok(()) => Err(error.context("selected PostgreSQL audience migration failed")),
            Err(rollback) => Err(error.context(format!(
                "selected PostgreSQL audience migration failed; rollback error={rollback}"
            ))),
        };
    }
    tx.commit().context(
        "selected PostgreSQL audience migration commit outcome is unknown on error; discard connection",
    )
}

// Scope limits: the caller's initialized-schema/bootstrap contract, trusted
// public-schema ownership/grants, connection-discard plumbing, CLI handoff,
// all-skipped preflight, and physical PostgreSQL tests remain separate. This
// draft does not create a ledger, accept arbitrary SQL, or authorize serving.

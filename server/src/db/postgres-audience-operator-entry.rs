// UNREGISTERED, UNRUN initialized-schema entry check. Stage privately with
// selected-ledger V2, legacy replay guard, and catalog/name V2 helpers. This
// checks storage consistency only; it is not a transferable authority token.
// Fresh/missing schema and missing selected ledger explicitly refuse. A future
// caller must separately define fresh bootstrap and trusted public privileges.
fn verify_initialized_signature_audience_operator_entry(
    conn: &mut postgres::Client,
    selected: &MigrationLedgerName,
) -> Result<()> {
    let mut tx = conn
        .transaction()
        .context("begin PostgreSQL audience operator entry check")?;
    let checked = (|| -> Result<()> {
        require_initialized_signature_schema(&mut tx)?;
        // Signature lock always precedes sorted ledger locks. Setup creates
        // only transaction-local references; no durable schema is changed.
        tx.batch_execute(PHASE657_REPLAY_SETUP)
            .context("set up locked PostgreSQL audience entry guard")?;
        setup_migration_ledger_reference(&mut tx)?;
        let ledgers = deterministic_existing_ledgers(&mut tx, selected)?;
        for ledger in &ledgers {
            require_existing_migration_ledger(&mut tx, ledger)?;
            validate_existing_public_migration_ledger(&mut tx, ledger)?;
        }
        refuse_old_ats_lineage(&mut tx, &ledgers)?;
        let shape = phase657_replay_catalog_shape(&mut tx)?;
        validate_ledger_history_shape(&mut tx, &ledgers, shape)?;
        Ok(())
    })();
    if let Err(error) = checked {
        return match tx.rollback() {
            Ok(()) => Err(error.context("PostgreSQL audience entry check failed")),
            Err(rollback) => Err(error.context(format!(
                "PostgreSQL audience entry check failed; rollback error={rollback}"
            ))),
        };
    }
    tx.commit()
        .context("PostgreSQL audience entry check commit outcome unknown on error")
}

// Entry-check success does not authorize a later mutation: every per-version
// transaction reacquires locks and repeats checks. On every returned error the
// checkout owner must mark/discard the pooled connection; a one-shot operator
// closes its connection. This helper taking &mut Client cannot enforce that.
// No all-skipped CLI call site, owner/grants validation, fresh-create protocol,
// DB tests or production/release proof is supplied.

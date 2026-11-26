// Canonical startup uses the same closed transaction primitive as the
// operator-selected path.  The only startup-specific choice is the canonical
// ledger name; there is no second 049 transaction implementation here.
fn execute_postgres_signature_audience_049(conn: &mut postgres::Client) -> Result<()> {
    let canonical = canonical_migration_ledger();
    execute_selected_signature_audience_migration(
        conn,
        SelectedSignatureAudienceMigration::Forward049,
        &canonical,
    )
}

// Scope limits remain those of the shared selected-ledger driver: initialized
// schema and existing-ledger only, no bootstrap, arbitrary SQL, or new runtime
// fault mode. A commit error has unknown outcome and the checkout must be
// discarded by its owner.

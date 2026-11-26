// Phase657 integrated-operator candidate; uncompiled, unrun, no release authority.
include!("operator-source-identity.rs");

use super::{
    execute_selected_signature_audience_migration, open_one_shot_postgres_client,
    verify_initialized_signature_audience_operator_entry, MigrationLedgerName, SafePostgresClient,
    SelectedSignatureAudienceMigration,
};
use anyhow::{Context, Result};

fn command(value: &str) -> Result<SelectedSignatureAudienceMigration> {
    match value {
        "036" => Ok(SelectedSignatureAudienceMigration::Replay036),
        "040" => Ok(SelectedSignatureAudienceMigration::Replay040),
        "045" => Ok(SelectedSignatureAudienceMigration::Replay045),
        "049" => Ok(SelectedSignatureAudienceMigration::Forward049),
        _ => anyhow::bail!("unsupported selected audience migration command"),
    }
}

fn abandon<T>(conn: &mut SafePostgresClient, outcome: Result<T>) -> Result<T> {
    if outcome.is_err() {
        conn.mark_broken();
    }
    outcome
}

pub fn run_selected_audience_command(
    database_url: &str,
    ledger: &str,
    token: Option<&str>,
) -> Result<()> {
    let selected = MigrationLedgerName::parse(ledger)
        .map_err(|_| anyhow::anyhow!("invalid selected migration ledger"))?;
    let command = token.map(command).transpose()?;
    // No connection/pool exists before both closed syntax checks succeed.
    let mut conn = open_one_shot_postgres_client(database_url)
        .context("operator database initialization failed")?;
    let entry = verify_initialized_signature_audience_operator_entry(&mut conn, &selected);
    abandon(&mut conn, entry)?;
    if let Some(command) = command {
        let apply = execute_selected_signature_audience_migration(&mut conn, command, &selected);
        abandon(&mut conn, apply)?;
    }
    Ok(())
}

#[cfg(test)]
mod dispatcher_preconnect_tests {
    use super::run_selected_audience_command;

    #[test]
    fn invalid_ledger_refuses_before_database_url_handling() {
        let error = run_selected_audience_command("not a postgres URL", "bad-name", None)
            .expect_err("invalid ledger must refuse");
        assert_eq!(error.to_string(), "invalid selected migration ledger");
    }

    #[test]
    fn invalid_command_refuses_before_database_url_handling() {
        let error = run_selected_audience_command("not a postgres URL", "select", Some("999"))
            .expect_err("invalid command must refuse");
        assert_eq!(
            error.to_string(),
            "unsupported selected audience migration command"
        );
    }
}

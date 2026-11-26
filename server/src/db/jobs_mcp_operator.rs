//! Separate MCP operator admission. This build exposes only a read-only check.

use std::time::Duration;

use anyhow::{ensure, Result};
use native_tls::TlsConnector;
use postgres::{Client, IsolationLevel, Transaction};
use postgres_native_tls::MakeTlsConnector;
use reqwest::Url;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::{
    POSTGRES_CONTEXT_ARTIFACT_REVISIONS, POSTGRES_JOBS_SCHEMA, POSTGRES_MIGRATIONS,
    POSTGRES_POST_JOBS_MIGRATIONS,
};

const INSTALLER: &str = "bluey_jobs_mcp_installer";
const OWNER: &str = "bluey_jobs_mcp_owner";
const READER: &str = "bluey_jobs_mcp_reader";
const INSTALL_SQL: &[u8] =
    include_bytes!("../../../infra/postgres/mcp-operator/read-shortlist-v1.sql");
const INSTALL_SHA256: &str = "88c0885e3403ea84d7fe1f200330c02b1589856819b2f1cb66c5744a50c4901e";
const ROLE_BASELINE: &str = include_str!("jobs-mcp-operator-role-baseline.sql");
const LEDGER_SHAPE: &str = include_str!("jobs-mcp-operator-ledger-v1.sql");
const INSTALLED_SHAPE: &str = include_str!("jobs-mcp-operator-installed-v1.sql");
const PG17_10: i32 = 170_010;
const PG17_10_DEFINITION_SHA256: &str =
    "64e1fe0e79dd945de8e30991821604f3f4b29b4c10dba46512dd3ceeb1aac5a2";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CheckedState {
    Absent,
    ExactInstalled,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Target {
    version: u8,
    host: String,
    port: u16,
    database: String,
    database_oid: u32,
    system_identifier: String,
}

fn closed_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 63
        && name.as_bytes()[0].is_ascii_lowercase()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn closed_host(host: &str) -> bool {
    host.len() <= 253
        && host.contains('.')
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
        && host.bytes().any(|byte| byte.is_ascii_lowercase())
}

fn target_and_config(raw: &str, url: &str) -> Result<(Target, postgres::Config)> {
    ensure!(
        raw.len() <= 1024 && url.len() <= 4096,
        "MCP operator input too large"
    );
    let target: Target =
        serde_json::from_str(raw).map_err(|_| anyhow::anyhow!("MCP operator target refused"))?;
    ensure!(
        target.version == 1
            && closed_host(&target.host)
            && closed_name(&target.database)
            && target.port != 0
            && target.database_oid != 0
            && !target.system_identifier.starts_with('0')
            && target.system_identifier.len() <= 20
            && target.system_identifier.bytes().all(|b| b.is_ascii_digit())
            && target.system_identifier.parse::<u64>().is_ok(),
        "MCP operator target refused"
    );
    let uri = Url::parse(url).map_err(|_| anyhow::anyhow!("MCP operator connection refused"))?;
    ensure!(
        uri.scheme() == "postgresql"
            && uri.host_str() == Some(target.host.as_str())
            && uri.port() == Some(target.port)
            && uri.username() == INSTALLER
            && uri.password().is_some_and(|password| !password.is_empty())
            && uri.path() == format!("/{}", target.database)
            && uri.query() == Some("sslmode=require")
            && uri.fragment().is_none(),
        "MCP operator connection refused"
    );
    // Parse the normalized URL, not a second interpretation of the original
    // input. Do not inherit the application's insecure-TLS escape hatches.
    let mut config = uri
        .as_str()
        .parse::<postgres::Config>()
        .map_err(|_| anyhow::anyhow!("MCP operator connection refused"))?;
    config.connect_timeout(Duration::from_secs(5));
    Ok((target, config))
}

pub fn source_id() -> Result<String> {
    ensure!(
        hex::encode(Sha256::digest(INSTALL_SQL)) == INSTALL_SHA256,
        "MCP operator artifact refused"
    );
    let mut hash = Sha256::new();
    hash.update(b"bluey.jobs.mcp.postgres.operator.v1\0");
    for raw in [
        include_bytes!("../bin/bluey-postgres-mcp-read-operator.rs").as_slice(),
        include_bytes!("jobs_mcp_operator.rs").as_slice(),
        ROLE_BASELINE.as_bytes(),
        LEDGER_SHAPE.as_bytes(),
        INSTALLED_SHAPE.as_bytes(),
        INSTALL_SQL,
    ] {
        hash.update((raw.len() as u64).to_be_bytes());
        hash.update(raw);
    }
    for (version, sql) in registered_migrations() {
        hash.update((version.len() as u64).to_be_bytes());
        hash.update(version.as_bytes());
        hash.update(Sha256::digest(sql.as_bytes()));
    }
    Ok(hex::encode(hash.finalize()))
}

fn registered_migrations() -> impl Iterator<Item = (&'static str, &'static str)> {
    POSTGRES_MIGRATIONS
        .iter()
        .copied()
        .chain(std::iter::once(("002_jobs.sql", POSTGRES_JOBS_SCHEMA)))
        .chain(POSTGRES_POST_JOBS_MIGRATIONS.iter().copied())
        .chain(std::iter::once((
            "006_context_artifact_revisions.sql",
            POSTGRES_CONTEXT_ARTIFACT_REVISIONS,
        )))
}

fn exact_migration_versions() -> Vec<String> {
    let mut versions = registered_migrations()
        .map(|(version, _)| version.to_owned())
        .collect::<Vec<_>>();
    versions.sort_unstable();
    versions
}

fn verify_catalog(tx: &mut Transaction<'_>, target: &Target) -> Result<CheckedState> {
    tx.batch_execute(
        "SET LOCAL search_path=pg_catalog,pg_temp;
         SET LOCAL statement_timeout='5s'; SET LOCAL lock_timeout='2s';",
    )?;
    let identity = tx.query_one(
        "SELECT current_database()::text, current_user::text, session_user::text,
                d.oid, s.system_identifier::text, current_setting('server_version_num')::int,
                inet_server_port(), current_setting('transaction_read_only'),
                EXISTS (SELECT 1 FROM pg_catalog.pg_stat_ssl
                         WHERE pid=pg_catalog.pg_backend_pid() AND ssl),
                pg_catalog.to_regnamespace('jobs_mcp') IS NOT NULL
           FROM pg_catalog.pg_database d CROSS JOIN pg_catalog.pg_control_system() s
          WHERE d.datname=current_database()",
        &[],
    )?;
    let server_version = identity.try_get::<_, i32>(5)?;
    let installed = identity.try_get::<_, bool>(9)?;
    ensure!(
        identity.try_get::<_, &str>(0)? == target.database
            && identity.try_get::<_, &str>(1)? == INSTALLER
            && identity.try_get::<_, &str>(2)? == INSTALLER
            && identity.try_get::<_, u32>(3)? == target.database_oid
            && identity.try_get::<_, &str>(4)? == target.system_identifier
            && (170_000..180_000).contains(&server_version)
            && identity.try_get::<_, Option<i32>>(6)? == Some(i32::from(target.port))
            && identity.try_get::<_, &str>(7)? == "on"
            && identity.try_get::<_, bool>(8)?,
        "MCP operator target identity disagrees"
    );
    let ledger_ok: bool = tx.query_one(LEDGER_SHAPE, &[])?.try_get(0)?;
    ensure!(ledger_ok, "MCP operator ledger shape refused");
    let mut versions = tx
        .query(
            "SELECT version FROM public.bluey_schema_migrations LIMIT 257",
            &[],
        )?
        .iter()
        .map(|row| row.try_get::<_, String>(0))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    versions.sort_unstable();
    ensure!(
        versions == exact_migration_versions(),
        "MCP operator ledger disagrees"
    );
    let accepted = tx
        .query_opt(ROLE_BASELINE, &[&INSTALLER, &OWNER, &READER, &installed])?
        .map(|row| row.try_get::<_, bool>(0))
        .transpose()?
        .unwrap_or(false);
    ensure!(accepted, "MCP operator role baseline refused");
    if !installed {
        return Ok(CheckedState::Absent);
    }
    ensure!(
        server_version == PG17_10,
        "MCP installed engine not qualified"
    );
    let row = tx
        .query_opt(INSTALLED_SHAPE, &[&INSTALLER, &OWNER, &READER])?
        .ok_or_else(|| anyhow::anyhow!("MCP installed shape disagrees"))?;
    ensure!(row.try_get::<_, bool>(0)?, "MCP installed shape disagrees");
    let definition: Option<&str> = row.try_get(1)?;
    ensure!(
        definition.is_some_and(|raw| {
            !raw.is_empty()
                && raw.len() <= 16 << 10
                && hex::encode(Sha256::digest(raw.as_bytes())) == PG17_10_DEFINITION_SHA256
        }),
        "MCP installed definition disagrees"
    );
    Ok(CheckedState::ExactInstalled)
}

pub fn check_configured_target(raw_target: &str, url: &str) -> Result<CheckedState> {
    source_id()?;
    let (target, config) = target_and_config(raw_target, url)?;
    let tls = TlsConnector::new().map_err(|_| anyhow::anyhow!("MCP operator TLS unavailable"))?;
    check_over_tls(&target, config, tls)
}

fn check_over_tls(
    target: &Target,
    config: postgres::Config,
    tls: TlsConnector,
) -> Result<CheckedState> {
    let mut client: Client = config
        .connect(MakeTlsConnector::new(tls))
        .map_err(|_| anyhow::anyhow!("MCP operator connection refused"))?;
    let mut tx = client
        .build_transaction()
        .isolation_level(IsolationLevel::RepeatableRead)
        .read_only(true)
        .start()
        .map_err(|_| anyhow::anyhow!("MCP operator catalog check unavailable"))?;
    let checked = verify_catalog(&mut tx, target);
    // A check never commits. Close this one-shot connection on any outcome.
    let closed = tx.rollback();
    #[cfg(test)]
    if let Err(error) = &checked {
        let reason = match error.to_string().as_str() {
            "MCP operator target identity disagrees" => "target_identity",
            "MCP operator ledger shape refused" => "ledger_shape",
            "MCP operator ledger disagrees" => "ledger_versions",
            "MCP operator role baseline refused" => "role_baseline",
            "MCP installed engine not qualified" => "installed_engine",
            "MCP installed shape disagrees" => "installed_shape",
            "MCP installed definition disagrees" => "installed_definition",
            _ => "query_or_decode",
        };
        let sqlstate = error
            .downcast_ref::<postgres::Error>()
            .and_then(postgres::Error::code)
            .map_or("none", postgres::error::SqlState::code);
        eprintln!("owned MCP catalog control: {reason}; sqlstate={sqlstate}");
    }
    checked
        .ok()
        .filter(|_| closed.is_ok())
        .ok_or_else(|| anyhow::anyhow!("MCP operator catalog check refused"))
}

#[cfg(test)]
#[path = "jobs_mcp_operator_tls_tests.rs"]
mod tls_tests;

#[cfg(test)]
mod tests {
    use super::*;

    const TARGET: &str = r#"{"version":1,"host":"db.example.test","port":5432,"database":"bluey",
                         "database_oid":42,"system_identifier":"7711652515072609986"}"#;
    const URL: &str = "postgresql://bluey_jobs_mcp_installer:synthetic@db.example.test:5432/bluey?sslmode=require";

    #[test]
    fn source_identity_pins_exact_install_bytes_and_cli() {
        assert_eq!(source_id().unwrap().len(), 64);
        assert_eq!(source_id().unwrap(), source_id().unwrap());
        assert_ne!(source_id().unwrap(), INSTALL_SHA256);
        assert!(exact_migration_versions().contains(&"051_jobs_mcp_read_grants.sql".to_owned()));
    }

    #[test]
    fn strict_tls_config_refuses_drift_before_opening_a_connection() {
        assert!(target_and_config(TARGET, URL).is_ok());
        for changed in [
            URL.replace("sslmode=require", "sslmode=prefer"),
            format!("{URL}&options=role%3Dpostgres"),
            URL.replace("db.example.test", "other.example.test"),
            URL.replace(":5432", ":6432"),
            URL.replace("/bluey?", "/other?"),
            URL.replace(INSTALLER, READER),
            URL.replace(":synthetic", ""),
            format!("{URL}#unexpected"),
        ] {
            let error = target_and_config(TARGET, &changed).err().unwrap();
            assert_eq!(error.to_string(), "MCP operator connection refused");
        }
    }

    #[test]
    fn exact_target_document_rejects_unknown_duplicate_and_invalid_fields() {
        for changed in [
            TARGET.replace("\"version\":1", "\"version\":1,\"version\":1"),
            TARGET.replace("\"version\":1", "\"version\":1,\"ready\":true"),
            TARGET.replace("\"version\":1", "\"version\":2"),
            TARGET.replace("db.example.test", "127.0.0.1"),
            TARGET.replace("\"bluey\"", "\"bluey.jobs\""),
            TARGET.replace("7711652515072609986", "077"),
            TARGET.replace("\"database_oid\":42", "\"database_oid\":0"),
        ] {
            assert_eq!(
                target_and_config(&changed, URL).err().unwrap().to_string(),
                "MCP operator target refused"
            );
        }
    }
}

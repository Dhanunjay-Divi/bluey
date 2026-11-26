//! Owned loopback TLS qualification. The address and CA overrides are test-only.

use super::*;
use crate::db::jobs_mcp_read::MatchReader;
use native_tls::Certificate;
use postgres::NoTls;
use serde_json::{json, Value};
use std::io::Cursor;
use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;

const CASE: &str =
    "db::jobs_mcp_operator::tls_tests::postgres_owned_mcp_operator_verified_tls_and_full_schema";
const INSTALL_CASE: &str =
    "db::jobs_mcp_operator::tls_tests::postgres_owned_mcp_fragment_on_registered_schema";
const HOST: &str = "mcp-fixture.bluey.test";

fn provision_disposable_roles(client: &mut Client, db: &str, password: &str, reader: &str) {
    const ROLES: &str = include_str!("../../tests/fixtures/jobs-mcp-postgres-role-boundary.sql");
    let (_, grants) = ROLES
        .split_once("GRANT SELECT (id, email_verified_at, is_temporary)")
        .expect("reviewed role columns");
    let (grants, _) = grants
        .split_once("-- Parsed at definition time")
        .expect("reviewed grant boundary");
    assert_eq!(grants.matches("TO jobs_mcp_fixture_owner;").count(), 17);
    let grants = grants.replace("jobs_mcp_fixture_owner", OWNER);
    assert!(closed_name(db));
    assert_ne!(password, reader);
    for value in [password, reader] {
        assert!(value.len() == 32 && value.bytes().all(|c| c.is_ascii_hexdigit()));
    }

    // The exact owned wrapper creates one case database in an isolated
    // cluster. These PUBLIC changes are fixture setup, not operator SQL.
    let mut databases = client
        .query(
            "SELECT datname::text FROM pg_database WHERE datallowconn",
            &[],
        )
        .unwrap()
        .iter()
        .map(|row| row.get::<_, String>(0))
        .collect::<Vec<_>>();
    databases.sort_unstable();
    assert_eq!(
        databases,
        [db, "bluey_jobs_nonprod", "postgres", "template1"]
    );
    let setup = format!(
        "BEGIN;
         REVOKE ALL ON DATABASE {db} FROM PUBLIC;
         REVOKE CONNECT ON DATABASE bluey_jobs_nonprod, postgres, template1 FROM PUBLIC;
         REVOKE ALL ON SCHEMA public FROM PUBLIC;
         CREATE ROLE {INSTALLER} LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE
           NOINHERIT NOREPLICATION NOBYPASSRLS PASSWORD '{password}';
         CREATE ROLE {OWNER} NOLOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE
           NOINHERIT NOREPLICATION NOBYPASSRLS;
         CREATE ROLE {READER} LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE
           NOINHERIT NOREPLICATION NOBYPASSRLS PASSWORD '{reader}';
         GRANT CONNECT, CREATE ON DATABASE {db} TO {INSTALLER};
         GRANT CONNECT ON DATABASE {db} TO {READER};
         GRANT {OWNER} TO {INSTALLER} WITH INHERIT FALSE;
         GRANT USAGE ON SCHEMA public TO {OWNER}, {INSTALLER};
         GRANT EXECUTE ON FUNCTION pg_catalog.pg_control_system() TO {INSTALLER};
         GRANT SELECT(version) ON public.bluey_schema_migrations TO {INSTALLER};
         GRANT SELECT (id, email_verified_at, is_temporary){grants}
         COMMIT;"
    );
    client
        .batch_execute(&setup)
        .unwrap_or_else(|_| panic!("isolated MCP role fixture setup refused"));
}

fn fixture_config(
    db: &str,
    password: &str,
    port: u16,
    oid: u32,
    system_id: &str,
    host: &str,
) -> (Target, postgres::Config) {
    let target = json!({
        "version": 1, "host": host, "port": port, "database": db,
        "database_oid": oid, "system_identifier": system_id,
    });
    let url = format!("postgresql://{INSTALLER}:{password}@{host}:{port}/{db}?sslmode=require");
    let (target, mut config) =
        target_and_config(&target.to_string(), &url).expect("exact fixture target");
    // Do not modify the system resolver. Keep the original hostname for
    // native TLS and supply the already-owned listener's address separately.
    config.hostaddr(IpAddr::V4(Ipv4Addr::LOCALHOST));
    (target, config)
}

struct Fixture {
    url: String,
    ca: Certificate,
    admin: Client,
    target: Target,
    installer: postgres::Config,
    reader: postgres::Config,
    installer_password: String,
}

fn trust_fixture(ca: &Certificate) -> TlsConnector {
    let mut builder = TlsConnector::builder();
    builder.add_root_certificate(ca.clone());
    builder.build().expect("test-only TLS root")
}

fn fixture(case: &str) -> Option<Fixture> {
    let url = super::super::owned_postgres_test::owned_url(case)?;
    let root = PathBuf::from(std::env::var_os("BLUEY_TEST_RUN_ROOT").expect("owned test root"));
    let ca_path = root.join("mcp-tls/ca.pem");
    assert_eq!(ca_path.canonicalize().expect("owned fixture CA"), ca_path);
    let ca = std::fs::read(ca_path).expect("read only the owned fixture CA");
    assert!(!ca.is_empty() && ca.len() <= 4096);
    let ca = Certificate::from_pem(&ca).expect("parse fixture CA");
    let pool = super::super::open_postgres_pool(&url).expect("owned complete schema pool");
    super::super::run_migrations(&pool).expect("apply full registered server startup schema");
    drop(pool);

    let mut admin = Client::connect(&url, NoTls).expect("owned setup role");
    let row = admin
        .query_one(
            "SELECT current_database()::text, d.oid, inet_server_port(), s.system_identifier::text
             FROM pg_catalog.pg_database d CROSS JOIN pg_catalog.pg_control_system() s
             WHERE d.datname=current_database()",
            &[],
        )
        .unwrap();
    let (db, oid, port, system_id): (&str, u32, i32, &str) =
        (row.get(0), row.get(1), row.get(2), row.get(3));
    let port = u16::try_from(port).expect("owned PostgreSQL port");
    let password = uuid::Uuid::new_v4().simple().to_string();
    let reader_password = uuid::Uuid::new_v4().simple().to_string();
    provision_disposable_roles(&mut admin, db, &password, &reader_password);
    let (target, config) = fixture_config(db, &password, port, oid, system_id, HOST);
    let mut reader = config.clone();
    reader.user(READER).password(reader_password);
    Some(Fixture {
        url,
        ca,
        admin,
        target,
        installer: config,
        reader,
        installer_password: password,
    })
}

#[test]
fn postgres_owned_mcp_operator_verified_tls_and_full_schema() {
    let Some(mut fixture) = fixture(CASE) else {
        return;
    };
    let config = &fixture.installer;
    let target = &fixture.target;
    let ca = &fixture.ca;
    let trusted = || trust_fixture(ca);
    let Target {
        database: db,
        port,
        database_oid: oid,
        system_identifier: system_id,
        ..
    } = target;
    let (port, oid) = (*port, *oid);
    let password = &fixture.installer_password;
    assert_eq!(
        check_over_tls(target, config.clone(), trusted()).expect("certificate and real catalog"),
        CheckedState::Absent
    );
    let refused = |result: Result<CheckedState>, expected| {
        assert_eq!(
            result
                .expect_err("negative control was accepted")
                .to_string(),
            expected
        );
    };
    refused(
        check_over_tls(target, config.clone(), TlsConnector::new().unwrap()),
        "MCP operator connection refused",
    );
    let (wrong_host, wrong_config) = fixture_config(
        db,
        password,
        port,
        oid,
        system_id,
        "wrong-fixture.bluey.test",
    );
    refused(
        check_over_tls(&wrong_host, wrong_config, trusted()),
        "MCP operator connection refused",
    );
    let (wrong_oid, wrong_oid_config) = fixture_config(
        db,
        password,
        port,
        oid.checked_add(1).unwrap(),
        system_id,
        HOST,
    );
    refused(
        check_over_tls(&wrong_oid, wrong_oid_config, trusted()),
        "MCP operator catalog check refused",
    );
    assert_eq!(
        fixture
            .admin
            .execute(
                "DELETE FROM public.bluey_schema_migrations WHERE version=$1",
                &[&"006_context_artifact_revisions.sql"],
            )
            .expect("remove the non-array startup migration from the disposable ledger"),
        1
    );
    refused(
        check_over_tls(target, config.clone(), trusted()),
        "MCP operator catalog check refused",
    );
    assert_eq!(
        fixture
            .admin
            .execute(
                "INSERT INTO public.bluey_schema_migrations(version) VALUES ($1)",
                &[&"006_context_artifact_revisions.sql"],
            )
            .expect("restore the disposable migration ledger"),
        1
    );
    check_over_tls(target, config.clone(), trusted()).expect("restored ledger accepted");
    fixture
        .admin
        .batch_execute(&format!(
            "GRANT SELECT(id) ON public.jobs_postings TO {INSTALLER};"
        ))
        .expect("change only the disposable installer ACL");
    refused(
        check_over_tls(target, config.clone(), trusted()),
        "MCP operator catalog check refused",
    );
    fixture
        .admin
        .batch_execute(&format!(
            "REVOKE SELECT(id) ON public.jobs_postings FROM {INSTALLER};"
        ))
        .expect("restore the disposable installer ACL");
    check_over_tls(target, config.clone(), trusted()).expect("restored read-only admission");
    assert!(fixture
        .admin
        .query_one("SELECT to_regnamespace('jobs_mcp') IS NULL", &[])
        .unwrap()
        .get::<_, bool>(0));
    println!("owned_postgres_mcp_operator_verified_tls_and_full_schema_passed");
}

fn installed_definition(client: &mut impl postgres::GenericClient) -> String {
    client
        .batch_execute("SET LOCAL search_path=pg_catalog,pg_temp;")
        .unwrap();
    let row = client.query_one(
        "SELECT pg_get_functiondef(p.oid),
                n.nspowner=(SELECT oid FROM pg_roles WHERE rolname::text=$1),
                p.proowner=(SELECT oid FROM pg_roles WHERE rolname::text=$2),
                p.prosecdef AND p.proisstrict AND p.provolatile='s' AND p.prosqlbody IS NOT NULL,
                has_schema_privilege($3::text,n.oid,'USAGE')
                   AND NOT has_schema_privilege($3::text,n.oid,'CREATE')
                   AND NOT has_schema_privilege($2::text,n.oid,'CREATE'),
                has_function_privilege($3::text,p.oid,'EXECUTE')
                   AND NOT has_function_privilege($3::text,p.oid,'EXECUTE WITH GRANT OPTION'),
                NOT EXISTS (SELECT 1 FROM aclexplode(COALESCE(p.proacl,acldefault('f',p.proowner))) a
                                WHERE a.grantee=0),
                (SELECT count(DISTINCT d.refobjid) FROM pg_depend d
                  JOIN pg_class c ON c.oid=d.refobjid
                  JOIN pg_namespace t ON t.oid=c.relnamespace
                 WHERE d.classid='pg_proc'::regclass AND d.objid=p.oid
                   AND d.refclassid='pg_class'::regclass AND t.nspname='public')
           FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace
          WHERE n.nspname='jobs_mcp' AND p.oid='jobs_mcp.read_shortlist(text)'::regprocedure",
        &[&INSTALLER, &OWNER, &READER],
    ).expect("read back registered-schema install projection");
    for col in 1..=6 {
        assert!(
            row.get::<_, bool>(col),
            "unexpected installed privilege or shape field {col}"
        );
    }
    assert_eq!(row.get::<_, i64>(7), 17);
    let definition: String = row.get(0);
    assert!(!definition.is_empty() && definition.len() <= 16 << 10);
    hex::encode(Sha256::digest(definition.as_bytes()))
}

fn fixture_sql<T>(value: std::result::Result<T, postgres::Error>, stage: &str) -> T {
    value.unwrap_or_else(|error| {
        let state = error.code().map_or("none", postgres::error::SqlState::code);
        panic!("owned registered MCP fragment {stage} refused; sqlstate={state}");
    })
}

fn fixture_token(marker: char) -> String {
    assert!(matches!(marker, 'a' | 'b' | 'c' | 'e'));
    format!("bjm1_{}", marker.to_string().repeat(64))
}

fn registered_reader_result(client: Client, marker: char) -> Value {
    // Route bytes through the real stdio service, not a shortcut around
    // persisted grant digest resolution and the result projection.
    const INPUT: &str = concat!(
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"owned-schema-fixture","version":"1"}}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"get_job_matches","arguments":{"limit":10}}}"#,
        "\n",
    );
    let mut reader = MatchReader::from_postgres(client, &fixture_token(marker))
        .expect("owned function-only MatchReader");
    let mut output = Vec::new();
    reader
        .serve(Cursor::new(INPUT.as_bytes()), &mut output)
        .expect("read through the registered-schema MCP stdio service");
    let frames = std::str::from_utf8(&output)
        .expect("owned reader UTF-8")
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("owned reader JSON"))
        .collect::<Vec<_>>();
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0]["id"], 1);
    assert_eq!(frames[1]["id"], 2);
    frames[1]["result"].clone()
}

fn require_match(result: &Value, track: &str, job: Option<&str>) {
    assert_eq!(result["isError"], false);
    let page = &result["structuredContent"];
    assert_eq!(page["trackId"], track);
    assert_eq!(page["projection"], "current_track_discovery_shortlist");
    assert_eq!(page["requiresOriginalVerification"], true);
    assert_eq!(page["shortlistTruncated"], false);
    assert_eq!(page["nextCursor"], Value::Null);
    let rows = page["matches"].as_array().expect("shortlist matches array");
    match job {
        Some(job) => {
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0]["jobId"], job);
            assert_eq!(rows[0].as_object().unwrap().len(), 5);
            for field in ["title", "company", "location"] {
                assert!(rows[0][field].is_string());
            }
            assert!(rows[0]["matchScore"].is_i64());
        }
        None => assert!(rows.is_empty()),
    }
    let rendered = result["content"][0]["text"]
        .as_str()
        .expect("text fallback");
    assert_eq!(serde_json::from_str::<Value>(rendered).unwrap(), *page);
}

fn unavailable(result: &Value) {
    assert_eq!(
        result,
        &json!({"isError": true,
                "content": [{"type": "text", "text": "Career Track unavailable"}]})
    );
}

fn require_drift(result: Result<CheckedState>) {
    assert_eq!(
        result
            .expect_err("installed drift was accepted")
            .to_string(),
        "MCP operator catalog check refused"
    );
}

#[test]
fn postgres_owned_mcp_fragment_on_registered_schema() {
    let Some(mut fixture) = fixture(INSTALL_CASE) else {
        return;
    };
    let artifact = std::str::from_utf8(INSTALL_SQL).expect("known fragment is UTF-8");
    source_id().expect("embedded fragment matches the reviewed hash");
    let connect = |config: &postgres::Config| {
        fixture_sql(
            config.connect(MakeTlsConnector::new(trust_fixture(&fixture.ca))),
            "connect",
        )
    };
    let check = || {
        check_over_tls(
            &fixture.target,
            fixture.installer.clone(),
            trust_fixture(&fixture.ca),
        )
    };
    let namespace_absent = |admin: &mut Client| {
        fixture_sql(
            admin.query_one("SELECT to_regnamespace('jobs_mcp') IS NULL", &[]),
            "absence",
        )
        .get::<_, bool>(0)
    };
    assert!(namespace_absent(&mut fixture.admin));
    assert_eq!(check().unwrap(), CheckedState::Absent);
    // Even an empty namespace must not fall back to prospective acceptance.
    fixture_sql(
        fixture.admin.batch_execute(&format!(
            "CREATE SCHEMA jobs_mcp AUTHORIZATION {INSTALLER};"
        )),
        "disposable empty namespace",
    );
    require_drift(check());
    fixture_sql(
        fixture.admin.batch_execute("DROP SCHEMA jobs_mcp;"),
        "restore absence",
    );
    assert_eq!(check().unwrap(), CheckedState::Absent);
    let mut installer = connect(&fixture.installer);
    let mut expected = None;
    for commit in [false, true] {
        // This is a fixture, not the protected writer. The operator's check
        // deliberately owns a separate read-only transaction.
        check_over_tls(
            &fixture.target,
            fixture.installer.clone(),
            trust_fixture(&fixture.ca),
        )
        .expect("disposable installer prospective baseline");
        let mut tx = fixture_sql(installer.transaction(), "begin");
        let identity = fixture_sql(tx.query_one(
            "SELECT current_user::text=$1 AND NOT rolsuper AND to_regnamespace('jobs_mcp') IS NULL
               FROM pg_catalog.pg_roles WHERE rolname=current_user", &[&INSTALLER]), "identity");
        assert!(identity.get::<_, bool>(0));
        fixture_sql(tx.batch_execute(artifact), "execute");
        let digest = installed_definition(&mut tx);
        if commit {
            assert_eq!(expected.as_deref(), Some(digest.as_str()));
            fixture_sql(tx.commit(), "commit once in the disposable database");
        } else {
            expected = Some(digest);
            fixture_sql(tx.rollback(), "transactional rollback");
            assert!(namespace_absent(&mut fixture.admin));
        }
    }
    drop(installer);

    assert_eq!(check().unwrap(), CheckedState::ExactInstalled);
    assert_eq!(check().unwrap(), CheckedState::ExactInstalled);
    for (drift, restore) in [
        (
            "CREATE DOMAIN jobs_mcp.fixture_extra AS text",
            "DROP DOMAIN jobs_mcp.fixture_extra",
        ),
        (
            "CREATE TABLE jobs_mcp.fixture_extra(id integer)",
            "DROP TABLE jobs_mcp.fixture_extra",
        ),
        (
            "CREATE FUNCTION jobs_mcp.read_shortlist(integer) RETURNS integer LANGUAGE sql AS 'SELECT 0'",
            "DROP FUNCTION jobs_mcp.read_shortlist(integer)",
        ),
        (
            "GRANT USAGE ON SCHEMA jobs_mcp TO PUBLIC",
            "REVOKE USAGE ON SCHEMA jobs_mcp FROM PUBLIC",
        ),
        (
            "GRANT EXECUTE ON FUNCTION jobs_mcp.read_shortlist(text) TO PUBLIC",
            "REVOKE EXECUTE ON FUNCTION jobs_mcp.read_shortlist(text) FROM PUBLIC",
        ),
        (
            "ALTER FUNCTION jobs_mcp.read_shortlist(text) VOLATILE",
            "ALTER FUNCTION jobs_mcp.read_shortlist(text) STABLE",
        ),
        (
            "ALTER FUNCTION jobs_mcp.read_shortlist(text) SET row_security TO off",
            "ALTER FUNCTION jobs_mcp.read_shortlist(text) SET row_security TO on",
        ),
        (
            "GRANT SELECT(id) ON public.jobs_postings TO bluey_jobs_mcp_installer",
            "REVOKE SELECT(id) ON public.jobs_postings FROM bluey_jobs_mcp_installer",
        ),
    ] {
        fixture_sql(
            fixture.admin.batch_execute(drift),
            "set only the disposable drift control",
        );
        require_drift(check());
        fixture_sql(
            fixture.admin.batch_execute(restore),
            "restore only the disposable drift control",
        );
        assert_eq!(check().unwrap(), CheckedState::ExactInstalled);
    }
    // Deparser output is a comparison source, not executable fixture input.
    // Reuse only the fixed CREATE statement from the reviewed artifact;
    // replacements preserve the existing function owner and ACL.
    let (_, body) = artifact
        .split_once("\nCREATE FUNCTION jobs_mcp.read_shortlist(text)")
        .expect("exact fragment declaration");
    let (body, _) = body
        .split_once("\n\nREVOKE ALL ON FUNCTION ")
        .expect("exact fragment ACL boundary");
    let definition = format!("CREATE OR REPLACE FUNCTION jobs_mcp.read_shortlist(text){body}");
    assert_eq!(definition.matches("LIMIT 241").count(), 1);
    let changed = definition.replacen("LIMIT 241", "LIMIT 240", 1);
    fixture_sql(
        fixture.admin.batch_execute(&changed),
        "disposable function-body drift",
    );
    require_drift(check());
    fixture_sql(
        fixture.admin.batch_execute(&definition),
        "restore exact disposable function body",
    );
    assert_eq!(check().unwrap(), CheckedState::ExactInstalled);

    let mut reader = connect(&fixture.reader);
    let read_scope = "SELECT count(*) FROM jobs_mcp.read_shortlist($1)";
    let missing_digest = "00".repeat(32);
    let unknown = fixture_sql(
        reader.query_one(read_scope, &[&missing_digest]),
        "function-only read",
    );
    assert_eq!(unknown.get::<_, i64>(0), 0);
    let direct = reader.query_one("SELECT id FROM public.jobs_postings LIMIT 1", &[]);
    assert_eq!(
        direct.unwrap_err().code(),
        Some(&postgres::error::SqlState::INSUFFICIENT_PRIVILEGE)
    );
    drop(reader);

    const SEED: &str = include_str!("jobs_mcp_registered_scope_fixture.sql");
    let mut seed_tx = fixture_sql(fixture.admin.transaction(), "synthetic seed begin");
    fixture_sql(seed_tx.batch_execute(SEED), "synthetic scope projection");
    fixture_sql(seed_tx.commit(), "synthetic seed commit");
    let read_as = |marker| registered_reader_result(connect(&fixture.reader), marker);
    let a = read_as('a');
    let b = read_as('b');
    let empty = read_as('c');
    require_match(&a, "mcp-scope-track-a", Some("mcp-scope-job-a"));
    require_match(&b, "mcp-scope-track-b", Some("mcp-scope-job-b"));
    require_match(&empty, "mcp-scope-track-c", None);
    unavailable(&read_as('e'));

    let grant_digest = hex::encode(Sha256::digest(fixture_token('a').as_bytes()));
    assert_eq!(
        fixture_sql(
            fixture.admin.execute(
                "UPDATE public.jobs_mcp_read_grants SET revoked_at_ms=issued_at_ms
          WHERE token_sha256=$1 AND account_id='mcp-scope-account-a' AND revoked_at_ms IS NULL",
                &[&grant_digest]
            ),
            "one-way candidate grant revoke"
        ),
        1
    );
    unavailable(&read_as('a'));
    assert_eq!(read_as('b'), b, "scope A revocation cannot affect B");

    // Exercise actual application database startup replay, not a postmaster restart.
    let pool = super::super::open_postgres_pool(&fixture.url).expect("owned replay pool");
    super::super::run_migrations(&pool).expect("registered schema is restart-compatible");
    drop(pool);
    let mut installer = connect(&fixture.installer);
    let mut tx = fixture_sql(
        installer.build_transaction().read_only(true).start(),
        "readback begin",
    );
    let after_replay = installed_definition(&mut tx);
    assert_eq!(expected.as_deref(), Some(after_replay.as_str()));
    fixture_sql(tx.rollback(), "read-only close");
    assert_eq!(check().unwrap(), CheckedState::ExactInstalled);
    unavailable(&read_as('a'));
    assert_eq!(
        read_as('b'),
        b,
        "scope B survives registered startup replay"
    );
    assert_eq!(read_as('c'), empty, "authorized empty is preserved");

    let authority = fixture.admin.batch_execute(&format!(
        "REVOKE EXECUTE ON FUNCTION jobs_mcp.read_shortlist(text) FROM {READER};"
    ));
    fixture_sql(authority, "revoke only fixture reader execute");
    require_drift(check());
    let denied = connect(&fixture.reader).query_one(read_scope, &[&missing_digest]);
    assert_eq!(
        denied.unwrap_err().code(),
        Some(&postgres::error::SqlState::INSUFFICIENT_PRIVILEGE)
    );
    unavailable(&read_as('b'));
    println!("owned_postgres_mcp_installed_drift_refused");
    println!("owned_postgres_mcp_registered_reader_scope_passed");
    println!(
        "owned_postgres_mcp_fragment_on_registered_schema_passed definition_sha256={after_replay}"
    );
}

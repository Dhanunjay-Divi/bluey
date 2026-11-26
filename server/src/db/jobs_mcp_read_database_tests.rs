//! Composed cursor checks through the actual reader with two synthetic tenants.

use super::*;
use postgres::NoTls;
use serde_json::Value;
use std::{
    cell::RefCell,
    collections::VecDeque,
    io::{self, BufRead, Cursor, Read, Write},
    rc::Rc,
};

const GRANT: &str = "bjm1_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const OTHER_GRANT: &str = "bjm1_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const TRANSITION: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
const OTHER_TRANSITION: &str = "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
const POSTGRES_CASE: &str = concat!(
    "db::jobs_mcp_read::database_cursor_tests::",
    "postgres_match_reader_cursor_uses_owned_database_when_configured",
);

fn fixture_sql(now: i64) -> String {
    let grant = grant_digest(GRANT).expect("fixture grant digest");
    let other_grant = grant_digest(OTHER_GRANT).expect("other fixture grant digest");
    format!(
        r#"
CREATE TABLE accounts (
  id TEXT PRIMARY KEY, email_verified_at BIGINT, is_temporary BIGINT NOT NULL
);
CREATE TABLE jobs_tracks (
  account_id TEXT NOT NULL, id TEXT NOT NULL, active BIGINT NOT NULL,
  updated_at_ms BIGINT NOT NULL, PRIMARY KEY(account_id,id)
);
CREATE TABLE jobs_track_policy_track_input_heads (
  account_id TEXT NOT NULL, career_track_id TEXT NOT NULL,
  input_generation BIGINT NOT NULL, input_transition_sha256 TEXT NOT NULL,
  updated_at_ms BIGINT NOT NULL, PRIMARY KEY(account_id,career_track_id)
);
CREATE TABLE jobs_profiles (account_id TEXT PRIMARY KEY, updated_at_ms BIGINT NOT NULL);
CREATE TABLE jobs_preferences (account_id TEXT PRIMARY KEY, updated_at_ms BIGINT NOT NULL);
CREATE TABLE jobs_public_beta_cohorts (id TEXT PRIMARY KEY, state TEXT NOT NULL);
CREATE TABLE jobs_public_beta_enrollments (
  cohort_id TEXT NOT NULL, account_id TEXT NOT NULL, PRIMARY KEY(cohort_id,account_id)
);
CREATE TABLE jobs_public_beta_overrides (
  cohort_id TEXT NOT NULL, account_id TEXT NOT NULL, denied BIGINT NOT NULL
);
CREATE TABLE account_deletion_intents (account_id TEXT PRIMARY KEY);
CREATE TABLE jobs_mcp_read_grants (
  token_sha256 TEXT PRIMARY KEY, account_id TEXT NOT NULL, track_id TEXT NOT NULL,
  audience TEXT NOT NULL, scope TEXT NOT NULL, track_input_generation BIGINT NOT NULL,
  track_input_transition_sha256 TEXT NOT NULL, issued_at_ms BIGINT NOT NULL,
  expires_at_ms BIGINT NOT NULL, revoked_at_ms BIGINT
);
CREATE TABLE jobs_global_candidate_materializations (
  account_id TEXT NOT NULL, candidate_id TEXT NOT NULL, job_id TEXT NOT NULL,
  track_id TEXT NOT NULL, source_updated_at_ms BIGINT NOT NULL,
  materialized_at_ms BIGINT NOT NULL
);
CREATE TABLE jobs_global_candidates (
  id TEXT PRIMARY KEY, title TEXT NOT NULL, company TEXT NOT NULL,
  location TEXT NOT NULL, availability_status TEXT NOT NULL,
  updated_at_ms BIGINT NOT NULL
);
CREATE TABLE jobs_postings (
  id TEXT PRIMARY KEY, account_id TEXT NOT NULL, status TEXT NOT NULL,
  match_score BIGINT NOT NULL
);
CREATE TABLE jobs_workday_reconciliation_holds (
  account_id TEXT NOT NULL, job_id TEXT NOT NULL
);
CREATE TABLE jobs_global_candidate_memberships (
  source_id TEXT NOT NULL, candidate_id TEXT NOT NULL,
  last_seen_run_id TEXT NOT NULL, availability_status TEXT NOT NULL
);
CREATE TABLE jobs_global_discovery_sources (
  id TEXT PRIMARY KEY, status TEXT NOT NULL, health TEXT NOT NULL
);
CREATE TABLE jobs_global_ingestion_runs (
  id TEXT PRIMARY KEY, source_id TEXT NOT NULL, status TEXT NOT NULL,
  completed_at_ms BIGINT
);

INSERT INTO accounts VALUES ('alpha',1,0),('beta',1,0);
INSERT INTO jobs_tracks VALUES
  ('alpha','alpha-track',1,1),('beta','beta-track',1,1);
INSERT INTO jobs_track_policy_track_input_heads VALUES
  ('alpha','alpha-track',1,'{TRANSITION}',1),
  ('beta','beta-track',1,'{OTHER_TRANSITION}',1);
INSERT INTO jobs_profiles VALUES ('alpha',1),('beta',1);
INSERT INTO jobs_preferences VALUES ('alpha',1),('beta',1);
INSERT INTO jobs_public_beta_cohorts VALUES ('public-v1','open');
INSERT INTO jobs_public_beta_enrollments VALUES
  ('public-v1','alpha'),('public-v1','beta');
INSERT INTO jobs_mcp_read_grants VALUES
  ('{grant}','alpha','alpha-track','bluey-jobs-mcp-stdio-v1',
   'jobs:matches:read',1,'{TRANSITION}',{issued},{expires},NULL),
  ('{other_grant}','beta','beta-track','bluey-jobs-mcp-stdio-v1',
   'jobs:matches:read',1,'{OTHER_TRANSITION}',{issued},{expires},NULL);
INSERT INTO jobs_global_discovery_sources VALUES ('source','active','healthy');
INSERT INTO jobs_global_ingestion_runs VALUES ('run','source','completed',{now});
INSERT INTO jobs_postings VALUES
  ('alpha-a','alpha','matched',90),
  ('alpha-b','alpha','matched',90),
  ('beta-a','beta','matched',100);
INSERT INTO jobs_global_candidates VALUES
  ('alpha-a','Data Engineer','Alpha A','Remote','active',20),
  ('alpha-b','Data Engineer','Alpha B','Remote','active',10),
  ('beta-a','Platform Engineer','Beta','Remote','active',30);
INSERT INTO jobs_global_candidate_memberships VALUES
  ('source','alpha-a','run','active'),
  ('source','alpha-b','run','active'),
  ('source','beta-a','run','active');
INSERT INTO jobs_global_candidate_materializations VALUES
  ('alpha','alpha-a','alpha-a','alpha-track',20,{now}),
  ('alpha','alpha-b','alpha-b','alpha-track',10,{now}),
  ('beta','beta-a','beta-a','beta-track',30,{now});
"#,
        issued = now.saturating_sub(1_000),
        expires = now.saturating_add(60_000),
    )
}

fn sqlite_reader_for(grant: &str) -> MatchReader {
    let db = Connection::open_in_memory().expect("SQLite fixture");
    db.execute_batch(&fixture_sql(chrono::Utc::now().timestamp_millis()))
        .expect("install SQLite MCP projection fixture");
    MatchReader::with_database(ReaderDb::Sqlite(db), grant).expect("SQLite reader")
}

fn sqlite_reader() -> MatchReader {
    sqlite_reader_for(GRANT)
}

fn execute_fixture(reader: &mut MatchReader, sql: &str) {
    let ReaderDb::Sqlite(connection) = &mut reader.db else {
        panic!("test mutates SQLite fixture only");
    };
    connection
        .execute_batch(sql)
        .expect("mutate SQLite fixture");
}

fn sqlite_changes(reader: &MatchReader) -> Option<i64> {
    match &reader.db {
        ReaderDb::Sqlite(connection) => Some(
            connection
                .query_row("SELECT total_changes()", [], |row| row.get(0))
                .expect("SQLite change census"),
        ),
        ReaderDb::Postgres(_) => None,
    }
}

fn read_without_writes(
    reader: &mut MatchReader,
    request: MatchRequest<'_>,
    chain: &mut Option<PageChain>,
) -> Option<MatchResult> {
    let before = sqlite_changes(reader);
    let result = reader.read(request, chain).expect("MCP fixture read");
    assert_eq!(sqlite_changes(reader), before, "reader mutated SQLite");
    result
}

fn value(result: MatchResult) -> Value {
    serde_json::to_value(result).expect("serialize MCP result")
}

fn begin(reader: &mut MatchReader, chain: &mut Option<PageChain>) -> (Value, String) {
    let first = value(
        read_without_writes(reader, MatchRequest::Start { limit: 1 }, chain).expect("first page"),
    );
    assert_eq!(first["trackId"], "alpha-track");
    assert_eq!(first["matches"][0]["jobId"], "alpha-a");
    assert_eq!(first["shortlistTruncated"], false);
    let cursor = first["nextCursor"]
        .as_str()
        .expect("page 2 cursor")
        .to_owned();
    (first, cursor)
}

fn exercise_cursor_contract(
    reader: &mut MatchReader,
    mut mutate: impl FnMut(&mut MatchReader, &str),
) {
    let mut chain = None;
    let (_, cursor) = begin(reader, &mut chain);

    let mut copied_session = None;
    assert!(read_without_writes(
        reader,
        MatchRequest::Continue { cursor: &cursor },
        &mut copied_session,
    )
    .is_none());

    let second = value(
        read_without_writes(
            reader,
            MatchRequest::Continue { cursor: &cursor },
            &mut chain,
        )
        .expect("second page"),
    );
    assert_eq!(second["matches"][0]["jobId"], "alpha-b");
    assert_eq!(second["nextCursor"], Value::Null);
    let retry = value(
        read_without_writes(
            reader,
            MatchRequest::Continue { cursor: &cursor },
            &mut chain,
        )
        .expect("exact page retry"),
    );
    assert_eq!(retry, second);

    let (_, candidate_cursor) = begin(reader, &mut chain);
    mutate(
        reader,
        "UPDATE jobs_global_candidates SET company='Changed' WHERE id='alpha-b'",
    );
    assert!(read_without_writes(
        reader,
        MatchRequest::Continue {
            cursor: &candidate_cursor,
        },
        &mut chain,
    )
    .is_none());
    assert!(chain.is_none());
    mutate(
        reader,
        "UPDATE jobs_global_candidates SET company='Alpha B' WHERE id='alpha-b'",
    );

    let (_, drift_cursor) = begin(reader, &mut chain);
    mutate(
        reader,
        "UPDATE jobs_track_policy_track_input_heads SET input_generation=2 \
         WHERE account_id='alpha'",
    );
    assert!(read_without_writes(
        reader,
        MatchRequest::Continue {
            cursor: &drift_cursor,
        },
        &mut chain,
    )
    .is_none());
    assert!(chain.is_none());
    mutate(
        reader,
        "UPDATE jobs_track_policy_track_input_heads SET input_generation=1 \
         WHERE account_id='alpha'",
    );

    let (_, revoked_cursor) = begin(reader, &mut chain);
    mutate(
        reader,
        "UPDATE jobs_mcp_read_grants SET revoked_at_ms=issued_at_ms+1 \
         WHERE account_id='alpha'",
    );
    assert!(read_without_writes(
        reader,
        MatchRequest::Continue {
            cursor: &revoked_cursor,
        },
        &mut chain,
    )
    .is_none());
    assert!(chain.is_none());
}

fn rpc(id: i64, method: &str, params: Value) -> Value {
    serde_json::json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
}

fn initialized_frames() -> VecDeque<Vec<u8>> {
    let records = [
        rpc(
            1,
            "initialize",
            serde_json::json!({
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": {"name": "database-fixture", "version": "1"}
            }),
        ),
        serde_json::json!({"jsonrpc": "2.0", "method": "notifications/initialized", "params": {}}),
        rpc(
            2,
            "tools/call",
            serde_json::json!({"name": "get_job_matches", "arguments": {"limit": 1}}),
        ),
    ];
    records.into_iter().map(json_frame).collect()
}

fn json_frame(value: Value) -> Vec<u8> {
    let mut frame = serde_json::to_vec(&value).expect("fixture JSON");
    frame.push(b'\n');
    frame
}

#[derive(Clone)]
struct SharedOutput(Rc<RefCell<Vec<u8>>>);

impl Write for SharedOutput {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct CursorAfterFirstPage {
    frames: VecDeque<Vec<u8>>,
    output: Rc<RefCell<Vec<u8>>>,
    continued: bool,
}

impl CursorAfterFirstPage {
    fn new(output: Rc<RefCell<Vec<u8>>>) -> Self {
        Self {
            frames: initialized_frames(),
            output,
            continued: false,
        }
    }

    fn continue_from_written_response(&mut self) {
        if self.continued {
            return;
        }
        let output = self.output.borrow();
        let first_page: Value = std::str::from_utf8(&output)
            .expect("stdio output UTF-8")
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .find(|message: &Value| message["id"] == 2)
            .expect("first page response");
        let cursor = first_page["result"]["structuredContent"]["nextCursor"]
            .as_str()
            .expect("first page cursor");
        self.frames.push_back(json_frame(rpc(
            3,
            "tools/call",
            serde_json::json!({"name": "get_job_matches", "arguments": {"cursor": cursor}}),
        )));
        self.continued = true;
    }
}

impl Read for CursorAfterFirstPage {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let copied = {
            let available = self.fill_buf()?;
            let count = available.len().min(bytes.len());
            bytes[..count].copy_from_slice(&available[..count]);
            count
        };
        self.consume(copied);
        Ok(copied)
    }
}

impl BufRead for CursorAfterFirstPage {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        if self.frames.is_empty() {
            self.continue_from_written_response();
        }
        Ok(self.frames.front().map_or(&[], Vec::as_slice))
    }

    fn consume(&mut self, amount: usize) {
        let Some(length) = self.frames.front().map(Vec::len) else {
            return;
        };
        if amount >= length {
            self.frames.pop_front();
        } else if let Some(frame) = self.frames.front_mut() {
            frame.drain(..amount);
        }
    }
}

fn output_records(bytes: &[u8]) -> Vec<Value> {
    std::str::from_utf8(bytes)
        .expect("stdio output UTF-8")
        .lines()
        .map(|line| serde_json::from_str(line).expect("stdio JSON-RPC frame"))
        .collect()
}

fn exercise_stdio_pagination(reader: &mut MatchReader, track: &str, pages: &[&str]) {
    let output = Rc::new(RefCell::new(Vec::new()));
    reader
        .serve(
            CursorAfterFirstPage::new(Rc::clone(&output)),
            SharedOutput(Rc::clone(&output)),
        )
        .expect("database-backed MCP stdio session");
    let records = output_records(&output.borrow());
    assert_eq!(records.len(), 3, "only requests receive stdio responses");
    assert_eq!(records[0]["id"], 1);
    for (index, job_id) in pages.iter().enumerate() {
        let record = &records[index + 1];
        assert_eq!(record["id"], Value::from(index + 2));
        assert_eq!(record["result"]["isError"], false);
        let page = &record["result"]["structuredContent"];
        assert_eq!(page["trackId"], track);
        assert_eq!(page["matches"].as_array().unwrap().len(), 1);
        assert_eq!(page["matches"][0]["jobId"], *job_id);
    }
    assert!(records[1]["result"]["structuredContent"]["nextCursor"].is_string());
    assert!(records[2]["result"]["structuredContent"]["nextCursor"].is_null());
}

fn exercise_stdio_first_page(reader: &mut MatchReader, track: &str, job_id: &str) {
    let input = initialized_frames()
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let mut output = Vec::new();
    reader
        .serve(Cursor::new(input), &mut output)
        .expect("database-backed owner-scoped MCP stdio session");
    let records = output_records(&output);
    assert_eq!(records.len(), 2);
    assert_eq!(records[1]["result"]["isError"], false);
    let page = &records[1]["result"]["structuredContent"];
    assert_eq!(page["trackId"], track);
    assert_eq!(page["matches"].as_array().unwrap().len(), 1);
    assert_eq!(page["matches"][0]["jobId"], job_id);
    assert!(page["nextCursor"].is_null());
}

fn exercise_stdio_refusal(reader: &mut MatchReader, request: Value) {
    let mut input = initialized_frames();
    input.pop_back();
    input.push_back(json_frame(request));
    let input = input.into_iter().flatten().collect::<Vec<_>>();
    let mut output = Vec::new();
    reader
        .serve(Cursor::new(input), &mut output)
        .expect("database-backed MCP refusal session");
    let records = output_records(&output);
    assert_eq!(records.len(), 2);
    assert_eq!(
        records[1],
        serde_json::json!({
            "jsonrpc": "2.0", "id": 3,
            "result": {"isError": true,
                "content": [{"type": "text", "text": "Career Track unavailable"}]}
        })
    );
}

#[test]
fn sqlite_match_reader_cursor_binds_snapshot_and_session() {
    exercise_cursor_contract(&mut sqlite_reader(), execute_fixture);
}

#[test]
fn sqlite_mcp_stdio_uses_database_grants_for_owner_pages_and_drift_refusals() {
    let mut alpha = sqlite_reader();
    exercise_stdio_pagination(&mut alpha, "alpha-track", &["alpha-a", "alpha-b"]);

    let mut beta = sqlite_reader_for(OTHER_GRANT);
    exercise_stdio_first_page(&mut beta, "beta-track", "beta-a");

    let mut session_drift = sqlite_reader();
    let mut direct_chain = None;
    let (_, cursor) = begin(&mut session_drift, &mut direct_chain);
    exercise_stdio_refusal(
        &mut session_drift,
        rpc(
            3,
            "tools/call",
            serde_json::json!({"name": "get_job_matches", "arguments": {"cursor": cursor}}),
        ),
    );

    let mut revoked_grant = sqlite_reader();
    execute_fixture(
        &mut revoked_grant,
        "UPDATE jobs_mcp_read_grants SET revoked_at_ms=issued_at_ms+1 WHERE account_id='alpha'",
    );
    exercise_stdio_refusal(
        &mut revoked_grant,
        rpc(
            3,
            "tools/call",
            serde_json::json!({"name": "get_job_matches", "arguments": {"limit": 1}}),
        ),
    );
}

#[test]
fn postgres_match_reader_cursor_uses_owned_database_when_configured() {
    let Some(url) = crate::db::owned_postgres_test::owned_url(POSTGRES_CASE) else {
        return;
    };
    let mut owner = postgres::Client::connect(&url, NoTls).expect("owned PostgreSQL fixture");
    owner
        .batch_execute(&fixture_sql(chrono::Utc::now().timestamp_millis()))
        .expect("install owned PostgreSQL rows");
    let mut denied = MatchReader::from_postgres(
        postgres::Client::connect(&url, NoTls).expect("owned pre-install connection"),
        GRANT,
    )
    .expect("create pre-install reader");
    let request = || {
        rpc(
            3,
            "tools/call",
            serde_json::json!({"name": "get_job_matches", "arguments": {"limit": 1}}),
        )
    };
    exercise_stdio_refusal(&mut denied, request());
    drop(denied);

    let (reader_config, reader_role) = install_postgres_function(&mut owner, &url);
    let mut restricted = reader_config
        .connect(NoTls)
        .expect("execute-only role connection");
    let row = restricted
        .query_one("SELECT session_user::text, current_user::text", &[])
        .expect("owned PostgreSQL reader identity");
    assert_eq!(row.get::<_, &str>(0), reader_role);
    assert_eq!(row.get::<_, &str>(1), reader_role);
    for sql in [
        "SELECT token_sha256 FROM public.jobs_mcp_read_grants",
        "UPDATE public.jobs_mcp_read_grants SET revoked_at_ms=issued_at_ms",
        "CREATE TEMP TABLE mcp_forbidden (id integer)",
        "CREATE TABLE jobs_mcp.mcp_forbidden (id integer)",
    ] {
        let denied = restricted
            .batch_execute(sql)
            .expect_err("execute-only role escaped");
        assert_eq!(
            denied.code(),
            Some(&postgres::error::SqlState::INSUFFICIENT_PRIVILEGE)
        );
    }

    let mut beta =
        MatchReader::from_postgres(restricted, OTHER_GRANT).expect("second grant reader");
    exercise_stdio_first_page(&mut beta, "beta-track", "beta-a");
    drop(beta);
    let mut reader = MatchReader::from_postgres(
        reader_config
            .connect(NoTls)
            .expect("reconnect as execute-only reader"),
        GRANT,
    )
    .expect("PostgreSQL shortlist reader");
    exercise_stdio_pagination(&mut reader, "alpha-track", &["alpha-a", "alpha-b"]);
    exercise_cursor_contract(&mut reader, |_, sql| {
        owner
            .batch_execute(sql)
            .expect("mutate owned fixture separately");
    });
    exercise_stdio_refusal(&mut reader, request());
    drop(reader);
    let mut unknown = MatchReader::from_postgres(
        reader_config
            .connect(NoTls)
            .expect("connect with unknown fixture grant"),
        "bjm1_eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
    )
    .expect("unknown grant reader initialization");
    exercise_stdio_refusal(&mut unknown, request());
    println!("owned_postgres_mcp_stdio_pagination_complete");
    println!("owned_postgres_mcp_pagination_complete");
    println!("owned_postgres_mcp_function_only_scope_complete");
}

fn install_postgres_function(
    owner: &mut postgres::Client,
    url: &str,
) -> (postgres::Config, &'static str) {
    const READER_ROLE: &str = "jobs_mcp_fixture_reader";
    const TEMPLATE: &str = include_str!("../../tests/fixtures/jobs-mcp-postgres-role-boundary.sql");
    const CANONICAL: &str = include_str!("jobs-mcp-read-postgres.sql");
    const RELATIONS: &[&str] = &[
        "account_deletion_intents",
        "accounts",
        "jobs_global_candidate_materializations",
        "jobs_global_candidate_memberships",
        "jobs_global_candidates",
        "jobs_global_discovery_sources",
        "jobs_global_ingestion_runs",
        "jobs_mcp_read_grants",
        "jobs_postings",
        "jobs_preferences",
        "jobs_profiles",
        "jobs_public_beta_cohorts",
        "jobs_public_beta_enrollments",
        "jobs_public_beta_overrides",
        "jobs_track_policy_track_input_heads",
        "jobs_tracks",
        "jobs_workday_reconciliation_holds",
    ];
    let mut config: postgres::Config = url.parse().expect("validated owned PostgreSQL URL");
    let database = config.get_dbname().expect("owned case database");
    assert!(database.starts_with("bluey_jobs_case_"));
    assert!(database
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'));
    let mut body = CANONICAL[CANONICAL
        .find("WITH authorized AS (")
        .expect("shortlist body")..]
        .replace(
            "$1",
            "(CASE WHEN $1 COLLATE pg_catalog.\"C\" ~ '^[0-9a-f]{64}$' \
                         AND pg_catalog.octet_length($1) = 64 THEN $1 END)",
        )
        .replace(
            "$2",
            "(EXTRACT(EPOCH FROM pg_catalog.statement_timestamp()) * 1000)::BIGINT",
        )
        .replace("COLLATE \"C\"", "COLLATE pg_catalog.\"C\"");
    for relation in RELATIONS {
        let from = format!("FROM {relation} AS");
        let join = format!("JOIN {relation} AS");
        assert_eq!(body.matches(&from).count() + body.matches(&join).count(), 1);
        body = body
            .replace(&from, &format!("FROM public.{relation} AS"))
            .replace(&join, &format!("JOIN public.{relation} AS"));
    }
    let sql = TEMPLATE
        .replace("jobs_mcp_fixture.", "jobs_mcp.")
        .replace("SCHEMA jobs_mcp_fixture", "SCHEMA jobs_mcp")
        .replace("ON DATABASE postgres", &format!("ON DATABASE {database}"))
        .replace("-- @PINNED_SHORTLIST_QUERY@", body.trim_end());
    owner
        .batch_execute(&sql)
        .expect("install least-privilege shortlist function");
    let password = uuid::Uuid::new_v4().simple().to_string();
    owner
        .batch_execute(&format!("ALTER ROLE {READER_ROLE} PASSWORD '{password}'"))
        .expect("set a fresh disposable fixture credential");
    config
        .user(READER_ROLE)
        .password(password)
        .connect_timeout(Duration::from_secs(5));
    (config, READER_ROLE)
}

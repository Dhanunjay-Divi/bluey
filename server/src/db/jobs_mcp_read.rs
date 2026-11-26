//! Read-only Jobs MCP projection for current, persisted Track materializations.
//! Cursor state belongs to one stdio session. It never caches candidate rows.

use std::{
    io,
    path::Path,
    time::{Duration, Instant},
};

use crate::jobs_mcp_stdio::{DiscoveryLead, MatchRequest, MatchResult, MAX_SHORTLIST_ROWS};
use anyhow::{ensure, Context, Result};
use rusqlite::{params, Connection, OpenFlags};
use serde::Serialize;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

// The trusted host supplies the execute-only connection. The runtime path
// deliberately cannot fall back to direct table reads if installation is
// absent or access is withdrawn.
const PG_READ: &str = r#"
SELECT account_id, track_id, input_generation, input_transition_sha256,
       issued_at_ms, expires_at_ms, inputs_updated_at_ms,
       job_id, title, company, location, match_score, candidate_updated_at_ms
  FROM jobs_mcp.read_shortlist($1::text)
 ORDER BY (job_id IS NULL), match_score DESC, candidate_updated_at_ms DESC,
          job_id COLLATE pg_catalog."C" ASC
 LIMIT 242
"#;
const SQLITE_READ: &str = include_str!("jobs-mcp-read-sqlite.sql");
const MAX_PAGE_SIZE: usize = 10;
const MAX_CURSOR_AGE_MS: i64 = 120_000;

enum ReaderDb {
    Postgres(postgres::Client),
    Sqlite(Connection),
}

pub struct MatchReader {
    db: ReaderDb,
    grant_sha256: String,
}

#[derive(Debug, Eq, PartialEq, Serialize)]
struct Authority {
    account_id: String,
    track_id: String,
    track_input_generation: i64,
    track_input_transition_sha256: String,
    issued_at_ms: i64,
    expires_at_ms: i64,
    inputs_updated_at_ms: i64,
}

type RawLead = (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<i64>,
    Option<i64>,
);

type RawRow = (Authority, RawLead);

// Both backends use the same numbered projection; their row APIs differ.
macro_rules! projected_row {
    ($row:expr, $get:ident) => {
        (
            Authority {
                account_id: $row.$get(0)?,
                track_id: $row.$get(1)?,
                track_input_generation: $row.$get(2)?,
                track_input_transition_sha256: $row.$get(3)?,
                issued_at_ms: $row.$get(4)?,
                expires_at_ms: $row.$get(5)?,
                inputs_updated_at_ms: $row.$get(6)?,
            },
            (
                $row.$get(7)?,
                $row.$get(8)?,
                $row.$get(9)?,
                $row.$get(10)?,
                $row.$get(11)?,
                $row.$get(12)?,
            ),
        )
    };
}

#[derive(Serialize)]
struct Snapshot {
    authority: Authority,
    // Preserve the actual source order key even when not rendered.
    leads: Vec<(DiscoveryLead, i64)>,
    truncated: bool,
}

impl Snapshot {
    fn decode(rows: Vec<RawRow>, now: i64) -> Result<Option<Self>> {
        let count = rows.len();
        ensure!(count <= MAX_SHORTLIST_ROWS + 1, "invalid Jobs MCP window");
        let mut rows = rows.into_iter();
        let Some((authority, first)) = rows.next() else {
            return Ok(None);
        };
        ensure!(
            safe_id(&authority.account_id, 128)
                && safe_id(&authority.track_id, 64)
                && authority.track_input_generation > 0
                && lowercase_hex_32(&authority.track_input_transition_sha256)
                && (0..=now).contains(&authority.issued_at_ms)
                && authority.expires_at_ms > now
                && authority.expires_at_ms - authority.issued_at_ms <= 900_000
                && authority.inputs_updated_at_ms >= 0,
            "invalid Jobs MCP authority projection"
        );
        let empty = count == 1 && first == (None, None, None, None, None, None);
        let mut leads = Vec::with_capacity(count.min(MAX_SHORTLIST_ROWS));
        if !empty {
            for raw in std::iter::once(Ok(first)).chain(rows.map(|(row_authority, raw)| {
                ensure!(row_authority == authority, "mixed Jobs MCP authority");
                Ok(raw)
            })) {
                let (
                    Some(job_id),
                    Some(title),
                    Some(company),
                    Some(location),
                    Some(score),
                    Some(updated_ms),
                ) = raw?
                else {
                    anyhow::bail!("invalid Jobs MCP match projection");
                };
                ensure!(
                    safe_id(&job_id, 128)
                        && valid_text(&title, 1024, false)
                        && valid_text(&company, 768, false)
                        && valid_text(&location, 512, true)
                        && (0..=100).contains(&score)
                        && updated_ms >= 0,
                    "invalid Jobs MCP match projection"
                );
                if leads.len() < MAX_SHORTLIST_ROWS {
                    leads.push((
                        DiscoveryLead::new(job_id, title, company, location, score),
                        updated_ms,
                    ));
                }
            }
        }
        Ok(Some(Self {
            authority,
            leads,
            truncated: count > MAX_SHORTLIST_ROWS,
        }))
    }

    fn digest(&self, grant_sha256: &str) -> Result<[u8; 32]> {
        // Tuple order and the derived struct layout define this private v1
        // canonical fingerprint. None of these metadata bytes leave the server.
        let wire =
            serde_json::to_vec(&("bluey-jobs-mcp-current-shortlist-v1", grant_sha256, self))?;
        Ok(Sha256::digest(wire).into())
    }
}

struct PageChain {
    fingerprint: [u8; 32],
    deadline: Instant,
    size: usize,
    // Entry 0 is the handle for page 2, not page 1. Handles contain no
    // account, Track, offset or query metadata, and survive exact page retry.
    handles: Vec<String>,
}

impl PageChain {
    fn new(
        fingerprint: [u8; 32],
        size: usize,
        snapshot: &Snapshot,
        observed: Instant,
        now: i64,
    ) -> Result<Self> {
        let ttl = (snapshot.authority.expires_at_ms - now).min(MAX_CURSOR_AGE_MS);
        let deadline = observed
            .checked_add(Duration::from_millis(u64::try_from(ttl)?))
            .context("invalid Jobs MCP window deadline")?;
        let count = snapshot.leads.len().div_ceil(size).saturating_sub(1);
        let mut handles = Vec::with_capacity(count);
        for _ in 0..count {
            let mut random = [0_u8; 32];
            getrandom::getrandom(&mut random)
                .map_err(|_| anyhow::anyhow!("Jobs MCP cursor unavailable"))?;
            let handle = format!("bjmcp1_{}", hex::encode(random));
            ensure!(!handles.contains(&handle), "Jobs MCP cursor collision");
            handles.push(handle);
        }
        Ok(Self {
            fingerprint,
            deadline,
            size,
            handles,
        })
    }

    fn page(&self, snapshot: &Snapshot, index: usize) -> MatchResult {
        let start = index * self.size;
        let matches = snapshot
            .leads
            .iter()
            .skip(start)
            .take(self.size)
            .map(|(lead, _)| lead.clone())
            .collect();
        MatchResult::discovery_shortlist(
            snapshot.authority.track_id.clone(),
            matches,
            self.handles.get(index).cloned(),
            snapshot.truncated,
        )
    }

    fn cursor_index(&self, cursor: &str) -> Option<usize> {
        self.handles
            .iter()
            .position(|handle| bool::from(handle.as_bytes().ct_eq(cursor.as_bytes())))
            .map(|zero_based| zero_based + 1)
    }
}

fn lowercase_hex_32(token: &str) -> bool {
    token.len() == 64
        && token
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn safe_id(value: &str, maximum: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

fn valid_text(value: &str, maximum: usize, allow_empty: bool) -> bool {
    value.len() <= maximum
        && (allow_empty || !value.trim().is_empty())
        && !value.chars().any(char::is_control)
}

fn grant_digest(value: &str) -> Result<String> {
    let token = value
        .strip_prefix("bjm1_")
        .context("Jobs MCP grant unavailable")?;
    ensure!(lowercase_hex_32(token), "Jobs MCP grant unavailable");
    Ok(hex::encode(Sha256::digest(value.as_bytes())))
}

fn sqlite_reader(path: &Path) -> Result<Connection> {
    ensure!(
        path.is_absolute() && path.canonicalize()? == path && path.is_file(),
        "Jobs MCP requires an existing canonical SQLite file"
    );
    let db = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )?;
    db.busy_timeout(Duration::from_secs(2))?;
    Ok(db)
}

impl MatchReader {
    /// Accepts the trusted host's execute-only connection to the installed
    /// shortlist function. Does not install a role, issue a grant or mount a host.
    pub fn from_postgres(db: postgres::Client, grant: &str) -> Result<Self> {
        Self::with_database(ReaderDb::Postgres(db), grant)
    }

    /// For a trusted test host with an existing read-only synthetic database.
    pub fn from_read_only_sqlite(path: &Path, grant: &str) -> Result<Self> {
        Self::with_database(ReaderDb::Sqlite(sqlite_reader(path)?), grant)
    }

    fn with_database(db: ReaderDb, grant: &str) -> Result<Self> {
        ensure!(
            super::jobs_beta_access::public_beta_master_enabled(),
            "Jobs MCP is disabled"
        );
        Ok(Self {
            db,
            grant_sha256: grant_digest(grant)?,
        })
    }

    pub fn serve<R: io::BufRead, W: io::Write>(&mut self, input: R, output: W) -> io::Result<()> {
        let mut chain = None;
        crate::jobs_mcp_stdio::run(input, output, |request| {
            self.read(request, &mut chain).map_err(|_| {
                chain = None;
            })
        })
    }

    fn read(
        &mut self,
        request: MatchRequest<'_>,
        chain: &mut Option<PageChain>,
    ) -> Result<Option<MatchResult>> {
        ensure!(
            super::jobs_beta_access::public_beta_master_enabled(),
            "Jobs MCP is disabled"
        );
        if chain
            .as_ref()
            .is_some_and(|state| Instant::now() >= state.deadline)
        {
            *chain = None;
        }
        let (size, index) = match request {
            MatchRequest::Start { limit } => {
                ensure!(
                    (1..=MAX_PAGE_SIZE).contains(&limit),
                    "invalid Jobs MCP read"
                );
                (limit, 0)
            }
            MatchRequest::Continue { cursor } => {
                let Some(state) = chain.as_ref() else {
                    return Ok(None);
                };
                let Some(index) = state.cursor_index(cursor) else {
                    return Ok(None);
                };
                (state.size, index)
            }
        };
        let now = chrono::Utc::now().timestamp_millis();
        let Some(snapshot) = self.read_at(now)? else {
            *chain = None;
            return Ok(None);
        };
        let observed = Instant::now();
        let completed_at = chrono::Utc::now().timestamp_millis();
        if !(snapshot.authority.issued_at_ms..snapshot.authority.expires_at_ms)
            .contains(&completed_at)
        {
            *chain = None;
            return Ok(None);
        }
        let fingerprint = snapshot.digest(&self.grant_sha256)?;
        let current = chain.as_ref().is_some_and(|state| {
            state.fingerprint == fingerprint
                && state.size == size
                && Instant::now() < state.deadline
        });
        if !current && index > 0 {
            *chain = None;
            return Ok(None);
        }
        if !current {
            *chain = Some(PageChain::new(
                fingerprint,
                size,
                &snapshot,
                observed,
                completed_at,
            )?);
        }
        Ok(chain.as_ref().map(|state| state.page(&snapshot, index)))
    }

    fn read_at(&mut self, now: i64) -> Result<Option<Snapshot>> {
        let mut rows = Vec::new();
        match &mut self.db {
            ReaderDb::Postgres(db) => {
                let mut tx = db.build_transaction().read_only(true).start()?;
                tx.batch_execute("SET LOCAL statement_timeout = '2s';")?;
                for row in tx.query(PG_READ, &[&self.grant_sha256])? {
                    rows.push(projected_row!(row, try_get));
                }
                tx.commit()?;
            }
            ReaderDb::Sqlite(db) => {
                let mut query = db.prepare_cached(SQLITE_READ)?;
                for row in query.query_map(params![self.grant_sha256, now], |row| {
                    Ok(projected_row!(row, get))
                })? {
                    rows.push(row?);
                }
            }
        }
        Snapshot::decode(rows, now)
    }
}

#[cfg(test)]
#[path = "jobs_mcp_read_database_tests.rs"]
mod database_cursor_tests;

#[cfg(test)]
mod tests {
    use super::*;

    fn current_row(job_id: &str, company: &str) -> RawRow {
        (
            Authority {
                account_id: "acct-test".into(),
                track_id: "track-test".into(),
                track_input_generation: 2,
                track_input_transition_sha256: "a".repeat(64),
                issued_at_ms: 1,
                expires_at_ms: 20_000,
                inputs_updated_at_ms: 3,
            },
            (
                Some(job_id.into()),
                Some("Data Engineer".into()),
                Some(company.into()),
                Some("Remote".into()),
                Some(83),
                Some(5),
            ),
        )
    }

    #[test]
    fn delegated_token_uses_distinct_version_and_exact_lowercase_encoding() {
        let suffix = "a".repeat(64);
        let token = format!("bjm1_{suffix}");
        let digest = grant_digest(&token).unwrap();
        assert_eq!(digest, hex::encode(Sha256::digest(token.as_bytes())));
        for other in [
            suffix.clone(),
            format!("bjm2_{suffix}"),
            format!("bjm1_{}", suffix.to_uppercase()),
            format!("{token}\n"),
            format!("bjm1_{}", &suffix[..63]),
        ] {
            assert!(grant_digest(&other).is_err());
        }
    }

    #[test]
    fn shortlist_cursor_is_session_local_and_exact_page_replay_is_stable() {
        let snapshot =
            Snapshot::decode(vec![current_row("id-a", "A"), current_row("id-b", "B")], 8)
                .unwrap()
                .unwrap();
        let fingerprint = snapshot.digest("test-grant").unwrap();
        let first = PageChain::new(fingerprint, 1, &snapshot, Instant::now(), 8).unwrap();
        let other = PageChain::new(fingerprint, 1, &snapshot, Instant::now(), 8).unwrap();
        let handle = &first.handles[0];
        assert_eq!(first.cursor_index(handle), Some(1));
        assert_eq!(other.cursor_index(handle), None);
        assert_eq!(first.page(&snapshot, 1), first.page(&snapshot, 1));
        let result = serde_json::to_value(first.page(&snapshot, 1)).unwrap();
        assert_eq!(result["matches"][0]["jobId"], "id-b");
        assert_eq!(result["nextCursor"], serde_json::Value::Null);
    }

    #[test]
    fn shortlist_fingerprint_binds_authority_display_data_order_and_grant() {
        let digest = |rows, token| {
            Snapshot::decode(rows, 8)
                .unwrap()
                .unwrap()
                .digest(token)
                .unwrap()
        };
        let before = digest(
            vec![current_row("id-a", "A"), current_row("id-b", "B")],
            "token-a",
        );
        assert_ne!(
            before,
            digest(
                vec![current_row("id-b", "B"), current_row("id-a", "A")],
                "token-a"
            )
        );
        assert_ne!(
            before,
            digest(
                vec![current_row("id-a", "changed"), current_row("id-b", "B")],
                "token-a"
            )
        );
        assert_ne!(
            before,
            digest(
                vec![current_row("id-a", "A"), current_row("id-b", "B")],
                "token-b"
            )
        );
        let mut advanced = vec![current_row("id-a", "A"), current_row("id-b", "B")];
        for (authority, _) in &mut advanced {
            authority.track_input_generation += 1;
        }
        assert_ne!(before, digest(advanced, "token-a"));
    }

    #[test]
    fn one_lead_pages_end_cleanly_at_the_declared_window() {
        let rows = (0..=MAX_SHORTLIST_ROWS)
            .map(|index| current_row(&format!("id-{index:03}"), "A"))
            .collect();
        let snapshot = Snapshot::decode(rows, 8).unwrap().unwrap();
        let fingerprint = snapshot.digest("test-grant").unwrap();
        let chain = PageChain::new(fingerprint, 1, &snapshot, Instant::now(), 8).unwrap();
        let mut page = 0;
        loop {
            let result = serde_json::to_value(chain.page(&snapshot, page)).unwrap();
            assert_eq!(result["matches"][0]["jobId"], format!("id-{page:03}"));
            assert_eq!(result["shortlistTruncated"], true);
            let Some(next) = result["nextCursor"].as_str() else {
                break;
            };
            page = chain.cursor_index(next).unwrap();
        }
        assert_eq!(page + 1, MAX_SHORTLIST_ROWS);
    }

    #[test]
    fn only_bounded_display_values_and_track_keys_are_projected() {
        assert!(safe_id("track_demo-1:example", 64));
        assert!(!safe_id("not a track", 64));
        assert!(!safe_id(&"x".repeat(65), 64));
        assert!(!safe_id(&"x".repeat(129), 128));
        assert!(valid_text("Data Engineer", 100, false));
        assert!(valid_text("", 100, true));
        assert!(!valid_text("  ", 100, false));
        assert!(!valid_text("line\nfeed", 100, false));
    }
}

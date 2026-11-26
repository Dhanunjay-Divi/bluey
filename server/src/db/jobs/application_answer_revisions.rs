/// Encrypted history of an owner-confirmed application answer revision.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApplicationAnswerRevision {
    pub id: String,
    pub application_id: String,
    pub intervention_id: String,
    pub previous_answers: Vec<Value>,
    pub revised_answers: Vec<Value>,
    pub invalidated_packet_checksum: Option<String>,
    pub revised_at_ms: i64,
}

fn answer_revision_payload(revision: &ApplicationAnswerRevision) -> Result<String> {
    let valid_id =
        |s: &str| !s.trim().is_empty() && s.len() <= 256 && !s.chars().any(char::is_control);
    if !valid_id(&revision.id)
        || !valid_id(&revision.application_id)
        || !valid_id(&revision.intervention_id)
        || revision.revised_at_ms < 0
        || revision
            .invalidated_packet_checksum
            .as_ref()
            .is_some_and(|s| s.len() > 256)
    {
        anyhow::bail!("application question revision is invalid");
    }
    let encoded = serde_json::to_vec(revision)
        .map_err(|_| anyhow::anyhow!("application question revision is invalid"))?;
    if encoded.len() > 1024 * 1024 {
        anyhow::bail!("application question revision is invalid");
    }
    to_json(revision, "application question revision")
}

fn persist_application_answer_revision_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    revision: &ApplicationAnswerRevision,
) -> Result<()> {
    let payload = answer_revision_payload(revision)?;
    tx.execute(
        "INSERT INTO jobs_application_answer_revisions
         (id,account_id,application_id,intervention_id,revision_json,revised_at_ms)
         VALUES (?1,?2,?3,?4,?5,?6)",
        params![
            revision.id,
            account_id,
            revision.application_id,
            revision.intervention_id,
            payload,
            revision.revised_at_ms
        ],
    )?;
    Ok(())
}

fn persist_application_answer_revision_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    revision: &ApplicationAnswerRevision,
) -> Result<()> {
    let payload = answer_revision_payload(revision)?;
    tx.execute(
        "INSERT INTO jobs_application_answer_revisions
         (id,account_id,application_id,intervention_id,revision_json,revised_at_ms)
         VALUES ($1,$2,$3,$4,$5,$6)",
        &[
            &revision.id,
            &account_id,
            &revision.application_id,
            &revision.intervention_id,
            &payload,
            &revision.revised_at_ms,
        ],
    )?;
    Ok(())
}

pub fn get_application_answer_revision(
    pool: &DbPool,
    account_id: &str,
    revision_id: &str,
) -> Result<Option<ApplicationAnswerRevision>> {
    crate::db::run_blocking_db(|| {
        let row: Option<(String, String, Option<String>, i64)> = match pool {
            DbPool::Sqlite(_) => pool.get()?.query_row(
                "SELECT application_id,intervention_id,
                 CASE WHEN length(CAST(revision_json AS BLOB))<=1398153 THEN revision_json ELSE NULL END,revised_at_ms
                 FROM jobs_application_answer_revisions WHERE account_id=?1 AND id=?2",
                params![account_id, revision_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            ).optional()?,
            DbPool::Postgres(_) => pool.get_pg()?.query_opt(
                "SELECT application_id,intervention_id,
                 CASE WHEN octet_length(revision_json)<=1398153 THEN revision_json ELSE NULL END,revised_at_ms
                 FROM jobs_application_answer_revisions WHERE account_id=$1 AND id=$2",
                &[&account_id, &revision_id],
            )?.map(|r| (r.get(0), r.get(1), r.get(2), r.get(3))),
        };
        row.map(|(application_id, intervention_id, raw, revised_at_ms)| {
            let raw =
                raw.ok_or_else(|| anyhow::anyhow!("application question revision is invalid"))?;
            decode_answer_revision(
                revision_id,
                &application_id,
                &intervention_id,
                revised_at_ms,
                &raw,
            )
            .map(|(revision, _)| revision)
        })
        .transpose()
    })
}

// 1 MiB plaintext + 12-byte nonce + 16-byte tag, URL-safe unpadded base64,
// plus the 14-byte encrypted payload prefix.
const ANSWER_REVISION_RAW_LIMIT: usize = 1_398_153;

fn decode_answer_revision(
    id: &str,
    application_id: &str,
    intervention_id: &str,
    revised_at_ms: i64,
    raw: &str,
) -> Result<(ApplicationAnswerRevision, usize)> {
    if raw.len() > ANSWER_REVISION_RAW_LIMIT || !raw.starts_with(ENCRYPTED_PAYLOAD_PREFIX) {
        anyhow::bail!("application question revision is invalid");
    }
    let plain = decrypt_payload(raw)?;
    if plain.len() > 1024 * 1024 {
        anyhow::bail!("application question revision is invalid");
    }
    let revision: ApplicationAnswerRevision = serde_json::from_str(&plain)
        .map_err(|_| anyhow::anyhow!("application question revision is invalid"))?;
    if revision.id != id
        || revision.application_id != application_id
        || revision.intervention_id != intervention_id
        || revision.revised_at_ms != revised_at_ms
    {
        anyhow::bail!("application question revision is invalid");
    }
    Ok((revision, plain.len()))
}

pub(crate) fn export_application_answer_revisions(
    pool: &DbPool,
    account_id: &str,
) -> Result<Vec<ApplicationAnswerRevision>> {
    crate::db::run_blocking_db(|| {
        let mut total = 0usize;
        let mut revisions = Vec::new();
        let mut accept = |id: String,
                          application_id: String,
                          intervention_id: String,
                          raw: Option<String>,
                          revised_at_ms: i64|
         -> Result<()> {
            if revisions.len() >= 10_000 {
                anyhow::bail!("application question revision export exceeds its limit");
            }
            let raw =
                raw.ok_or_else(|| anyhow::anyhow!("application question revision is invalid"))?;
            let (revision, bytes) = decode_answer_revision(
                &id,
                &application_id,
                &intervention_id,
                revised_at_ms,
                &raw,
            )?;
            total = total.checked_add(bytes).ok_or_else(|| {
                anyhow::anyhow!("application question revision export exceeds its limit")
            })?;
            if total > 64 * 1024 * 1024 {
                anyhow::bail!("application question revision export exceeds its limit");
            }
            revisions.push(revision);
            Ok(())
        };
        match pool {
            DbPool::Sqlite(_) => {
                let conn = pool.get()?;
                let mut stmt = conn.prepare(
                    "SELECT id,application_id,intervention_id,
                     CASE WHEN length(CAST(revision_json AS BLOB))<=1398153 THEN revision_json ELSE NULL END,revised_at_ms
                     FROM jobs_application_answer_revisions WHERE account_id=?1 ORDER BY id LIMIT 10001",
                )?;
                let mut rows = stmt.query(params![account_id])?;
                while let Some(row) = rows.next()? {
                    accept(
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    )?;
                }
            }
            DbPool::Postgres(_) => {
                use postgres::fallible_iterator::FallibleIterator;
                let mut conn = pool.get_pg()?;
                let mut rows = conn.query_raw(
                    "SELECT id,application_id,intervention_id,
                     CASE WHEN octet_length(revision_json)<=1398153 THEN revision_json ELSE NULL END,revised_at_ms
                     FROM jobs_application_answer_revisions WHERE account_id=$1 ORDER BY id LIMIT 10001",
                    [&account_id as &(dyn postgres::types::ToSql + Sync)],
                )?;
                while let Some(row) = rows.next()? {
                    accept(row.get(0), row.get(1), row.get(2), row.get(3), row.get(4))?;
                }
            }
        }
        Ok(revisions)
    })
}

#[cfg(test)]
mod application_answer_revision_tests {
    use super::*;

    fn pool() -> DbPool {
        let manager = r2d2_sqlite::SqliteConnectionManager::memory().with_init(|conn| {
            conn.execute_batch(
                "PRAGMA foreign_keys=ON;
                 CREATE TABLE accounts(id TEXT PRIMARY KEY);
                 CREATE TABLE jobs_applications(id TEXT PRIMARY KEY, account_id TEXT NOT NULL);
                 INSERT INTO accounts VALUES('owner'),('other');
                 INSERT INTO jobs_applications VALUES('application','owner');",
            )?;
            conn.execute_batch(include_str!(
                "../../../../infra/sqlite/server-runtime/070_jobs_application_answer_revisions.sql"
            ))
        });
        DbPool::Sqlite(
            r2d2::Pool::builder()
                .max_size(1)
                .build(crate::db::SafeSqliteConnectionManager::new(manager))
                .unwrap(),
        )
    }

    fn revision(id: &str, intervention: &str) -> ApplicationAnswerRevision {
        ApplicationAnswerRevision {
            id: id.into(),
            application_id: "application".into(),
            intervention_id: intervention.into(),
            previous_answers: vec![json!({"question":"private question","value":"private before"})],
            revised_answers: vec![json!({"question":"private question","value":"private after"})],
            invalidated_packet_checksum: Some("checksum".into()),
            revised_at_ms: 1,
        }
    }

    #[test]
    fn encrypted_history_is_owned_immutable_and_survives_application_removal() {
        let pool = pool();
        let expected = revision("r1", "i1");
        {
            let mut conn = pool.get().unwrap();
            let tx = conn.transaction().unwrap();
            persist_application_answer_revision_sqlite_tx(&tx, "owner", &expected).unwrap();
            tx.commit().unwrap();
            let raw: String = conn
                .query_row(
                    "SELECT revision_json FROM jobs_application_answer_revisions WHERE id='r1'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert!(
                !raw.contains("private question")
                    && !raw.contains("private before")
                    && !raw.contains("private after")
            );
            assert!(conn
                .execute(
                    "UPDATE jobs_application_answer_revisions SET revised_at_ms=2 WHERE id='r1'",
                    []
                )
                .is_err());
            assert!(conn
                .execute(
                    "DELETE FROM jobs_application_answer_revisions WHERE id='r1'",
                    []
                )
                .is_err());
            for setting in ["OFF", "ON"] {
                conn.execute_batch(&format!("PRAGMA recursive_triggers={setting}"))
                    .unwrap();
                for (id, intervention) in [("r1", "other"), ("other", "i1")] {
                    assert!(conn.execute(
                        "INSERT OR REPLACE INTO jobs_application_answer_revisions VALUES(?1,'owner','application',?2,'x',1)",
                        params![id, intervention],
                    ).is_err());
                }
            }
            conn.execute("DELETE FROM jobs_applications WHERE id='application'", [])
                .unwrap();
        }
        assert_eq!(
            get_application_answer_revision(&pool, "owner", "r1").unwrap(),
            Some(expected.clone())
        );
        assert!(get_application_answer_revision(&pool, "other", "r1")
            .unwrap()
            .is_none());
        assert_eq!(
            export_application_answer_revisions(&pool, "owner").unwrap(),
            vec![expected]
        );
        assert!(export_application_answer_revisions(&pool, "other")
            .unwrap()
            .is_empty());
        pool.get()
            .unwrap()
            .execute("DELETE FROM accounts WHERE id='owner'", [])
            .unwrap();
        assert!(get_application_answer_revision(&pool, "owner", "r1")
            .unwrap()
            .is_none());
    }

    #[test]
    fn revision_insert_and_related_application_change_roll_back_together() {
        let pool = pool();
        let mut conn = pool.get().unwrap();
        let tx = conn.transaction().unwrap();
        assert!(persist_application_answer_revision_sqlite_tx(
            &tx,
            "other",
            &revision("bad", "i0")
        )
        .is_err());
        persist_application_answer_revision_sqlite_tx(&tx, "owner", &revision("r1", "i1")).unwrap();
        tx.execute(
            "UPDATE jobs_applications SET id='changed' WHERE id='application'",
            [],
        )
        .unwrap();
        tx.rollback().unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM jobs_application_answer_revisions",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let app: String = conn
            .query_row("SELECT id FROM jobs_applications", [], |r| r.get(0))
            .unwrap();
        assert_eq!((count, app.as_str()), (0, "application"));
    }

    #[test]
    fn revision_payload_rejects_oversize_without_echoing_values() {
        let mut item = revision("r", "i");
        item.revised_answers = vec![json!({"value":"x".repeat(1024 * 1024)})];
        let error = answer_revision_payload(&item).unwrap_err();
        assert!(error
            .to_string()
            .contains("application question revision is invalid"));
        assert!(!error.to_string().contains(&"x".repeat(100)));
    }

    #[test]
    fn revision_exact_plaintext_size_limit_round_trips() {
        let mut item = revision("boundary", "boundary-intervention");
        item.revised_answers = vec![json!({"value":""})];
        let baseline = serde_json::to_vec(&item).unwrap().len();
        item.revised_answers = vec![json!({"value":"x".repeat(1024 * 1024 - baseline)})];
        assert_eq!(serde_json::to_vec(&item).unwrap().len(), 1024 * 1024);
        let payload = answer_revision_payload(&item).unwrap();
        assert!(payload.len() <= ANSWER_REVISION_RAW_LIMIT);
        let (decoded, bytes) = decode_answer_revision(
            &item.id,
            &item.application_id,
            &item.intervention_id,
            item.revised_at_ms,
            &payload,
        )
        .unwrap();
        assert_eq!(bytes, 1024 * 1024);
        assert!(decoded == item);
    }

    #[test]
    fn postgres_answer_revision_owned_history_and_account_cascade() {
        const CASE: &str = "db::jobs::application_answer_revision_tests::postgres_answer_revision_owned_history_and_account_cascade";
        let Some(url) = crate::db::owned_postgres_test::owned_url(CASE) else {
            return;
        };
        let pool = crate::db::open_postgres_pool(&url).unwrap();
        crate::db::run_migrations(&pool).unwrap();
        // SQL may commit before an operator records its separate migration ledger.
        // Replaying the exact migration must preserve all guards without duplicate-trigger errors.
        {
            let mut conn = pool.get_pg().unwrap();
            let mut tx = conn.transaction().unwrap();
            tx.batch_execute(include_str!(
                "../../../../infra/postgres/server-runtime/048_jobs_application_answer_revisions.sql"
            )).unwrap();
            tx.commit().unwrap();
        }
        let prior = vec![json!({"key":"question","question":"Question?","value":"before"})];
        let application: JobApplication = serde_json::from_value(json!({
            "id":"pg-application","job_id":"pg-job","state":"needs_input",
            "answers":prior,"receipt":{"approved_execution":{"checksum":"old-checksum"}}
        }))
        .unwrap();
        let intervention: Intervention = serde_json::from_value(json!({
            "id":"pg-intervention","application_id":"pg-application","kind":"unknown_question",
            "status":"open","title":"Question?","resolution_kind":"answer",
            "metadata":{"receipt":{"intervention":{"field":"Question?","question":"Question?"}}}
        }))
        .unwrap();
        {
            let mut conn = pool.get_pg().unwrap();
            conn.execute(
                "INSERT INTO accounts(id,email,password_hash) VALUES($1,$2,$3)",
                &[&"pg-owner", &"pg-owner@example.test", &"hash"],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO accounts(id,email,password_hash) VALUES($1,$2,$3)",
                &[&"pg-other", &"pg-other@example.test", &"hash"],
            )
            .unwrap();
            conn.execute("INSERT INTO jobs_postings(id,account_id,canonical_key,posting_json,source,company,title,created_at_ms,updated_at_ms)
                VALUES($1,$2,$3,$4,$5,$6,$7,0,0)",
                &[&"pg-job", &"pg-owner", &"synthetic-key", &"{}", &"synthetic", &"Synthetic", &"Role"]).unwrap();
            let app_payload = to_json(&application, "job application").unwrap();
            conn.execute("INSERT INTO jobs_applications(id,account_id,job_id,state,application_json,created_at_ms,updated_at_ms)
                VALUES($1,$2,$3,$4,$5,0,0)",
                &[&"pg-application", &"pg-owner", &"pg-job", &"needs_input", &app_payload]).unwrap();
            let intervention_payload = to_json(&intervention, "intervention").unwrap();
            conn.execute("INSERT INTO jobs_interventions(id,account_id,application_id,kind,status,intervention_json,created_at_ms)
                VALUES($1,$2,$3,$4,$5,$6,0)",
                &[&"pg-intervention", &"pg-owner", &"pg-application", &"unknown_question", &"open", &intervention_payload]).unwrap();
        }
        // Exercise the real resolver's fence after an authenticated request could
        // already have passed middleware. No answer, history or approval may change.
        {
            let mut conn = pool.get_pg().unwrap();
            conn.execute("INSERT INTO account_deletion_intents(account_id,requested_at_ms,last_checked_at_ms,fresh_upload_cutoff_ms)
                VALUES($1,0,0,0)", &[&"pg-owner"]).unwrap();
        }
        let fenced =
            resolve_intervention_answer_for_review(&pool, "pg-owner", "pg-intervention", "after")
                .err()
                .unwrap();
        assert!(matches!(
            fenced.downcast_ref::<crate::db::object_uploads::UploadControlError>(),
            Some(crate::db::object_uploads::UploadControlError::AccountDeleting)
        ));
        assert!(export_application_answer_revisions(&pool, "pg-owner")
            .unwrap()
            .is_empty());
        {
            let mut conn = pool.get_pg().unwrap();
            let stored: String = conn
                .query_one(
                    "SELECT application_json FROM jobs_applications WHERE id=$1",
                    &[&"pg-application"],
                )
                .unwrap()
                .get(0);
            let unchanged: JobApplication = parse_json(stored, "job application").unwrap();
            assert_eq!(
                serde_json::to_value(unchanged).unwrap(),
                serde_json::to_value(&application).unwrap()
            );
            let status: String = conn
                .query_one(
                    "SELECT status FROM jobs_interventions WHERE id=$1",
                    &[&"pg-intervention"],
                )
                .unwrap()
                .get(0);
            assert_eq!(status, "open");
            // Test-fixture reset only; production deletion is never reversed here.
            conn.execute(
                "DELETE FROM account_deletion_intents WHERE account_id=$1",
                &[&"pg-owner"],
            )
            .unwrap();
        }
        let result =
            resolve_intervention_answer_for_review(&pool, "pg-owner", "pg-intervention", "after")
                .unwrap();
        let revision = result.answer_revision;
        assert!(revision.previous_answers == application.answers);
        assert!(revision.revised_answers == result.application.answers);
        assert_eq!(
            revision.invalidated_packet_checksum.as_deref(),
            Some("old-checksum")
        );
        assert_eq!(
            result
                .application
                .receipt
                .pointer("/packet_revision/answer_revision_id")
                .and_then(Value::as_str),
            Some(revision.id.as_str())
        );
        assert!(
            get_application_answer_revision(&pool, "pg-owner", &revision.id).unwrap()
                == Some(revision.clone())
        );
        assert!(
            get_application_answer_revision(&pool, "pg-other", &revision.id)
                .unwrap()
                .is_none()
        );
        let exported = account_export(&pool, "pg-owner", "pg-owner@example.test")
            .unwrap()
            .unwrap();
        assert!(exported.workspace.is_none());
        assert!(exported.application_answer_revisions == vec![revision.clone()]);
        {
            let mut conn = pool.get_pg().unwrap();
            let raw: String = conn
                .query_one(
                    "SELECT revision_json FROM jobs_application_answer_revisions WHERE id=$1",
                    &[&revision.id],
                )
                .unwrap()
                .get(0);
            assert!(!raw.contains("before") && !raw.contains("after"));
            assert!(conn
                .execute(
                    "UPDATE jobs_application_answer_revisions SET revised_at_ms=2 WHERE id=$1",
                    &[&revision.id]
                )
                .is_err());
            assert!(conn
                .execute(
                    "DELETE FROM jobs_application_answer_revisions WHERE id=$1",
                    &[&revision.id]
                )
                .is_err());
            for (id, intervention_id) in [
                (revision.id.as_str(), "other"),
                ("other", "pg-intervention"),
            ] {
                assert!(conn.execute("INSERT INTO jobs_application_answer_revisions(id,account_id,application_id,intervention_id,revision_json,revised_at_ms)
                    VALUES($1,$2,$3,$4,$5,1)", &[&id, &"pg-owner", &"pg-application", &intervention_id, &raw]).is_err());
            }
            conn.execute("DELETE FROM accounts WHERE id=$1", &[&"pg-owner"])
                .unwrap();
        }
        assert!(
            get_application_answer_revision(&pool, "pg-owner", &revision.id)
                .unwrap()
                .is_none()
        );
        println!("phase656_owned_answer_revision_postgres_complete");
    }
}

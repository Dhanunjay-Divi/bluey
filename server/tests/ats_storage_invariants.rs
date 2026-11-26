//! Database-level regressions, using disposable in-memory migrated SQLite only.
//! These exercise storage constraints, not signature verification or routes.

use bluey_server::db::{run_migrations, DbPool, SafeSqliteConnectionManager};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;

fn migrated_pool(recursive: bool) -> DbPool {
    let manager = SqliteConnectionManager::memory()
        .with_init(|conn| conn.execute_batch("PRAGMA foreign_keys = ON;"));
    let pool = DbPool::Sqlite(
        Pool::builder()
            .max_size(1)
            .build(SafeSqliteConnectionManager::from(manager))
            .unwrap(),
    );
    run_migrations(&pool).unwrap();
    let conn = pool.get().unwrap();
    conn.execute_batch(if recursive {
        "PRAGMA recursive_triggers = ON;"
    } else {
        "PRAGMA recursive_triggers = OFF;"
    })
    .unwrap();
    assert_eq!(
        conn.query_row("PRAGMA recursive_triggers", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        if recursive { 1 } else { 0 }
    );
    assert_eq!(
        conn.query_row("PRAGMA foreign_keys", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        1
    );
    drop(conn);
    pool
}

fn assert_fk_clean(conn: &Connection) {
    let mut query = conn.prepare("PRAGMA foreign_key_check").unwrap();
    assert!(query.query([]).unwrap().next().unwrap().is_none());
}

fn seed_browser_parent(conn: &Connection) {
    conn.execute_batch(
        "INSERT INTO jobs_browser_release_signature_sets
         VALUES (printf('%064x',1),'signatures',1,'release',
           'bluey-jobs-browser-release-manifest-v1',printf('%064x',2),0,1,'x','test',0);
         INSERT INTO jobs_browser_release_manifests
         VALUES (printf('%064x',2),'manifest',1,'release',1,'build','1',1,
           printf('%040x',3),'electron','playwright','chromium','https://example.test/notes',
           5,'x',printf('%064x',1),0,'test',0);
         INSERT INTO jobs_browser_release_artifacts
         VALUES ('artifact',printf('%064x',2),'darwin','arm64','darwin-dmg',printf('%064x',3),
           'x',printf('%086d',1),'key','https://example.test/artifact','artifact.dmg',1,
           printf('%064x',4),printf('%064x',5),printf('%064x',6),'apple-developer-id','test',0);",
    )
    .unwrap();
    assert_fk_clean(conn);
}

fn component_insert(verb: &str, automation_hash: u32) -> String {
    format!(
        "{verb} INTO jobs_browser_release_artifact_runtime_components VALUES (
          printf('%064x',2),'artifact',printf('%064x',3),printf('%064x',4),
          'darwin','arm64','darwin-dmg',printf('%064x',{automation_hash}),printf('%064x',8),0)"
    )
}

#[test]
fn sqlite_runtime_component_replacement_fails_with_recursive_triggers_off_and_on() {
    for recursive in [false, true] {
        let pool = migrated_pool(recursive);
        let conn = pool.get().unwrap();
        seed_browser_parent(&conn);
        conn.execute_batch(&component_insert("INSERT", 7)).unwrap();
        // Existing fixture code uses OR IGNORE for byte-identical replay.
        conn.execute_batch(&component_insert("INSERT OR IGNORE", 7))
            .unwrap();
        assert!(conn
            .execute_batch(&component_insert("INSERT OR REPLACE", 9))
            .is_err());
        let stored: String = conn.query_row(
            "SELECT automation_bundle_sha256 FROM jobs_browser_release_artifact_runtime_components",
            [], |row| row.get(0),
        ).unwrap();
        assert_eq!(stored, format!("{:064x}", 7));
        assert_fk_clean(&conn);
    }
}

#[test]
fn sqlite_runtime_component_alternate_unique_replace_is_fenced_independently_of_fk() {
    // Two valid parents cannot share this target: the parent itself has the
    // same target uniqueness. Disable FK only for this isolated trigger test;
    // production-like FK-on primary-key coverage is above.
    let pool = migrated_pool(false);
    let conn = pool.get().unwrap();
    seed_browser_parent(&conn);
    conn.execute_batch(&component_insert("INSERT", 7)).unwrap();
    conn.execute_batch("PRAGMA foreign_keys = OFF").unwrap();
    assert_eq!(
        conn.query_row("PRAGMA foreign_keys", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    let alternate =
        component_insert("INSERT OR REPLACE", 9).replace("'artifact'", "'other-artifact'");
    assert!(conn.execute_batch(&alternate).is_err());
    conn.execute_batch("PRAGMA foreign_keys = ON").unwrap();
    assert_fk_clean(&conn);
    assert_eq!(
        conn.query_row(
            "SELECT artifact_id FROM jobs_browser_release_artifact_runtime_components",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "artifact"
    );
}

fn seed_circuit_event(conn: &Connection, id: &str, scope: &str, subject: &str, transition: &str) {
    conn.execute(
        "INSERT INTO jobs_ats_certification_circuit_events VALUES
        (?1,printf('%064x',?2),'x',?3,?4,?5,'evidence_failure',0,0,0,0,0,'test',0,'test',0)",
        rusqlite::params![
            id,
            match id {
                "first" => 1,
                "next" => 2,
                "other" => 3,
                _ => 4,
            },
            scope,
            subject,
            transition
        ],
    )
    .unwrap();
}

fn seed_circuits(conn: &Connection) {
    seed_circuit_event(conn, "first", "provider", "a", "opened");
    seed_circuit_event(conn, "next", "provider", "a", "held");
    seed_circuit_event(conn, "other", "provider", "b", "opened");
    conn.execute_batch(
        "INSERT INTO jobs_ats_certification_circuit_heads VALUES ('provider','a',1,\
        'first','opened',0)",
    )
    .unwrap();
    assert_fk_clean(conn);
}

#[test]
fn sqlite_circuit_head_scope_and_event_projection_are_bound() {
    let pool = migrated_pool(false);
    let conn = pool.get().unwrap();
    seed_circuits(&conn);
    assert!(conn
        .execute_batch(
            "INSERT INTO jobs_ats_certification_circuit_heads VALUES ('runtime','x',1,\
            'other','opened',0)"
        )
        .is_err());
    assert!(conn
        .execute_batch(
            "INSERT INTO jobs_ats_certification_circuit_heads VALUES ('provider','b',1,\
            'other','closed',0)"
        )
        .is_err());
    // The destination event matches the new scope, isolating key immutability
    // from the independent event/scope projection guard.
    assert!(conn
        .execute_batch(
            "UPDATE jobs_ats_certification_circuit_heads SET subject_key='b',\
            head_revision=2,current_event_id='other',state='opened' WHERE \
            scope_kind='provider' AND subject_key='a'"
        )
        .is_err());
    assert!(conn
        .execute_batch(
            "UPDATE jobs_ats_certification_circuit_heads SET head_revision=2,\
            current_event_id='other',state='opened' WHERE scope_kind='provider' AND subject_key='a'"
        )
        .is_err());
    assert!(conn
        .execute_batch(
            "UPDATE jobs_ats_certification_circuit_heads SET head_revision=2,\
            current_event_id='next',state='closed' WHERE scope_kind='provider' AND subject_key='a'"
        )
        .is_err());
    assert_eq!(
        conn.execute(
            "UPDATE jobs_ats_certification_circuit_heads SET head_revision=2,\
            current_event_id='next',state='held' WHERE scope_kind='provider' AND \
            subject_key='a' AND head_revision=1 AND current_event_id='first'",
            []
        )
        .unwrap(),
        1
    );
    assert_fk_clean(&conn);
}

#[test]
fn sqlite_circuit_head_replace_cannot_bypass_monotonicity_or_unique_event() {
    for recursive in [false, true] {
        let pool = migrated_pool(recursive);
        let conn = pool.get().unwrap();
        seed_circuits(&conn);
        assert!(conn
            .execute_batch(
                "INSERT OR REPLACE INTO jobs_ats_certification_circuit_heads VALUES \
                ('provider','a',1,'next','held',0)"
            )
            .is_err());
        assert!(conn
            .execute_batch(
                "INSERT OR REPLACE INTO jobs_ats_certification_circuit_heads VALUES \
                ('provider','b',1,'first','opened',0)"
            )
            .is_err());
        let row: (i64, String) = conn
            .query_row(
                "SELECT head_revision,current_event_id FROM \
                jobs_ats_certification_circuit_heads WHERE scope_kind='provider' AND \
                subject_key='a'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(row, (1, "first".to_string()));
        assert_fk_clean(&conn);
    }
}

#[test]
fn sqlite_fence_migration_refuses_existing_inconsistent_circuit_head_without_repairing_it() {
    let pool = migrated_pool(false);
    let conn = pool.get().unwrap();
    seed_circuits(&conn);
    // Simulate a database created before the new insert fence, whether this
    // integration test itself is run before or after migration registration.
    conn.execute_batch("DROP TRIGGER IF EXISTS trg_jobs_ats_circuit_event_scope_insert;")
        .unwrap();
    conn.execute_batch(
        "INSERT INTO jobs_ats_certification_circuit_heads VALUES ('runtime','x',1,\
        'other','opened',0)",
    )
    .unwrap();
    assert_fk_clean(&conn); // Legacy FK checks event ID, not its scope.
    let error = conn
        .execute_batch(include_str!(
            "../../infra/sqlite/server-runtime/069_jobs_ats_storage_fences.sql"
        ))
        .unwrap_err();
    assert!(
        error.to_string().contains("CHECK constraint failed"),
        "{error}"
    );
    conn.execute_batch("ROLLBACK;").unwrap();
    let count: i64 = conn
        .query_row(
            "SELECT count(*) FROM jobs_ats_certification_circuit_heads WHERE \
            scope_kind='runtime' AND subject_key='x' AND current_event_id='other'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 1, "preflight must not silently repair legacy state");
    assert_fk_clean(&conn);
}

fn seed_ats_policy(conn: &Connection) {
    conn.execute_batch(
        "INSERT INTO jobs_ats_certification_trust_policies (
          policy_sha256,policy_id,trust_generation,maximum_clock_skew_ms,
          maximum_manifest_size_bytes,maximum_target_count,maximum_observation_count,
          maximum_evidence_object_count,canonical_policy_base64url,authorization_sha256,
          root_trust_anchor_sha256,canonical_authorization_base64url,issued_at_ms,
          valid_from_ms,expires_at_ms,recorded_by,recorded_at_ms)
        VALUES (printf('%064x',10),'policy',1,0,1024,1,1,1,'x',printf('%064x',11),
          printf('%064x',12),'x',0,0,100,'test',0);",
    )
    .unwrap();
}

fn quarantine_command(conn: &Connection, hash: u32, scope_id: &str, sequence: u32) {
    conn.execute(
        "INSERT INTO jobs_ats_certification_quarantine_commands (
          command_sha256,command_id,command_generation,scope_kind,scope_id,scope_sha256,
          command_sequence,action,reason_ref,canonical_command_base64url,authorization_sha256,
          trust_policy_sha256,canonical_authorization_base64url,issued_at_ms,recorded_by,\
          recorded_at_ms)
        VALUES (printf('%064x',?1),?2,1,'provider',?3,printf('%064x',?4),?5,
          'quarantine','reason','x',printf('%064x',?1),printf('%064x',10),'x',0,'test',0)",
        rusqlite::params![
            hash,
            format!("command-{hash}"),
            scope_id,
            if scope_id == "a" { 20 } else { 21 },
            sequence
        ],
    )
    .unwrap();
}

#[test]
fn sqlite_quarantine_head_rejects_scope_move_with_valid_destination_command() {
    let pool = migrated_pool(false);
    let conn = pool.get().unwrap();
    seed_ats_policy(&conn);
    quarantine_command(&conn, 30, "a", 1);
    quarantine_command(&conn, 31, "a", 2);
    quarantine_command(&conn, 32, "b", 2);
    assert!(conn
        .execute_batch(
            "INSERT INTO jobs_ats_certification_quarantine_heads VALUES ('provider','a',\
            printf('%064x',20),1,printf('%064x',30),1,'released','test',0)"
        )
        .is_err());
    conn.execute_batch(
        "INSERT INTO jobs_ats_certification_quarantine_heads VALUES ('provider','a',\
        printf('%064x',20),1,printf('%064x',30),1,'quarantined','test',0)",
    )
    .unwrap();
    assert_fk_clean(&conn);
    assert!(conn
        .execute_batch(
            "UPDATE jobs_ats_certification_quarantine_heads SET scope_id='b',\
            scope_sha256=printf('%064x',21),head_revision=2,\
            current_command_sha256=printf('%064x',32),current_command_sequence=2 WHERE \
            scope_kind='provider' AND scope_id='a'"
        )
        .is_err());
    assert!(conn
        .execute_batch(
            "UPDATE jobs_ats_certification_quarantine_heads SET head_revision=2,\
            current_command_sha256=printf('%064x',31),current_command_sequence=2,\
            state='released' WHERE scope_kind='provider' AND scope_id='a'"
        )
        .is_err());
    assert_eq!(
        conn.execute(
            "UPDATE jobs_ats_certification_quarantine_heads SET head_revision=2,\
            current_command_sha256=printf('%064x',31),current_command_sequence=2 WHERE \
            scope_kind='provider' AND scope_id='a' AND head_revision=1",
            []
        )
        .unwrap(),
        1
    );
    assert_fk_clean(&conn);
}

fn seed_ats_manifest(conn: &Connection) {
    conn.execute_batch(
        "INSERT INTO jobs_ats_certification_manifests (
          manifest_sha256,certification_id,manifest_generation,provider,target_key,
          allowed_provider_hosts_json,variant_key,surface_sha256,scope_sha256,adapter_version,
          final_submit_control_id,adapter_bundle_sha256,source_commit,layout_contract_version,
          layout_contract_sha256,maximum_capability,layout_set_sha256,suite_id,suite_version,
          suite_manifest_sha256,layout_observation_count,check_result_count,hard_filter_violations,
          unsupported_factual_claims,duplicate_submit_activations,false_submitted_states,
          incomplete_receipts,pii_bearing_observations,evidence_count,runtime_target_count,
          canonical_manifest_base64url,authorization_sha256,trust_policy_sha256,
          canonical_authorization_base64url,tested_at_ms,issued_at_ms,not_before_ms,expires_at_ms,
          recorded_by,recorded_at_ms)
        VALUES (printf('%064x',40),'cert',1,'greenhouse','target','[]','variant',printf('%064x',41),
          printf('%064x',20),'version','submit',printf('%064x',42),printf('%040x',43),1,
          printf('%064x',44),'observe_only',printf('%064x',45),'suite','v1',printf('%064x',46),
          1,1,0,0,0,0,0,0,1,1,'x',printf('%064x',47),printf('%064x',10),'x',0,0,0,100,'test',0);",
    )
    .unwrap();
}

fn certification_activation(conn: &Connection, hash: u32, scope: u32, sequence: u32) {
    conn.execute(
        "INSERT INTO jobs_ats_certification_activations (
          activation_sha256,activation_id,activation_generation,manifest_sha256,provider,
          adapter_version,scope_sha256,channel,channel_sequence,capability,canary_max_submissions,
          canary_account_cap,canary_concurrency_cap,canary_daily_side_effect_cap,approval_ref,
          canonical_activation_base64url,authorization_sha256,trust_policy_sha256,
          canonical_authorization_base64url,issued_at_ms,not_before_ms,expires_at_ms,\
          recorded_by,recorded_at_ms)
        VALUES (printf('%064x',?1),?2,?1,printf('%064x',40),'greenhouse','version',
          printf('%064x',?3),'shadow',?4,'observe_only',0,0,0,0,'approval','x',
          printf('%064x',?1),printf('%064x',10),'x',0,0,100,'test',0)",
        rusqlite::params![hash, format!("activation-{hash}"), scope, sequence],
    )
    .unwrap();
}

fn certification_transition(
    conn: &Connection,
    hash: u32,
    scope: u32,
    revision: u32,
    activation: u32,
) {
    let previous = if revision == 1 {
        "NULL"
    } else {
        "printf('%064x',50)"
    };
    let previous_activation = if revision == 1 {
        "NULL"
    } else {
        "printf('%064x',60)"
    };
    let sql = format!(
        "INSERT INTO jobs_ats_certification_head_transitions VALUES (
          printf('%064x',?1),printf('%064x',?2),'shadow',?3,?3-1,{previous},
          {previous_activation},printf('%064x',?4),?3,'test',0)"
    );
    conn.execute(&sql, rusqlite::params![hash, scope, revision, activation])
        .unwrap();
}

#[test]
fn sqlite_certification_head_rejects_scope_move_with_valid_destination_transition() {
    let pool = migrated_pool(false);
    let conn = pool.get().unwrap();
    seed_ats_policy(&conn);
    seed_ats_manifest(&conn);
    certification_activation(&conn, 60, 20, 1);
    certification_activation(&conn, 61, 20, 2);
    certification_activation(&conn, 62, 21, 2);
    certification_activation(&conn, 63, 21, 1);
    certification_transition(&conn, 50, 20, 1, 60);
    certification_transition(&conn, 51, 20, 2, 61);
    certification_transition(&conn, 52, 21, 2, 62);
    certification_transition(&conn, 53, 22, 1, 63);
    assert!(conn
        .execute_batch(
            "INSERT INTO jobs_ats_certification_heads VALUES (printf('%064x',22),\
            'shadow',1,printf('%064x',53),printf('%064x',63),1,'test',0)"
        )
        .is_err());
    assert!(conn
        .execute_batch(
            "INSERT INTO jobs_ats_certification_heads VALUES (printf('%064x',20),\
            'shadow',1,printf('%064x',50),printf('%064x',60),2,'test',0)"
        )
        .is_err());
    conn.execute_batch(
        "INSERT INTO jobs_ats_certification_heads VALUES (printf('%064x',20),'shadow',1,\
        printf('%064x',50),printf('%064x',60),1,'test',0)",
    )
    .unwrap();
    assert_fk_clean(&conn);
    assert!(conn
        .execute_batch(
            "UPDATE jobs_ats_certification_heads SET scope_sha256=printf('%064x',21),\
            head_revision=2,current_transition_sha256=printf('%064x',52),\
            current_activation_sha256=printf('%064x',62),current_channel_sequence=2 \
            WHERE scope_sha256=printf('%064x',20) AND channel='shadow'"
        )
        .is_err());
    assert!(conn
        .execute_batch(
            "UPDATE jobs_ats_certification_heads SET head_revision=2,\
            current_transition_sha256=printf('%064x',51),\
            current_activation_sha256=printf('%064x',61),current_channel_sequence=3 \
            WHERE scope_sha256=printf('%064x',20) AND channel='shadow'"
        )
        .is_err());
    assert_eq!(
        conn.execute(
            "UPDATE jobs_ats_certification_heads SET head_revision=2,\
            current_transition_sha256=printf('%064x',51),\
            current_activation_sha256=printf('%064x',61),current_channel_sequence=2 \
            WHERE scope_sha256=printf('%064x',20) AND channel='shadow' AND head_revision=1",
            []
        )
        .unwrap(),
        1
    );
    assert_fk_clean(&conn);
}

#[test]
fn sqlite_trust_head_projection_rejects_mixed_generation_and_preserves_cas() {
    let pool = migrated_pool(false);
    let conn = pool.get().unwrap();
    seed_ats_policy(&conn);
    conn.execute_batch(
        "INSERT INTO jobs_ats_certification_trust_policies (
      policy_sha256,policy_id,trust_generation,predecessor_policy_sha256,maximum_clock_skew_ms,
      maximum_manifest_size_bytes,maximum_target_count,maximum_observation_count,\
      maximum_evidence_object_count,
      canonical_policy_base64url,authorization_sha256,root_trust_anchor_sha256,\
      canonical_authorization_base64url,
      issued_at_ms,valid_from_ms,expires_at_ms,recorded_by,recorded_at_ms)
      VALUES (printf('%064x',14),'policy2',2,printf('%064x',10),0,1024,1,1,1,'x',\
      printf('%064x',15),printf('%064x',12),'x',0,0,100,'test',0),
             (printf('%064x',16),'policy3',3,printf('%064x',14),0,1024,1,1,1,'x',\
             printf('%064x',17),printf('%064x',12),'x',0,0,100,'test',0);",
    )
    .unwrap();
    assert!(conn
        .execute_batch(
            "INSERT INTO jobs_ats_certification_trust_head VALUES (1,printf('%064x',10),\
            2,1,printf('%064x',12),'test',0)"
        )
        .is_err());
    assert!(conn
        .execute_batch(
            "INSERT INTO jobs_ats_certification_trust_head VALUES (1,printf('%064x',10),\
            1,1,printf('%064x',13),'test',0)"
        )
        .is_err());
    conn.execute_batch(
        "INSERT INTO jobs_ats_certification_trust_head VALUES (1,printf('%064x',10),1,1,\
        printf('%064x',12),'test',0)",
    )
    .unwrap();
    assert!(conn
        .execute_batch(
            "UPDATE jobs_ats_certification_trust_head SET \
            current_policy_sha256=printf('%064x',16),current_trust_generation=2,\
            head_revision=2 WHERE singleton_id=1"
        )
        .is_err());
    assert!(conn
        .execute_batch(
            "UPDATE jobs_ats_certification_trust_head SET \
            current_policy_sha256=printf('%064x',14),current_trust_generation=2,\
            head_revision=2,root_trust_anchor_sha256=printf('%064x',13) WHERE singleton_id=1"
        )
        .is_err());
    assert_eq!(
        conn.execute(
            "UPDATE jobs_ats_certification_trust_head SET \
            current_policy_sha256=printf('%064x',14),current_trust_generation=2,\
            head_revision=2 WHERE singleton_id=1 AND head_revision=1",
            []
        )
        .unwrap(),
        1
    );
    assert_fk_clean(&conn);
    drop(conn);
    run_migrations(&pool).expect("populated consistent SQLite replay");
}

#[test]
fn sqlite_fence_preflight_refuses_existing_fk_corruption_before_install_commit() {
    let pool = migrated_pool(false);
    let conn = pool.get().unwrap();
    conn.execute_batch(
        "DROP TRIGGER IF EXISTS trg_jobs_browser_runtime_component_insert_fence; PRAGMA \
        foreign_keys=OFF;",
    )
    .unwrap();
    conn.execute_batch(&component_insert("INSERT", 7)).unwrap();
    conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
    let error = conn
        .execute_batch(include_str!(
            "../../infra/sqlite/server-runtime/069_jobs_ats_storage_fences.sql"
        ))
        .unwrap_err();
    assert!(
        error.to_string().contains("CHECK constraint failed"),
        "{error}"
    );
    conn.execute_batch("ROLLBACK;").unwrap();
    assert!(conn.is_autocommit());
    let installed: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='trigger' AND \
            name='trg_jobs_browser_runtime_component_insert_fence'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(installed, 0, "failed preflight must not install a trigger");
    let count: i64 = conn
        .query_row(
            "SELECT count(*) FROM jobs_browser_release_artifact_runtime_components",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1, "preflight must preserve the legacy row");
    assert_eq!(
        conn.query_row("PRAGMA foreign_keys", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn sqlite_managed_pool_enables_recursive_guards_on_every_connection() {
    let root = std::path::PathBuf::from(
        std::env::var_os("BLUEY_TEST_RUN_ROOT").expect("canonical test launcher required"),
    );
    assert_eq!(std::fs::canonicalize(&root).unwrap(), root);
    let parent = root.parent().expect("owned test parent");
    let temp_root = std::fs::canonicalize("/tmp").expect("canonical local temporary root");
    assert_eq!(parent.parent(), Some(temp_root.as_path()));
    assert!(parent
        .file_name()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.starts_with("bluey-test-runs-") && s.len() > "bluey-test-runs-".len()));
    let path = root.join(format!("ats-storage-{}.sqlite", uuid::Uuid::new_v4()));
    assert!(!path.exists());
    let pool = bluey_server::db::open_pool(&path).unwrap();
    run_migrations(&pool).unwrap();
    let first = pool.get().unwrap();
    let second = pool.get().unwrap();
    for conn in [&*first, &*second] {
        assert_eq!(
            conn.query_row("PRAGMA recursive_triggers", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            conn.query_row("PRAGMA foreign_keys", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
    seed_circuit_event(&first, "first", "provider", "a", "opened");
    let replacement = first.execute_batch(
        "INSERT OR REPLACE INTO jobs_ats_certification_circuit_events VALUES ('first',\
        printf('%064x',99),'changed','provider','a','opened','evidence_failure',0,0,\
        0,0,0,'test',0,'test',0)",
    );
    assert!(replacement.is_err());
    let original: String = first
        .query_row(
            "SELECT canonical_event_base64url FROM jobs_ats_certification_circuit_events \
            WHERE event_id='first'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(original, "x");
}

//! Storage regression qualification against the canonical disposable PostgreSQL wrapper only.
#[cfg(unix)]
#[path = "../src/db/owned_postgres_test.rs"]
mod owned_postgres_test;

#[cfg(unix)]
mod phase654_postgres {
    use super::owned_postgres_test::private_file;
    use bluey_server::db::{open_postgres_pool, run_migrations};
    use postgres::{Client, NoTls};
    use sha2::{Digest, Sha256};
    use std::fs;
    use std::io::{Read, Write};
    use std::os::unix::fs::MetadataExt;
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    use std::path::{Path, PathBuf};
    use std::process::{Command, ExitStatus, Stdio};
    use std::time::{Duration, Instant};

    const MIGRATION: &str = "047_jobs_ats_storage_fences.sql";
    const CASE: &str =
        "phase654_postgres::postgres_owned_circuit_projection_scope_cas_and_legacy_preflight";
    const CUSTOM_CASE: &str =
        "phase654_postgres::postgres_owned_custom_ledger_full_bootstrap_and_replay";
    const ENV_CASE: &str =
        "phase654_postgres::postgres_owned_envfile_ledger_full_bootstrap_and_replay";
    const AMBIENT_CASE: &str =
        "phase654_postgres::postgres_owned_rust_ambient_ledger_rollback_and_replay";
    const OPERATOR_SQL: &str =
        include_str!("../../infra/postgres/server-runtime/047_jobs_ats_storage_fences.sql");
    // Compile-time exact public migration inputs; runtime source must match each.
    const FULL_OPERATOR_SQL: &[(&str, &[u8])] = &[
        ("001_server_runtime_compat.sql", include_bytes!("../../infra/postgres/server-runtime/001_server_runtime_compat.sql")),
        ("002_jobs.sql", include_bytes!("../../infra/postgres/server-runtime/002_jobs.sql")),
        ("002_usage_reservations.sql", include_bytes!("../../infra/postgres/server-runtime/002_usage_reservations.sql")),
        ("003_object_upload_controls.sql", include_bytes!("../../infra/postgres/server-runtime/003_object_upload_controls.sql")),
        ("004_stripe_auto_reload.sql", include_bytes!("../../infra/postgres/server-runtime/004_stripe_auto_reload.sql")),
        ("005_jobs_candidate_events.sql", include_bytes!("../../infra/postgres/server-runtime/005_jobs_candidate_events.sql")),
        ("006_context_artifact_revisions.sql", include_bytes!("../../infra/postgres/server-runtime/006_context_artifact_revisions.sql")),
        ("007_jobs_resume_generations.sql", include_bytes!("../../infra/postgres/server-runtime/007_jobs_resume_generations.sql")),
        ("008_jobs_generation_allowance.sql", include_bytes!("../../infra/postgres/server-runtime/008_jobs_generation_allowance.sql")),
        ("009_jobs_discovery_board_owner.sql", include_bytes!("../../infra/postgres/server-runtime/009_jobs_discovery_board_owner.sql")),
        ("010_provider_usage_provenance.sql", include_bytes!("../../infra/postgres/server-runtime/010_provider_usage_provenance.sql")),
        ("011_jobs_candidate_evidence.sql", include_bytes!("../../infra/postgres/server-runtime/011_jobs_candidate_evidence.sql")),
        ("012_jobs_global_candidate_index.sql", include_bytes!("../../infra/postgres/server-runtime/012_jobs_global_candidate_index.sql")),
        ("013_jobs_resume_source_assets.sql", include_bytes!("../../infra/postgres/server-runtime/013_jobs_resume_source_assets.sql")),
        ("014_jobs_provider_connections.sql", include_bytes!("../../infra/postgres/server-runtime/014_jobs_provider_connections.sql")),
        ("015_jobs_mailbox_sync.sql", include_bytes!("../../infra/postgres/server-runtime/015_jobs_mailbox_sync.sql")),
        ("016_jobs_global_ingestion_quarantine.sql", include_bytes!("../../infra/postgres/server-runtime/016_jobs_global_ingestion_quarantine.sql")),
        ("017_jobs_global_candidate_archive.sql", include_bytes!("../../infra/postgres/server-runtime/017_jobs_global_candidate_archive.sql")),
        ("018_jobs_auto_submit_authorizations.sql", include_bytes!("../../infra/postgres/server-runtime/018_jobs_auto_submit_authorizations.sql")),
        ("019_jobs_communication_actions.sql", include_bytes!("../../infra/postgres/server-runtime/019_jobs_communication_actions.sql")),
        ("020_jobs_browser_profile_snapshots.sql", include_bytes!("../../infra/postgres/server-runtime/020_jobs_browser_profile_snapshots.sql")),
        ("021_account_deletion_intents.sql", include_bytes!("../../infra/postgres/server-runtime/021_account_deletion_intents.sql")),
        ("022_jobs_submission_evidence_reservations.sql", include_bytes!("../../infra/postgres/server-runtime/022_jobs_submission_evidence_reservations.sql")),
        ("023_jobs_account_object_upload_backfill.sql", include_bytes!("../../infra/postgres/server-runtime/023_jobs_account_object_upload_backfill.sql")),
        ("024_jobs_runner_volume_purge.sql", include_bytes!("../../infra/postgres/server-runtime/024_jobs_runner_volume_purge.sql")),
        ("025_jobs_browser_release_authority.sql", include_bytes!("../../infra/postgres/server-runtime/025_jobs_browser_release_authority.sql")),
        ("026_jobs_ats_certification_authority.sql", include_bytes!("../../infra/postgres/server-runtime/026_jobs_ats_certification_authority.sql")),
        ("027_jobs_browser_release_runtime_components.sql", include_bytes!("../../infra/postgres/server-runtime/027_jobs_browser_release_runtime_components.sql")),
        ("028_jobs_runner_process_runtime_authority.sql", include_bytes!("../../infra/postgres/server-runtime/028_jobs_runner_process_runtime_authority.sql")),
        ("029_jobs_communication_execution.sql", include_bytes!("../../infra/postgres/server-runtime/029_jobs_communication_execution.sql")),
        ("030_jobs_operational_holds.sql", include_bytes!("../../infra/postgres/server-runtime/030_jobs_operational_holds.sql")),
        ("031_jobs_workflow_commands.sql", include_bytes!("../../infra/postgres/server-runtime/031_jobs_workflow_commands.sql")),
        ("032_jobs_workflow_cleanup_authority.sql", include_bytes!("../../infra/postgres/server-runtime/032_jobs_workflow_cleanup_authority.sql")),
        ("033_jobs_managed_cloud_release_authority.sql", include_bytes!("../../infra/postgres/server-runtime/033_jobs_managed_cloud_release_authority.sql")),
        ("034_jobs_canonical_taxonomy_authority.sql", include_bytes!("../../infra/postgres/server-runtime/034_jobs_canonical_taxonomy_authority.sql")),
        ("035_jobs_original_source_verification_authority.sql", include_bytes!("../../infra/postgres/server-runtime/035_jobs_original_source_verification_authority.sql")),
        ("036_jobs_signed_job_integrity_authority.sql", include_bytes!("../../infra/postgres/server-runtime/036_jobs_signed_job_integrity_authority.sql")),
        ("038_jobs_public_beta_access.sql", include_bytes!("../../infra/postgres/server-runtime/038_jobs_public_beta_access.sql")),
        ("039_jobs_workspace_pagination.sql", include_bytes!("../../infra/postgres/server-runtime/039_jobs_workspace_pagination.sql")),
        ("040_jobs_original_source_verifier_startup_authority.sql", include_bytes!("../../infra/postgres/server-runtime/040_jobs_original_source_verifier_startup_authority.sql")),
        ("041_jobs_source_verifier_boot_generation_authority.sql", include_bytes!("../../infra/postgres/server-runtime/041_jobs_source_verifier_boot_generation_authority.sql")),
        ("042_jobs_source_verifier_heartbeat_boot_generation_fence.sql", include_bytes!("../../infra/postgres/server-runtime/042_jobs_source_verifier_heartbeat_boot_generation_fence.sql")),
        ("043_jobs_source_verifier_heartbeat_boot_bindings.sql", include_bytes!("../../infra/postgres/server-runtime/043_jobs_source_verifier_heartbeat_boot_bindings.sql")),
        ("044_jobs_attempt_employer_identity.sql", include_bytes!("../../infra/postgres/server-runtime/044_jobs_attempt_employer_identity.sql")),
        ("045_jobs_provisioned_runtime_startup.sql", include_bytes!("../../infra/postgres/server-runtime/045_jobs_provisioned_runtime_startup.sql")),
        ("046_jobs_provisioned_runtime_observations.sql", include_bytes!("../../infra/postgres/server-runtime/046_jobs_provisioned_runtime_observations.sql")),
        ("047_jobs_ats_storage_fences.sql", include_bytes!("../../infra/postgres/server-runtime/047_jobs_ats_storage_fences.sql")),
        ("048_jobs_application_answer_revisions.sql", include_bytes!("../../infra/postgres/server-runtime/048_jobs_application_answer_revisions.sql")),
    ];
    // This marker is physically present in the current source directory, but it
    // is deliberately not a legacy full-operator SQL fixture input. Production
    // applies it only through the guarded Rust selected-migration operator.
    const GUARDED_OPERATOR_SOURCE_MARKER: (&str, &[u8]) = (
        "049_jobs_process_attachment_signature_audience.sql",
        include_bytes!("../../infra/postgres/server-runtime/049_jobs_process_attachment_signature_audience.sql"),
    );

    struct RunAuthority {
        device: u64,
        inode: u64,
        uid: u32,
        marker: String,
    }

    unsafe extern "C" {
        fn getpgrp() -> std::os::raw::c_int;
    }

    fn current_group() -> Option<i32> {
        // SAFETY: POSIX getpgrp takes no arguments and only reads the caller's
        // current process-group identity; it neither accesses memory nor changes state.
        let group = unsafe { getpgrp() };
        (group > 1).then_some(group)
    }

    fn abort_owned_group(root: &Path, authority: &RunAuthority) -> ! {
        eprintln!("phase654_operator_cli_timeout");
        let group = std::panic::catch_unwind(|| {
            let current = fs::symlink_metadata(root).ok()?;
            if !current.is_dir()
                || current.file_type().is_symlink()
                || (current.dev(), current.ino(), current.uid())
                    != (authority.device, authority.inode, authority.uid)
                || current.mode() & 0o7777 != 0o700
                || fs::canonicalize(root).ok()?.as_path() != root
            {
                return None;
            }
            let marker = private_file(&root.join(".bluey-test-run-v2"), authority.uid);
            if marker != authority.marker {
                return None;
            }
            let lines: Vec<_> = marker.lines().collect();
            if lines.len() != 9 || lines[8] != "state=running" {
                return None;
            }
            let raw_group = lines[5].strip_prefix("process_group_id=")?;
            let raw_launcher = lines[6].strip_prefix("launcher_group_id=")?;
            let group = raw_group.parse::<i32>().ok()?;
            let launcher = raw_launcher.parse::<i32>().ok()?;
            if group <= 1
                || launcher <= 1
                || group == launcher
                || group.to_string() != raw_group
                || launcher.to_string() != raw_launcher
                || current_group()? != group
            {
                return None;
            }
            let after = fs::symlink_metadata(root).ok()?;
            if (after.dev(), after.ino(), after.uid(), after.mode())
                != (current.dev(), current.ino(), current.uid(), current.mode())
                || private_file(&root.join(".bluey-test-run-v2"), authority.uid) != authority.marker
            {
                return None;
            }
            Some(group)
        })
        .ok()
        .flatten();
        if let Some(group) = group {
            // The launcher is outside this group and supervises its termination.
            let sent = Command::new("/bin/kill")
                .args(["-s", "TERM", "--", &format!("-{group}")])
                .env_clear()
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
            if let Ok(mut signaler) = sent {
                let deadline = Instant::now() + Duration::from_secs(2);
                while Instant::now() < deadline {
                    if let Ok(Some(status)) = signaler.try_wait() {
                        if status.success() {
                            panic!("phase654_operator_timeout_group_signal_returned");
                        }
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                let _ = signaler.kill();
            }
        }
        eprintln!("phase654_operator_timeout_group_authority_unproven");
        panic!("phase654_operator_timeout_cleanup_unproven");
    }

    fn prepare_operator_fixture(root: &Path) -> PathBuf {
        let directory = root.join("phase654-operator-migrations");
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&directory)
            .expect("create owned migration directory");
        let path = directory.join(MIGRATION);
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .expect("create owned migration file");
        file.write_all(OPERATOR_SQL.as_bytes())
            .expect("write exact embedded migration");
        file.sync_all().expect("sync owned migration file");
        drop(file);
        let metadata = fs::symlink_metadata(&path).expect("owned migration metadata");
        assert!(metadata.is_file() && metadata.uid() == fs::metadata(root).unwrap().uid());
        assert_eq!(metadata.mode() & 0o7777, 0o600);
        assert_eq!(metadata.nlink(), 1);
        assert_eq!(
            fs::read(&path).expect("owned migration readback"),
            OPERATOR_SQL.as_bytes()
        );
        directory
    }

    fn nofollow_flag() -> i32 {
        #[cfg(target_os = "macos")]
        {
            0x00000100
        }
        #[cfg(target_os = "linux")]
        {
            0x00020000
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            panic!("unsupported no-follow platform")
        }
    }

    fn bounded_source_read(path: &Path) -> Vec<u8> {
        let before = fs::symlink_metadata(path).expect("migration source metadata");
        assert!(before.is_file() && !before.file_type().is_symlink());
        assert!(before.len() <= 16 * 1024 * 1024 && before.nlink() == 1);
        let mut file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(nofollow_flag())
            .open(path)
            .expect("open exact migration source");
        let opened = file.metadata().expect("opened migration metadata");
        let identity = |m: &fs::Metadata| {
            (
                m.dev(),
                m.ino(),
                m.uid(),
                m.mode(),
                m.nlink(),
                m.len(),
                m.mtime(),
                m.mtime_nsec(),
                m.ctime(),
                m.ctime_nsec(),
            )
        };
        assert_eq!(identity(&before), identity(&opened));
        let mut bytes = Vec::new();
        (&mut file)
            .take(16 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .expect("read migration source");
        assert_eq!(bytes.len() as u64, before.len());
        assert_eq!(identity(&file.metadata().unwrap()), identity(&before));
        assert_eq!(
            identity(&fs::symlink_metadata(path).unwrap()),
            identity(&before)
        );
        bytes
    }

    fn legacy_fixture_names() -> Vec<String> {
        FULL_OPERATOR_SQL
            .iter()
            .map(|(name, _)| (*name).to_owned())
            .collect()
    }

    fn verify_current_operator_source_inventory(mut actual: Vec<String>) {
        actual.sort();
        let legacy_fixture_names = legacy_fixture_names();
        let mut expected_source_names = legacy_fixture_names.clone();
        expected_source_names.push(GUARDED_OPERATOR_SOURCE_MARKER.0.to_owned());
        expected_source_names.sort();
        assert_eq!(
            actual, expected_source_names,
            "migration source inventory changed"
        );
        assert_eq!(legacy_fixture_names.len(), 48);
        assert_eq!(expected_source_names.len(), 49);
    }

    fn verify_guarded_operator_source_bytes(source_bytes: &[u8]) {
        let (_, marker_embedded) = GUARDED_OPERATOR_SOURCE_MARKER;
        assert_eq!(
            source_bytes, marker_embedded,
            "guarded migration source bytes changed"
        );
        assert_eq!(
            Sha256::digest(marker_embedded),
            Sha256::digest(source_bytes),
            "guarded migration source hash changed"
        );
    }

    fn verify_legacy_fixture_inventory(actual: &[String]) {
        let expected = legacy_fixture_names();
        assert_eq!(expected.len(), 48, "legacy fixture contract changed");
        assert!(
            !actual
                .iter()
                .any(|name| name == GUARDED_OPERATOR_SOURCE_MARKER.0),
            "guarded Rust-only migration must not be copied into legacy fixture"
        );
        assert_eq!(actual, expected, "operator fixture inventory changed");
    }

    fn verify_full_operator_sources(source: &Path) {
        assert_eq!(
            fs::canonicalize(source).expect("canonical source directory"),
            source
        );
        let actual: Vec<String> = fs::read_dir(source)
            .expect("migration source directory")
            .map(|entry| {
                entry
                    .expect("source entry")
                    .file_name()
                    .into_string()
                    .expect("source name")
            })
            .collect();
        verify_current_operator_source_inventory(actual);
        for &(name, embedded) in FULL_OPERATOR_SQL {
            assert!(name.ends_with(".sql") && !name.contains('/'));
            let source_bytes = bounded_source_read(&source.join(name));
            assert_eq!(
                Sha256::digest(&source_bytes),
                Sha256::digest(embedded),
                "migration source hash changed"
            );
            assert!(
                std::str::from_utf8(embedded)
                    .expect("migration UTF-8")
                    .lines()
                    .take(12)
                    .any(|line| line.to_ascii_lowercase().contains("target: postgres")),
                "migration target header missing"
            );
        }
        let (marker_name, marker_embedded) = GUARDED_OPERATOR_SOURCE_MARKER;
        assert!(marker_name.ends_with(".sql") && !marker_name.contains('/'));
        let marker_source_bytes = bounded_source_read(&source.join(marker_name));
        assert_eq!(marker_source_bytes.len(), marker_embedded.len());
        verify_guarded_operator_source_bytes(&marker_source_bytes);
    }

    fn prepare_full_operator_fixture(root: &Path, mode: u8) -> (PathBuf, PathBuf) {
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("infra/postgres/server-runtime");
        verify_full_operator_sources(&source);
        let directory = root.join(match mode {
            0 => "phase655-operator-full-migrations",
            1 => "phase655-operator-custom-migrations",
            2 => "phase655-operator-env-migrations",
            _ => panic!("unknown fixture mode"),
        });
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&directory)
            .expect("create full fixture");
        for &(name, embedded) in FULL_OPERATOR_SQL {
            let path = directory.join(name);
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .custom_flags(nofollow_flag())
                .open(&path)
                .expect("create migration copy");
            file.write_all(embedded).expect("write migration copy");
            file.sync_all().expect("sync migration copy");
            drop(file);
            let metadata = fs::symlink_metadata(&path).unwrap();
            assert_eq!(metadata.uid(), fs::metadata(root).unwrap().uid());
            assert_eq!(metadata.mode() & 0o7777, 0o600);
            assert_eq!(
                Sha256::digest(bounded_source_read(&path)),
                Sha256::digest(embedded),
                "migration copy hash mismatch"
            );
        }
        fs::File::open(&directory)
            .unwrap()
            .sync_all()
            .expect("sync migration directory");
        verify_full_operator_sources(&source);
        (directory, source)
    }

    fn verify_full_operator_copies(directory: &Path) {
        let mut actual: Vec<_> = fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        actual.sort();
        verify_legacy_fixture_inventory(&actual);
        for &(name, embedded) in FULL_OPERATOR_SQL {
            assert_eq!(
                Sha256::digest(bounded_source_read(&directory.join(name))),
                Sha256::digest(embedded),
                "operator fixture content changed"
            );
        }
    }

    #[cfg(test)]
    mod legacy_fixture_catalog_tests {
        use super::*;

        fn current_source_names() -> Vec<String> {
            let mut names = legacy_fixture_names();
            names.push(GUARDED_OPERATOR_SOURCE_MARKER.0.to_owned());
            names
        }

        #[test]
        fn current_source_catalog_requires_exact_known_guarded_marker() {
            verify_current_operator_source_inventory(current_source_names());

            let mut missing = current_source_names();
            missing.retain(|name| name != GUARDED_OPERATOR_SOURCE_MARKER.0);
            assert!(std::panic::catch_unwind(|| {
                verify_current_operator_source_inventory(missing);
            })
            .is_err());

            let mut unknown = current_source_names();
            unknown.push("050_unreviewed.sql".to_owned());
            assert!(std::panic::catch_unwind(|| {
                verify_current_operator_source_inventory(unknown);
            })
            .is_err());
        }

        #[test]
        fn guarded_marker_requires_exact_compiled_bytes_and_stays_out_of_legacy_fixture() {
            let (_, embedded) = GUARDED_OPERATOR_SOURCE_MARKER;
            verify_guarded_operator_source_bytes(embedded);

            let mut altered = embedded.to_vec();
            altered[0] ^= 1;
            assert!(std::panic::catch_unwind(|| {
                verify_guarded_operator_source_bytes(&altered);
            })
            .is_err());

            let legacy = legacy_fixture_names();
            verify_legacy_fixture_inventory(&legacy);
            let mut polluted = legacy;
            polluted.push(GUARDED_OPERATOR_SOURCE_MARKER.0.to_owned());
            polluted.sort();
            assert!(std::panic::catch_unwind(|| {
                verify_legacy_fixture_inventory(&polluted);
            })
            .is_err());
        }
    }

    fn report_full_operator_failure(output: &str, error: &str) {
        let class = if error.contains("ERROR:  relation \"bluey_schema_migrations\" does not exist")
        {
            "missing_default_ledger"
        } else if error.contains("duplicate key value violates unique constraint")
            && error.contains("bluey_schema_migrations_pkey")
        {
            "ledger_duplicate"
        } else if error.contains("ERROR:") {
            "other_sql"
        } else {
            "unknown"
        };
        let last = output
            .lines()
            .filter_map(|line| line.strip_prefix("apply: "))
            .rfind(|name| {
                FULL_OPERATOR_SQL
                    .iter()
                    .any(|(expected, _)| expected == name)
            })
            .unwrap_or("none");
        eprintln!("phase655_operator_bootstrap_failure class={class} last_apply={last} stderr_bytes={} stderr_sha256={:x}",
            error.len(), Sha256::digest(error.as_bytes()));
    }

    fn full_operator_bootstrap(url: &str, root: &Path, mode: u8) {
        let custom = mode != 0;
        let mut db = Client::connect(url, NoTls)
            .unwrap_or_else(|_| panic!("owned bootstrap connection failed"));
        let ledger_exists: bool = db
            .query_one(
                "SELECT to_regclass('public.bluey_schema_migrations') IS NOT NULL",
                &[],
            )
            .unwrap()
            .get(0);
        assert!(
            !ledger_exists,
            "operator bootstrap requires fresh case database"
        );
        let custom_exists: bool = db
            .query_one(
                "SELECT to_regclass('public.phase655_custom_ledger') IS NOT NULL",
                &[],
            )
            .unwrap()
            .get(0);
        assert!(!custom_exists, "custom ledger must initially be absent");
        let reserved_exists: bool = db
            .query_one("SELECT to_regclass('public.\"select\"') IS NOT NULL", &[])
            .unwrap()
            .get(0);
        assert!(!reserved_exists, "reserved ledger must initially be absent");
        drop(db);
        let (fixture, source) = prepare_full_operator_fixture(root, mode);
        if mode == 2 {
            let snapshot = |db: &mut Client| -> Vec<(String, String)> {
                db.query(
                    "SELECT c.relname,c.relkind::text FROM pg_class c \
                    JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='public' \
                    ORDER BY c.relname,c.relkind",
                    &[],
                )
                .unwrap()
                .iter()
                .map(|row| (row.get(0), row.get(1)))
                .collect()
            };
            let mut db = Client::connect(url, NoTls).unwrap();
            let before = snapshot(&mut db);
            drop(db);
            for rejected in ["bad-name", &"a".repeat(64)] {
                let (status, output, _) = operator_cli(url, root, &fixture, Some(rejected));
                assert!(!status.success(), "invalid identifier was accepted");
                assert!(!output.lines().any(|line| line.starts_with("apply: ")));
                let mut db = Client::connect(url, NoTls).unwrap();
                assert_eq!(
                    snapshot(&mut db),
                    before,
                    "identifier refusal changed public schema"
                );
            }
        }
        let table = if mode == 1 {
            Some("phase655_custom_ledger")
        } else {
            None
        };
        let env_file = if mode == 2 {
            let path = root.join("phase655-operator-envfile");
            let fixture_path = fixture.to_str().expect("fixture path");
            assert!(fixture_path
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"/._-".contains(&b)));
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .custom_flags(nofollow_flag())
                .open(&path)
                .expect("private env file");
            write!(file, "BLUEY_POSTGRES_MIGRATION_TABLE=Select\nBLUEY_POSTGRES_MIGRATIONS_DIR='{fixture_path}'\n").unwrap();
            file.sync_all().unwrap();
            Some(path)
        } else {
            None
        };
        let (status, output, error) =
            operator_cli_with_env(url, root, &fixture, table, env_file.as_deref());
        verify_full_operator_sources(&source);
        verify_full_operator_copies(&fixture);
        if !status.success() {
            report_full_operator_failure(&output, &error);
        }
        assert!(status.success(), "fresh operator bootstrap failed");
        let applied: Vec<_> = output
            .lines()
            .filter_map(|line| line.strip_prefix("apply: "))
            .collect();
        let names: Vec<_> = FULL_OPERATOR_SQL.iter().map(|(name, _)| *name).collect();
        assert_eq!(
            applied, names,
            "operator bootstrap migration inventory differs"
        );
        let mut db = Client::connect(url, NoTls)
            .unwrap_or_else(|_| panic!("owned bootstrap readback failed"));
        let snapshot = |db: &mut Client| {
            if custom {
                let default_exists: bool = db
                    .query_one(
                        "SELECT to_regclass('public.bluey_schema_migrations') IS NOT NULL",
                        &[],
                    )
                    .unwrap()
                    .get(0);
                assert!(!default_exists, "custom bootstrap created default ledger");
            }
            let ledger: Vec<(String, String)> = db
                .query(
                    if mode == 2 {
                        "SELECT version,applied_at::text FROM \"select\" ORDER BY version"
                    } else if custom {
                        "SELECT version,applied_at::text FROM phase655_custom_ledger ORDER BY version"
                    } else {
                        "SELECT version,applied_at::text FROM bluey_schema_migrations ORDER BY version"
                    },
                    &[],
                )
                .unwrap()
                .iter()
                .map(|row| (row.get(0), row.get(1)))
                .collect();
            let schema: Vec<(String, String, String)> = db
                .query(
                    "SELECT n.nspname,c.relname,c.relkind::text FROM pg_class c
                 JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='public'
                 ORDER BY n.nspname,c.relname,c.relkind",
                    &[],
                )
                .unwrap()
                .iter()
                .map(|row| (row.get(0), row.get(1), row.get(2)))
                .collect();
            (ledger, schema)
        };
        let before = snapshot(&mut db);
        assert_eq!(
            before
                .0
                .iter()
                .map(|row| row.0.as_str())
                .collect::<Vec<_>>(),
            names
        );
        drop(db);
        let (status, output, error) =
            operator_cli_with_env(url, root, &fixture, table, env_file.as_deref());
        verify_full_operator_sources(&source);
        verify_full_operator_copies(&fixture);
        if !status.success() {
            report_full_operator_failure(&output, &error);
        }
        assert!(status.success(), "operator bootstrap replay failed");
        let skipped: Vec<_> = output
            .lines()
            .filter_map(|line| line.strip_prefix("skip: "))
            .filter_map(|line| line.strip_suffix(" already applied"))
            .collect();
        assert_eq!(
            skipped, names,
            "operator bootstrap replay inventory differs"
        );
        let mut db =
            Client::connect(url, NoTls).unwrap_or_else(|_| panic!("owned replay readback failed"));
        assert_eq!(
            snapshot(&mut db),
            before,
            "operator bootstrap replay changed schema or ledger"
        );
        if mode == 2 {
            println!("phase655_operator_envfile_bootstrap_and_replay_complete");
        } else if custom {
            println!("phase655_operator_custom_bootstrap_and_replay_complete");
        } else {
            println!("phase655_operator_full_bootstrap_and_replay_complete");
        }
    }

    fn operator_cli(
        url: &str,
        root: &Path,
        migrations: &Path,
        table: Option<&str>,
    ) -> (ExitStatus, String, String) {
        operator_cli_with_env(url, root, migrations, table, None)
    }

    fn operator_cli_with_env(
        url: &str,
        root: &Path,
        migrations: &Path,
        table: Option<&str>,
        env_file: Option<&Path>,
    ) -> (ExitStatus, String, String) {
        let root_meta = fs::symlink_metadata(root).expect("owned root metadata");
        assert!(root_meta.is_dir() && root_meta.mode() & 0o7777 == 0o700);
        assert_eq!(
            fs::canonicalize(root).expect("owned root canonical path"),
            root
        );
        let authority = RunAuthority {
            device: root_meta.dev(),
            inode: root_meta.ino(),
            uid: root_meta.uid(),
            marker: private_file(&root.join(".bluey-test-run-v2"), root_meta.uid()),
        };
        let lines: Vec<_> = authority.marker.lines().collect();
        assert!(lines.len() == 9 && lines[8] == "state=running");
        let group_raw = lines[5]
            .strip_prefix("process_group_id=")
            .expect("command group");
        let launcher_raw = lines[6]
            .strip_prefix("launcher_group_id=")
            .expect("launcher group");
        let group: i32 = group_raw.parse().expect("canonical command group");
        let launcher: i32 = launcher_raw.parse().expect("canonical launcher group");
        assert!(group > 1 && launcher > 1 && group != launcher);
        assert!(group.to_string() == group_raw && launcher.to_string() == launcher_raw);
        assert_eq!(current_group(), Some(group), "owned command group mismatch");
        let bin = PathBuf::from(
            std::env::var_os("BLUEY_JOBS_TEST_POSTGRES_BIN")
                .expect("owned wrapper PostgreSQL bin required"),
        );
        assert_eq!(
            fs::canonicalize(&bin).expect("canonical PostgreSQL bin"),
            bin
        );
        let bin_meta = fs::symlink_metadata(&bin).expect("PostgreSQL bin metadata");
        let uid = fs::metadata(root).unwrap().uid();
        assert!(bin_meta.is_dir() && (bin_meta.uid() == uid || bin_meta.uid() == 0));
        assert_eq!(bin_meta.mode() & 0o022, 0);
        let psql = bin.join("psql");
        assert_eq!(fs::canonicalize(&psql).expect("canonical psql"), psql);
        let psql_meta = fs::symlink_metadata(&psql).expect("psql metadata");
        assert!(psql_meta.is_file() && (psql_meta.uid() == uid || psql_meta.uid() == 0));
        assert_eq!(psql_meta.mode() & 0o6022, 0);
        assert_ne!(psql_meta.mode() & 0o111, 0);

        let script = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("repository root")
            .join("scripts/bluey-postgres-migrate.sh");
        assert_eq!(
            fs::canonicalize(&script).expect("canonical operator CLI"),
            script
        );
        let path = format!("{}:/usr/bin:/bin", bin.display());
        let mut command = Command::new("/bin/bash");
        command
            .arg(script)
            .env_clear()
            .env("PATH", path)
            .env("HOME", root.join("nonprod-postgres/child-home"))
            .env("LANG", "C")
            .env("LC_ALL", "C")
            .env("PGCONNECT_TIMEOUT", "5")
            .env("BLUEY_DATABASE_URL", url)
            .env(
                "BLUEY_POSTGRES_MIGRATIONS_DIR",
                if env_file.is_some() {
                    root.join("phase655-envfile-wrong-directory")
                } else {
                    migrations.to_path_buf()
                },
            )
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(path) = env_file {
            command.arg(path);
        }
        if let Some(table) = table {
            command.env("BLUEY_POSTGRES_MIGRATION_TABLE", table);
        }
        let mut child = command.spawn().expect("start owned operator CLI");
        let stdout = child.stdout.take().expect("operator CLI stdout");
        let stderr = child.stderr.take().expect("operator CLI stderr");
        // Closing the pipe after the cap also prevents retaining unbounded data.
        let (out_sender, out_receiver) = std::sync::mpsc::sync_channel(1);
        let (err_sender, err_receiver) = std::sync::mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let result = stdout.take(131073).read_to_end(&mut bytes).map(|_| bytes);
            let _ = out_sender.send(result);
        });
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            // The actual 045 red baseline produced123707 bytes of public DDL
            // notices. Bound the complete48-file run without truncating it.
            let result = stderr.take(524289).read_to_end(&mut bytes).map(|_| bytes);
            let _ = err_sender.send(result);
        });
        let deadline = Instant::now() + Duration::from_secs(180);
        let status = loop {
            if let Some(status) = child.try_wait().expect("poll owned operator CLI") {
                break status;
            }
            if Instant::now() >= deadline {
                abort_owned_group(root, &authority);
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        let bytes = out_receiver
            .recv_timeout(Duration::from_secs(5))
            .unwrap_or_else(|_| abort_owned_group(root, &authority))
            .expect("operator output read");
        let error_bytes = err_receiver
            .recv_timeout(Duration::from_secs(5))
            .unwrap_or_else(|_| abort_owned_group(root, &authority))
            .expect("operator error read");
        assert!(bytes.len() <= 131072, "operator output exceeded bound");
        assert!(error_bytes.len() <= 524288, "operator error exceeded bound");
        let output = String::from_utf8(bytes).expect("operator output encoding");
        let error = String::from_utf8(error_bytes).expect("operator error encoding");
        assert!(
            !output.contains(url) && !error.contains(url),
            "operator output contained URL"
        );
        (status, output, error)
    }

    fn owned_url(case: &str) -> Option<String> {
        assert!(case == CASE || case == CUSTOM_CASE || case == ENV_CASE || case == AMBIENT_CASE);
        super::owned_postgres_test::owned_url(case)
    }

    #[test]
    fn postgres_owned_circuit_projection_scope_cas_and_legacy_preflight() {
        let Some(url) = owned_url(CASE) else { return };
        let root = PathBuf::from(std::env::var_os("BLUEY_TEST_RUN_ROOT").unwrap());
        full_operator_bootstrap(&url, &root, 0);
        let operator_migrations = prepare_operator_fixture(&root);
        let pool = open_postgres_pool(&url).unwrap_or_else(|_| panic!("owned test pool failed"));
        run_migrations(&pool).expect("registered PostgreSQL migrations");
        {
            let mut db = pool.get_pg().unwrap();
            db.execute(
                "INSERT INTO bluey_schema_migrations(version) VALUES($1)",
                &[&"045_jobs_ats_storage_fences.sql"],
            )
            .unwrap();
            let before: i64 = db
                .query_one("SELECT count(*) FROM bluey_schema_migrations", &[])
                .unwrap()
                .get(0);
            drop(db);
            let error =
                run_migrations(&pool).expect_err("old ATS ledger must refuse before migration");
            assert!(format!("{error:#}")
                .contains("PostgreSQL ATS storage lineage requires manual review"));
            for table in [None, Some("phase655_custom_ledger")] {
                let (status, _, error) = operator_cli(&url, &root, &operator_migrations, table);
                assert!(!status.success());
                assert!(error
                    .contains("unreleased ATS migration lineage requires explicit upgrade review"));
            }
            let mut db = pool.get_pg().unwrap();
            let after: i64 = db
                .query_one("SELECT count(*) FROM bluey_schema_migrations", &[])
                .unwrap()
                .get(0);
            assert_eq!(before, after);
            let custom_exists: bool = db
                .query_one(
                    "SELECT to_regclass('phase655_custom_ledger') IS NOT NULL",
                    &[],
                )
                .unwrap()
                .get(0);
            assert!(
                !custom_exists,
                "refusal must precede custom ledger creation"
            );
            assert_eq!(
                db.execute(
                    "DELETE FROM bluey_schema_migrations WHERE version=$1",
                    &[&"045_jobs_ats_storage_fences.sql"]
                )
                .unwrap(),
                1
            );
            db.batch_execute(
                "CREATE TABLE phase655_custom_ledger(version TEXT PRIMARY KEY);
                INSERT INTO phase655_custom_ledger VALUES('045_jobs_ats_storage_fences.sql');",
            )
            .unwrap();
            drop(db);
            let (status, _, error) = operator_cli(
                &url,
                &root,
                &operator_migrations,
                Some("phase655_custom_ledger"),
            );
            assert!(!status.success());
            assert!(
                error.contains("unreleased ATS migration lineage requires explicit upgrade review")
            );
            let mut db = pool.get_pg().unwrap();
            let custom_count: i64 = db
                .query_one("SELECT count(*) FROM phase655_custom_ledger", &[])
                .unwrap()
                .get(0);
            assert_eq!(custom_count, 1);
            db.batch_execute("DROP TABLE phase655_custom_ledger;")
                .unwrap();
        }
        {
            let mut db = pool.get_pg().expect("owned test connection");
            let registered: bool = db
                .query_one(
                    "SELECT EXISTS(SELECT 1 FROM bluey_schema_migrations WHERE version=$1)",
                    &[&MIGRATION],
                )
                .unwrap()
                .get(0);
            assert!(
                registered,
                "047 must be registered before PostgreSQL qualification"
            );
            db.batch_execute(
                "INSERT INTO jobs_ats_certification_circuit_events VALUES
              ('first',repeat('a',64),'x','provider','a','opened','evidence_failure',0,0,\
              0,0,0,'test',0,'test',0),
              ('next',repeat('b',64),'x','provider','a','held','evidence_failure',0,0,0,\
              0,0,'test',0,'test',0),
              ('other',repeat('c',64),'x','provider','b','opened','evidence_failure',0,0,\
              0,0,0,'test',0,'test',0);
              INSERT INTO jobs_ats_certification_circuit_heads VALUES ('provider','a',1,\
              'first','opened',0);",
            )
            .unwrap();
            assert!(db
                .batch_execute(
                    "INSERT INTO jobs_ats_certification_circuit_heads VALUES ('runtime',\
                    'x',1,'other','opened',0)"
                )
                .is_err());
            assert!(db
                .batch_execute(
                    "INSERT INTO jobs_ats_certification_circuit_heads VALUES ('provider',\
                    'b',1,'other','closed',0)"
                )
                .is_err());
            // Matching destination event ensures this tests scope-key movement itself.
            assert!(db
                .batch_execute(
                    "UPDATE jobs_ats_certification_circuit_heads SET subject_key='b',\
                    head_revision=2,current_event_id='other',state='opened' WHERE \
                    scope_kind='provider' AND subject_key='a'"
                )
                .is_err());
            assert!(db
                .batch_execute(
                    "UPDATE jobs_ats_certification_circuit_heads SET head_revision=2,\
                    current_event_id='other',state='opened' WHERE scope_kind='provider' AND \
                    subject_key='a'"
                )
                .is_err());
            assert!(db
                .batch_execute(
                    "UPDATE jobs_ats_certification_circuit_heads SET head_revision=2,\
                    current_event_id='next',state='closed' WHERE scope_kind='provider' AND \
                    subject_key='a'"
                )
                .is_err());
            assert_eq!(
                db.execute(
                    "UPDATE jobs_ats_certification_circuit_heads SET head_revision=2,\
                    current_event_id='next',state='held' WHERE scope_kind='provider' AND \
                    subject_key='a' AND head_revision=1 AND current_event_id='first'",
                    &[]
                )
                .unwrap(),
                1
            );
            db.batch_execute(
                "INSERT INTO jobs_ats_certification_trust_policies (
                policy_sha256,policy_id,trust_generation,maximum_clock_skew_ms,\
                maximum_manifest_size_bytes,
                maximum_target_count,maximum_observation_count,maximum_evidence_object_count,
                canonical_policy_base64url,authorization_sha256,root_trust_anchor_sha256,
                canonical_authorization_base64url,issued_at_ms,valid_from_ms,\
                expires_at_ms,recorded_by,recorded_at_ms)
              VALUES (repeat('a',64),'policy',1,0,1024,1,1,1,'x',repeat('b',64),\
              repeat('c',64),'x',0,0,100,'test',0);
              INSERT INTO jobs_ats_certification_quarantine_commands (
                command_sha256,command_id,command_generation,scope_kind,scope_id,\
                scope_sha256,command_sequence,
                action,reason_ref,canonical_command_base64url,authorization_sha256,\
                trust_policy_sha256,
                canonical_authorization_base64url,issued_at_ms,recorded_by,recorded_at_ms)
              VALUES (repeat('1',64),'command1',1,'provider','a',repeat('d',64),1,\
              'quarantine','reason','x',repeat('1',64),repeat('a',64),'x',0,'test',0),
                     (repeat('2',64),'command2',1,'provider','a',repeat('d',64),2,\
                     'quarantine','reason','x',repeat('2',64),repeat('a',64),'x',0,'test',0),
                     (repeat('3',64),'command3',1,'provider','b',repeat('e',64),2,\
                     'quarantine','reason','x',repeat('3',64),repeat('a',64),'x',0,'test',0);",
            )
            .unwrap();
            assert!(db
                .batch_execute(
                    "INSERT INTO jobs_ats_certification_quarantine_heads VALUES \
                    ('provider','a',repeat('d',64),1,repeat('1',64),1,'released','test',0)"
                )
                .is_err());
            db.batch_execute(
                "INSERT INTO jobs_ats_certification_quarantine_heads VALUES ('provider',\
                'a',repeat('d',64),1,repeat('1',64),1,'quarantined','test',0)",
            )
            .unwrap();
            assert!(db
                .batch_execute(
                    "UPDATE jobs_ats_certification_quarantine_heads SET scope_id='b',\
                    scope_sha256=repeat('e',64),head_revision=2,\
                    current_command_sha256=repeat('3',64),current_command_sequence=2 WHERE \
                    scope_kind='provider' AND scope_id='a'"
                )
                .is_err());
            assert!(db
                .batch_execute(
                    "UPDATE jobs_ats_certification_quarantine_heads SET head_revision=2,\
                    current_command_sha256=repeat('2',64),current_command_sequence=2,\
                    state='released' WHERE scope_kind='provider' AND scope_id='a'"
                )
                .is_err());
            assert_eq!(
                db.execute(
                    "UPDATE jobs_ats_certification_quarantine_heads SET head_revision=2,\
                    current_command_sha256=repeat('2',64),current_command_sequence=2 WHERE \
                    scope_kind='provider' AND scope_id='a' AND head_revision=1",
                    &[]
                )
                .unwrap(),
                1
            );
            db.batch_execute(
                "INSERT INTO jobs_ats_certification_trust_policies (
              policy_sha256,policy_id,trust_generation,predecessor_policy_sha256,\
              maximum_clock_skew_ms,maximum_manifest_size_bytes,
              maximum_target_count,maximum_observation_count,\
              maximum_evidence_object_count,canonical_policy_base64url,authorization_sha256,
              root_trust_anchor_sha256,canonical_authorization_base64url,issued_at_ms,\
              valid_from_ms,expires_at_ms,recorded_by,recorded_at_ms)
              VALUES (repeat('d',64),'policy2',2,repeat('a',64),0,1024,1,1,1,'x',\
              repeat('d',64),repeat('c',64),'x',0,0,100,'test',0),
                     (repeat('e',64),'policy3',3,repeat('d',64),0,1024,1,1,1,'x',\
                     repeat('e',64),repeat('c',64),'x',0,0,100,'test',0);",
            )
            .unwrap();
            assert!(db
                .batch_execute(
                    "INSERT INTO jobs_ats_certification_trust_head VALUES (1,repeat('a',\
                    64),2,1,repeat('c',64),'test',0)"
                )
                .is_err());
            assert!(db
                .batch_execute(
                    "INSERT INTO jobs_ats_certification_trust_head VALUES (1,repeat('a',\
                    64),1,1,repeat('f',64),'test',0)"
                )
                .is_err());
            db.batch_execute(
                "INSERT INTO jobs_ats_certification_trust_head VALUES (1,repeat('a',64),\
                1,1,repeat('c',64),'test',0)",
            )
            .unwrap();
            assert!(db
                .batch_execute(
                    "UPDATE jobs_ats_certification_trust_head SET \
                    current_policy_sha256=repeat('e',64),current_trust_generation=2,\
                    head_revision=2 WHERE singleton_id=1"
                )
                .is_err());
            assert!(db
                .batch_execute(
                    "UPDATE jobs_ats_certification_trust_head SET \
                    current_policy_sha256=repeat('d',64),current_trust_generation=2,\
                    head_revision=2,root_trust_anchor_sha256=repeat('f',64) WHERE singleton_id=1"
                )
                .is_err());
            assert_eq!(
                db.execute(
                    "UPDATE jobs_ats_certification_trust_head SET \
                    current_policy_sha256=repeat('d',64),current_trust_generation=2,\
                    head_revision=2 WHERE singleton_id=1 AND head_revision=1",
                    &[]
                )
                .unwrap(),
                1
            );
            db.batch_execute(
                "INSERT INTO jobs_ats_certification_manifests (
                manifest_sha256,certification_id,manifest_generation,provider,target_key,\
                allowed_provider_hosts_json,
                variant_key,surface_sha256,scope_sha256,adapter_version,\
                final_submit_control_id,adapter_bundle_sha256,
                source_commit,layout_contract_version,layout_contract_sha256,\
                maximum_capability,layout_set_sha256,
                suite_id,suite_version,suite_manifest_sha256,layout_observation_count,\
                check_result_count,
                hard_filter_violations,unsupported_factual_claims,\
                duplicate_submit_activations,false_submitted_states,
                incomplete_receipts,pii_bearing_observations,evidence_count,\
                runtime_target_count,canonical_manifest_base64url,
                authorization_sha256,trust_policy_sha256,\
                canonical_authorization_base64url,tested_at_ms,issued_at_ms,
                not_before_ms,expires_at_ms,recorded_by,recorded_at_ms)
              VALUES (repeat('4',64),'cert',1,'greenhouse','target','[]','variant',\
              repeat('5',64),repeat('d',64),
                'version','submit',repeat('6',64),repeat('7',40),1,repeat('8',64),\
                'observe_only',repeat('9',64),
                'suite','v1',repeat('0',64),1,1,0,0,0,0,0,0,1,1,'x',repeat('f',64),\
                repeat('a',64),'x',0,0,0,100,'test',0);
              INSERT INTO jobs_ats_certification_activations (
                activation_sha256,activation_id,activation_generation,manifest_sha256,\
                provider,adapter_version,
                scope_sha256,channel,channel_sequence,capability,canary_max_submissions,\
                canary_account_cap,
                canary_concurrency_cap,canary_daily_side_effect_cap,approval_ref,\
                canonical_activation_base64url,
                authorization_sha256,trust_policy_sha256,\
                canonical_authorization_base64url,issued_at_ms,not_before_ms,
                expires_at_ms,recorded_by,recorded_at_ms)
              VALUES (repeat('6',64),'activation1',1,repeat('4',64),'greenhouse',\
              'version',repeat('d',64),'shadow',1,'observe_only',0,0,0,0,'approval','x',\
              repeat('6',64),repeat('a',64),'x',0,0,100,'test',0),
                     (repeat('7',64),'activation2',2,repeat('4',64),'greenhouse',\
                     'version',repeat('d',64),'shadow',2,'observe_only',0,0,0,0,'approval','x',\
                     repeat('7',64),repeat('a',64),'x',0,0,100,'test',0),
                     (repeat('8',64),'activation3',3,repeat('4',64),'greenhouse',\
                     'version',repeat('e',64),'shadow',2,'observe_only',0,0,0,0,'approval','x',\
                     repeat('8',64),repeat('a',64),'x',0,0,100,'test',0),
                     (repeat('9',64),'activation4',4,repeat('4',64),'greenhouse',\
                     'version',repeat('e',64),'shadow',1,'observe_only',0,0,0,0,'approval','x',\
                     repeat('9',64),repeat('a',64),'x',0,0,100,'test',0);
              INSERT INTO jobs_ats_certification_head_transitions VALUES
                (repeat('1',64),repeat('d',64),'shadow',1,0,NULL,NULL,repeat('6',64),1,'test',0),
                (repeat('2',64),repeat('d',64),'shadow',2,1,repeat('1',64),repeat('6',64)\
                ,repeat('7',64),2,'test',0),
                (repeat('3',64),repeat('e',64),'shadow',2,1,repeat('1',64),repeat('6',64)\
                ,repeat('8',64),2,'test',0),
                (repeat('4',64),repeat('f',64),'shadow',1,0,NULL,NULL,repeat('9',64),1,'test',0);",
            )
            .unwrap();
            assert!(db
                .batch_execute(
                    "INSERT INTO jobs_ats_certification_heads VALUES (repeat('f',64),\
                    'shadow',1,repeat('4',64),repeat('9',64),1,'test',0)"
                )
                .is_err());
            assert!(db
                .batch_execute(
                    "INSERT INTO jobs_ats_certification_heads VALUES (repeat('d',64),\
                    'shadow',1,repeat('1',64),repeat('6',64),2,'test',0)"
                )
                .is_err());
            db.batch_execute(
                "INSERT INTO jobs_ats_certification_heads VALUES (repeat('d',64),\
                'shadow',1,repeat('1',64),repeat('6',64),1,'test',0)",
            )
            .unwrap();
            assert!(db
                .batch_execute(
                    "UPDATE jobs_ats_certification_heads SET scope_sha256=repeat('e',64),\
                    head_revision=2,current_transition_sha256=repeat('3',64),\
                    current_activation_sha256=repeat('8',64),current_channel_sequence=2 WHERE \
                    scope_sha256=repeat('d',64) AND channel='shadow'"
                )
                .is_err());
            assert!(db
                .batch_execute(
                    "UPDATE jobs_ats_certification_heads SET head_revision=2,\
                    current_transition_sha256=repeat('2',64),\
                    current_activation_sha256=repeat('7',64),current_channel_sequence=3 WHERE \
                    scope_sha256=repeat('d',64) AND channel='shadow'"
                )
                .is_err());
            assert_eq!(
                db.execute(
                    "UPDATE jobs_ats_certification_heads SET head_revision=2,\
                    current_transition_sha256=repeat('2',64),\
                    current_activation_sha256=repeat('7',64),current_channel_sequence=2 WHERE \
                    scope_sha256=repeat('d',64) AND channel='shadow' AND head_revision=1",
                    &[]
                )
                .unwrap(),
                1
            );
            drop(db);
            run_migrations(&pool).expect("populated consistent PostgreSQL replay");
            let mut db = pool.get_pg().expect("connection after valid replay");
            assert_eq!(
                db.execute(
                    "DELETE FROM bluey_schema_migrations WHERE version=$1",
                    &[&MIGRATION]
                )
                .unwrap(),
                1
            );
            drop(db);
            let (applied, output, _) = operator_cli(&url, &root, &operator_migrations, None);
            assert!(applied.success(), "operator CLI valid migration failed");
            assert!(output
                .lines()
                .any(|line| line == "apply: 047_jobs_ats_storage_fences.sql"));
            assert!(output
                .lines()
                .any(|line| line.starts_with("ok: Postgres migrations complete")));
            let mut db = pool.get_pg().expect("connection after operator apply");
            let count: i64 = db
                .query_one(
                    "SELECT count(*) FROM bluey_schema_migrations WHERE version=$1",
                    &[&MIGRATION],
                )
                .unwrap()
                .get(0);
            assert_eq!(count, 1, "operator CLI must record migration once");
            drop(db);
            let (replayed, output, _) = operator_cli(&url, &root, &operator_migrations, None);
            assert!(replayed.success(), "operator CLI replay failed");
            assert!(output
                .lines()
                .any(|line| line == "skip: 047_jobs_ats_storage_fences.sql already applied"));
            let mut db = pool.get_pg().expect("connection after operator replay");
            let count: i64 = db
                .query_one(
                    "SELECT count(*) FROM bluey_schema_migrations WHERE version=$1",
                    &[&MIGRATION],
                )
                .unwrap()
                .get(0);
            assert_eq!(count, 1, "operator replay must leave one ledger row");
            // Simulate a pre-047 database only inside this fresh owned cluster.
            db.batch_execute(
                "DROP TRIGGER trg_jobs_ats_circuit_scope_and_event ON \
                jobs_ats_certification_circuit_heads;
              INSERT INTO jobs_ats_certification_circuit_heads VALUES ('runtime','x',1,\
              'other','opened',0);
              DELETE FROM bluey_schema_migrations WHERE version='047_jobs_ats_storage_fences.sql';",
            )
            .unwrap();
        }
        let err = run_migrations(&pool).expect_err("legacy inconsistency must refuse migration");
        assert!(format!("{err:#}").contains("inconsistent ATS authority head"));
        let (refused, output, error) = operator_cli(&url, &root, &operator_migrations, None);
        assert!(
            !refused.success(),
            "operator CLI must reject inconsistent legacy authority"
        );
        assert!(output
            .lines()
            .any(|line| line == "apply: 047_jobs_ats_storage_fences.sql"));
        assert!(
            error.contains("inconsistent ATS authority head"),
            "unexpected operator failure"
        );
        assert!(!output
            .lines()
            .any(|line| line.starts_with("ok: Postgres migrations complete")));
        let mut db = pool.get_pg().expect("connection after failed migration");
        assert_eq!(
            db.query_one("SELECT 1::int4", &[])
                .unwrap()
                .get::<_, i32>(0),
            1
        );
        let legacy: i64 = db
            .query_one(
                "SELECT count(*) FROM jobs_ats_certification_circuit_heads WHERE \
                scope_kind='runtime' AND subject_key='x' AND current_event_id='other'",
                &[],
            )
            .unwrap()
            .get(0);
        assert_eq!(
            legacy, 1,
            "failed preflight must not repair the legacy head"
        );
        let ledger: i64 = db
            .query_one(
                "SELECT count(*) FROM bluey_schema_migrations WHERE version=$1",
                &[&MIGRATION],
            )
            .unwrap()
            .get(0);
        assert_eq!(ledger, 0, "failed migration must not record itself");
        db.batch_execute(
            "BEGIN; LOCK TABLE jobs_ats_certification_circuit_heads IN SHARE ROW \
            EXCLUSIVE MODE NOWAIT; ROLLBACK;",
        )
        .expect("failed migration must not retain its table lock");
        println!("phase654_owned_postgres_assertions_complete");
    }

    #[test]
    fn postgres_owned_custom_ledger_full_bootstrap_and_replay() {
        let Some(url) = owned_url(CUSTOM_CASE) else {
            return;
        };
        let root = PathBuf::from(std::env::var_os("BLUEY_TEST_RUN_ROOT").unwrap());
        full_operator_bootstrap(&url, &root, 1);
    }

    #[test]
    fn postgres_owned_envfile_ledger_full_bootstrap_and_replay() {
        let Some(url) = owned_url(ENV_CASE) else {
            return;
        };
        let root = PathBuf::from(std::env::var_os("BLUEY_TEST_RUN_ROOT").unwrap());
        full_operator_bootstrap(&url, &root, 2);
    }

    #[test]
    fn postgres_owned_rust_ambient_ledger_rollback_and_replay() {
        let Some(url) = owned_url(AMBIENT_CASE) else {
            return;
        };
        let mut parsed = reqwest::Url::parse(&url).expect("validated owned URL");
        // PostgreSQL URI options use percent encoding, not form encoding:
        // query_pairs_mut would encode the space as '+', which this driver
        // preserves literally. The original owned URL has exactly sslmode=disable.
        parsed.set_query(Some(
            "sslmode=disable&options=-c%20bluey.migration_table%3Dphase655_ambient_ledger",
        ));
        let config = parsed
            .as_str()
            .parse::<postgres::Config>()
            .expect("owned ambient PostgreSQL configuration");
        assert_eq!(
            config.get_options(),
            Some("-c bluey.migration_table=phase655_ambient_ledger")
        );
        let pool = open_postgres_pool(parsed.as_str()).expect("owned ambient pool");
        {
            let mut db = pool.get_pg().unwrap();
            let setting: String = db
                .query_one("SELECT current_setting('bluey.migration_table')", &[])
                .unwrap()
                .get(0);
            assert_eq!(setting, "phase655_ambient_ledger");
        }
        // Public startup must use the canonical ledger despite the ambient GUC.
        run_migrations(&pool).expect("owned ambient normal startup");
        let snapshot = || {
            let mut db = pool.get_pg().unwrap();
            let setting: String = db
                .query_one("SELECT current_setting('bluey.migration_table')", &[])
                .unwrap()
                .get(0);
            assert_eq!(setting, "phase655_ambient_ledger");
            let absent: bool = db
                .query_one(
                    "SELECT to_regclass('public.phase655_ambient_ledger') IS NULL",
                    &[],
                )
                .unwrap()
                .get(0);
            assert!(absent);
            db.query(
                "SELECT version,applied_at::text FROM bluey_schema_migrations
                 WHERE version IN ('045_jobs_provisioned_runtime_startup.sql',
                 '046_jobs_provisioned_runtime_observations.sql') ORDER BY version",
                &[],
            )
            .unwrap()
            .iter()
            .map(|row| (row.get::<_, String>(0), row.get::<_, String>(1)))
            .collect::<Vec<_>>()
        };
        // A new checkout observes durable state; it does not claim failed-PID reuse.
        let catalog_snapshot = || {
            let mut db = pool.get_pg().unwrap();
            let mut tx = db
                .build_transaction()
                .isolation_level(postgres::IsolationLevel::RepeatableRead)
                .read_only(true)
                .start()
                .unwrap();
            let rows = tx.query(
                "SELECT 'history:' || version || ':' || (applied_at AT TIME ZONE 'UTC')::text
                   FROM public.bluey_schema_migrations
                 UNION ALL SELECT 'column:' || row_to_json(value)::text FROM (
                   SELECT c.relname,a.attnum,a.attname,format_type(a.atttypid,a.atttypmod) AS data_type,
                          a.attnotnull,a.attisdropped,a.attidentity,a.attgenerated,
                          pg_get_expr(d.adbin,d.adrelid) AS default_expr
                     FROM pg_attribute a JOIN pg_class c ON c.oid=a.attrelid
                     JOIN pg_namespace n ON n.oid=c.relnamespace
                     LEFT JOIN pg_attrdef d ON d.adrelid=a.attrelid AND d.adnum=a.attnum
                    WHERE n.nspname='public' AND a.attnum>0
                      AND c.relname IN ('bluey_schema_migrations','jobs_managed_cloud_signature_sets')
                 ) value
                 UNION ALL SELECT 'constraint:' || conrelid::regclass::text || ':' || conname
                   || ':' || pg_get_constraintdef(oid) FROM pg_constraint
                   WHERE conrelid IN ('public.bluey_schema_migrations'::regclass,
                     'public.jobs_managed_cloud_signature_sets'::regclass)
                 UNION ALL SELECT 'index:' || indexdef FROM pg_indexes
                   WHERE schemaname='public' AND tablename IN
                     ('bluey_schema_migrations','jobs_managed_cloud_signature_sets')
                 UNION ALL SELECT 'trigger:' || pg_get_triggerdef(oid) FROM pg_trigger
                   WHERE tgrelid='public.bluey_schema_migrations'::regclass
                 UNION ALL SELECT 'schema:' || row_to_json(value)::text FROM (
                   SELECT to_regclass('public.phase655_ambient_ledger')::text AS ambient,
                          to_regclass('public.jobs_managed_cloud_preproduction_contexts')::text AS contexts,
                          to_regclass('public.jobs_managed_cloud_bootstrap_sets')::text AS bootstraps
                 ) value ORDER BY 1",
                &[],
            ).unwrap().into_iter().map(|row| row.get::<_, String>(0)).collect::<Vec<_>>();
            tx.commit().unwrap();
            rows
        };
        let before = snapshot();
        assert_eq!(before.len(), 2);
        {
            let mut db = pool.get_pg().unwrap();
            let row = db.query_one(
                "SELECT to_regclass('public.jobs_managed_cloud_preproduction_contexts') IS NOT NULL,
                        to_regclass('public.jobs_managed_cloud_bootstrap_sets') IS NOT NULL",
                &[],
            ).unwrap();
            assert!(row.get::<_, bool>(0) && row.get::<_, bool>(1));
        }
        let clean_catalog = catalog_snapshot();
        // An inert user trigger tests catalog refusal, never a late migration fault.
        {
            let mut db = pool.get_pg().unwrap();
            db.batch_execute(
                "CREATE FUNCTION phase655_ledger_fault() RETURNS trigger LANGUAGE plpgsql AS $$
                 BEGIN RETURN NEW; END; $$;
                 CREATE TRIGGER phase655_ledger_fault BEFORE INSERT ON bluey_schema_migrations
                 FOR EACH ROW EXECUTE FUNCTION phase655_ledger_fault();",
            )
            .unwrap();
        }
        let malformed_catalog = catalog_snapshot();
        assert_ne!(malformed_catalog, clean_catalog);
        let error =
            run_migrations(&pool).expect_err("owned ledger trigger must be catalog-refused");
        assert_eq!(
            error.root_cause().to_string(),
            "migration ledger schema mismatch"
        );
        assert!(format!("{error:#}").contains("selected PostgreSQL audience migration failed"));
        assert!(!format!("{error:#}").contains("phase655_owned_fault:"));
        assert_eq!(
            catalog_snapshot(),
            malformed_catalog,
            "catalog refusal changed durable state"
        );
        assert_eq!(
            snapshot(),
            before,
            "catalog refusal changed canonical timestamps"
        );
        {
            let mut db = pool.get_pg().unwrap();
            db.batch_execute(
                "DROP TRIGGER phase655_ledger_fault ON bluey_schema_migrations;
                 DROP FUNCTION phase655_ledger_fault();",
            )
            .unwrap();
        }
        run_migrations(&pool).expect("owned ambient normal replay after catalog refusal");
        assert_eq!(snapshot(), before, "canonical migration timestamps changed");
        assert_eq!(
            catalog_snapshot(),
            clean_catalog,
            "normal replay changed protected schema"
        );

        // Exercise only the uniquely tagged actual compile-time self-record blocks.
        let mut blocks = Vec::new();
        for sql in [
            include_str!(
                "../../infra/postgres/server-runtime/045_jobs_provisioned_runtime_startup.sql"
            ),
            include_str!(
                "../../infra/postgres/server-runtime/046_jobs_provisioned_runtime_observations.sql"
            ),
        ] {
            let start = "DO $migration$\n";
            let end = "$migration$;";
            assert_eq!(sql.matches(start).count(), 1);
            assert_eq!(sql.matches(end).count(), 1);
            let (_, suffix) = sql.split_once(start).expect("self-record start");
            let (body, tail) = suffix.split_once(end).expect("self-record end");
            assert_eq!(tail.trim(), "COMMIT;");
            blocks.push(format!("{start}{body}{end}"));
        }
        let mut direct = Client::connect(&url, NoTls).expect("owned direct session");
        let initially_unset: Option<String> = direct
            .query_one("SELECT current_setting('bluey.migration_table', true)", &[])
            .unwrap()
            .get(0);
        assert_eq!(initially_unset, None, "direct session must start unset");
        for block in &blocks {
            direct
                .batch_execute(block)
                .expect("NULL default self-record");
        }
        assert_eq!(snapshot(), before, "default self-record changed timestamps");
        direct
            .batch_execute(
                "CREATE TABLE phase655_doblock_test_ledger (version TEXT PRIMARY KEY, \
            applied_at TIMESTAMPTZ NOT NULL DEFAULT now())",
            )
            .unwrap();
        {
            let mut tx = direct.transaction().unwrap();
            tx.query_one(
                "SELECT set_config('bluey.migration_table', $1, true)",
                &[&"phase655_doblock_test_ledger"],
            )
            .unwrap();
            for block in &blocks {
                tx.batch_execute(block).expect("custom self-record");
            }
            tx.commit().unwrap();
        }
        let rows = |db: &mut Client| -> Vec<(String, String)> {
            db.query(
                "SELECT version,applied_at::text FROM phase655_doblock_test_ledger \
                ORDER BY version",
                &[],
            )
            .unwrap()
            .iter()
            .map(|row| (row.get(0), row.get(1)))
            .collect()
        };
        let custom_before = rows(&mut direct);
        assert_eq!(
            custom_before
                .iter()
                .map(|row| row.0.as_str())
                .collect::<Vec<_>>(),
            vec![
                "045_jobs_provisioned_runtime_startup.sql",
                "046_jobs_provisioned_runtime_observations.sql"
            ]
        );
        for invalid in ["", "bad-name", "UPPERCASE", &"a".repeat(64)] {
            for block in &blocks {
                let mut tx = direct.transaction().unwrap();
                tx.query_one(
                    "SELECT set_config('bluey.migration_table', $1, true)",
                    &[&invalid],
                )
                .unwrap();
                let error = tx.batch_execute(block).expect_err("invalid GUC must fail");
                assert_eq!(
                    error.as_db_error().expect("database error").message(),
                    "invalid migration ledger identifier"
                );
                tx.rollback().expect("invalid GUC rollback");
                assert_eq!(
                    direct
                        .query_one("SELECT 1::int4", &[])
                        .unwrap()
                        .get::<_, i32>(0),
                    1
                );
                assert_eq!(rows(&mut direct), custom_before);
                assert_eq!(snapshot(), before);
            }
        }
        direct
            .query_one(
                "SELECT set_config('bluey.migration_table', $1, false)",
                &[&"phase655_doblock_test_ledger"],
            )
            .unwrap();
        direct.batch_execute("RESET bluey.migration_table").unwrap();
        let after_reset: Option<String> = direct
            .query_one("SELECT current_setting('bluey.migration_table', true)", &[])
            .unwrap()
            .get(0);
        assert_eq!(
            after_reset.as_deref(),
            Some(""),
            "RESET must expose empty setting"
        );
        for block in &blocks {
            let mut tx = direct.transaction().unwrap();
            let error = tx
                .batch_execute(block)
                .expect_err("reset GUC must fail closed");
            assert_eq!(
                error.as_db_error().expect("database error").message(),
                "invalid migration ledger identifier"
            );
            tx.rollback().unwrap();
            assert_eq!(
                direct
                    .query_one("SELECT 1::int4", &[])
                    .unwrap()
                    .get::<_, i32>(0),
                1
            );
            assert_eq!(rows(&mut direct), custom_before);
            assert_eq!(snapshot(), before);
        }
        direct
            .batch_execute("DROP TABLE phase655_doblock_test_ledger")
            .unwrap();
        // This selector now binds catalog rollback/startup; late045 DDL rollback is C1.
        println!("phase657_owned_ats_ambient_catalog_refusal_normal_replay_complete");
        println!("phase655_owned_ambient_rollback_and_replay_complete");
    }
}

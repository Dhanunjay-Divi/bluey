// External unrun component regression: actual operator process against a
// wrapper-owned PostgreSQL database, not production authority or TLS proof.
#[cfg(test)]
mod phase657_postgres_audience_cli_owned_tests_v1 {
    use super::*;
    use std::fs;
    use std::io::Read;
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, Stdio};
    use std::sync::mpsc::{self, Receiver, TryRecvError};
    use std::thread::{self, sleep};
    use std::time::{Duration, Instant};

    const CASE: &str = "db::phase657_postgres_audience_cli_owned_tests_v1::postgres_owned_operator_process_check_forward_retry_when_configured";
    const V049: &str = "049_jobs_process_attachment_signature_audience.sql";
    const APP: &str = "phase657_owned_operator_process";
    const CAP: u64 = 8192;

    fn binary() -> PathBuf {
        let target =
            PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").expect("owned target required"));
        assert_eq!(fs::canonicalize(&target).expect("canonical target"), target);
        let path = target.join("debug").join(format!(
            "bluey-postgres-audience-operator{}",
            std::env::consts::EXE_SUFFIX
        ));
        let meta = fs::symlink_metadata(&path).expect("controller-built operator required");
        assert!(meta.is_file() && !meta.file_type().is_symlink());
        assert_eq!(fs::canonicalize(&path).expect("canonical operator"), path);
        path
    }

    struct Captured {
        receiver: Receiver<std::io::Result<Vec<u8>>>,
        bytes: Option<Vec<u8>>,
    }
    impl Captured {
        fn start(reader: impl Read + Send + 'static) -> Self {
            let (sender, receiver) = mpsc::channel();
            thread::spawn(move || {
                let mut bytes = Vec::new();
                let result = reader.take(CAP + 1).read_to_end(&mut bytes).map(|_| bytes);
                let _ = sender.send(result);
            });
            Self {
                receiver,
                bytes: None,
            }
        }
        fn poll(&mut self) {
            if self.bytes.is_some() {
                return;
            }
            match self.receiver.try_recv() {
                Ok(result) => {
                    let bytes = result.expect("bounded pipe read");
                    assert!(bytes.len() as u64 <= CAP, "operator output limit exceeded");
                    self.bytes = Some(bytes);
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => panic!("output reader unavailable"),
            }
        }
    }
    struct Run {
        child: Child,
        stdout: Captured,
        stderr: Captured,
        deadline: Instant,
    }
    impl Drop for Run {
        fn drop(&mut self) {
            if self.child.try_wait().ok().flatten().is_none() {
                let _ = self.child.kill();
                let until = Instant::now() + Duration::from_secs(5);
                while Instant::now() < until {
                    if self.child.try_wait().ok().flatten().is_some() {
                        return;
                    }
                    sleep(Duration::from_millis(25));
                }
            }
        }
    }
    fn start(mut command: Command, label: &'static str) -> Run {
        let mut child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|error| {
                panic!(
                    "failed to spawn owned {label}: kind={:?}, os_code={:?}",
                    error.kind(),
                    error.raw_os_error()
                )
            });
        let stdout = Captured::start(child.stdout.take().expect("stdout pipe"));
        let stderr = Captured::start(child.stderr.take().expect("stderr pipe"));
        Run {
            child,
            stdout,
            stderr,
            deadline: Instant::now() + Duration::from_secs(20),
        }
    }
    fn poll(run: &mut Run) {
        run.stdout.poll();
        run.stderr.poll();
        assert!(Instant::now() < run.deadline, "process timed out");
    }
    fn finish(mut run: Run) -> (bool, Vec<u8>, Vec<u8>) {
        let status = loop {
            poll(&mut run);
            if let Some(status) = run.child.try_wait().expect("poll process") {
                break status;
            }
            sleep(Duration::from_millis(25));
        };
        while run.stdout.bytes.is_none() || run.stderr.bytes.is_none() {
            poll(&mut run);
            sleep(Duration::from_millis(25));
        }
        (
            status.success(),
            run.stdout.bytes.take().expect("stdout complete"),
            run.stderr.bytes.take().expect("stderr complete"),
        )
    }
    #[cfg(unix)]
    fn pgid(pid: u32) -> u32 {
        let raw = i32::try_from(pid).expect("process ID out of range");
        let pid = rustix::process::Pid::from_raw(raw).expect("invalid process ID");
        u32::try_from(
            rustix::process::getpgid(Some(pid))
                .expect("process group unavailable")
                .as_raw_pid(),
        )
        .expect("invalid process group")
    }
    #[cfg(not(unix))]
    fn pgid(_pid: u32) -> u32 {
        panic!("owned process group observation requires Unix")
    }
    #[cfg(unix)]
    #[test]
    fn process_group_observer_matches_current_process_group() {
        assert_eq!(
            pgid(std::process::id()),
            u32::try_from(rustix::process::getpgrp().as_raw_pid()).expect("invalid process group")
        );
    }
    fn spawn(binary: &Path, url: &str, ledger: &str, token: &str) -> Run {
        let mut command = Command::new(binary);
        command
            .arg(token)
            .env_clear()
            .env("BLUEY_DATABASE_URL", url)
            .env("BLUEY_POSTGRES_MIGRATION_TABLE", ledger);
        start(command, "operator")
    }
    fn rows(client: &mut postgres::Client) -> Vec<Vec<String>> {
        client.query("SELECT signature_set_sha256::text,signature_set_id::text,trust_generation::text,role::text,target_audience::text,target_sha256::text,signed_at_ms::text,signature_count::text,canonical_signature_set_base64url::text,recorded_by::text,recorded_at_ms::text FROM public.jobs_managed_cloud_signature_sets ORDER BY signature_set_sha256", &[]).expect("all signature fields").into_iter().map(|r| (0..11).map(|i| r.get(i)).collect()).collect()
    }
    fn history(client: &mut postgres::Client) -> Vec<(String, String)> {
        client.query("SELECT version, applied_at::text FROM public.bluey_schema_migrations ORDER BY version", &[]).expect("all history").into_iter().map(|r| (r.get(0), r.get(1))).collect()
    }
    fn constraint(client: &mut postgres::Client) -> String {
        client.query_one("SELECT pg_get_constraintdef(oid) FROM pg_constraint WHERE conrelid='public.jobs_managed_cloud_signature_sets'::regclass AND conname='jobs_managed_cloud_signature_sets_target_audience_check'", &[]).expect("constraint").get(0)
    }
    fn audiences(client: &mut postgres::Client) -> Vec<String> {
        client.query_one("SELECT ARRAY(SELECT m.parts[1] FROM pg_catalog.regexp_matches(pg_catalog.pg_get_constraintdef(c.oid), '(bluey-jobs-managed-cloud-[^'']+)', 'g') WITH ORDINALITY AS m(parts, n) ORDER BY m.n) FROM pg_catalog.pg_constraint c WHERE c.conrelid='public.jobs_managed_cloud_signature_sets'::regclass AND c.conname='jobs_managed_cloud_signature_sets_target_audience_check'", &[]).expect("exact audience list").get(0)
    }
    fn expected_audiences(successor: bool) -> Vec<&'static str> {
        let mut values = vec![
            "bluey-jobs-managed-cloud-activation-v1",
            "bluey-jobs-managed-cloud-activation-v2",
            "bluey-jobs-managed-cloud-activation-v3",
            "bluey-jobs-managed-cloud-cohort-v1",
            "bluey-jobs-managed-cloud-release-v1",
            "bluey-jobs-managed-cloud-release-v2",
            "bluey-jobs-managed-cloud-release-v3",
            "bluey-jobs-managed-cloud-revocation-v1",
            "bluey-jobs-managed-cloud-rollback-v1",
            "bluey-jobs-managed-cloud-trust-policy-v1",
            "bluey-jobs-managed-cloud-provisioning-v4",
            "bluey-jobs-managed-cloud-preproduction-context-v1",
            "bluey-jobs-managed-cloud-serving-latch-v1",
        ];
        if successor {
            values.push("bluey-jobs-managed-cloud-process-attachment-v1");
        }
        values
    }
    fn singleton(client: &mut postgres::Client) -> Vec<(i16, i16)> {
        client.query("SELECT singleton,version FROM public.jobs_signature_audience_upgrade_state ORDER BY singleton", &[]).expect("singleton").into_iter().map(|r| (r.get(0), r.get(1))).collect()
    }

    #[test]
    fn postgres_owned_operator_process_check_forward_retry_when_configured() {
        if std::env::args().skip(1).eq([
            CASE.to_owned(),
            "--exact".into(),
            "--nocapture".into(),
            "--test-threads=1".into(),
        ]) {
            assert_eq!(
                std::env::var("BLUEY_JOBS_REQUIRE_POSTGRES_TESTS").as_deref(),
                Ok("1"),
                "owned operator wrapper required"
            );
        }
        let Some(raw_url) = crate::db::owned_postgres_test::owned_url(CASE) else {
            return;
        };
        let _lock =
            crate::db::jobs::managed_cloud_release_authority_tests::postgres_boot_test_lock()
                .lock()
                .unwrap();
        let root = PathBuf::from(std::env::var_os("BLUEY_TEST_RUN_ROOT").expect("owned root"));
        assert_eq!(fs::canonicalize(&root).expect("canonical root"), root);
        let executable = binary();
        let mut url = reqwest::Url::parse(&raw_url).expect("owned URL");
        assert_eq!(url.query(), Some("sslmode=disable"));
        url.query_pairs_mut().append_pair("application_name", APP);
        let url = url.to_string();
        let pool = open_postgres_pool(&raw_url).expect("owned pool");
        let migrations = POSTGRES_POST_JOBS_MIGRATIONS.to_vec();
        let index = migrations
            .iter()
            .position(|(id, _)| *id == V049)
            .expect("registered 049");
        run_postgres_migration_list(&pool, &migrations[..index]).expect("registered prefix");
        let mut client = pool.get_pg().expect("owned client");
        client.execute("INSERT INTO public.jobs_managed_cloud_signature_sets (signature_set_sha256,signature_set_id,trust_generation,role,target_audience,target_sha256,signed_at_ms,signature_count,canonical_signature_set_base64url,recorded_by,recorded_at_ms) VALUES ($1,'phase657-cli',1,'root','bluey-jobs-managed-cloud-release-v1',$2,1,1,'fixture','fixture',1)", &[&"a".repeat(64), &"b".repeat(64)]).expect("schema-only row");
        let original_rows = rows(&mut client);
        let original_history = history(&mut client);
        let original_constraint = constraint(&mut client);
        assert_eq!(
            audiences(&mut client),
            expected_audiences(false),
            "actual legacy-13"
        );
        assert!(!original_history.iter().any(|(v, _)| v == V049));
        assert!(client
            .query_one(
                "SELECT to_regclass('public.jobs_signature_audience_upgrade_state') IS NULL",
                &[]
            )
            .expect("state absent")
            .get::<_, bool>(0));
        let (ok, out, err) = finish(spawn(&executable, &url, "bluey_schema_migrations", "check"));
        assert!(
            ok && out.is_empty() && err.is_empty(),
            "check must succeed silently"
        );
        assert_eq!(rows(&mut client), original_rows);
        assert_eq!(history(&mut client), original_history);
        assert_eq!(constraint(&mut client), original_constraint);
        assert!(client
            .query_one(
                "SELECT to_regclass('public.jobs_signature_audience_upgrade_state') IS NULL",
                &[]
            )
            .expect("state absent")
            .get::<_, bool>(0));

        let mut tx = client.transaction().expect("hold signature lock");
        tx.batch_execute(
            "LOCK TABLE public.jobs_managed_cloud_signature_sets IN ACCESS EXCLUSIVE MODE",
        )
        .expect("lock");
        let mut run = spawn(&executable, &url, "bluey_schema_migrations", "049");
        let mut observer = pool.get_pg().expect("observer");
        observer
            .batch_execute("SET statement_timeout='2s'")
            .expect("bounded observer statement");
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            poll(&mut run);
            assert!(
                run.child.try_wait().expect("operator status").is_none(),
                "operator exited before lock observation"
            );
            let observed = observer.query_one("SELECT count(DISTINCT a.pid),count(DISTINCT a.pid) FILTER (WHERE a.wait_event_type='Lock' AND l.relation='public.jobs_managed_cloud_signature_sets'::regclass AND l.mode='AccessExclusiveLock' AND NOT l.granted) FROM pg_stat_activity a LEFT JOIN pg_locks l ON l.pid=a.pid WHERE a.application_name=$1 AND a.datname=current_database()", &[&APP]).expect("bounded backend observation");
            let total: i64 = observed.get(0);
            let blocked: i64 = observed.get(1);
            assert!(total <= 1, "more than one matching backend");
            if total == 1 && blocked == 1 {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "operator did not block on signature lock"
            );
            sleep(Duration::from_millis(25));
        }
        let expected_pgid = pgid(std::process::id());
        assert!(
            expected_pgid > 1 && pgid(run.child.id()) == expected_pgid,
            "blocked operator process group mismatch"
        );
        drop(observer);
        tx.commit().expect("release lock");
        let (ok, out, err) = finish(run);
        assert!(
            ok && out.is_empty() && err.is_empty(),
            "forward must succeed silently"
        );
        let forward_history = history(&mut client);
        assert_eq!(forward_history.len(), original_history.len() + 1);
        assert!(forward_history.iter().any(|(v, _)| v == V049));
        assert_eq!(
            forward_history
                .iter()
                .filter(|(v, _)| v != V049)
                .collect::<Vec<_>>(),
            original_history.iter().collect::<Vec<_>>()
        );
        assert_eq!(singleton(&mut client), vec![(1, 1)]);
        assert_eq!(rows(&mut client), original_rows);
        let forward_constraint = constraint(&mut client);
        assert_ne!(forward_constraint, original_constraint);
        assert_eq!(
            audiences(&mut client),
            expected_audiences(true),
            "actual successor-14"
        );
        let (ok, out, err) = finish(spawn(&executable, &url, "bluey_schema_migrations", "049"));
        assert!(
            ok && out.is_empty() && err.is_empty(),
            "retry must succeed silently"
        );
        assert_eq!(history(&mut client), forward_history);
        assert_eq!(rows(&mut client), original_rows);
        assert_eq!(constraint(&mut client), forward_constraint);
        assert_eq!(singleton(&mut client), vec![(1, 1)]);

        let (ok, out, err) = finish(spawn(&executable, &url, "phase657_missing_ledger", "049"));
        assert!(!ok && out.is_empty());
        assert_eq!(
            err,
            b"Error: operator command failed; outcome may be unknown; no retry performed\n"
        );
        assert_eq!(history(&mut client), forward_history);
        assert_eq!(rows(&mut client), original_rows);
        assert_eq!(constraint(&mut client), forward_constraint);
        assert_eq!(singleton(&mut client), vec![(1, 1)]);
        println!("phase657_owned_postgres_operator_process_check_forward_retry_complete");
    }
}

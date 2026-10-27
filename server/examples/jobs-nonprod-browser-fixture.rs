//! Private, debug-only bridge for the owned browser/API/PostgreSQL qualification.
//! No HTTP seed route, production credentials, candidate edits, or employer calls.
#![forbid(unsafe_code)]

#[cfg(not(debug_assertions))]
compile_error!("the nonproduction browser fixture must never be built in release mode");

#[cfg(unix)]
mod unix {
    use base64::Engine;
    use bluey_server::db::{self, accounts::Account, jobs};
    use serde::{Deserialize, Serialize};
    use sha2::{Digest, Sha256};
    use std::fs::{self, File, Metadata, OpenOptions};
    use std::io::{Read, Write};
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    type Checked<T> = Result<T, &'static str>;
    const MARKER: &str = ".bluey-test-run-v2";
    const REQUEST: &str = "browser-packet-fixture-request.json";
    const RESULT: &str = "browser-packet-fixture-result.json";

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Request {
        version: u8,
        account_id: String,
        track_id: String,
    }

    #[derive(Serialize)]
    struct Response {
        version: u8,
        job_id: String,
    }

    struct RootGuard {
        path: PathBuf,
        identity: Metadata,
        marker: Vec<u8>,
        uid: u32,
    }

    fn require(condition: bool, code: &'static str) -> Checked<()> {
        if condition {
            Ok(())
        } else {
            Err(code)
        }
    }

    fn required(name: &str) -> Checked<String> {
        std::env::var(name).map_err(|_| "required_environment_missing")
    }

    fn validate_data_key(raw: &str) -> Checked<()> {
        let key = raw.trim();
        let decoded = if key.len() == 64 && key.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            hex::decode(key).map_err(|_| "data_key_encoding")?
        } else {
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(key)
                .or_else(|_| base64::engine::general_purpose::STANDARD.decode(key))
                .map_err(|_| "data_key_encoding")?
        };
        require(
            decoded.len() == 32
                && decoded.iter().any(|byte| *byte != decoded[0])
                && decoded.as_slice() != &Sha256::digest(b"bluey-jobs-development-only-key")[..],
            "data_key_required",
        )
    }

    fn same_file(left: &Metadata, right: &Metadata) -> bool {
        left.dev() == right.dev() && left.ino() == right.ino()
    }

    fn private_metadata(path: &Path, uid: u32, directory: bool) -> Checked<Metadata> {
        let metadata = fs::symlink_metadata(path).map_err(|_| "private_path_missing")?;
        require(
            !metadata.file_type().is_symlink()
                && metadata.uid() == uid
                && metadata.mode() & 0o7777 == if directory { 0o700 } else { 0o600 }
                && if directory {
                    metadata.is_dir()
                } else {
                    metadata.is_file() && metadata.nlink() == 1
                },
            "private_path_identity",
        )?;
        Ok(metadata)
    }

    fn read_private(path: &Path, uid: u32, limit: u64) -> Checked<Vec<u8>> {
        let before = private_metadata(path, uid, false)?;
        require(
            before.len() > 0 && before.len() <= limit,
            "private_file_size",
        )?;
        let file = File::open(path).map_err(|_| "private_file_open")?;
        let opened = file.metadata().map_err(|_| "private_file_metadata")?;
        require(same_file(&before, &opened), "private_file_changed")?;
        let mut bytes = Vec::new();
        file.take(limit + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "private_file_read")?;
        require(bytes.len() as u64 == before.len(), "private_file_changed")?;
        Ok(bytes)
    }

    fn decimal(value: &str) -> Option<u64> {
        let parsed = value.parse::<u64>().ok()?;
        (parsed.to_string() == value).then_some(parsed)
    }

    fn validate_marker(bytes: &[u8], root: &Path, uid: u32) -> Checked<()> {
        let text = std::str::from_utf8(bytes).map_err(|_| "marker_encoding")?;
        let lines: Vec<_> = text.lines().collect();
        require(lines.len() == 9 && text.ends_with('\n'), "marker_shape")?;
        let value =
            |index: usize, prefix: &str| lines[index].strip_prefix(prefix).ok_or("marker_shape");
        require(lines[0] == "magic=bluey-test-run-v2", "marker_magic")?;
        require(
            value(1, "run_root=")? == root.to_str().ok_or("root_encoding")?,
            "marker_root",
        )?;
        let name = value(2, "run_name=")?;
        require(
            Some(name) == root.file_name().and_then(|part| part.to_str()),
            "marker_name",
        )?;
        require(
            decimal(value(3, "owner_uid=")?) == Some(u64::from(uid)),
            "marker_uid",
        )?;
        let owner = decimal(value(4, "owner_pid=")?)
            .filter(|pid| *pid > 1)
            .ok_or("marker_owner")?;
        let group = decimal(value(5, "process_group_id=")?)
            .filter(|pid| *pid > 1)
            .ok_or("marker_group")?;
        let launcher = decimal(value(6, "launcher_group_id=")?)
            .filter(|pid| *pid > 1)
            .ok_or("marker_launcher")?;
        let epoch = decimal(value(7, "created_epoch=")?).ok_or("marker_epoch")?;
        require(
            group != launcher && lines[8] == "state=running",
            "marker_state",
        )?;
        let parts: Vec<_> = name.split('.').collect();
        require(
            parts.len() == 4
                && parts[0] == "run"
                && decimal(parts[1]) == Some(epoch)
                && decimal(parts[2]) == Some(owner)
                && !parts[3].is_empty()
                && parts[3].len() <= 64
                && parts[3].bytes().all(|byte| byte.is_ascii_alphanumeric()),
            "marker_name_binding",
        )
    }

    impl RootGuard {
        fn load() -> Checked<Self> {
            // No unsafe getuid or new dependency: use the fixed system identity tool,
            // never an environment-supplied UID or executable search path.
            let identity = std::process::Command::new("/usr/bin/id")
                .arg("-u")
                .output()
                .map_err(|_| "current_uid")?;
            require(
                identity.status.success() && identity.stdout.len() <= 16,
                "current_uid",
            )?;
            let uid = std::str::from_utf8(&identity.stdout)
                .ok()
                .and_then(|text| decimal(text.trim()))
                .and_then(|value| u32::try_from(value).ok())
                .ok_or("current_uid")?;
            let path = PathBuf::from(required("BLUEY_TEST_RUN_ROOT")?);
            let canonical = fs::canonicalize(&path).map_err(|_| "root_canonical")?;
            require(path == canonical, "root_not_canonical")?;
            let parent = path.parent().ok_or("root_parent")?;
            let grandparent = parent.parent().ok_or("root_parent")?;
            require(
                matches!(grandparent.to_str(), Some("/tmp" | "/private/tmp"))
                    && parent
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.starts_with("bluey-test-runs-")),
                "root_prefix",
            )?;
            private_metadata(parent, uid, true)?;
            let identity = private_metadata(&path, uid, true)?;
            let marker = read_private(&path.join(MARKER), uid, 4096)?;
            validate_marker(&marker, &path, uid)?;
            Ok(Self {
                path,
                identity,
                marker,
                uid,
            })
        }

        fn revalidate(&self) -> Checked<()> {
            let current = private_metadata(&self.path, self.uid, true)?;
            require(same_file(&self.identity, &current), "root_changed")?;
            require(
                fs::canonicalize(&self.path).ok().as_ref() == Some(&self.path),
                "root_changed",
            )?;
            require(
                read_private(&self.path.join(MARKER), self.uid, 4096)? == self.marker,
                "marker_changed",
            )
        }
    }

    fn database_config(raw: &str) -> Checked<(postgres::Config, u16)> {
        let url = reqwest::Url::parse(raw).map_err(|_| "postgres_url")?;
        let port = url
            .port()
            .filter(|port| *port >= 1024 && *port != 5432)
            .ok_or("postgres_port")?;
        require(
            url.scheme() == "postgresql"
                && url.host_str() == Some("127.0.0.1")
                && url.username() == "bluey_jobs_test"
                && url.path() == "/bluey_jobs_nonprod"
                && url.query() == Some("sslmode=disable")
                && url.fragment().is_none()
                && url.password().is_some_and(|password| {
                    password.len() == 64 && password.bytes().all(|byte| byte.is_ascii_hexdigit())
                }),
            "postgres_url_scope",
        )?;
        let mut config: postgres::Config = raw.parse().map_err(|_| "postgres_config")?;
        config.connect_timeout(Duration::from_secs(5));
        config.options("-c statement_timeout=5000 -c lock_timeout=5000");
        Ok((config, port))
    }

    fn validate_database(
        client: &mut postgres::Client,
        root: &RootGuard,
        port: u16,
    ) -> Checked<()> {
        let expected = root.path.join("nonprod-postgres/data");
        private_metadata(&root.path.join("nonprod-postgres"), root.uid, true)?;
        private_metadata(&expected, root.uid, true)?;
        require(
            fs::canonicalize(&expected).ok().as_ref() == Some(&expected),
            "postgres_data_path",
        )?;
        let row = client
            .query_one(
                "SELECT current_setting('data_directory'), host(inet_server_addr()),
                inet_server_port(), current_database(), current_user",
                &[],
            )
            .map_err(|_| "postgres_identity_query")?;
        require(
            row.get::<_, String>(0) == expected.to_str().ok_or("postgres_data_path")?
                && row.get::<_, String>(1) == "127.0.0.1"
                && row.get::<_, i32>(2) == i32::from(port)
                && row.get::<_, String>(3) == "bluey_jobs_nonprod"
                && row.get::<_, String>(4) == "bluey_jobs_test",
            "postgres_physical_identity",
        )
    }

    fn validate_request(request: &Request) -> Checked<()> {
        require(request.version == 1, "request_version")?;
        for id in [&request.account_id, &request.track_id] {
            require(
                !id.is_empty()
                    && id.len() <= 120
                    && id
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')),
                "request_id",
            )?;
        }
        Ok(())
    }

    fn run() -> Checked<()> {
        require(std::env::args_os().len() == 1, "arguments_forbidden")?;
        require(
            std::env::var_os("BLUEY_DATABASE_URL").is_none(),
            "owner_database_forbidden",
        )?;
        for name in [
            "BLUEY_POSTGRES_CA_CERT_PATH",
            "BLUEY_POSTGRES_ACCEPT_INVALID_CERTS",
            "BLUEY_POSTGRES_ACCEPT_INVALID_HOSTNAMES",
        ] {
            require(
                std::env::var_os(name).is_none(),
                "postgres_override_forbidden",
            )?;
        }
        let root = RootGuard::load()?;
        validate_data_key(&required("BLUEY_JOBS_DATA_KEY")?)?;
        let bytes = read_private(&root.path.join(REQUEST), root.uid, 4096)?;
        let request: Request = serde_json::from_slice(&bytes).map_err(|_| "request_schema")?;
        validate_request(&request)?;
        require(
            fs::symlink_metadata(root.path.join(RESULT))
                .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
            "result_already_exists",
        )?;
        let raw = required("BLUEY_TEST_POSTGRES_URL")?;
        let (config, port) = database_config(&raw)?;
        let mut client = config
            .connect(postgres::NoTls)
            .map_err(|_| "postgres_connect")?;
        validate_database(&mut client, &root, port)?;
        root.revalidate()?;
        // Existing API migrations are required; the bridge never creates/upgrades a database.
        let pool_url = format!(
        "{raw}&connect_timeout=5&options=-c%20statement_timeout%3D5000%20-c%20lock_timeout%3D5000"
    );
        let pool = db::open_postgres_pool(&pool_url).map_err(|_| "postgres_pool")?;
        let account = Account::fetch_by_id(&pool, &request.account_id)
            .map_err(|_| "account_read")?
            .ok_or("account_missing")?;
        require(
            account.id == request.account_id
                && account.email_verified_at.is_some()
                && account.email.ends_with("@example.test"),
            "synthetic_verified_account_required",
        )?;
        let existing = client
            .query_one(
                "SELECT EXISTS(SELECT 1 FROM jobs_profiles WHERE account_id=$1),
                EXISTS(SELECT 1 FROM jobs_preferences WHERE account_id=$1)",
                &[&request.account_id],
            )
            .map_err(|_| "candidate_read")?;
        require(
            existing.get::<_, bool>(0) && existing.get::<_, bool>(1),
            "saved_candidate_required",
        )?;
        let profile =
            jobs::get_profile(&pool, &account.id, &account.email).map_err(|_| "profile_read")?;
        let preferences =
            jobs::get_preferences(&pool, &account.id).map_err(|_| "preferences_read")?;
        require(profile.onboarding_complete, "onboarding_required")?;
        let track = jobs::list_tracks(&pool, &account.id)
            .map_err(|_| "track_read")?
            .into_iter()
            .find(|track| track.id == request.track_id && track.active)
            .ok_or("owned_active_track_required")?;
        require(
            track.application_identity_id.is_some(),
            "track_identity_required",
        )?;
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        let mut posting = jobs::JobPosting {
        id: String::new(),
        canonical_key: String::new(),
        source: "greenhouse".into(),
        external_id: format!("phase632-{suffix}"),
        company: "Synthetic Qualification Employer".into(),
        title: "Software Engineer".into(),
        location: "New York, NY".into(),
        workplace: "hybrid".into(),
        canonical_url: format!("https://boards.greenhouse.io/blueynonprod/jobs/phase632-{suffix}"),
        description: "Full-time W2 employment. Hybrid work in New York, NY. Visa sponsorship available. Build reliable software systems with Rust and TypeScript.".into(),
        compensation: "$170k-$200k".into(),
        employment_type: "full_time".into(),
        track_id: track.id,
        match_score: 99,
        matched_reasons: Vec::new(),
        missing_requirements: Vec::new(),
        posted_at_ms: Some(now),
        last_verified_at_ms: Some(now),
        availability_status: "active".into(),
        status: "matched".into(),
        created_at_ms: 0,
        updated_at_ms: 0,
        discovery_evidence: jobs::JobDiscoveryEvidence::default(),
        eligibility: None,
    };
        posting.canonical_key = jobs::canonical_job_key(&posting);
        root.revalidate()?;
        validate_database(&mut client, &root, port)?;
        // Only the shared test-support fixture publishes signed authorities. It uses
        // production lifecycle APIs, not direct eligibility/trust SQL or external I/O.
        let posting = jobs::install_integration_test_production_positive_job_authorities(
            jobs::IntegrationTestProductionPositiveJobAuthoritiesRequest {
                pool: &pool,
                account_id: &account.id,
                posting: &posting,
                profile: &profile,
                preferences: &preferences,
                canonical_employer_domain: "bluey-nonprod.example.test",
                suffix: &format!("phase632-{suffix}"),
                runner_kind: "local",
            },
        );
        let eligibility = jobs::evaluate_job_eligibility(&pool, &account.id, &posting, false, None)
            .map_err(|_| "eligibility_read")?;
        require(eligibility.can_prepare, "signed_posting_not_preparable")?;
        root.revalidate()?;
        let response = serde_json::to_vec(&Response {
            version: 1,
            job_id: posting.id,
        })
        .map_err(|_| "result_encode")?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(root.path.join(RESULT))
            .map_err(|_| "result_create_new")?;
        output.write_all(&response).map_err(|_| "result_write")?;
        output.sync_all().map_err(|_| "result_sync")?;
        private_metadata(&root.path.join(RESULT), root.uid, false)?;
        Ok(())
    }

    pub(super) fn main() {
        // Fixture assertions can contain SDK/DB error internals; expose only closed
        // diagnostic codes, never account data, connection strings, or signing keys.
        std::panic::set_hook(Box::new(|_| {}));
        let outcome = std::panic::catch_unwind(run).unwrap_or(Err("fixture_assertion_failed"));
        if let Err(code) = outcome {
            eprintln!("{{\"event\":\"browser_packet_fixture_failed\",\"code\":\"{code}\"}}");
            std::process::exit(1);
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn accepts_production_key_encodings_including_standard_padded_base64() {
            let key: Vec<u8> = (0..32).map(|index| index * 7).collect();
            let standard = base64::engine::general_purpose::STANDARD.encode(&key);
            assert!(standard.ends_with('='));
            for encoded in [
                standard,
                base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&key),
                hex::encode(&key),
            ] {
                assert!(validate_data_key(&encoded).is_ok());
            }
            for encoded in [
                hex::encode(&key[..31]),
                hex::encode([0; 32]),
                hex::encode(Sha256::digest(b"bluey-jobs-development-only-key")),
                String::new(),
            ] {
                assert!(validate_data_key(&encoded).is_err());
            }
        }

        #[test]
        fn rejects_remote_default_port_and_connection_overrides() {
            let base = format!(
            "postgresql://bluey_jobs_test:{}@127.0.0.1:15432/bluey_jobs_nonprod?sslmode=disable",
            "a".repeat(64)
        );
            assert!(database_config(&base).is_ok());
            for invalid in [
                base.replace("127.0.0.1", "localhost"),
                base.replace("15432", "5432"),
                base.replace("bluey_jobs_nonprod", "postgres"),
                format!("{base}&host=remote.invalid"),
            ] {
                assert!(database_config(&invalid).is_err());
            }
        }

        #[test]
        fn rejects_unknown_request_fields_and_invalid_ids() {
            assert!(serde_json::from_str::<Request>(
                r#"{"version":1,"account_id":"a","track_id":"t","ready":true}"#
            )
            .is_err());
            assert!(validate_request(&Request {
                version: 1,
                account_id: "../a".into(),
                track_id: "t".into()
            })
            .is_err());
        }

        #[test]
        fn marker_binds_the_exact_root_owner_and_running_name() {
            let root = Path::new("/tmp/bluey-test-runs-501/run.123.456.abc");
            let marker = format!("magic=bluey-test-run-v2\nrun_root={}\nrun_name=run.123.456.abc\nowner_uid=501\nowner_pid=456\nprocess_group_id=789\nlauncher_group_id=456\ncreated_epoch=123\nstate=running\n", root.display());
            assert!(validate_marker(marker.as_bytes(), root, 501).is_ok());
            assert!(validate_marker(marker.as_bytes(), root, 502).is_err());
            assert!(validate_marker(
                marker.replace("state=running", "state=starting").as_bytes(),
                root,
                501
            )
            .is_err());
            assert!(validate_marker(marker.as_bytes(), Path::new("/tmp/other"), 501).is_err());
        }
    }
}

#[cfg(unix)]
fn main() {
    unix::main();
}

#[cfg(not(unix))]
fn main() {
    eprintln!("{{\"event\":\"browser_packet_fixture_failed\",\"code\":\"unsupported_platform\"}}");
    std::process::exit(1);
}

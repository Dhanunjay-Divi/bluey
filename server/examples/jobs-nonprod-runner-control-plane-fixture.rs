//! Private debug-only synthetic runner protocol fixture; not browser, VM, or employer evidence.
//! Bootstrap uses existing lifecycle APIs. Post-queue is a database-enforced read-only handoff.
#![forbid(unsafe_code)]

#[cfg(not(debug_assertions))]
compile_error!(
    "the nonproduction runner control-plane fixture must never be built in release mode"
);

#[cfg(unix)]
mod unix {
    use base64::Engine;
    use bluey_server::db::{self, jobs::integration_test_runner_control_plane as fixture};
    use rand::RngCore;
    use serde::Serialize;
    use sha2::{Digest, Sha256};
    use std::fs::{self, File, Metadata, OpenOptions};
    use std::io::{Read, Write};
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    type Checked<T> = Result<T, &'static str>;
    const MARKER: &str = ".bluey-test-run-v2";
    const BOOTSTRAP_REQUEST: &str = "runner-control-plane-bootstrap-request.json";
    const BOOTSTRAP_RESULT: &str = "runner-control-plane-bootstrap-result.json";
    const POST_QUEUE_REQUEST: &str = "runner-control-plane-post-queue-request.json";
    const POST_QUEUE_RESULT: &str = "runner-control-plane-post-queue-result.json";
    const VOLUME_KEY: &str = "runner-control-plane-volume-key.json";
    const AUTO_BOOTSTRAP_REQUEST: &str = "runner-control-plane-bootstrap-auto-submit-request.json";
    const AUTO_BOOTSTRAP_RESULT: &str = "runner-control-plane-bootstrap-auto-submit-result.json";
    const AUTO_POST_QUEUE_REQUEST: &str =
        "runner-control-plane-post-queue-auto-submit-request.json";
    const AUTO_POST_QUEUE_RESULT: &str = "runner-control-plane-post-queue-auto-submit-result.json";
    const AUTO_VOLUME_KEY: &str = "runner-control-plane-auto-submit-volume-key.json";
    const AUTO_INSPECT_REQUEST: &str =
        "runner-control-plane-inspect-auto-submit-claim-request.json";
    const AUTO_INSPECT_RESULT: &str = "runner-control-plane-inspect-auto-submit-claim-result.json";
    const CHECKPOINT_BEFORE_REQUEST: &str = "runner-control-plane-checkpoint-before-request.json";
    const CHECKPOINT_BEFORE_RESULT: &str = "runner-control-plane-checkpoint-before-result.json";
    const CHECKPOINT_AFTER_REQUEST: &str = "runner-control-plane-checkpoint-after-request.json";
    const CHECKPOINT_AFTER_RESULT: &str = "runner-control-plane-checkpoint-after-result.json";
    const CHECKPOINT_REPLAY_REQUEST: &str = "runner-control-plane-checkpoint-replay-request.json";
    const CHECKPOINT_REPLAY_RESULT: &str = "runner-control-plane-checkpoint-replay-result.json";

    const REVIEW_FILES: [&str; 5] = [
        BOOTSTRAP_REQUEST,
        BOOTSTRAP_RESULT,
        POST_QUEUE_REQUEST,
        POST_QUEUE_RESULT,
        VOLUME_KEY,
    ];
    const AUTO_FILES: [&str; 7] = [
        AUTO_BOOTSTRAP_REQUEST,
        AUTO_BOOTSTRAP_RESULT,
        AUTO_POST_QUEUE_REQUEST,
        AUTO_POST_QUEUE_RESULT,
        AUTO_VOLUME_KEY,
        AUTO_INSPECT_REQUEST,
        AUTO_INSPECT_RESULT,
    ];
    const CHECKPOINT_FILES: [&str; 6] = [
        CHECKPOINT_BEFORE_REQUEST,
        CHECKPOINT_BEFORE_RESULT,
        CHECKPOINT_AFTER_REQUEST,
        CHECKPOINT_AFTER_RESULT,
        CHECKPOINT_REPLAY_REQUEST,
        CHECKPOINT_REPLAY_RESULT,
    ];

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum CheckpointFileStage {
        Before,
        After,
        Replay,
    }

    impl CheckpointFileStage {
        fn request_name(self) -> &'static str {
            match self {
                Self::Before => CHECKPOINT_BEFORE_REQUEST,
                Self::After => CHECKPOINT_AFTER_REQUEST,
                Self::Replay => CHECKPOINT_REPLAY_REQUEST,
            }
        }

        fn result_name(self) -> &'static str {
            match self {
                Self::Before => CHECKPOINT_BEFORE_RESULT,
                Self::After => CHECKPOINT_AFTER_RESULT,
                Self::Replay => CHECKPOINT_REPLAY_RESULT,
            }
        }

        fn request_stage(self) -> fixture::RunnerControlPlaneCheckpointStage {
            match self {
                Self::Before => fixture::RunnerControlPlaneCheckpointStage::BeforeRecovery,
                Self::After => fixture::RunnerControlPlaneCheckpointStage::AfterRecovery,
                Self::Replay => fixture::RunnerControlPlaneCheckpointStage::AfterExactReplay,
            }
        }
    }

    fn select_checkpoint_stage(present: [bool; 6]) -> Checked<Option<CheckpointFileStage>> {
        match present {
            [false, false, false, false, false, false] => Ok(None),
            [true, false, false, false, false, false] => Ok(Some(CheckpointFileStage::Before)),
            [true, true, true, false, false, false] => Ok(Some(CheckpointFileStage::After)),
            [true, true, true, true, true, false] => Ok(Some(CheckpointFileStage::Replay)),
            _ => Err("checkpoint_state_conflict"),
        }
    }

    fn select_auto_family(review_present: bool, auto_present: bool) -> Checked<bool> {
        require(
            !(review_present && auto_present),
            "mixed_fixture_file_families",
        )?;
        Ok(auto_present)
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

    fn bootstrap_failure_code(error: &anyhow::Error) -> &'static str {
        // Match only closed stage labels; never emit underlying DB/PII/key errors.
        for (label, code) in [
            (
                "fixture_fleet_initial_status",
                "bootstrap_fleet_initial_status",
            ),
            (
                "fixture legacy inventory already initialized",
                "bootstrap_fleet_not_initial",
            ),
            (
                "fixture_fleet_inventory_begin",
                "bootstrap_fleet_inventory_begin",
            ),
            (
                "fixture_fleet_inventory_ready",
                "bootstrap_fleet_inventory_ready",
            ),
            (
                "fixture_fleet_inventory_status",
                "bootstrap_fleet_inventory_status",
            ),
            (
                "fixture_fleet_cutover_begin",
                "bootstrap_fleet_cutover_begin",
            ),
            (
                "fixture_fleet_cutover_ready",
                "bootstrap_fleet_cutover_ready",
            ),
            ("fixture_fleet_final_status", "bootstrap_fleet_final_status"),
            ("fixture fleet not ready", "bootstrap_fleet_not_ready"),
            ("fixture_volume_admission", "bootstrap_volume_admission"),
            ("fixture_volume_enrollment", "bootstrap_volume_enrollment"),
            (
                "fixture_volume_runtime_grant",
                "bootstrap_volume_runtime_grant",
            ),
            (
                "fixture_volume_instance_claim",
                "bootstrap_volume_instance_claim",
            ),
            (
                "fixture_volume_sign_attestation",
                "bootstrap_volume_sign_attestation",
            ),
            (
                "fixture_volume_record_attestation",
                "bootstrap_volume_record_attestation",
            ),
            ("fixture_volume_activation", "bootstrap_volume_activation"),
            ("fixture_volume_fleet", "bootstrap_volume_fleet"),
            ("fixture_bootstrap_volume", "bootstrap_volume"),
            ("fixture_bootstrap_readiness", "bootstrap_readiness"),
            (
                "fixture managed readiness unavailable",
                "bootstrap_readiness_unavailable",
            ),
            (
                "fixture_bootstrap_runtime_descriptors",
                "bootstrap_runtime_descriptors",
            ),
            ("fixture_bootstrap_eligibility", "bootstrap_eligibility"),
            (
                "fixture signed posting not preparable",
                "bootstrap_not_preparable",
            ),
            (
                "fixture signed job authority unavailable",
                "bootstrap_job_authority",
            ),
            ("fixture_bootstrap_entitlement", "bootstrap_entitlement"),
            ("fixture cloud entitlement", "bootstrap_entitlement"),
            (
                "fixture requires saved candidate",
                "bootstrap_saved_candidate",
            ),
            (
                "fixture requires saved onboarding",
                "bootstrap_saved_onboarding",
            ),
            ("fixture track missing", "bootstrap_track"),
            ("fixture track identity missing", "bootstrap_identity"),
            (
                "fixture track requires verified identity",
                "bootstrap_identity",
            ),
            (
                "fixture account is not fresh",
                "bootstrap_account_not_fresh",
            ),
            ("fixture fleet is not fresh", "bootstrap_fleet_not_fresh"),
        ] {
            if error.chain().any(|cause| cause.to_string() == label) {
                return code;
            }
        }
        "bootstrap_lifecycle"
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

    fn absent(path: &Path) -> Checked<bool> {
        match fs::symlink_metadata(path) {
            Ok(_) => Ok(false),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
            Err(_) => Err("private_path_inspection"),
        }
    }

    fn private_result_bytes<T: Serialize>(value: &T) -> Checked<Vec<u8>> {
        let bytes = serde_json::to_vec(value).map_err(|_| "result_encode")?;
        require(!bytes.is_empty() && bytes.len() <= 65_536, "result_size")?;
        Ok(bytes)
    }

    fn write_private<T: Serialize>(root: &RootGuard, name: &str, value: &T) -> Checked<()> {
        root.revalidate()?;
        let bytes = private_result_bytes(value)?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(root.path.join(name))
            .map_err(|_| "result_create_new")?;
        output.write_all(&bytes).map_err(|_| "result_write")?;
        output.sync_all().map_err(|_| "result_sync")?;
        private_metadata(&root.path.join(name), root.uid, false)?;
        root.revalidate()
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
        let mut review_present = false;
        let mut auto_present = false;
        for name in REVIEW_FILES {
            review_present |= !absent(&root.path.join(name))?;
        }
        for name in AUTO_FILES {
            auto_present |= !absent(&root.path.join(name))?;
        }
        let mut checkpoint_present = [false; 6];
        for (index, name) in CHECKPOINT_FILES.into_iter().enumerate() {
            checkpoint_present[index] = !absent(&root.path.join(name))?;
        }
        let checkpoint_stage = select_checkpoint_stage(checkpoint_present)?;
        let auto = select_auto_family(review_present, auto_present)?;
        let (
            bootstrap_request_name,
            bootstrap_result_name,
            post_request_name,
            post_result_name,
            key_name,
        ) = if auto {
            (
                AUTO_BOOTSTRAP_REQUEST,
                AUTO_BOOTSTRAP_RESULT,
                AUTO_POST_QUEUE_REQUEST,
                AUTO_POST_QUEUE_RESULT,
                AUTO_VOLUME_KEY,
            )
        } else {
            (
                BOOTSTRAP_REQUEST,
                BOOTSTRAP_RESULT,
                POST_QUEUE_REQUEST,
                POST_QUEUE_RESULT,
                VOLUME_KEY,
            )
        };
        let bootstrap = absent(&root.path.join(bootstrap_result_name))?;
        let inspect =
            checkpoint_stage.is_none() && auto && !absent(&root.path.join(post_result_name))?;
        require(
            if let Some(stage) = checkpoint_stage {
                absent(&root.path.join(stage.result_name()))?
            } else if inspect {
                absent(&root.path.join(AUTO_INSPECT_RESULT))?
            } else {
                absent(&root.path.join(post_result_name))?
            },
            "result_already_exists",
        )?;
        if checkpoint_stage.is_none() && auto && !inspect {
            require(
                absent(&root.path.join(AUTO_INSPECT_REQUEST))?
                    && absent(&root.path.join(AUTO_INSPECT_RESULT))?,
                "inspection_state_conflict",
            )?;
        }
        if bootstrap {
            require(
                checkpoint_stage.is_none()
                    && !inspect
                    && absent(&root.path.join(key_name))?
                    && absent(&root.path.join(post_request_name))?,
                "bootstrap_state_conflict",
            )?;
        }
        if checkpoint_stage.is_some() {
            for name in [
                bootstrap_request_name,
                bootstrap_result_name,
                post_request_name,
                post_result_name,
                key_name,
            ] {
                require(
                    !absent(&root.path.join(name))?,
                    "checkpoint_file_family_incomplete",
                )?;
            }
        }
        // Parse and validate every private file before opening the pool or mutating anything.
        let bootstrap_request = if bootstrap {
            let bytes = read_private(&root.path.join(bootstrap_request_name), root.uid, 4096)?;
            Some(
                serde_json::from_slice::<fixture::RunnerControlPlaneBootstrapRequest>(&bytes)
                    .map_err(|_| "bootstrap_request_schema")?,
            )
        } else {
            None
        };
        let post_queue = if bootstrap || inspect || checkpoint_stage.is_some() {
            None
        } else {
            let bytes = read_private(&root.path.join(post_request_name), root.uid, 4096)?;
            let request =
                serde_json::from_slice::<fixture::RunnerControlPlanePostQueueRequest>(&bytes)
                    .map_err(|_| "post_queue_request_schema")?;
            let bytes = read_private(&root.path.join(bootstrap_result_name), root.uid, 65_536)?;
            let result =
                serde_json::from_slice::<fixture::RunnerControlPlaneBootstrapResult>(&bytes)
                    .map_err(|_| "bootstrap_result_schema")?;
            let bytes = read_private(&root.path.join(key_name), root.uid, 4096)?;
            let key = serde_json::from_slice::<fixture::RunnerControlPlanePrivateVolumeKey>(&bytes)
                .map_err(|_| "volume_key_schema")?;
            Some((request, result, key))
        };
        let inspection = if inspect {
            let request =
                serde_json::from_slice::<fixture::RunnerControlPlaneInspectAutoSubmitClaimRequest>(
                    &read_private(&root.path.join(AUTO_INSPECT_REQUEST), root.uid, 4096)?,
                )
                .map_err(|_| "inspect_auto_submit_request_schema")?;
            let bootstrap = serde_json::from_slice::<fixture::RunnerControlPlaneBootstrapResult>(
                &read_private(&root.path.join(AUTO_BOOTSTRAP_RESULT), root.uid, 65_536)?,
            )
            .map_err(|_| "bootstrap_result_schema")?;
            let post = serde_json::from_slice::<fixture::RunnerControlPlanePostQueueResult>(
                &read_private(&root.path.join(AUTO_POST_QUEUE_RESULT), root.uid, 65_536)?,
            )
            .map_err(|_| "post_queue_result_schema")?;
            let key = serde_json::from_slice::<fixture::RunnerControlPlanePrivateVolumeKey>(
                &read_private(&root.path.join(AUTO_VOLUME_KEY), root.uid, 4096)?,
            )
            .map_err(|_| "volume_key_schema")?;
            Some((request, bootstrap, post, key))
        } else {
            None
        };
        let checkpoint_inspection = if let Some(stage) = checkpoint_stage {
            let request = serde_json::from_slice::<
                fixture::RunnerControlPlaneInspectCheckpointRecoveryRequest,
            >(&read_private(
                &root.path.join(stage.request_name()),
                root.uid,
                4096,
            )?)
            .map_err(|_| "inspect_checkpoint_request_schema")?;
            require(
                request.stage == stage.request_stage(),
                "inspect_checkpoint_stage_scope",
            )?;
            let bootstrap = serde_json::from_slice::<fixture::RunnerControlPlaneBootstrapResult>(
                &read_private(&root.path.join(bootstrap_result_name), root.uid, 65_536)?,
            )
            .map_err(|_| "bootstrap_result_schema")?;
            let post = serde_json::from_slice::<fixture::RunnerControlPlanePostQueueResult>(
                &read_private(&root.path.join(post_result_name), root.uid, 65_536)?,
            )
            .map_err(|_| "post_queue_result_schema")?;
            let key = serde_json::from_slice::<fixture::RunnerControlPlanePrivateVolumeKey>(
                &read_private(&root.path.join(key_name), root.uid, 4096)?,
            )
            .map_err(|_| "volume_key_schema")?;
            Some((stage, request, bootstrap, post, key))
        } else {
            None
        };
        let raw = required("BLUEY_TEST_POSTGRES_URL")?;
        let (config, port) = database_config(&raw)?;
        let mut client = config
            .connect(postgres::NoTls)
            .map_err(|_| "postgres_connect")?;
        validate_database(&mut client, &root, port)?;
        root.revalidate()?;
        // No init/migrations: ordinary API startup must already have installed the schema.
        let pool_url = format!("{raw}&connect_timeout=5&options=-c%20statement_timeout%3D5000%20-c%20lock_timeout%3D5000");
        let pool = db::open_postgres_pool(&pool_url).map_err(|_| "postgres_pool")?;
        if let Some(request) = bootstrap_request {
            let mut seed = [0_u8; 32];
            rand::rngs::OsRng.fill_bytes(&mut seed);
            root.revalidate()?;
            validate_database(&mut client, &root, port)?;
            let (result, key) = if auto {
                fixture::bootstrap_auto_submit(&pool, &request, seed)
            } else {
                fixture::bootstrap(&pool, &request, seed)
            }
            .map_err(|error| bootstrap_failure_code(&error))?;
            root.revalidate()?;
            validate_database(&mut client, &root, port)?;
            // Key first; an interrupted publication is not retryable bootstrap state.
            write_private(&root, key_name, &key)?;
            write_private(&root, bootstrap_result_name, &result)?;
        } else if let Some((request, bootstrap, key)) = post_queue {
            root.revalidate()?;
            validate_database(&mut client, &root, port)?;
            let result = if auto {
                fixture::post_queue_auto_submit(&pool, &request, &bootstrap, &key)
            } else {
                fixture::post_queue(&pool, &request, &bootstrap, &key)
            }
            .map_err(|_| "post_queue_readonly_handoff")?;
            root.revalidate()?;
            validate_database(&mut client, &root, port)?;
            write_private(&root, post_result_name, &result)?;
        } else if let Some((request, bootstrap, post, key)) = inspection {
            root.revalidate()?;
            validate_database(&mut client, &root, port)?;
            let result =
                fixture::inspect_auto_submit_claim(&pool, &request, &bootstrap, &post, &key)
                    .map_err(|_| "inspect_auto_submit_readonly_handoff")?;
            root.revalidate()?;
            validate_database(&mut client, &root, port)?;
            write_private(&root, AUTO_INSPECT_RESULT, &result)?;
        } else if let Some((stage, request, bootstrap, post, key)) = checkpoint_inspection {
            root.revalidate()?;
            validate_database(&mut client, &root, port)?;
            let result =
                fixture::inspect_checkpoint_recovery(&pool, &request, &bootstrap, &post, &key)
                    .map_err(|_| "inspect_checkpoint_recovery_readonly_handoff")?;
            root.revalidate()?;
            validate_database(&mut client, &root, port)?;
            write_private(&root, stage.result_name(), &result)?;
        } else {
            return Err("operation_missing");
        }
        Ok(())
    }

    pub(super) fn main() {
        // Never expose DB errors, panics, connection strings, tokens or private signing material.
        std::panic::set_hook(Box::new(|_| {}));
        let outcome = std::panic::catch_unwind(run).unwrap_or(Err("fixture_assertion_failed"));
        if let Err(code) = outcome {
            eprintln!("{{\"event\":\"runner_control_plane_fixture_failed\",\"code\":\"{code}\"}}");
            std::process::exit(1);
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn auto_file_family_is_separate_and_mixed_families_fail_closed() {
            assert!(!select_auto_family(false, false).unwrap());
            assert!(!select_auto_family(true, false).unwrap());
            assert!(select_auto_family(false, true).unwrap());
            assert!(select_auto_family(true, true).is_err());
            for name in AUTO_FILES {
                assert!(!REVIEW_FILES.contains(&name));
                assert!(name.starts_with("runner-control-plane-"));
                assert!(!name.contains('/') && name.ends_with(".json"));
            }
            for name in CHECKPOINT_FILES {
                assert!(!REVIEW_FILES.contains(&name) && !AUTO_FILES.contains(&name));
                assert!(name.starts_with("runner-control-plane-checkpoint-"));
                assert!(!name.contains('/') && name.ends_with(".json"));
            }
        }

        #[test]
        fn checkpoint_file_sequence_accepts_only_one_pending_scoped_stage() {
            assert_eq!(select_checkpoint_stage([false; 6]).unwrap(), None);
            assert_eq!(
                select_checkpoint_stage([true, false, false, false, false, false]).unwrap(),
                Some(CheckpointFileStage::Before)
            );
            assert_eq!(
                select_checkpoint_stage([true, true, true, false, false, false]).unwrap(),
                Some(CheckpointFileStage::After)
            );
            assert_eq!(
                select_checkpoint_stage([true, true, true, true, true, false]).unwrap(),
                Some(CheckpointFileStage::Replay)
            );
            for invalid in [
                [true, true, false, false, false, false],
                [false, false, true, false, false, false],
                [true, true, true, true, false, false],
                [true, true, true, true, true, true],
            ] {
                assert!(select_checkpoint_stage(invalid).is_err());
            }
            assert_eq!(
                CheckpointFileStage::Before.request_stage(),
                fixture::RunnerControlPlaneCheckpointStage::BeforeRecovery
            );
            assert_eq!(
                CheckpointFileStage::After.request_stage(),
                fixture::RunnerControlPlaneCheckpointStage::AfterRecovery
            );
            assert_eq!(
                CheckpointFileStage::Replay.request_stage(),
                fixture::RunnerControlPlaneCheckpointStage::AfterExactReplay
            );
        }

        #[test]
        fn auto_bootstrap_and_post_queue_keep_closed_request_schemas() {
            let bootstrap = serde_json::json!({"version":1,"operation":"bootstrap_auto_submit",
                "account_id":"a","track_id":"t","worker_id":"w"});
            assert!(
                serde_json::from_value::<fixture::RunnerControlPlaneBootstrapRequest>(
                    bootstrap.clone()
                )
                .is_ok()
            );
            let post = serde_json::json!({"version":1,"operation":"post_queue_auto_submit",
                "account_id":"a","application_id":"p","run_id":"r","browser_session_id":"s",
                "browser_profile_id":"b","command_id":"c","request_id":"q","workflow_id":"w"});
            assert!(
                serde_json::from_value::<fixture::RunnerControlPlanePostQueueRequest>(post.clone())
                    .is_ok()
            );
            for field in ["environment", "claim", "lease_token", "resume_content"] {
                let mut invalid = bootstrap.clone();
                invalid[field] = serde_json::json!("forbidden");
                assert!(
                    serde_json::from_value::<fixture::RunnerControlPlaneBootstrapRequest>(invalid)
                        .is_err()
                );
                let mut invalid = post.clone();
                invalid[field] = serde_json::json!("forbidden");
                assert!(
                    serde_json::from_value::<fixture::RunnerControlPlanePostQueueRequest>(invalid)
                        .is_err()
                );
            }
        }

        #[test]
        fn auto_inspection_private_output_is_bounded_and_minimized() {
            let result = fixture::RunnerControlPlaneInspectAutoSubmitClaimResult {
                version: 1,
                operation: "inspect_auto_submit_claim",
                account_id: "synthetic-account".into(),
                application_id: "synthetic-application".into(),
                run_id: "synthetic-run".into(),
                browser_session_id: "cloud-synthetic-application".into(),
                browser_profile_id: "synthetic-profile".into(),
                lease_token_sha256: "a".repeat(64),
                lease_fence: 1,
                ats_account_count: 1,
                ats_application_count: 1,
                ats_run_count: 1,
                binding_sha256: "b".repeat(64),
                nonce_sha256: "a".repeat(64),
                phase: "preflight".into(),
                binding_fence: 0,
                expires_future: true,
                exact_frozen_authority: true,
                exact_prepared_lease: true,
                no_phase_b_or_invalidation: true,
            };
            let bytes = private_result_bytes(&result).unwrap();
            let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            let fields = value.as_object().unwrap();
            assert_eq!(fields.len(), 20);
            for forbidden in [
                "lease_token",
                "frozen_certification_base64url",
                "resume_content",
                "signing_seed_base64url",
                "application_email",
            ] {
                assert!(!fields.contains_key(forbidden));
            }
            assert!(bytes.len() < 4096);
            assert!(private_result_bytes(&"x".repeat(65_536)).is_err());
        }

        #[test]
        fn checkpoint_private_output_has_only_the_closed_result_contract() {
            let result = fixture::RunnerControlPlaneInspectCheckpointRecoveryResult {
                version: 1,
                operation: "inspect_checkpoint_recovery",
                stage: fixture::RunnerControlPlaneCheckpointStage::AfterRecovery,
                account_id: "synthetic-account".into(),
                application_id: "synthetic-application".into(),
                run_id: "synthetic-run".into(),
                browser_session_id: "cloud-synthetic-application".into(),
                browser_profile_id: "synthetic-profile".into(),
                submission_mode: "review_first".into(),
                application_stable_sha256: "a".repeat(64),
                session_stable_sha256: "b".repeat(64),
                application_state: "failed".into(),
                session_status: "failed".into(),
                session_current_step: "Browser run stopped before submission".into(),
                submitted_absent: true,
                takeover_absent: true,
                lease_phase: "released".into(),
                attempt_status: "released".into(),
                cloud_recovery: Some(fixture::RunnerControlPlaneCloudRecoverySummary {
                    schema_version: 1,
                    status: "released".into(),
                    checkpoint_version: 2,
                    checkpoint_phase: "prepared".into(),
                    lease_phase: "prepared".into(),
                    recorded_at_ms: 30,
                    run_id: "synthetic-run".into(),
                }),
                cloud_recovery_sha256: Some("c".repeat(64)),
            };
            let bytes = private_result_bytes(&result).unwrap();
            let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            let fields = value.as_object().unwrap();
            assert_eq!(fields.len(), 20);
            assert_eq!(fields["cloud_recovery"].as_object().unwrap().len(), 7);
            for forbidden in [
                "lease_token",
                "data_key",
                "candidate",
                "resume_content",
                "application_json",
                "session_json",
                "packet",
                "answers",
            ] {
                assert!(!fields.contains_key(forbidden));
            }
            assert!(bytes.len() < 4096);
        }

        #[test]
        fn bootstrap_diagnostics_expose_only_closed_stage_labels() {
            let private = "synthetic private payload must never be exposed";
            let error = anyhow::anyhow!(private)
                .context("fixture_volume_enrollment")
                .context("fixture_bootstrap_volume");
            assert_eq!(
                bootstrap_failure_code(&error),
                "bootstrap_volume_enrollment"
            );
            assert_eq!(
                bootstrap_failure_code(&anyhow::anyhow!(private)),
                "bootstrap_lifecycle"
            );
        }

        #[test]
        fn data_key_requires_real_length_and_accepts_standard_padded_base64() {
            let key: Vec<u8> = (0..32).map(|index| index * 7).collect();
            for encoded in [
                base64::engine::general_purpose::STANDARD.encode(&key),
                base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&key),
                hex::encode(&key),
            ] {
                assert!(validate_data_key(&encoded).is_ok());
            }
            assert!(validate_data_key(&hex::encode([0; 32])).is_err());
            assert!(validate_data_key(&hex::encode(Sha256::digest(
                b"bluey-jobs-development-only-key"
            )))
            .is_err());
        }

        #[test]
        fn connection_scope_rejects_remote_default_or_extra_options() {
            let url = format!("postgresql://bluey_jobs_test:{}@127.0.0.1:15432/bluey_jobs_nonprod?sslmode=disable", "a".repeat(64));
            assert!(database_config(&url).is_ok());
            for invalid in [
                url.replace("127.0.0.1", "localhost"),
                url.replace("15432", "5432"),
                url.replace("bluey_jobs_nonprod", "postgres"),
                format!("{url}&host=remote.invalid"),
            ] {
                assert!(database_config(&invalid).is_err());
            }
        }

        #[test]
        fn bootstrap_is_closed_and_has_no_scope_or_predicted_run() {
            assert!(serde_json::from_str::<fixture::RunnerControlPlaneBootstrapRequest>(
                r#"{"version":1,"operation":"bootstrap","account_id":"a","track_id":"t","worker_id":"w","environment":"production"}"#).is_err());
            assert!(serde_json::from_str::<fixture::RunnerControlPlaneBootstrapRequest>(
                r#"{"version":1,"operation":"bootstrap","account_id":"a","track_id":"t","worker_id":"w","run_id":"r"}"#).is_err());
            assert!(serde_json::from_str::<fixture::RunnerControlPlanePostQueueRequest>(
                r#"{"version":1,"operation":"post_queue","account_id":"a","application_id":"p","run_id":"r","browser_session_id":"s","browser_profile_id":"b","command_id":"c","request_id":"q","workflow_id":"f","claim":true}"#).is_err());
        }
    }
}

#[cfg(unix)]
fn main() {
    unix::main();
}

#[cfg(not(unix))]
fn main() {
    eprintln!(
        "{{\"event\":\"runner_control_plane_fixture_failed\",\"code\":\"unsupported_platform\"}}"
    );
    std::process::exit(1);
}

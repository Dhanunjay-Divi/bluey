//! Private, debug-only bridge for the owned browser/API/PostgreSQL qualification.
//! No HTTP seed route, production credentials, candidate edits, or employer calls.
#![forbid(unsafe_code)]

#[cfg(not(debug_assertions))]
compile_error!("the nonproduction browser fixture must never be built in release mode");

#[cfg(unix)]
mod unix {
    use base64::Engine;
    use bluey_server::{
        auth::password::hash_password,
        db::{self, accounts::Account, jobs},
    };
    use serde::{Deserialize, Serialize};
    use sha2::{Digest, Sha256};
    use std::collections::BTreeMap;
    use std::fs::{self, File, Metadata, OpenOptions};
    use std::io::{Read, Write};
    use std::os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt};
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    type Checked<T> = Result<T, &'static str>;
    const MARKER: &str = ".bluey-test-run-v2";
    const REQUEST: &str = "browser-packet-fixture-request.json";
    const RESULT: &str = "browser-packet-fixture-result.json";
    const PASSWORD_HASH: &str = "browser-fixture-password-hash";
    const DURABLE_STATE: &str = "bluey-jobs-qa-draft";
    const DURABLE_REQUEST: &str = "durable-qa-posting-request.json";
    const DURABLE_CONSUMED: &str = "durable-qa-posting-request.consumed.json";
    const DURABLE_RESULT: &str = "durable-qa-posting-result.json";
    const DURABLE_IMPORTED_RESUME_REQUEST: &str = "durable-imported-resume-qa-posting-request.json";
    const DURABLE_IMPORTED_RESUME_CONSUMED: &str =
        "durable-imported-resume-qa-posting-request.consumed.json";
    const DURABLE_IMPORTED_RESUME_RESULT: &str = "durable-imported-resume-qa-posting-result.json";
    const DURABLE_REVIEW_FIRST_REQUEST: &str =
        "durable-review-first-mock-receipt-posting-request.json";
    const DURABLE_REVIEW_FIRST_CONSUMED: &str =
        "durable-review-first-mock-receipt-posting-request.consumed.json";
    const DURABLE_REVIEW_FIRST_RESULT: &str =
        "durable-review-first-mock-receipt-posting-result.json";
    const MOCK_RECEIPT_REQUEST: &str = "durable-review-first-mock-receipt-request.json";
    const MOCK_RECEIPT_CONSUMED: &str = "durable-review-first-mock-receipt-request.consumed.json";
    const MOCK_RECEIPT_RESULT: &str = "durable-review-first-mock-receipt-result.json";
    const DURABLE_IMPORTED_RESUME_SHA256: &str =
        "92b41ea87bbc39031cac457edb51ba6693bb9e5773b0e18f5c370fee0e2a1cdb";
    const LOCAL_QA_MARKER: &str = ".bluey-local-qa.json";
    const LOCAL_QA_READY: &str = ".local-qa-postgres-ready.json";
    const LOCAL_QA_PASSWORD: &str = ".local-qa-postgres-password";
    const LOCAL_QA_BOOTSTRAP: &str = ".local-qa-bootstrap.json";
    const LOCAL_QA_CONFIGS: [&str; 5] = [
        "postgresql.conf",
        "postgresql.auto.conf",
        "pg_hba.conf",
        "pg_ident.conf",
        "PG_VERSION",
    ];

    #[derive(Deserialize, Serialize)]
    #[serde(deny_unknown_fields)]
    struct Request {
        version: u8,
        account_id: String,
        track_id: String,
        #[serde(default)]
        queue_entitlement: bool,
    }

    #[derive(Serialize)]
    struct Response {
        version: u8,
        job_id: String,
    }

    #[derive(Serialize)]
    struct DurableImportedResumeResponse {
        version: u8,
        job_id: String,
        source_resume_asset_id: String,
        source_resume_sha256: String,
    }

    #[derive(Deserialize, Serialize)]
    #[serde(deny_unknown_fields)]
    struct DurableRequest {
        version: u8,
        candidate_id: String,
        track_id: String,
        expected_system_identifier: String,
        profile_scope: String,
        job_scope: DurableJobScope,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source_resume_asset_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source_resume_sha256: Option<String>,
    }

    #[derive(Deserialize, Serialize)]
    #[serde(deny_unknown_fields)]
    struct DurableJobScope {
        title: String,
        location: String,
        skills: Vec<String>,
    }

    #[derive(Clone, Copy)]
    enum PostingKind {
        W2,
        C2c,
        ImportedResume,
        ReviewFirstMockReceipt,
    }

    impl PostingKind {
        fn durable_version(self) -> u8 {
            match self {
                Self::W2 => 1,
                Self::C2c => 2,
                Self::ImportedResume => 3,
                Self::ReviewFirstMockReceipt => 4,
            }
        }

        fn employment(self) -> &'static str {
            match self {
                Self::W2 | Self::ImportedResume | Self::ReviewFirstMockReceipt => "full_time",
                Self::C2c => "contract",
            }
        }

        fn engagement(self) -> &'static str {
            match self {
                Self::W2 | Self::ImportedResume | Self::ReviewFirstMockReceipt => "w2",
                Self::C2c => "c2c",
            }
        }

        fn imported(self) -> bool {
            matches!(self, Self::ImportedResume | Self::ReviewFirstMockReceipt)
        }

        fn files(self) -> (&'static str, &'static str, &'static str) {
            match self {
                Self::W2 | Self::C2c => (DURABLE_REQUEST, DURABLE_CONSUMED, DURABLE_RESULT),
                Self::ImportedResume => (
                    DURABLE_IMPORTED_RESUME_REQUEST,
                    DURABLE_IMPORTED_RESUME_CONSUMED,
                    DURABLE_IMPORTED_RESUME_RESULT,
                ),
                Self::ReviewFirstMockReceipt => (
                    DURABLE_REVIEW_FIRST_REQUEST,
                    DURABLE_REVIEW_FIRST_CONSUMED,
                    DURABLE_REVIEW_FIRST_RESULT,
                ),
            }
        }
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct LocalQaMarker {
        schema_version: u8,
        kind: String,
        qa_id: String,
        owner_uid: u32,
        created_at: String,
        bucket: String,
        prefix: String,
        secret_hashes: BTreeMap<String, String>,
        root_identity: String,
        directory_identities: BTreeMap<String, String>,
        object_store_marker_sha256: String,
        postgres_initialization: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct LocalQaReady {
        version: u8,
        qa_id: String,
        root_identity: String,
        data_identity: String,
        system_identifier: String,
        password_sha256: String,
        config_hashes: BTreeMap<String, String>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct LocalQaBootstrap {
        version: u8,
        qa_id: String,
        root: String,
        config_sha256: String,
        intent_sha256: String,
        system_identifier: String,
        operator_id: String,
        candidate_id: String,
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

    fn durable_data_key_hash(raw: &str) -> Checked<String> {
        require(raw == raw.trim(), "durable_data_key_shape")?;
        validate_data_key(raw)?;
        Ok(sha256(format!("{raw}\n").as_bytes()))
    }

    fn validate_durable_data_key_marker(
        marker_hash: Option<&str>,
        data_key_hash: &str,
    ) -> Checked<()> {
        require(
            marker_hash == Some(data_key_hash),
            "durable_data_key_marker",
        )
    }

    fn validate_durable_password_marker(
        password: &[u8],
        marker_hash: &str,
        url_password: &str,
    ) -> Checked<()> {
        require(
            password.len() == 65
                && password[64] == b'\n'
                && password[..64].iter().all(|byte| byte.is_ascii_hexdigit())
                && sha256(password) == marker_hash
                && password[..64] == url_password.as_bytes()[..],
            "durable_password_marker",
        )
    }

    fn validate_durable_postwrite_census(
        census: (i64, bool, i64, i64, &str, i32, i32, i64),
        kind: PostingKind,
    ) -> Checked<()> {
        let (plan, local, cloud) = if matches!(kind, PostingKind::ReviewFirstMockReceipt) {
            ("cloud", 1, 1)
        } else {
            ("free", 0, 0)
        };
        require(
            census == (1, true, 0, 0, plan, local, cloud, 0),
            "durable_postwrite_census",
        )
    }

    fn same_file(left: &Metadata, right: &Metadata) -> bool {
        left.dev() == right.dev() && left.ino() == right.ino()
    }

    fn directory_identity(metadata: &Metadata) -> String {
        format!(
            "{}:{}:{}:{}",
            metadata.dev(),
            metadata.ino(),
            metadata.uid(),
            metadata.mode()
        )
    }

    fn sha256(bytes: &[u8]) -> String {
        hex::encode(Sha256::digest(bytes))
    }

    fn canonical_uuid_v4(value: &str) -> bool {
        uuid::Uuid::parse_str(value)
            .is_ok_and(|parsed| parsed.get_version_num() == 4 && parsed.to_string() == value)
    }

    fn system_identifier(value: &str) -> bool {
        value
            .parse::<u64>()
            .is_ok_and(|parsed| parsed > 0 && parsed.to_string() == value)
    }

    fn lowercase_sha256(value: &str) -> bool {
        value.len() == 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
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

    fn sync_directory(path: &Path) -> Checked<()> {
        let directory = File::open(path).map_err(|_| "private_directory_open")?;
        require(
            directory.metadata().is_ok_and(|metadata| metadata.is_dir()),
            "private_directory_identity",
        )?;
        directory.sync_all().map_err(|_| "private_directory_sync")
    }

    struct DurableGuard {
        path: PathBuf,
        identity: Metadata,
        data_path: PathBuf,
        data_identity: Metadata,
        marker: Vec<u8>,
        ready: Vec<u8>,
        bootstrap: Vec<u8>,
        uid: u32,
        system_identifier: String,
        candidate_id: String,
    }

    impl DurableGuard {
        fn load(
            root: &RootGuard,
            expected_system_identifier: &str,
            candidate_id: &str,
            data_key_hash: &str,
            url_password: &str,
        ) -> Checked<Self> {
            let path = root.path.join(DURABLE_STATE);
            require(
                fs::canonicalize(&path).ok().as_ref() == Some(&path),
                "durable_state_canonical",
            )?;
            let identity = private_metadata(&path, root.uid, true)?;
            let data_path = path.join("postgres");
            require(
                fs::canonicalize(&data_path).ok().as_ref() == Some(&data_path),
                "durable_postgres_canonical",
            )?;
            let data_identity = private_metadata(&data_path, root.uid, true)?;
            let marker = read_private(&path.join(LOCAL_QA_MARKER), root.uid, 8192)?;
            let ready = read_private(&path.join(LOCAL_QA_READY), root.uid, 8192)?;
            let bootstrap = read_private(&path.join(LOCAL_QA_BOOTSTRAP), root.uid, 8192)?;
            let state: LocalQaMarker =
                serde_json::from_slice(&marker).map_err(|_| "durable_state_marker")?;
            let ready_value: LocalQaReady =
                serde_json::from_slice(&ready).map_err(|_| "durable_ready_marker")?;
            let bootstrap_value: LocalQaBootstrap =
                serde_json::from_slice(&bootstrap).map_err(|_| "durable_bootstrap_marker")?;
            require(
                state.schema_version == 1
                    && state.kind == "bluey_jobs_durable_local_qa"
                    && canonical_uuid_v4(&state.qa_id)
                    && state.owner_uid == root.uid
                    && chrono::DateTime::parse_from_rfc3339(&state.created_at).is_ok()
                    && state.bucket.starts_with("qa-")
                    && state.bucket.len() == 35
                    && state.prefix.starts_with("qa-")
                    && state.prefix.len() == 35
                    && state.secret_hashes.len() == 4
                    && [
                        "jwtSecret",
                        "jobsDataKey",
                        "objectAccessKey",
                        "objectSecret",
                    ]
                    .iter()
                    .all(|name| {
                        state
                            .secret_hashes
                            .get(*name)
                            .is_some_and(|value| lowercase_sha256(value))
                    })
                    && state.root_identity == directory_identity(&identity)
                    && state.directory_identities.len() == 3
                    && ["objects", "postgres", "logs"]
                        .iter()
                        .all(|name| state.directory_identities.contains_key(*name))
                    && state.directory_identities.get("postgres")
                        == Some(&directory_identity(&data_identity))
                    && lowercase_sha256(&state.object_store_marker_sha256)
                    && state.postgres_initialization == "claimed",
                "durable_state_marker",
            )?;
            validate_durable_data_key_marker(
                state.secret_hashes.get("jobsDataKey").map(String::as_str),
                data_key_hash,
            )?;
            require(
                ready_value.version == 1
                    && ready_value.qa_id == state.qa_id
                    && ready_value.root_identity == state.root_identity
                    && ready_value.data_identity == directory_identity(&data_identity)
                    && ready_value.system_identifier == expected_system_identifier
                    && system_identifier(&ready_value.system_identifier)
                    && lowercase_sha256(&ready_value.password_sha256)
                    && ready_value.config_hashes.len() == LOCAL_QA_CONFIGS.len(),
                "durable_ready_marker",
            )?;
            let mut password = read_private(&path.join(LOCAL_QA_PASSWORD), root.uid, 128)?;
            validate_durable_password_marker(
                &password,
                &ready_value.password_sha256,
                url_password,
            )?;
            password.fill(0);
            for name in LOCAL_QA_CONFIGS {
                let bytes = read_private(&data_path.join(name), root.uid, 1024 * 1024)?;
                require(
                    ready_value.config_hashes.get(name) == Some(&sha256(&bytes)),
                    "durable_config_marker",
                )?;
            }
            require(
                bootstrap_value.version == 1
                    && bootstrap_value.qa_id == state.qa_id
                    && bootstrap_value.root == state.root_identity
                    && lowercase_sha256(&bootstrap_value.config_sha256)
                    && lowercase_sha256(&bootstrap_value.intent_sha256)
                    && bootstrap_value.system_identifier == ready_value.system_identifier
                    && canonical_uuid_v4(&bootstrap_value.operator_id)
                    && bootstrap_value.candidate_id == candidate_id
                    && canonical_uuid_v4(&bootstrap_value.candidate_id)
                    && bootstrap_value.operator_id != bootstrap_value.candidate_id,
                "durable_bootstrap_marker",
            )?;
            Ok(Self {
                path,
                identity,
                data_path,
                data_identity,
                marker,
                ready,
                bootstrap,
                uid: root.uid,
                system_identifier: ready_value.system_identifier,
                candidate_id: bootstrap_value.candidate_id,
            })
        }

        fn revalidate(&self, root: &RootGuard) -> Checked<()> {
            root.revalidate()?;
            let state = private_metadata(&self.path, self.uid, true)?;
            let data = private_metadata(&self.data_path, self.uid, true)?;
            require(
                same_file(&self.identity, &state)
                    && same_file(&self.data_identity, &data)
                    && fs::canonicalize(&self.path).ok().as_ref() == Some(&self.path)
                    && fs::canonicalize(&self.data_path).ok().as_ref() == Some(&self.data_path)
                    && read_private(&self.path.join(LOCAL_QA_MARKER), self.uid, 8192)?
                        == self.marker
                    && read_private(&self.path.join(LOCAL_QA_READY), self.uid, 8192)? == self.ready
                    && read_private(&self.path.join(LOCAL_QA_BOOTSTRAP), self.uid, 8192)?
                        == self.bootstrap,
                "durable_state_changed",
            )
        }
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

    fn durable_database_config(raw: &str) -> Checked<(postgres::Config, u16)> {
        let url = reqwest::Url::parse(raw).map_err(|_| "durable_postgres_url")?;
        let port = url
            .port()
            .filter(|port| *port >= 1024 && *port != 5432)
            .ok_or("durable_postgres_port")?;
        require(
            url.scheme() == "postgresql"
                && url.host_str() == Some("127.0.0.1")
                && url.username() == "bluey_qa_local"
                && url.path() == "/bluey_jobs_qa"
                && url.query() == Some("sslmode=disable")
                && url.fragment().is_none()
                && url.password().is_some_and(|password| {
                    password.len() == 64
                        && password
                            .bytes()
                            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                }),
            "durable_postgres_url_scope",
        )?;
        let mut config: postgres::Config = raw.parse().map_err(|_| "durable_postgres_config")?;
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

    fn validate_durable_database(
        client: &mut postgres::Client,
        state: &DurableGuard,
        port: u16,
    ) -> Checked<()> {
        let row = client
            .query_one(
                "SELECT current_setting('data_directory'), host(inet_server_addr()),
                inet_server_port(), current_database(), current_user,
                system_identifier::text FROM pg_control_system()",
                &[],
            )
            .map_err(|_| "durable_postgres_identity_query")?;
        require(
            row.get::<_, String>(0) == state.data_path.to_str().ok_or("durable_postgres_path")?
                && row.get::<_, String>(1) == "127.0.0.1"
                && row.get::<_, i32>(2) == i32::from(port)
                && row.get::<_, String>(3) == "bluey_jobs_qa"
                && row.get::<_, String>(4) == "bluey_qa_local"
                && row.get::<_, String>(5) == state.system_identifier,
            "durable_postgres_physical_identity",
        )?;
        let pid = read_private(&state.data_path.join("postmaster.pid"), state.uid, 8192)?;
        let lines: Vec<_> = std::str::from_utf8(&pid)
            .map_err(|_| "durable_postmaster_encoding")?
            .lines()
            .collect();
        require(
            lines.len() >= 4
                && decimal(lines[0]).is_some_and(|value| value > 1)
                && lines[1] == state.data_path.to_str().ok_or("durable_postgres_path")?
                && decimal(lines[3]) == Some(u64::from(port)),
            "durable_postmaster_identity",
        )
    }

    fn validate_durable_request(request: &DurableRequest, kind: PostingKind) -> Checked<()> {
        let imported = kind.imported();
        require(
            request.version == kind.durable_version()
                && canonical_uuid_v4(&request.candidate_id)
                && canonical_uuid_v4(&request.track_id)
                && system_identifier(&request.expected_system_identifier)
                && request.profile_scope == "candidate@bluey-qa.test"
                && request.job_scope.title
                    == if imported {
                        "Data Engineer"
                    } else {
                        "Software Engineer"
                    }
                && request.job_scope.location == "New York, NY"
                && request.job_scope.skills.len() == 2
                && request.job_scope.skills[0] == if imported { "Python" } else { "Rust" }
                && request.job_scope.skills[1] == if imported { "SQL" } else { "TypeScript" }
                && if imported {
                    request
                        .source_resume_asset_id
                        .as_deref()
                        .is_some_and(canonical_uuid_v4)
                        && request.source_resume_sha256.as_deref()
                            == Some(DURABLE_IMPORTED_RESUME_SHA256)
                } else {
                    request.source_resume_asset_id.is_none()
                        && request.source_resume_sha256.is_none()
                },
            "durable_request_scope",
        )
    }

    fn validate_c2c_dispatch_empty(client: &mut postgres::Client, account_id: &str) -> Checked<()> {
        let row = client
            .query_one(
                "SELECT NOT EXISTS(SELECT 1 FROM jobs_browser_sessions WHERE account_id=$1)
                 AND NOT EXISTS(SELECT 1 FROM jobs_auto_submit_authorizations WHERE account_id=$1)",
                &[&account_id],
            )
            .map_err(|_| "durable_c2c_dispatch_read")?;
        require(
            row.try_get::<_, bool>(0)
                .map_err(|_| "durable_c2c_dispatch_read")?,
            "durable_c2c_dispatch_not_empty",
        )
    }

    fn validate_review_first_effects_empty(
        client: &mut postgres::Client,
        account_id: &str,
    ) -> Checked<()> {
        validate_c2c_dispatch_empty(client, account_id)?;
        let row = client.query_one(
            "SELECT (SELECT balance_cents FROM accounts WHERE id=$1)=0
              AND NOT EXISTS(SELECT 1 FROM balance_ledger_entries WHERE account_id=$1)
              AND NOT EXISTS(SELECT 1 FROM jobs_attempt_reservations WHERE account_id=$1)
              AND NOT EXISTS(SELECT 1 FROM jobs_execution_leases WHERE account_id=$1)
              AND NOT EXISTS(SELECT 1 FROM jobs_workflow_commands WHERE account_id=$1)
              AND NOT EXISTS(SELECT 1 FROM jobs_workflow_executions WHERE account_id=$1)
              AND NOT EXISTS(SELECT 1 FROM jobs_run_events WHERE account_id=$1)
              AND NOT EXISTS(SELECT 1 FROM jobs_interventions WHERE account_id=$1)
              AND NOT EXISTS(SELECT 1 FROM jobs_communication_actions WHERE account_id=$1)
              AND NOT EXISTS(SELECT 1 FROM jobs_application_evidence WHERE account_id=$1)
              AND NOT EXISTS(SELECT 1 FROM jobs_submission_evidence_capacity WHERE account_id=$1)
              AND NOT EXISTS(SELECT 1 FROM jobs_managed_cloud_irreversible_effect_receipts WHERE account_id=$1)",
            &[&account_id],
        ).map_err(|_| "durable_review_first_effects_read")?;
        require(
            row.try_get::<_, bool>(0)
                .map_err(|_| "durable_review_first_effects_read")?,
            "durable_review_first_effects_not_empty",
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

    fn synthetic_password(bytes: &[u8]) -> Checked<&str> {
        require(
            bytes.len() == 49 && bytes[48] == b'\n',
            "password_pipe_frame",
        )?;
        let value = std::str::from_utf8(&bytes[..48]).map_err(|_| "password_encoding")?;
        require(
            value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
            "password_hex",
        )?;
        Ok(value)
    }

    fn hash_synthetic_password() -> Checked<()> {
        require(
            std::env::var_os("BLUEY_DATABASE_URL").is_none(),
            "owner_database_forbidden",
        )?;
        require(
            std::env::var_os("BLUEY_TEST_POSTGRES_URL").is_none(),
            "test_database_forbidden",
        )?;
        let root = RootGuard::load()?;
        let input = fs::metadata("/dev/stdin")
            .map_err(|_| "password_pipe_metadata")?
            .file_type();
        // Node child stdio uses a Unix socketpair; a shell pipe is a FIFO.
        require(
            input.is_fifo() || input.is_socket(),
            "password_pipe_required",
        )?;
        let result = root.path.join(PASSWORD_HASH);
        require(
            fs::symlink_metadata(&result)
                .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
            "password_hash_create_new",
        )?;
        let mut bytes = Vec::new();
        std::io::stdin()
            .lock()
            .take(50)
            .read_to_end(&mut bytes)
            .map_err(|_| "password_pipe_read")?;
        let password = synthetic_password(&bytes)?;
        let hash = hash_password(password).map_err(|_| "password_hash")?;
        require(
            hash.len() == 60 && hash.starts_with("$2") && hash.as_bytes().get(4..7) == Some(b"12$"),
            "password_hash_format",
        )?;
        root.revalidate()?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&result)
            .map_err(|_| "password_hash_create_new")?;
        output
            .write_all(hash.as_bytes())
            .map_err(|_| "password_hash_write")?;
        output.write_all(b"\n").map_err(|_| "password_hash_write")?;
        output.sync_all().map_err(|_| "password_hash_sync")?;
        let held = output.metadata().map_err(|_| "password_hash_metadata")?;
        let readback = private_metadata(&result, root.uid, false)?;
        require(
            same_file(&held, &readback) && readback.len() == 61,
            "password_hash_identity",
        )?;
        root.revalidate()
    }

    fn install_signed_posting(
        pool: &db::DbPool,
        account: &Account,
        profile: &jobs::CareerProfile,
        preferences: &jobs::JobPreferences,
        track_id: &str,
        namespace: &str,
        kind: PostingKind,
    ) -> jobs::JobPosting {
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let external_id = format!("{namespace}-{suffix}");
        let now = chrono::Utc::now().timestamp_millis();
        let mut posting = jobs::JobPosting {
            id: String::new(),
            canonical_key: String::new(),
            source: "greenhouse".into(),
            external_id: external_id.clone(),
            company: "Synthetic Qualification Employer".into(),
            title: if kind.imported() {
                "Data Engineer"
            } else {
                "Software Engineer"
            }
            .into(),
            location: "New York, NY".into(),
            workplace: "hybrid".into(),
            canonical_url: format!("https://boards.greenhouse.io/blueynonprod/jobs/{external_id}"),
            description: match kind {
                PostingKind::W2 => "Full-time W2 employment. Hybrid work in New York, NY. Visa \
                    sponsorship available. Build reliable software systems with Rust and TypeScript.",
                PostingKind::C2c => "Synthetic C2C software engineering contract. Hybrid work in \
                    New York, NY. Build reliable software systems with Rust and TypeScript.",
                PostingKind::ImportedResume | PostingKind::ReviewFirstMockReceipt => "Full-time W2 Data Engineer role in New York, NY. \
                    Build reliable data pipelines with Python and SQL.",
            }
            .into(),
            compensation: match kind {
                PostingKind::W2 | PostingKind::ImportedResume | PostingKind::ReviewFirstMockReceipt => "$170k-$200k",
                PostingKind::C2c => "$90-$110/hour",
            }
            .into(),
            // Match discovery's typed employment + engagement representation.
            // Prose mentioning W2 is deliberately not positive engagement authority.
            employment_type: format!("{} {}", kind.employment(), kind.engagement()),
            track_id: track_id.into(),
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
        jobs::install_integration_test_production_positive_job_authorities(
            jobs::IntegrationTestProductionPositiveJobAuthoritiesRequest {
                pool,
                account_id: &account.id,
                posting: &posting,
                profile,
                preferences,
                canonical_employer_domain: "bluey-nonprod.example.test",
                suffix: &external_id,
                runner_kind: "local",
            },
        )
    }

    fn seed_durable_qa_posting(kind: PostingKind) -> Checked<()> {
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
        let data_key = required("BLUEY_JOBS_DATA_KEY")?;
        let data_key_hash = durable_data_key_hash(&data_key)?;
        let state_path = root.path.join(DURABLE_STATE);
        let (request_name, consumed_name, result_name) = kind.files();
        let request_path = state_path.join(request_name);
        let consumed_path = state_path.join(consumed_name);
        let result_path = state_path.join(result_name);
        require(
            fs::symlink_metadata(&consumed_path)
                .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
                && fs::symlink_metadata(&result_path)
                    .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
            "durable_request_already_consumed",
        )?;
        let request_bytes = read_private(&request_path, root.uid, 4096)?;
        let request: DurableRequest =
            serde_json::from_slice(&request_bytes).map_err(|_| "durable_request_schema")?;
        validate_durable_request(&request, kind)?;
        let mut canonical_request =
            serde_json::to_vec(&request).map_err(|_| "durable_request_encoding")?;
        canonical_request.push(b'\n');
        require(
            canonical_request == request_bytes,
            "durable_request_encoding",
        )?;
        let raw = required("BLUEY_TEST_POSTGRES_URL")?;
        let (config, port) = durable_database_config(&raw)?;
        let parsed_url = reqwest::Url::parse(&raw).map_err(|_| "durable_postgres_url")?;
        let url_password = parsed_url.password().ok_or("durable_postgres_url_scope")?;
        let state = DurableGuard::load(
            &root,
            &request.expected_system_identifier,
            &request.candidate_id,
            &data_key_hash,
            url_password,
        )?;
        require(
            state.path == state_path
                && state.system_identifier == request.expected_system_identifier
                && state.candidate_id == request.candidate_id,
            "durable_request_binding",
        )?;
        let mut client = config
            .connect(postgres::NoTls)
            .map_err(|_| "durable_postgres_connect")?;
        validate_durable_database(&mut client, &state, port)?;
        state.revalidate(&root)?;
        let pool_url = format!(
            "{raw}&connect_timeout=5&options=-c%20statement_timeout%3D5000%20-c%20lock_timeout%3D5000"
        );
        let pool = db::open_postgres_pool(&pool_url).map_err(|_| "durable_postgres_pool")?;
        let account = Account::fetch_by_id(&pool, &request.candidate_id)
            .map_err(|_| "durable_account_read")?
            .ok_or("durable_account_missing")?;
        require(
            account.id == request.candidate_id
                && account.email == "candidate@bluey-qa.test"
                && account.email_verified_at.is_some()
                && !account.is_admin
                && !account.is_temporary,
            "durable_candidate_required",
        )?;
        let existing = client
            .query_one(
                "SELECT EXISTS(SELECT 1 FROM jobs_profiles WHERE account_id=$1),
                EXISTS(SELECT 1 FROM jobs_preferences WHERE account_id=$1),
                (SELECT count(*) FROM jobs_postings WHERE account_id=$1),
                (SELECT count(*) FROM jobs_applications WHERE account_id=$1),
                (SELECT count(*) FROM jobs_packet_metering WHERE account_id=$1)",
                &[&account.id],
            )
            .map_err(|_| "durable_candidate_read")?;
        require(
            existing.get::<_, bool>(0)
                && existing.get::<_, bool>(1)
                && existing.get::<_, i64>(2) == 0
                && existing.get::<_, i64>(3) == 0
                && existing.get::<_, i64>(4) == 0,
            "durable_candidate_not_fresh",
        )?;
        let entitlement = client
            .query_opt(
                "SELECT plan, local_browser, cloud_browser, used_packets
                   FROM jobs_entitlements WHERE account_id=$1",
                &[&account.id],
            )
            .map_err(|_| "durable_entitlement_read")?
            .ok_or("durable_entitlement_missing")?;
        require(
            entitlement.get::<_, String>(0) == "free"
                && entitlement.get::<_, i32>(1) == 0
                && entitlement.get::<_, i32>(2) == 0
                && entitlement.get::<_, i64>(3) == 0,
            "durable_entitlement_scope",
        )?;
        let profile =
            jobs::get_profile(&pool, &account.id, &account.email).map_err(|_| "profile_read")?;
        let preferences =
            jobs::get_preferences(&pool, &account.id).map_err(|_| "preferences_read")?;
        if kind.imported() {
            let source = jobs::get_resume_source_asset(&pool, &account.id)
                .map_err(|_| "durable_source_read")?
                .ok_or("durable_source_missing")?;
            require(
                source.id
                    == request
                        .source_resume_asset_id
                        .as_deref()
                        .unwrap_or_default()
                    && source.file_name == "bluey-qa-resume.txt"
                    && source.media_type == "text/plain"
                    && source.file_type == "txt"
                    && source.template_status == "text_only"
                    && source.sha256 == DURABLE_IMPORTED_RESUME_SHA256
                    && source.size_bytes == 368
                    && request.source_resume_sha256.as_deref()
                        == Some(DURABLE_IMPORTED_RESUME_SHA256),
                "durable_source_scope",
            )?;
        }
        require(
            (if kind.imported() {
                profile.full_name == "Casey Rowan"
                    && profile.email == "candidate@bluey-qa.test"
                    && profile.headline == "Data Engineer"
                    && profile.current_location == "New York, NY"
                    && profile.skills == ["Python", "SQL", "PostgreSQL", "Airflow"]
                    && profile.employment.len() == 1
                    && canonical_uuid_v4(&profile.employment[0].id)
                    && profile.employment[0].company == "Bluey QA Systems"
                    && profile.employment[0].title == "Data Engineer"
                    && profile.employment[0].current
                    && profile.source_resume_asset_id
                        == request
                            .source_resume_asset_id
                            .as_deref()
                            .unwrap_or_default()
                    && profile.source_resume_sha256 == DURABLE_IMPORTED_RESUME_SHA256
                    && profile.source_resume_name == "bluey-qa-resume.txt"
                    && profile.source_resume_media_type == "text/plain"
                    && profile.source_resume_template_status == "text_only"
            } else {
                profile.full_name == "Synthetic Candidate"
                    && profile.email == request.profile_scope
                    && profile.headline == "Software Engineer"
                    && profile.current_location == "New York, NY"
                    && profile.skills.len() == 1
                    && profile.skills[0] == "TypeScript"
                    && profile.employment.len() == 1
                    && canonical_uuid_v4(&profile.employment[0].id)
                    && profile.employment[0].company == "Synthetic Systems"
                    && profile.employment[0].title == "Software Engineer"
                    && profile.employment[0].location == "New York, NY"
                    && profile.employment[0].current
            }) && profile.resume_mode == "factual"
                && profile.review_new_claims
                && profile.default_submission_mode == "review_first"
                && profile.daily_limit == jobs::CareerProfile::default().daily_limit
                && profile.onboarding_complete,
            "durable_profile_scope",
        )?;
        require(
            preferences.desired_roles.len() == 1
                && preferences.desired_roles[0]
                    == if kind.imported() {
                        "Data Engineer"
                    } else {
                        "Software Engineer"
                    }
                && preferences.desired_locations.len() == 1
                && preferences.desired_locations[0] == "New York, NY"
                && preferences.location_policy == "local"
                && preferences.remote_preference == "any"
                && preferences.employment_types.len() == 1
                && preferences.employment_types[0] == kind.employment()
                && preferences.engagement_types.len() == 1
                && preferences.engagement_types[0] == kind.engagement()
                && preferences.daily_limit == jobs::JobPreferences::default().daily_limit
                && preferences.apply_once_per_company,
            "durable_preferences_scope",
        )?;
        let identities = jobs::list_application_identities(&pool, &account.id)
            .map_err(|_| "durable_identity_read")?;
        let tracks = jobs::list_tracks(&pool, &account.id).map_err(|_| "track_read")?;
        require(
            identities.len() == 1 && tracks.len() == 1,
            "durable_identity_scope",
        )?;
        let identity = &identities[0];
        let track = &tracks[0];
        require(
            canonical_uuid_v4(&identity.id)
                && identity.email == request.profile_scope
                && identity.verification_status == "verified"
                && identity.is_default
                && track.id == request.track_id
                && track.name
                    == if kind.imported() {
                        "Synthetic Data Engineer search"
                    } else {
                        "Synthetic software engineer search"
                    }
                && track.role
                    == if kind.imported() {
                        "Data Engineer"
                    } else {
                        "Software Engineer"
                    }
                && track.locations.len() == 1
                && track.locations[0] == "New York, NY"
                && track.remote_preference == "any"
                && track.application_identity_id.as_deref() == Some(identity.id.as_str())
                && track.active
                && track.match_count == 0
                && track.policy.relevant_employment_ids.len() == 1
                && track.policy.relevant_employment_ids[0] == profile.employment[0].id
                && track.policy.employment_types.len() == 1
                && track.policy.employment_types[0] == kind.employment()
                && track.policy.engagement_types.len() == 1
                && track.policy.engagement_types[0] == kind.engagement()
                && track.policy.work_authorizations.is_empty(),
            "durable_track_scope",
        )?;
        state.revalidate(&root)?;
        validate_durable_database(&mut client, &state, port)?;
        match kind {
            PostingKind::ReviewFirstMockReceipt => {
                validate_review_first_effects_empty(&mut client, &account.id)?
            }
            PostingKind::C2c | PostingKind::ImportedResume => {
                validate_c2c_dispatch_empty(&mut client, &account.id)?
            }
            PostingKind::W2 => {}
        }
        fs::hard_link(&request_path, &consumed_path).map_err(|_| "durable_request_consume")?;
        sync_directory(&state.path)?;
        fs::remove_file(&request_path).map_err(|_| "durable_request_consume")?;
        sync_directory(&state.path)?;
        require(
            fs::symlink_metadata(&request_path)
                .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
                && read_private(&consumed_path, root.uid, 4096)? == request_bytes,
            "durable_request_consume",
        )?;
        state.revalidate(&root)?;
        validate_durable_database(&mut client, &state, port)?;
        let posting = match kind {
            PostingKind::ReviewFirstMockReceipt => {
                use jobs::integration_test_runner_control_plane::{
                    bootstrap_durable_review_first_posting, DurableReviewFirstPostingRequest,
                };
                let source_asset_id = request
                    .source_resume_asset_id
                    .as_deref()
                    .ok_or("durable_source_missing")?;
                let source_sha256 = request
                    .source_resume_sha256
                    .as_deref()
                    .ok_or("durable_source_missing")?;
                let job_id = db::run_blocking_db(|| {
                    bootstrap_durable_review_first_posting(
                        &pool,
                        DurableReviewFirstPostingRequest {
                            account_id: &account.id,
                            track_id: &track.id,
                            source_asset_id,
                            source_sha256,
                            data_directory: &state.data_path,
                            system_identifier: &state.system_identifier,
                            port,
                        },
                    )
                })
                .map_err(|_| "durable_review_first_bootstrap")?;
                jobs::get_posting(&pool, &account.id, &job_id)
                    .map_err(|_| "durable_review_first_posting_read")?
                    .ok_or("durable_review_first_posting_read")?
            }
            _ => install_signed_posting(
                &pool,
                &account,
                &profile,
                &preferences,
                &track.id,
                match kind {
                    PostingKind::W2 => "durable-qa",
                    PostingKind::C2c => "durable-c2c-qa",
                    PostingKind::ImportedResume => "durable-imported-resume-qa",
                    PostingKind::ReviewFirstMockReceipt => return Err("durable_mode_scope"),
                },
                kind,
            ),
        };
        let eligibility = jobs::evaluate_job_eligibility(&pool, &account.id, &posting, false, None)
            .map_err(|_| "eligibility_read")?;
        require(eligibility.can_prepare, "signed_posting_not_preparable")?;
        if matches!(kind, PostingKind::ReviewFirstMockReceipt) {
            require(
                posting.track_id == track.id
                    && posting.title == "Data Engineer"
                    && posting.company == "Synthetic Qualification Employer",
                "durable_review_first_posting_read",
            )?;
        }
        if matches!(kind, PostingKind::C2c) {
            require(
                eligibility.hard_failures.is_empty()
                    && !eligibility.can_queue_local
                    && !eligibility.can_queue_cloud
                    && !eligibility.can_auto_submit
                    && [
                        "employment_type_allowed",
                        "engagement_type_allowed",
                        "track_employment_type_allowed",
                        "track_engagement_type_allowed",
                    ]
                    .iter()
                    .all(|check| {
                        eligibility
                            .passed_checks
                            .iter()
                            .any(|passed| passed == *check)
                    })
                    && eligibility
                        .review_reasons
                        .iter()
                        .any(|reason| reason.code == "c2c_submission_requires_candidate"),
                "durable_c2c_candidate_only",
            )?;
        }
        state.revalidate(&root)?;
        validate_durable_database(&mut client, &state, port)?;
        let post = client
            .query_one(
                "SELECT (SELECT count(*) FROM jobs_postings WHERE account_id=$1),
                 EXISTS(SELECT 1 FROM jobs_postings WHERE account_id=$1 AND id=$2),
                 (SELECT count(*) FROM jobs_applications WHERE account_id=$1),
                 (SELECT count(*) FROM jobs_packet_metering WHERE account_id=$1),
                 (SELECT plan FROM jobs_entitlements WHERE account_id=$1),
                 (SELECT local_browser FROM jobs_entitlements WHERE account_id=$1),
                 (SELECT cloud_browser FROM jobs_entitlements WHERE account_id=$1),
                 (SELECT used_packets FROM jobs_entitlements WHERE account_id=$1)",
                &[&account.id, &posting.id],
            )
            .map_err(|_| "durable_postwrite_census")?;
        let posting_count = post
            .try_get::<_, i64>(0)
            .map_err(|_| "durable_postwrite_census")?;
        let posting_present = post
            .try_get::<_, bool>(1)
            .map_err(|_| "durable_postwrite_census")?;
        let application_count = post
            .try_get::<_, i64>(2)
            .map_err(|_| "durable_postwrite_census")?;
        let metering_count = post
            .try_get::<_, i64>(3)
            .map_err(|_| "durable_postwrite_census")?;
        let plan = post
            .try_get::<_, String>(4)
            .map_err(|_| "durable_postwrite_census")?;
        let local_browser = post
            .try_get::<_, i32>(5)
            .map_err(|_| "durable_postwrite_census")?;
        let cloud_browser = post
            .try_get::<_, i32>(6)
            .map_err(|_| "durable_postwrite_census")?;
        let used_packets = post
            .try_get::<_, i64>(7)
            .map_err(|_| "durable_postwrite_census")?;
        validate_durable_postwrite_census(
            (
                posting_count,
                posting_present,
                application_count,
                metering_count,
                plan.as_str(),
                local_browser,
                cloud_browser,
                used_packets,
            ),
            kind,
        )?;
        state.revalidate(&root)?;
        validate_durable_database(&mut client, &state, port)?;
        match kind {
            PostingKind::ReviewFirstMockReceipt => {
                validate_review_first_effects_empty(&mut client, &account.id)?
            }
            PostingKind::C2c | PostingKind::ImportedResume => {
                validate_c2c_dispatch_empty(&mut client, &account.id)?
            }
            PostingKind::W2 => {}
        }
        let mut response = if kind.imported() {
            serde_json::to_vec(&DurableImportedResumeResponse {
                version: kind.durable_version(),
                job_id: posting.id,
                source_resume_asset_id: request
                    .source_resume_asset_id
                    .clone()
                    .ok_or("durable_source_missing")?,
                source_resume_sha256: request
                    .source_resume_sha256
                    .clone()
                    .ok_or("durable_source_missing")?,
            })
        } else {
            serde_json::to_vec(&Response {
                version: kind.durable_version(),
                job_id: posting.id,
            })
        }
        .map_err(|_| "durable_result_encode")?;
        response.push(b'\n');
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&result_path)
            .map_err(|_| "durable_result_create_new")?;
        output
            .write_all(&response)
            .map_err(|_| "durable_result_write")?;
        output.sync_all().map_err(|_| "durable_result_sync")?;
        let held = output.metadata().map_err(|_| "durable_result_metadata")?;
        let readback = private_metadata(&result_path, root.uid, false)?;
        require(
            same_file(&held, &readback)
                && readback.len() == response.len() as u64
                && read_private(&result_path, root.uid, 4096)? == response,
            "durable_result_identity",
        )?;
        sync_directory(&state.path)?;
        state.revalidate(&root)
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
        if request.queue_entitlement {
            // Private fixture account only, after physical database and ownership
            // checks. Exercise the normal plan function, never payment providers
            // or hand-written execution/eligibility authority rows.
            root.revalidate()?;
            validate_database(&mut client, &root, port)?;
            let entitlement = jobs::set_entitlement_plan(&pool, &account.id, "pro")
                .map_err(|_| "fixture_queue_entitlement")?;
            require(
                entitlement.plan == "pro"
                    && entitlement.local_browser
                    && !entitlement.cloud_browser
                    && entitlement.used_packets == 0,
                "fixture_queue_entitlement",
            )?;
        }
        root.revalidate()?;
        validate_database(&mut client, &root, port)?;
        // Only the shared test-support fixture publishes signed authorities. It uses
        // production lifecycle APIs, not direct eligibility/trust SQL or external I/O.
        let posting = install_signed_posting(
            &pool,
            &account,
            &profile,
            &preferences,
            &track.id,
            "phase632",
            PostingKind::W2,
        );
        let eligibility = jobs::evaluate_job_eligibility(&pool, &account.id, &posting, false, None)
            .map_err(|_| "eligibility_read")?;
        require(eligibility.can_prepare, "signed_posting_not_preparable")?;
        if request.queue_entitlement {
            require(
                ["employment_type_allowed", "engagement_type_allowed"]
                    .iter()
                    .all(|check| {
                        eligibility
                            .passed_checks
                            .iter()
                            .any(|value| value == *check)
                    }),
                "signed_posting_typed_categories_required",
            )?;
        }
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

    fn bound_mock_object_store(
        state: &DurableGuard,
        database_port: u16,
    ) -> Checked<bluey_server::config::ObjectStorageConfig> {
        let endpoint_url = required("BLUEY_OBJECT_ENDPOINT_URL")?;
        let endpoint_port = endpoint_url
            .strip_prefix("http://127.0.0.1:")
            .and_then(|text| decimal(text).and_then(|port| u16::try_from(port).ok()))
            .ok_or("durable_mock_object_scope")?;
        let bucket = required("BLUEY_OBJECT_BUCKET")?;
        let key_prefix = required("BLUEY_OBJECT_KEY_PREFIX")?;
        let access_key_id = required("BLUEY_OBJECT_ACCESS_KEY_ID")?;
        let secret_access_key = required("BLUEY_OBJECT_SECRET_ACCESS_KEY")?;
        let marker: LocalQaMarker =
            serde_json::from_slice(&state.marker).map_err(|_| "durable_state_marker")?;
        let object_path = state.path.join("objects");
        let object_identity = private_metadata(&object_path, state.uid, true)?;
        let object_marker = read_private(
            &object_path.join(".local-qa-object-store.json"),
            state.uid,
            8192,
        )?;
        require(
            endpoint_port > 1024
                && endpoint_port != 5432
                && endpoint_port != database_port
                && bucket == marker.bucket
                && key_prefix == marker.prefix
                && marker.directory_identities.get("objects")
                    == Some(&directory_identity(&object_identity))
                && marker.object_store_marker_sha256 == sha256(&object_marker)
                && marker.secret_hashes.get("objectAccessKey")
                    == Some(&sha256(format!("{access_key_id}\n").as_bytes()))
                && marker.secret_hashes.get("objectSecret")
                    == Some(&sha256(format!("{secret_access_key}\n").as_bytes()))
                && required("BLUEY_OBJECT_REGION")? == "us-east-1"
                && required("BLUEY_OBJECT_MAX_BYTES")? == "10485760",
            "durable_mock_object_scope",
        )?;
        Ok(bluey_server::config::ObjectStorageConfig {
            endpoint_url,
            bucket,
            access_key_id,
            secret_access_key,
            region: "us-east-1".into(),
            key_prefix,
            retention_days: 1,
            max_object_bytes: 10 * 1024 * 1024,
        })
    }

    fn publish_durable_review_first_mock_receipt() -> Checked<()> {
        use bluey_server::jobs_review_first_mock_receipt_fixture::{
            publish_durable_mock_receipt, DurableMockReceiptRequest, DURABLE_MOCK_EFFECT_FLAGS,
        };
        use rand::{rngs::OsRng, RngCore};
        require(
            [
                "BLUEY_DATABASE_URL",
                "BLUEY_JWT_SECRET",
                "BLUEY_JOBS_WORKER_TOKEN",
                "BLUEY_JOBS_WORKFLOW_ORIGIN",
                "BLUEY_JOBS_WORKFLOW_TOKEN",
                "BLUEY_POSTGRES_CA_CERT_PATH",
                "BLUEY_POSTGRES_ACCEPT_INVALID_CERTS",
                "BLUEY_POSTGRES_ACCEPT_INVALID_HOSTNAMES",
                "HTTP_PROXY",
                "http_proxy",
                "HTTPS_PROXY",
                "https_proxy",
                "ALL_PROXY",
                "all_proxy",
            ]
            .iter()
            .all(|key| std::env::var_os(key).is_none()),
            "durable_mock_environment",
        )?;
        let root = RootGuard::load()?;
        let path = root.path.join(DURABLE_STATE);
        let request_path = path.join(MOCK_RECEIPT_REQUEST);
        let consumed_path = path.join(MOCK_RECEIPT_CONSUMED);
        let result_path = path.join(MOCK_RECEIPT_RESULT);
        require(
            [&consumed_path, &result_path].iter().all(|path| {
                fs::symlink_metadata(path)
                    .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
            }),
            "durable_mock_already_consumed",
        )?;
        let bytes = read_private(&request_path, root.uid, 4096)?;
        let request: DurableMockReceiptRequest =
            serde_json::from_slice(&bytes).map_err(|_| "durable_mock_request_schema")?;
        let mut canonical =
            serde_json::to_vec(&request).map_err(|_| "durable_mock_request_schema")?;
        canonical.push(b'\n');
        require(
            request.valid() && canonical == bytes,
            "durable_mock_request_schema",
        )?;
        let raw = required("BLUEY_TEST_POSTGRES_URL")?;
        let (config, port) = durable_database_config(&raw)?;
        let url = reqwest::Url::parse(&raw).map_err(|_| "durable_postgres_url")?;
        let key_hash = durable_data_key_hash(&required("BLUEY_JOBS_DATA_KEY")?)?;
        let state = DurableGuard::load(
            &root,
            &request.expected_system_identifier,
            &request.account_id,
            &key_hash,
            url.password().ok_or("durable_postgres_url_scope")?,
        )?;
        let store = bound_mock_object_store(&state, port)?;

        let mut random = [0_u8; 64];
        OsRng
            .try_fill_bytes(&mut random)
            .map_err(|_| "durable_mock_random")?;
        let jwt_secret = hex::encode(&random[..32]);
        let worker_token = hex::encode(&random[32..]);
        random.fill(0);
        // This example is a separate process. Install the single-use debug
        // bearer and fixed no-effect settings before starting the runtime or pool.
        std::env::set_var("BLUEY_JOBS_WORKER_TOKEN", &worker_token);
        std::env::set_var("BLUEY_JOBS_BETA_ENABLED", "1");
        for name in DURABLE_MOCK_EFFECT_FLAGS {
            std::env::set_var(name, "0");
        }
        let mut client = config
            .connect(postgres::NoTls)
            .map_err(|_| "durable_postgres_connect")?;
        validate_durable_database(&mut client, &state, port)?;
        state.revalidate(&root)?;
        validate_review_first_effects_empty(&mut client, &request.account_id)?;
        fs::hard_link(&request_path, &consumed_path).map_err(|_| "durable_mock_consume")?;
        sync_directory(&path)?;
        fs::remove_file(&request_path).map_err(|_| "durable_mock_consume")?;
        sync_directory(&path)?;
        require(
            read_private(&consumed_path, root.uid, 4096)? == bytes
                && fs::symlink_metadata(&request_path)
                    .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
            "durable_mock_consume",
        )?;
        state.revalidate(&root)?;
        validate_durable_database(&mut client, &state, port)?;
        let pool_url = format!(
            "{raw}&connect_timeout=5&options=-c%20statement_timeout%3D5000%20-c%20lock_timeout%3D5000"
        );
        let pool = db::open_postgres_pool(&pool_url).map_err(|_| "durable_postgres_pool")?;
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(|_| "durable_mock_actor_runtime")?;
        let result = runtime
            .block_on(publish_durable_mock_receipt(
                &pool,
                &request,
                &state.data_path,
                port,
                store,
                jwt_secret,
                &worker_token,
            ))
            .map_err(|_| "durable_mock_actor_refused")?;
        state.revalidate(&root)?;
        validate_durable_database(&mut client, &state, port)?;
        bound_mock_object_store(&state, port)?;
        require(
            read_private(&consumed_path, root.uid, 4096)? == bytes,
            "durable_mock_consume",
        )?;
        let mut result_bytes = serde_json::to_vec(&result).map_err(|_| "durable_mock_result")?;
        result_bytes.push(b'\n');
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&result_path)
            .map_err(|_| "durable_mock_result")?;
        output
            .write_all(&result_bytes)
            .map_err(|_| "durable_mock_result")?;
        output.sync_all().map_err(|_| "durable_mock_result")?;
        sync_directory(&path)?;
        require(
            read_private(&result_path, root.uid, 4096)? == result_bytes,
            "durable_mock_result",
        )
    }

    pub(super) fn main() {
        // Fixture assertions can contain SDK/DB error internals; expose only closed
        // diagnostic codes, never account data, connection strings, or signing keys.
        std::panic::set_hook(Box::new(|_| {}));
        let args: Vec<_> = std::env::args_os().collect();
        let outcome = std::panic::catch_unwind(|| match args.as_slice() {
            [_] => run(),
            [_, flag] if flag == "--hash-synthetic-password" => hash_synthetic_password(),
            [_, flag] if flag == "--seed-durable-qa-posting" => {
                seed_durable_qa_posting(PostingKind::W2)
            }
            [_, flag] if flag == "--seed-durable-c2c-qa-posting" => {
                seed_durable_qa_posting(PostingKind::C2c)
            }
            [_, flag] if flag == "--seed-durable-imported-resume-qa-posting" => {
                seed_durable_qa_posting(PostingKind::ImportedResume)
            }
            [_, flag] if flag == "--seed-durable-review-first-mock-receipt-posting" => {
                seed_durable_qa_posting(PostingKind::ReviewFirstMockReceipt)
            }
            [_, flag] if flag == "--publish-durable-review-first-mock-receipt" => {
                publish_durable_review_first_mock_receipt()
            }
            _ => Err("arguments_forbidden"),
        })
        .unwrap_or(Err("fixture_assertion_failed"));
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

            let durable = format!(
                "postgresql://bluey_qa_local:{}@127.0.0.1:15432/bluey_jobs_qa?sslmode=disable",
                "b".repeat(64)
            );
            assert!(durable_database_config(&durable).is_ok());
            for invalid in [
                durable.replace("127.0.0.1", "localhost"),
                durable.replace("15432", "5432"),
                durable.replace("bluey_jobs_qa", "bluey_jobs_nonprod"),
                durable.replace("bluey_qa_local", "bluey_jobs_test"),
                format!("{durable}&host=remote.invalid"),
            ] {
                assert!(durable_database_config(&invalid).is_err());
            }
        }

        #[test]
        fn durable_request_is_exact_candidate_track_and_scope() {
            let request = DurableRequest {
                version: 1,
                candidate_id: "123e4567-e89b-42d3-a456-426614174000".into(),
                track_id: "123e4567-e89b-42d3-a456-426614174001".into(),
                expected_system_identifier: "123456789".into(),
                profile_scope: "candidate@bluey-qa.test".into(),
                job_scope: DurableJobScope {
                    title: "Software Engineer".into(),
                    location: "New York, NY".into(),
                    skills: vec!["Rust".into(), "TypeScript".into()],
                },
                source_resume_asset_id: None,
                source_resume_sha256: None,
            };
            assert!(validate_durable_request(&request, PostingKind::W2).is_ok());
            assert!(validate_durable_request(&request, PostingKind::C2c).is_err());
            let mut canonical = serde_json::to_vec(&request).unwrap();
            canonical.push(b'\n');
            assert!(canonical.ends_with(b"}\n"));

            let mut c2c = request;
            c2c.version = 2;
            assert!(validate_durable_request(&c2c, PostingKind::C2c).is_ok());
            assert!(validate_durable_request(&c2c, PostingKind::W2).is_err());
            let mut invalid = c2c;
            invalid.track_id = "track-current".into();
            assert!(validate_durable_request(&invalid, PostingKind::C2c).is_err());

            let imported = DurableRequest {
                version: 3,
                candidate_id: "123e4567-e89b-42d3-a456-426614174000".into(),
                track_id: "123e4567-e89b-42d3-a456-426614174001".into(),
                expected_system_identifier: "123456789".into(),
                profile_scope: "candidate@bluey-qa.test".into(),
                job_scope: DurableJobScope {
                    title: "Data Engineer".into(),
                    location: "New York, NY".into(),
                    skills: vec!["Python".into(), "SQL".into()],
                },
                source_resume_asset_id: Some("123e4567-e89b-42d3-a456-426614174002".into()),
                source_resume_sha256: Some(DURABLE_IMPORTED_RESUME_SHA256.into()),
            };
            assert!(validate_durable_request(&imported, PostingKind::ImportedResume).is_ok());
            assert!(validate_durable_request(&imported, PostingKind::W2).is_err());
            assert!(
                validate_durable_request(&imported, PostingKind::ReviewFirstMockReceipt).is_err()
            );
            let mut mock = imported;
            mock.version = 4;
            assert!(validate_durable_request(&mock, PostingKind::ReviewFirstMockReceipt).is_ok());
            assert!(validate_durable_request(&mock, PostingKind::ImportedResume).is_err());
            mock.source_resume_sha256 = Some("0".repeat(64));
            assert!(validate_durable_request(&mock, PostingKind::ReviewFirstMockReceipt).is_err());
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
                track_id: "t".into(),
                queue_entitlement: false,
            })
            .is_err());
        }

        #[test]
        fn queue_entitlement_is_explicit_boolean_and_defaults_off() {
            let legacy = r#"{"version":1,"account_id":"a","track_id":"t"}"#;
            assert!(
                !serde_json::from_str::<Request>(legacy)
                    .unwrap()
                    .queue_entitlement
            );
            let granting =
                r#"{"version":1,"account_id":"a","track_id":"t","queue_entitlement":true}"#;
            assert!(
                serde_json::from_str::<Request>(granting)
                    .unwrap()
                    .queue_entitlement
            );
            assert!(serde_json::from_str::<Request>(
                r#"{"version":1,"account_id":"a","track_id":"t","queue_entitlement":"true"}"#
            )
            .is_err());
            assert!(serde_json::from_str::<Request>(
                r#"{"version":1,"account_id":"a","track_id":"t","plan":"cloud"}"#
            )
            .is_err());
        }

        #[test]
        fn synthetic_password_requires_one_lowercase_hex_line() {
            let valid = format!("{}\n", "a".repeat(48));
            assert_eq!(
                synthetic_password(valid.as_bytes()).unwrap(),
                "a".repeat(48)
            );
            for invalid in [
                b"a\n".as_slice(),
                b"Aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n",
                b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\nextra",
            ] {
                assert!(synthetic_password(invalid).is_err());
            }
        }

        #[test]
        fn synthetic_password_hash_has_bcrypt_cost_format() {
            let hash = hash_password(&"a".repeat(48)).unwrap();
            assert_eq!(hash.len(), 60);
            assert!(hash.starts_with("$2") && hash.as_bytes().get(4..7) == Some(b"12$"));
            assert!(bluey_server::auth::password::verify_password(
                &"a".repeat(48),
                &hash
            ));
            assert!(!bluey_server::auth::password::verify_password(
                &"b".repeat(48),
                &hash
            ));
        }

        #[test]
        fn durable_guard_successor_denies_each_exact_binding_gap() {
            let key: Vec<u8> = (0..32).map(|index| index * 7).collect();
            let encoded = hex::encode(key);
            let key_hash = durable_data_key_hash(&encoded).unwrap();
            assert_eq!(key_hash, sha256(format!("{encoded}\n").as_bytes()));
            assert!(validate_durable_data_key_marker(Some(&key_hash), &key_hash).is_ok());
            assert_eq!(
                durable_data_key_hash(&format!("{encoded}\n")),
                Err("durable_data_key_shape")
            );
            assert_eq!(
                validate_durable_data_key_marker(Some(&"0".repeat(64)), &key_hash),
                Err("durable_data_key_marker")
            );
            assert_eq!(
                validate_durable_data_key_marker(None, &key_hash),
                Err("durable_data_key_marker")
            );

            let password = format!("{}\n", "b".repeat(64)).into_bytes();
            let password_hash = sha256(&password);
            assert!(
                validate_durable_password_marker(&password, &password_hash, &"b".repeat(64),)
                    .is_ok()
            );
            assert_eq!(
                validate_durable_password_marker(&password, &password_hash, &"c".repeat(64),),
                Err("durable_password_marker")
            );

            let free = (1, true, 0, 0, "free", 0, 0, 0);
            let cloud = (1, true, 0, 0, "cloud", 1, 1, 0);
            assert!(validate_durable_postwrite_census(free, PostingKind::ImportedResume).is_ok());
            assert!(
                validate_durable_postwrite_census(cloud, PostingKind::ReviewFirstMockReceipt)
                    .is_ok()
            );
            assert!(
                validate_durable_postwrite_census(free, PostingKind::ReviewFirstMockReceipt)
                    .is_err()
            );
            assert!(validate_durable_postwrite_census(cloud, PostingKind::ImportedResume).is_err());
            assert!(validate_durable_postwrite_census(
                (1, true, 0, 1, "cloud", 1, 1, 1),
                PostingKind::ReviewFirstMockReceipt,
            )
            .is_err());
            for invalid in [
                (0, true, 0, 0, "free", 0, 0, 0),
                (2, true, 0, 0, "free", 0, 0, 0),
                (1, false, 0, 0, "free", 0, 0, 0),
                (1, true, 1, 0, "free", 0, 0, 0),
                (1, true, 0, 1, "free", 0, 0, 0),
                (1, true, 0, 0, "paid", 0, 0, 0),
                (1, true, 0, 0, "free", 1, 0, 0),
                (1, true, 0, 0, "free", 0, 1, 0),
                (1, true, 0, 0, "free", 0, 0, 1),
            ] {
                assert_eq!(
                    validate_durable_postwrite_census(invalid, PostingKind::ImportedResume),
                    Err("durable_postwrite_census")
                );
            }
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

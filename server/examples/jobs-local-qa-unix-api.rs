//! Jobs QA over a disposable launcher-attested PostgreSQL cluster. The default
//! lane permits reads and local login; a separate synthetic lane can save only
//! the pinned candidate's profile. Migrations and login may write locally.
//! No production startup, service workers, SMTP, providers or TCP listeners.
#![forbid(unsafe_code)]

#[cfg(not(all(unix, debug_assertions, feature = "jobs-local-qa-unix")))]
compile_error!("the local Jobs QA API requires Unix and the debug-only QA feature");

use anyhow::{ensure, Context, Result};
use axum::{
    body::{to_bytes, Body},
    http::{header, Method, Request, StatusCode},
    response::IntoResponse,
    Router,
};
use bluey_server::{
    api,
    auth::jwt::{self, TokenKind},
    config::{Config, ServerDbBackend, TrialAbuseConfig, UpstreamKeys},
    db::{self, DbPool},
};
use hyper::{body::Incoming, server::conn::http1, service::service_fn};
use hyper_util::rt::TokioIo;
use std::{
    convert::Infallible,
    env, fs,
    os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::{net::UnixListener, signal::unix::SignalKind, sync::Semaphore, task::JoinSet};
use tower::ServiceExt;

const LOCAL_PG_PORT: u16 = 55432;
const LOCAL_PG_USER: &str = "bluey_unix_qa";
const SYNTHETIC_CANDIDATE_ID: &str = "11111111-1111-4111-8111-111111111111";
const SYNTHETIC_CANDIDATE_EMAIL: &str = "candidate@bluey-qa.test";
const SYNTHETIC_PROFILE: &[u8] = concat!(
    r#"{"full_name":"Synthetic QA Candidate","email":"candidate@bluey-qa.test","#,
    r#""headline":"Synthetic profile persistence check","onboarding_step":0,"#,
    r#""onboarding_complete":false}"#
)
.as_bytes();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Lane {
    ReadOnly,
    SyntheticProfile,
}

fn lane(args: impl IntoIterator<Item = String>) -> Result<Lane> {
    let mut args = args.into_iter();
    let lane = match args.next().as_deref() {
        Some("--owned-read-only") => Lane::ReadOnly,
        Some("--owned-synthetic-profile") => Lane::SyntheticProfile,
        _ => anyhow::bail!("QA admission argument"),
    };
    ensure!(args.next().is_none(), "QA admission argument");
    Ok(lane)
}

fn owns_private_dir(path: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(path).context("inspect private QA directory")?;
    ensure!(
        path.canonicalize()? == path
            && meta.is_dir()
            && meta.mode() & 0o7777 == 0o700
            && meta.uid() == rustix::process::getuid().as_raw(),
        "QA directory custody"
    );
    Ok(())
}

fn socket_identity(path: &Path, permissions: u32) -> Result<(u64, u64)> {
    let meta = fs::symlink_metadata(path).context("inspect private QA socket")?;
    ensure!(
        path.canonicalize()? == path
            && meta.file_type().is_socket()
            && meta.mode() & 0o7777 == permissions
            && meta.uid() == rustix::process::getuid().as_raw(),
        "QA socket custody"
    );
    Ok((meta.dev(), meta.ino()))
}

fn require_absent(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        _ => anyhow::bail!("QA endpoint already exists or is unobservable"),
    }
}

fn root() -> Result<PathBuf> {
    for name in [
        "BLUEY_API_HOST",
        "BLUEY_PORT",
        "BLUEY_DATABASE_URL",
        "BLUEY_POSTGRES_CA_CERT_PATH",
        "BLUEY_POSTGRES_ACCEPT_INVALID_CERTS",
        "BLUEY_POSTGRES_ACCEPT_INVALID_HOSTNAMES",
    ] {
        ensure!(
            env::var_os(name).is_none(),
            "foreign QA transport configuration"
        );
    }
    let root = PathBuf::from(env::var("BLUEY_TEST_RUN_ROOT").context("missing QA run root")?);
    let parent = root
        .parent()
        .and_then(Path::to_str)
        .context("QA run parent")?;
    let suffix = parent
        .strip_prefix("/private/tmp/bluey-test-runs-")
        .context("QA run parent namespace")?;
    ensure!(
        suffix.len() == 12
            && suffix
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit()),
        "QA run parent namespace"
    );
    owns_private_dir(root.parent().context("QA run parent missing")?)?;
    owns_private_dir(&root)?;
    let name = root
        .file_name()
        .and_then(|value| value.to_str())
        .context("QA run name")?;
    let mut parts = name.split('.');
    ensure!(parts.next() == Some("run"), "QA run name");
    let epoch = parts.next().context("QA run epoch")?;
    let pid = parts.next().context("QA run owner")?;
    let nonce = parts.next().context("QA run nonce")?;
    let canonical = |s: &str| s.parse::<u64>().is_ok_and(|n| n.to_string() == s);
    ensure!(
        canonical(epoch)
            && canonical(pid)
            && pid.parse::<u64>()? > 1
            && !nonce.is_empty()
            && nonce.len() <= 64
            && nonce.bytes().all(|b| b.is_ascii_alphanumeric()),
        "QA run name"
    );
    ensure!(parts.next().is_none(), "QA run name");
    let marker = root.join(".bluey-test-run-v2");
    let meta = fs::symlink_metadata(&marker)?;
    ensure!(
        meta.is_file()
            && meta.nlink() == 1
            && meta.len() <= 1024
            && meta.mode() & 0o7777 == 0o600
            && meta.uid() == rustix::process::getuid().as_raw(),
        "QA run marker custody"
    );
    let text = fs::read_to_string(marker)?;
    let mut lines = text.split_terminator('\n');
    let launcher = |line: &str| {
        line.strip_prefix("launcher_group_id=")
            .is_some_and(|s| canonical(s) && s.parse::<u64>().is_ok_and(|n| n > 1))
    };
    let expected_root = format!("run_root={}", root.display());
    let expected_name = format!("run_name={name}");
    let expected_uid = format!("owner_uid={}", rustix::process::getuid().as_raw());
    let expected_pid = format!("owner_pid={pid}");
    let expected_group = format!(
        "process_group_id={}",
        rustix::process::getpgrp().as_raw_pid()
    );
    let expected_epoch = format!("created_epoch={epoch}");
    ensure!(
        text.ends_with('\n')
            && lines.next() == Some("magic=bluey-test-run-v2")
            && lines.next() == Some(expected_root.as_str())
            && lines.next() == Some(expected_name.as_str())
            && lines.next() == Some(expected_uid.as_str())
            && lines.next() == Some(expected_pid.as_str())
            && lines.next() == Some(expected_group.as_str())
            && lines.next().is_some_and(launcher)
            && lines.next() == Some(expected_epoch.as_str())
            && lines.next() == Some("state=running")
            && lines.next().is_none(),
        "QA run marker mismatch"
    );
    Ok(root)
}

fn setup(root: &Path, lane: Lane) -> Result<DbPool> {
    let directory = root.join("p");
    let data = root.join("pg");
    owns_private_dir(&directory)?;
    owns_private_dir(&data)?;
    let socket = directory.join(format!(".s.PGSQL.{LOCAL_PG_PORT}"));
    let observed = socket_identity(&socket, 0o700)?;
    // The host is an absolute, validated Unix directory in conninfo, not a
    // URL obtained from an environment or request. Port identifies the socket.
    let conninfo = format!(
        "host={} port={LOCAL_PG_PORT} user={LOCAL_PG_USER} dbname=postgres sslmode=disable connect_timeout=2",
        directory.display()
    );
    let pool = db::open_postgres_pool(&conninfo).context("open QA Unix database")?;
    let row = pool.get_pg()?.query_one(
        "SELECT inet_server_addr() IS NULL AND inet_client_addr() IS NULL,
                current_setting('listen_addresses'), current_setting('data_directory'),
                current_setting('unix_socket_directories'), current_user, current_database()",
        &[],
    )?;
    ensure!(
        row.get::<_, bool>(0)
            && row.get::<_, String>(1).is_empty()
            && Path::new(&row.get::<_, String>(2)) == data
            && Path::new(&row.get::<_, String>(3)) == directory
            && row.get::<_, String>(4) == LOCAL_PG_USER
            && row.get::<_, String>(5) == "postgres"
            && socket_identity(&socket, 0o700)? == observed,
        "QA database Unix identity"
    );
    db::run_migrations(&pool).context("migrate owned QA database")?;
    if lane == Lane::SyntheticProfile {
        let row = pool.get_pg()?.query_one(
            "SELECT COUNT(*) = 1 AND COUNT(*) FILTER (
                WHERE id = $1 AND email = $2 AND is_admin = 0 AND is_temporary = 0
                AND email_verified_at IS NOT NULL
             ) = 1 FROM accounts",
            &[&SYNTHETIC_CANDIDATE_ID, &SYNTHETIC_CANDIDATE_EMAIL],
        )?;
        ensure!(
            row.get::<_, bool>(0),
            "QA profile requires one verified synthetic account"
        );
    }
    ensure!(
        socket_identity(&socket, 0o700)? == observed,
        "QA database socket changed"
    );
    Ok(pool)
}

fn config() -> Result<Config> {
    let mut bytes = [0_u8; 32];
    getrandom::getrandom(&mut bytes).map_err(|_| anyhow::anyhow!("QA signer initialization"))?;
    Ok(Config {
        port: 0,
        db_path: PathBuf::new(),
        db_backend: ServerDbBackend::Postgres,
        database_url: None,
        jwt_secret: hex::encode(bytes),
        public_url: "http://127.0.0.1".into(),
        stripe_secret_key: None,
        stripe_webhook_secret: None,
        upstream: UpstreamKeys::default(),
        upstream_spend_guard: None,
        smtp: None,
        admin_emails: Vec::new(),
        trial_abuse: TrialAbuseConfig {
            max_trials_per_email: 1,
            max_trials_per_email_domain_per_day: 1,
            max_trials_per_device: 1,
            max_trials_per_device_per_30_days: 1,
            max_trials_per_ip_per_day: 1,
            max_trials_per_ip_user_agent_per_day: 1,
        },
        turnstile_site_key: None,
        turnstile_secret_key: None,
        require_turnstile: true,
        object_storage: None,
        log_storage: None,
    })
}

fn is_profile_write<B>(request: &Request<B>) -> bool {
    request.method() == Method::PUT && request.uri().path() == "/api/jobs/profile"
}

fn allowed<B>(request: &Request<B>, profile_secret: Option<&str>) -> bool {
    if request.uri().scheme().is_some()
        || request.uri().authority().is_some()
        || request.uri().query().is_some()
        || request.headers().contains_key("upgrade")
    {
        return false;
    }
    if is_profile_write(request) {
        return profile_secret.is_some_and(|secret| {
            let headers = request.headers();
            if headers.contains_key(header::CONTENT_ENCODING) {
                return false;
            }
            let mut types = headers.get_all(header::CONTENT_TYPE).iter();
            if !matches!(
                types.next().and_then(|v| v.to_str().ok()),
                Some("application/json")
            ) || types.next().is_some()
            {
                return false;
            }
            let mut values = headers.get_all(header::AUTHORIZATION).iter();
            let token = values
                .next()
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.strip_prefix("Bearer "));
            values.next().is_none()
                && token.is_some_and(|token| {
                    token.len() <= 8192
                        && jwt::verify(secret, token).is_ok_and(|claims| {
                            claims.sub == SYNTHETIC_CANDIDATE_ID
                                && claims
                                    .token_kind()
                                    .is_ok_and(|kind| kind == TokenKind::Access)
                        })
                })
        });
    }
    matches!(
        (request.method(), request.uri().path()),
        (&Method::POST, "/auth/login")
            | (
                &Method::GET,
                "/health"
                    | "/account/me"
                    | "/api/jobs/beta-access"
                    | "/api/jobs/workspace"
                    | "/api/jobs/taxonomy"
                    | "/api/jobs/resume-source"
            )
    )
}

async fn serve(
    listener: UnixListener,
    app: Router,
    profile_secret: Option<Arc<str>>,
) -> Result<()> {
    let mut term = tokio::signal::unix::signal(SignalKind::terminate())?;
    let mut interrupt = tokio::signal::unix::signal(SignalKind::interrupt())?;
    let mut peers = JoinSet::new();
    let permits = Arc::new(Semaphore::new(16));
    println!("jobs_local_qa_unix_ready");
    loop {
        tokio::select! {
            biased;
            _ = term.recv() => break,
            _ = interrupt.recv() => break,
            Some(result) = peers.join_next(), if !peers.is_empty() => {
                if let Err(error) = result {
                    anyhow::bail!("QA Unix peer failed: {error}");
                }
            }
            accepted = listener.accept() => {
                let (stream, _) = accepted.context("accept owned QA Unix peer")?;
                let Ok(permit) = Arc::clone(&permits).try_acquire_owned() else { continue; };
                let app = app.clone();
                let profile_secret = profile_secret.clone();
                peers.spawn(async move {
                    let _permit = permit;
                    let service = service_fn(move |mut req: Request<Incoming>| {
                        let app = app.clone();
                        let profile_secret = profile_secret.clone();
                        async move {
                            if !allowed(&req, profile_secret.as_deref()) {
                                return Ok::<_, Infallible>(StatusCode::NOT_FOUND.into_response());
                            }
                            // No TCP ConnectInfo and no forwarded peer identity in this lane.
                            for header in ["forwarded", "x-forwarded-for", "x-real-ip", "cf-connecting-ip"] {
                                req.headers_mut().remove(header);
                            }
                            if is_profile_write(&req) {
                                let (parts, body) = req.into_parts();
                                let Ok(bytes) = to_bytes(Body::new(body), SYNTHETIC_PROFILE.len()).await else {
                                    return Ok(StatusCode::NOT_FOUND.into_response());
                                };
                                if bytes.as_ref() != SYNTHETIC_PROFILE {
                                    return Ok(StatusCode::NOT_FOUND.into_response());
                                }
                                return app.oneshot(Request::from_parts(parts, Body::from(bytes))).await;
                            }
                            app.oneshot(req.map(Body::new)).await
                        }
                    });
                    let _ = tokio::time::timeout(Duration::from_secs(8),
                        http1::Builder::new().keep_alive(false)
                            .serve_connection(TokioIo::new(stream), service)).await;
                });
            }
        }
    }
    drop(listener);
    peers.shutdown().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "isolated-test-signer-not-used-outside-these-tests";

    fn profile_request(target: &str, account: &str, kind: TokenKind) -> Request<()> {
        let token = jwt::issue(SECRET, account, kind).unwrap();
        Request::builder()
            .method(Method::PUT)
            .uri(target)
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(())
            .unwrap()
    }

    #[test]
    fn synthetic_write_lane_has_an_exact_explicit_argument() {
        assert_eq!(lane(["--owned-read-only".into()]).unwrap(), Lane::ReadOnly);
        assert_eq!(
            lane(["--owned-synthetic-profile".into()]).unwrap(),
            Lane::SyntheticProfile
        );
        assert!(lane([]).is_err());
        assert!(lane(["--owned-synthetic-profile".into(), "--extra".into()]).is_err());
    }

    #[test]
    fn synthetic_profile_write_requires_bound_access_token_and_exact_request() {
        let request = profile_request(
            "/api/jobs/profile",
            SYNTHETIC_CANDIDATE_ID,
            TokenKind::Access,
        );
        assert!(!allowed(&request, None));
        assert!(allowed(&request, Some(SECRET)));
        assert!(!allowed(&request, Some("wrong-secret")));
        assert!(!allowed(
            &profile_request("/api/jobs/profile", "other-account", TokenKind::Access),
            Some(SECRET)
        ));
        assert!(!allowed(
            &profile_request(
                "/api/jobs/profile",
                SYNTHETIC_CANDIDATE_ID,
                TokenKind::Refresh
            ),
            Some(SECRET)
        ));
        for target in [
            "/api/jobs/profile?x=1",
            "/api/jobs/profile/",
            "/api/jobs/preferences",
        ] {
            assert!(!allowed(
                &profile_request(target, SYNTHETIC_CANDIDATE_ID, TokenKind::Access),
                Some(SECRET)
            ));
        }
    }

    #[test]
    fn profile_fixture_does_not_complete_onboarding_or_change_execution_mode() {
        let profile: db::jobs::CareerProfile = serde_json::from_slice(SYNTHETIC_PROFILE).unwrap();
        assert_eq!(profile.email, SYNTHETIC_CANDIDATE_EMAIL);
        assert!(!profile.onboarding_complete);
        assert_eq!(profile.onboarding_step, 0);
        assert_eq!(profile.default_submission_mode, "review_first");
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let lane = lane(env::args().skip(1))?;
    let root = root()?;
    let endpoint = root.join("a");
    require_absent(&endpoint)?;
    let owned = root.clone();
    let pool =
        tokio::task::spawn_blocking(move || db::run_blocking_db(|| setup(&owned, lane))).await??;
    let config = config()?;
    let profile_secret =
        (lane == Lane::SyntheticProfile).then(|| Arc::<str>::from(config.jwt_secret.as_str()));
    let app = api::build_router(pool, config);
    let listener = UnixListener::bind(&endpoint).context("bind owned QA Unix endpoint")?;
    fs::set_permissions(&endpoint, fs::Permissions::from_mode(0o600))?;
    let identity = socket_identity(&endpoint, 0o600)?;
    let served = serve(listener, app, profile_secret).await;
    ensure!(
        socket_identity(&endpoint, 0o600)? == identity,
        "QA socket changed"
    );
    fs::remove_file(&endpoint).context("close owned QA Unix endpoint")?;
    served?;
    println!("jobs_local_qa_unix_stopped");
    Ok(())
}

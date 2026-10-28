pub const JOBS_WORKSPACE_DEFAULT_PAGE_SIZE: usize = 50;
pub const JOBS_WORKSPACE_MAX_PAGE_SIZE: usize = 100;
const JOBS_WORKSPACE_APPLICATION_CONTEXT_QUERY_CHUNK: usize = 100;
const JOBS_LEGACY_MATCHES_MAX_POSTINGS: usize = GLOBAL_DISCOVERY_MAX_MATERIALIZED_PER_ACCOUNT;
// Every export page enters the same source -> ATS -> integrity page resolver as
// the interactive queue. Keep the exhaustive outer traversal separate from
// the resolver's fixed per-page authority envelope.
const ACCOUNT_EXPORT_REPRESENTATION_PAGE_SIZE: usize = JOBS_WORKSPACE_MAX_PAGE_SIZE;
const JOBS_MATCH_CURSOR_PURPOSE: &str = "bluey_jobs_interactive_matches_v1";
const JOBS_MATCH_CURSOR_TTL_MS: i64 = 30 * 60 * 1_000;
const JOBS_MATCH_CURSOR_MAX_FUTURE_SKEW_MS: i64 = 60 * 1_000;
const INVALID_JOBS_MATCH_CURSOR: &str =
    "Jobs match cursor is invalid or expired; refresh the workspace";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct JobsMatchCursorV1 {
    schema_version: i64,
    purpose: String,
    account_sha256: String,
    page_size: i64,
    match_score: i64,
    updated_at_ms: i64,
    job_id: String,
    issued_at_ms: i64,
    expires_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobsMatchPageKey {
    match_score: i64,
    updated_at_ms: i64,
    job_id: String,
}

#[derive(Clone, Copy)]
enum PostingRepresentationSelection<'a> {
    Page {
        limit: usize,
        after: Option<&'a JobsMatchPageKey>,
        include_application_context: bool,
    },
    LegacyAll,
    JobId(&'a str),
}

struct PostingRepresentationContext<'a> {
    profile: &'a CareerProfile,
    preferences: &'a JobPreferences,
    reservations: &'a [AttemptReservation],
    application_ids_by_job: &'a BTreeMap<&'a str, &'a str>,
    tracks_by_id: &'a BTreeMap<&'a str, &'a CareerTrack>,
}

struct PostingRepresentationInputs {
    profile: CareerProfile,
    preferences: JobPreferences,
    tracks: Vec<CareerTrack>,
    applications: Vec<JobApplication>,
    reservations: Vec<AttemptReservation>,
}

#[derive(Debug, Error)]
#[error("Jobs representation inputs changed while the snapshot was assembled")]
struct PostingRepresentationSnapshotChanged;

struct WorkspaceRepresentation {
    profile: CareerProfile,
    preferences: JobPreferences,
    tracks: Vec<CareerTrack>,
    applications: Vec<JobApplication>,
    matches: Vec<JobPosting>,
    application_jobs: Vec<ApplicationJobContext>,
    matches_page: JobsMatchesPageInfo,
    candidate_count: usize,
    authority_batch_count: usize,
    application_context_count: usize,
    application_context_fallback_batch_count: usize,
}

struct LoadedRepresentationPostings {
    postings: Vec<JobPosting>,
    discovery: BTreeMap<String, Vec<JobDiscoveryAuthority>>,
    candidate_count: usize,
    next_key: Option<JobsMatchPageKey>,
    page_size: usize,
}

struct LoadedApplicationContextJobs {
    jobs: Vec<ApplicationJobContext>,
    fallback_batch_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobsWorkspaceLoadDiagnostics {
    pub candidate_count: usize,
    pub returned_count: usize,
    pub has_more: bool,
    pub authority_batch_count: usize,
    pub application_context_count: usize,
    pub application_context_fallback_batch_count: usize,
    pub representation_ms: u64,
    pub ancillary_ms: u64,
    pub total_ms: u64,
}

pub struct JobsWorkspaceLoad {
    pub workspace: JobsWorkspace,
    pub diagnostics: JobsWorkspaceLoadDiagnostics,
}

pub struct JobsMatchesPageLoad {
    pub page: JobsMatchesPage,
    pub diagnostics: JobsWorkspaceLoadDiagnostics,
}

pub fn workspace(pool: &DbPool, account_id: &str, email: &str) -> Result<JobsWorkspace> {
    workspace_page(
        pool,
        account_id,
        email,
        JOBS_WORKSPACE_DEFAULT_PAGE_SIZE,
        None,
    )
    .map(|load| load.workspace)
}

pub fn workspace_page(
    pool: &DbPool,
    account_id: &str,
    email: &str,
    limit: usize,
    cursor: Option<&str>,
) -> Result<JobsWorkspaceLoad> {
    let total_started = std::time::Instant::now();
    let after = decode_jobs_match_cursor(cursor, account_id, limit)?;
    let _ = ensure_primary_application_identity(pool, account_id, email)?;
    let representation_started = std::time::Instant::now();
    let representation = current_authority_posting_representations(
        pool,
        account_id,
        email,
        PostingRepresentationSelection::Page {
            limit,
            after: after.as_ref(),
            include_application_context: true,
        },
    )?;
    let representation_ms = elapsed_millis(representation_started);
    let candidate_count = representation.candidate_count;
    let returned_count = representation.matches.len();
    let has_more = representation.matches_page.has_more;
    let authority_batch_count = representation.authority_batch_count;
    let application_context_count = representation.application_context_count;
    let application_context_fallback_batch_count =
        representation.application_context_fallback_batch_count;
    let ancillary_started = std::time::Instant::now();
    let workspace = build_workspace(pool, account_id, email, representation)?;
    let ancillary_ms = elapsed_millis(ancillary_started);
    Ok(JobsWorkspaceLoad {
        workspace,
        diagnostics: JobsWorkspaceLoadDiagnostics {
            candidate_count,
            returned_count,
            has_more,
            authority_batch_count,
            application_context_count,
            application_context_fallback_batch_count,
            representation_ms,
            ancillary_ms,
            total_ms: elapsed_millis(total_started),
        },
    })
}

fn elapsed_millis(started: std::time::Instant) -> u64 {
    started.elapsed().as_millis().min(u64::MAX as u128) as u64
}

fn validate_jobs_match_page_size(limit: usize) -> Result<()> {
    if !(1..=JOBS_WORKSPACE_MAX_PAGE_SIZE).contains(&limit) {
        anyhow::bail!(
            "Jobs match page size must be between 1 and {}",
            JOBS_WORKSPACE_MAX_PAGE_SIZE
        )
    }
    Ok(())
}

fn decode_jobs_match_cursor(
    value: Option<&str>,
    account_id: &str,
    limit: usize,
) -> Result<Option<JobsMatchPageKey>> {
    validate_jobs_match_page_size(limit)?;
    let Some(value) = value else {
        return Ok(None);
    };
    if value.len() > 2_048 || !value.starts_with(ENCRYPTED_PAYLOAD_PREFIX) {
        anyhow::bail!(INVALID_JOBS_MATCH_CURSOR)
    }
    validate_data_encryption_config()?;
    let plain = decrypt_payload(value).map_err(|_| anyhow::anyhow!(INVALID_JOBS_MATCH_CURSOR))?;
    let cursor: JobsMatchCursorV1 =
        serde_json::from_str(&plain).map_err(|_| anyhow::anyhow!(INVALID_JOBS_MATCH_CURSOR))?;
    let account_sha256 = private_lookup_hash(JOBS_MATCH_CURSOR_PURPOSE, account_id)?;
    let decoded_at_ms = now_ms();
    let valid_expiry = cursor
        .issued_at_ms
        .checked_add(JOBS_MATCH_CURSOR_TTL_MS)
        .is_some_and(|expires_at_ms| expires_at_ms == cursor.expires_at_ms);
    if cursor.schema_version != 1
        || cursor.purpose != JOBS_MATCH_CURSOR_PURPOSE
        || cursor.account_sha256 != account_sha256
        || cursor.page_size != limit as i64
        || cursor.updated_at_ms < 0
        || cursor.job_id.is_empty()
        || cursor.issued_at_ms < 0
        || cursor.issued_at_ms > decoded_at_ms.saturating_add(JOBS_MATCH_CURSOR_MAX_FUTURE_SKEW_MS)
        || !valid_expiry
        || cursor.expires_at_ms <= decoded_at_ms
    {
        anyhow::bail!(INVALID_JOBS_MATCH_CURSOR)
    }
    Ok(Some(JobsMatchPageKey {
        match_score: cursor.match_score,
        updated_at_ms: cursor.updated_at_ms,
        job_id: cursor.job_id,
    }))
}

fn encode_jobs_match_cursor(
    account_id: &str,
    page_size: usize,
    key: &JobsMatchPageKey,
) -> Result<String> {
    validate_jobs_match_page_size(page_size)?;
    let issued_at_ms = now_ms();
    let encoded = to_json(
        &JobsMatchCursorV1 {
            schema_version: 1,
            purpose: JOBS_MATCH_CURSOR_PURPOSE.to_string(),
            account_sha256: private_lookup_hash(JOBS_MATCH_CURSOR_PURPOSE, account_id)?,
            page_size: page_size as i64,
            match_score: key.match_score,
            updated_at_ms: key.updated_at_ms,
            job_id: key.job_id.clone(),
            issued_at_ms,
            expires_at_ms: issued_at_ms.saturating_add(JOBS_MATCH_CURSOR_TTL_MS),
        },
        "Jobs match cursor",
    )?;
    if encoded.len() > 2_048 {
        anyhow::bail!(INVALID_JOBS_MATCH_CURSOR)
    }
    Ok(encoded)
}

fn build_workspace(
    pool: &DbPool,
    account_id: &str,
    email: &str,
    representation: WorkspaceRepresentation,
) -> Result<JobsWorkspace> {
    Ok(JobsWorkspace {
        profile: representation.profile,
        preferences: representation.preferences,
        facts: list_facts(pool, account_id)?,
        tracks: representation.tracks,
        matches: representation.matches,
        matches_page: representation.matches_page,
        applications: representation.applications,
        application_jobs: representation.application_jobs,
        application_evidence: list_application_evidence(pool, account_id, None)?,
        browser_sessions: list_browser_sessions(pool, account_id)?,
        interventions: list_interventions(pool, account_id)?,
        answer_memory: list_answer_memory(pool, account_id)?,
        candidate_events: list_candidate_events(pool, account_id)?,
        integrations: list_integrations(pool, account_id)?,
        application_identities: list_application_identities(pool, account_id)?,
        auto_submit_authorizations: list_auto_submit_authorizations(pool, account_id, email)?,
        mailbox_connections: list_mailbox_connections(pool, account_id)?,
        discovery_sources: list_discovery_sources(pool, account_id)?
            .iter()
            .map(DiscoverySourceSummary::from)
            .collect(),
        entitlement: get_entitlement(pool, account_id)?,
        runner_availability: RunnerAvailability::default(),
    })
}

pub fn list_current_posting_representations(
    pool: &DbPool,
    account_id: &str,
    email: &str,
) -> Result<Vec<JobPosting>> {
    current_authority_posting_representations(
        pool,
        account_id,
        email,
        PostingRepresentationSelection::LegacyAll,
    )
    .map(|representation| representation.matches)
}

pub fn list_current_posting_representation_page(
    pool: &DbPool,
    account_id: &str,
    email: &str,
    limit: usize,
    cursor: Option<&str>,
) -> Result<JobsMatchesPage> {
    load_current_posting_representation_page(pool, account_id, email, limit, cursor)
        .map(|load| load.page)
}

pub fn load_current_posting_representation_page(
    pool: &DbPool,
    account_id: &str,
    email: &str,
    limit: usize,
    cursor: Option<&str>,
) -> Result<JobsMatchesPageLoad> {
    let total_started = std::time::Instant::now();
    let after = decode_jobs_match_cursor(cursor, account_id, limit)?;
    let representation_started = std::time::Instant::now();
    let representation = current_authority_posting_representations(
        pool,
        account_id,
        email,
        PostingRepresentationSelection::Page {
            limit,
            after: after.as_ref(),
            include_application_context: false,
        },
    )?;
    let representation_ms = elapsed_millis(representation_started);
    let diagnostics = JobsWorkspaceLoadDiagnostics {
        candidate_count: representation.candidate_count,
        returned_count: representation.matches.len(),
        has_more: representation.matches_page.has_more,
        authority_batch_count: representation.authority_batch_count,
        application_context_count: 0,
        application_context_fallback_batch_count: 0,
        representation_ms,
        ancillary_ms: 0,
        total_ms: elapsed_millis(total_started),
    };
    Ok(JobsMatchesPageLoad {
        page: JobsMatchesPage {
            matches: representation.matches,
            page: representation.matches_page,
        },
        diagnostics,
    })
}

pub fn get_current_posting_representation(
    pool: &DbPool,
    account_id: &str,
    email: &str,
    job_id: &str,
) -> Result<Option<JobPosting>> {
    Ok(current_authority_posting_representations(
        pool,
        account_id,
        email,
        PostingRepresentationSelection::JobId(job_id),
    )?
    .matches
    .into_iter()
    .next())
}

fn current_authority_posting_representations(
    pool: &DbPool,
    account_id: &str,
    email: &str,
    selection: PostingRepresentationSelection<'_>,
) -> Result<WorkspaceRepresentation> {
    for _ in 0..3 {
        match current_authority_posting_representations_once(pool, account_id, email, selection) {
            Err(error) if error.is::<PostingRepresentationSnapshotChanged>() => continue,
            result => return result,
        }
    }
    Err(anyhow::Error::new(PostingRepresentationSnapshotChanged))
}

fn current_authority_posting_representations_once(
    pool: &DbPool,
    account_id: &str,
    email: &str,
    selection: PostingRepresentationSelection<'_>,
) -> Result<WorkspaceRepresentation> {
    // All mutable posting documents and the source/discovery/ATS authorities
    // used to represent one bounded page are read from one database snapshot.
    // Account export intentionally uses its separate exhaustive pager below.
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Deferred)?;
            let inputs = load_posting_representation_inputs_sqlite(&tx, account_id, email)?;
            let loaded = load_representation_postings_sqlite(
                &tx,
                account_id,
                selection,
                &inputs.applications,
            )?;
            let application_context = match selection {
                PostingRepresentationSelection::Page {
                    include_application_context: true,
                    ..
                } => load_application_context_jobs_sqlite(
                    &tx,
                    account_id,
                    &inputs.applications,
                    &loaded.postings,
                )?,
                _ => LoadedApplicationContextJobs {
                    jobs: Vec::new(),
                    fallback_batch_count: 0,
                },
            };
            let db_time_ms = representation_db_now_sqlite(&tx)?;
            let matches_page = representation_page_info(account_id, &loaded)?;
            let candidate_count = loaded.candidate_count;
            let LoadedRepresentationPostings {
                postings,
                discovery,
                ..
            } = loaded;
            let matches = represent_posting_page_from_inputs_sqlite_with_discovery(
                &tx, account_id, postings, discovery, &inputs, db_time_ms,
            )?;
            let application_jobs = application_context.jobs;
            let application_context_count = application_jobs.len();
            let application_context_fallback_batch_count = application_context.fallback_batch_count;
            tx.commit()?;
            Ok(WorkspaceRepresentation {
                profile: inputs.profile,
                preferences: inputs.preferences,
                tracks: inputs.tracks,
                applications: inputs.applications,
                matches,
                application_jobs,
                matches_page,
                candidate_count,
                authority_batch_count: 4,
                application_context_count,
                application_context_fallback_batch_count,
            })
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            lock_operational_hold_shared_postgres_tx(&mut tx).map_err(anyhow::Error::new)?;
            lock_managed_cloud_release_registry_shared_postgres_tx(&mut tx)?;
            lock_postgres_ats_certification(&mut tx).map_err(anyhow::Error::new)?;
            lock_discovery_account_shared_postgres(&mut tx, account_id)?;
            lock_job_integrity_publication_fence_shared_postgres_tx(&mut tx)
                .map_err(anyhow::Error::new)?;
            lock_account_policy_inputs_postgres(&mut tx, account_id, false)?;
            let mutable_inputs_sha256 =
                posting_representation_mutable_inputs_sha256_postgres(&mut tx, account_id)?;
            let inputs = load_posting_representation_inputs_postgres(&mut tx, account_id, email)?;
            let loaded = load_representation_postings_postgres(
                &mut tx,
                account_id,
                selection,
                &inputs.applications,
            )?;
            let application_context = match selection {
                PostingRepresentationSelection::Page {
                    include_application_context: true,
                    ..
                } => load_application_context_jobs_postgres(
                    &mut tx,
                    account_id,
                    &inputs.applications,
                    &loaded.postings,
                )?,
                _ => LoadedApplicationContextJobs {
                    jobs: Vec::new(),
                    fallback_batch_count: 0,
                },
            };
            let db_time_ms = representation_db_now_postgres(&mut tx)?;
            let matches_page = representation_page_info(account_id, &loaded)?;
            let candidate_count = loaded.candidate_count;
            let LoadedRepresentationPostings {
                postings,
                discovery,
                ..
            } = loaded;
            let matches = represent_posting_page_from_inputs_postgres_with_discovery(
                &mut tx, account_id, postings, discovery, &inputs, db_time_ms,
            )?;
            let application_jobs = application_context.jobs;
            let application_context_count = application_jobs.len();
            let application_context_fallback_batch_count = application_context.fallback_batch_count;
            if posting_representation_mutable_inputs_sha256_postgres(&mut tx, account_id)?
                != mutable_inputs_sha256
            {
                return Err(anyhow::Error::new(PostingRepresentationSnapshotChanged));
            }
            tx.commit()?;
            Ok(WorkspaceRepresentation {
                profile: inputs.profile,
                preferences: inputs.preferences,
                tracks: inputs.tracks,
                applications: inputs.applications,
                matches,
                application_jobs,
                matches_page,
                candidate_count,
                authority_batch_count: 4,
                application_context_count,
                application_context_fallback_batch_count,
            })
        }
    })
}

fn representation_page_info(
    account_id: &str,
    loaded: &LoadedRepresentationPostings,
) -> Result<JobsMatchesPageInfo> {
    Ok(JobsMatchesPageInfo {
        schema_version: 1,
        page_size: loaded.page_size as i64,
        has_more: loaded.next_key.is_some(),
        next_cursor: loaded
            .next_key
            .as_ref()
            .map(|key| encode_jobs_match_cursor(account_id, loaded.page_size, key))
            .transpose()?,
    })
}

fn represent_posting_page_from_inputs_sqlite(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    postings: Vec<JobPosting>,
    inputs: &PostingRepresentationInputs,
    db_time_ms: i64,
) -> Result<Vec<JobPosting>> {
    let discovery = load_discovery_authorities_sqlite(tx, account_id, &postings)?;
    represent_posting_page_from_inputs_sqlite_with_discovery(
        tx, account_id, postings, discovery, inputs, db_time_ms,
    )
}

fn represent_posting_page_from_inputs_sqlite_with_discovery(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    postings: Vec<JobPosting>,
    discovery: BTreeMap<String, Vec<JobDiscoveryAuthority>>,
    inputs: &PostingRepresentationInputs,
    db_time_ms: i64,
) -> Result<Vec<JobPosting>> {
    let application_ids_by_job = inputs
        .applications
        .iter()
        .map(|application| (application.job_id.as_str(), application.id.as_str()))
        .collect::<BTreeMap<_, _>>();
    let tracks_by_id = inputs
        .tracks
        .iter()
        .map(|track| (track.id.as_str(), track))
        .collect::<BTreeMap<_, _>>();
    represent_posting_page_sqlite(
        tx,
        account_id,
        postings,
        discovery,
        &PostingRepresentationContext {
            profile: &inputs.profile,
            preferences: &inputs.preferences,
            reservations: &inputs.reservations,
            application_ids_by_job: &application_ids_by_job,
            tracks_by_id: &tracks_by_id,
        },
        db_time_ms,
    )
}

fn represent_posting_page_from_inputs_postgres(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    postings: Vec<JobPosting>,
    inputs: &PostingRepresentationInputs,
    db_time_ms: i64,
) -> Result<Vec<JobPosting>> {
    let discovery = load_discovery_authorities_postgres(tx, account_id, &postings)?;
    represent_posting_page_from_inputs_postgres_with_discovery(
        tx, account_id, postings, discovery, inputs, db_time_ms,
    )
}

fn represent_posting_page_from_inputs_postgres_with_discovery(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    postings: Vec<JobPosting>,
    discovery: BTreeMap<String, Vec<JobDiscoveryAuthority>>,
    inputs: &PostingRepresentationInputs,
    db_time_ms: i64,
) -> Result<Vec<JobPosting>> {
    let application_ids_by_job = inputs
        .applications
        .iter()
        .map(|application| (application.job_id.as_str(), application.id.as_str()))
        .collect::<BTreeMap<_, _>>();
    let tracks_by_id = inputs
        .tracks
        .iter()
        .map(|track| (track.id.as_str(), track))
        .collect::<BTreeMap<_, _>>();
    represent_posting_page_postgres(
        tx,
        account_id,
        postings,
        discovery,
        &PostingRepresentationContext {
            profile: &inputs.profile,
            preferences: &inputs.preferences,
            reservations: &inputs.reservations,
            application_ids_by_job: &application_ids_by_job,
            tracks_by_id: &tracks_by_id,
        },
        db_time_ms,
    )
}

fn load_posting_representation_inputs_sqlite(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    email: &str,
) -> Result<PostingRepresentationInputs> {
    let mut profile = tx
        .query_row(
            "SELECT profile_json FROM jobs_profiles WHERE account_id=?1",
            params![account_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .map(|raw| parse_json::<CareerProfile>(raw, "Jobs representation profile"))
        .transpose()?
        .unwrap_or_else(|| default_profile(email));
    profile.auto_submit_threshold = default_auto_submit_threshold();
    profile.daily_limit = default_daily_limit();

    let preferences = enforce_job_preference_safety(
        tx.query_row(
            "SELECT preferences_json FROM jobs_preferences WHERE account_id=?1",
            params![account_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .map(|raw| parse_json::<JobPreferences>(raw, "Jobs representation preferences"))
        .transpose()?
        .unwrap_or_default(),
    );

    let track_rows = {
        let mut statement = tx.prepare(
            "SELECT id,track_json,active FROM jobs_tracks
              WHERE account_id=?1 ORDER BY active DESC,updated_at_ms DESC",
        )?;
        let rows = statement
            .query_map(params![account_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)? != 0,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows
    };
    let mut tracks = Vec::with_capacity(track_rows.len());
    for (id, raw, active) in track_rows {
        let mut track: CareerTrack = parse_json(raw, "Jobs representation Career Track")?;
        track.id = id;
        let activation_drift = track.active != active;
        track.active = active;
        if track.policy.authority.review_state == "approved"
            && (activation_drift
                || validate_track_policy_ledger_sqlite(tx, account_id, &track).is_err())
        {
            downgrade_track_policy_ledger(&mut track);
        }
        tracks.push(track);
    }

    // Keep the same reservation -> application row-lock order as execution
    // writers. PostgreSQL uses this order below; SQLite mirrors it so the two
    // representation paths remain structurally comparable.
    let reservations = {
        let mut statement = tx.prepare(
            "SELECT id,application_id,company_key,period_key,runner,status,
                    reserved_at_ms,updated_at_ms,
                    canonical_employer_id, canonical_employer_domain,
                    employer_attestation_sha256
               FROM jobs_attempt_reservations
              WHERE account_id=?1 ORDER BY reserved_at_ms DESC",
        )?;
        let rows = statement
            .query_map(params![account_id], |row| {
                Ok(AttemptReservation {
                    id: row.get(0)?,
                    application_id: row.get(1)?,
                    company_key: row.get(2)?,
                    period_key: row.get(3)?,
                    runner: row.get(4)?,
                    status: row.get(5)?,
                    reserved_at_ms: row.get(6)?,
                    updated_at_ms: row.get(7)?,
                    canonical_employer_id: row.get(8)?,
                    canonical_employer_domain: row.get(9)?,
                    employer_attestation_sha256: row.get(10)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows
    };

    let application_rows = {
        let mut statement = tx.prepare(
            "SELECT id,job_id,application_json FROM jobs_applications
              WHERE account_id=?1 ORDER BY updated_at_ms DESC",
        )?;
        let rows = statement
            .query_map(params![account_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows
    };
    let applications = application_rows
        .into_iter()
        .map(|(id, job_id, raw)| {
            parse_application_json(raw, &id, &job_id, "Jobs representation application")
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(PostingRepresentationInputs {
        profile,
        preferences,
        tracks,
        applications,
        reservations,
    })
}

fn load_posting_representation_inputs_postgres(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    email: &str,
) -> Result<PostingRepresentationInputs> {
    let mut profile = tx
        .query_opt(
            "SELECT profile_json FROM jobs_profiles WHERE account_id=$1",
            &[&account_id],
        )?
        .map(|row| parse_json::<CareerProfile>(row.get(0), "Jobs representation profile"))
        .transpose()?
        .unwrap_or_else(|| default_profile(email));
    profile.auto_submit_threshold = default_auto_submit_threshold();
    profile.daily_limit = default_daily_limit();

    let preferences = enforce_job_preference_safety(
        tx.query_opt(
            "SELECT preferences_json FROM jobs_preferences WHERE account_id=$1",
            &[&account_id],
        )?
        .map(|row| parse_json::<JobPreferences>(row.get(0), "Jobs representation preferences"))
        .transpose()?
        .unwrap_or_default(),
    );

    let track_rows = tx.query(
        "SELECT id,track_json,active FROM jobs_tracks
          WHERE account_id=$1 ORDER BY active DESC,updated_at_ms DESC FOR SHARE",
        &[&account_id],
    )?;
    let mut tracks = Vec::with_capacity(track_rows.len());
    for row in track_rows {
        let id: String = row.get(0);
        let active = row.get::<_, i32>(2) != 0;
        let mut track: CareerTrack = parse_json(row.get(1), "Jobs representation Career Track")?;
        track.id = id;
        let activation_drift = track.active != active;
        track.active = active;
        if track.policy.authority.review_state == "approved"
            && (activation_drift
                || validate_track_policy_ledger_postgres(tx, account_id, &track).is_err())
        {
            downgrade_track_policy_ledger(&mut track);
        }
        tracks.push(track);
    }

    // Do not lock application or reservation rows here. Production writers
    // legitimately use both cross-table orders; the before/after tuple-version
    // fingerprint below detects committed drift without introducing a reader /
    // writer deadlock edge.
    let reservations = tx
        .query(
            "SELECT id,application_id,company_key,period_key,runner,status,
                    reserved_at_ms,updated_at_ms,
                    canonical_employer_id, canonical_employer_domain,
                    employer_attestation_sha256
               FROM jobs_attempt_reservations
              WHERE account_id=$1 ORDER BY reserved_at_ms DESC",
            &[&account_id],
        )?
        .into_iter()
        .map(|row| AttemptReservation {
            id: row.get(0),
            application_id: row.get(1),
            company_key: row.get(2),
            period_key: row.get(3),
            runner: row.get(4),
            status: row.get(5),
            reserved_at_ms: row.get(6),
            updated_at_ms: row.get(7),
            canonical_employer_id: row.get(8),
            canonical_employer_domain: row.get(9),
            employer_attestation_sha256: row.get(10),
        })
        .collect();

    let applications = tx
        .query(
            "SELECT id,job_id,application_json FROM jobs_applications
              WHERE account_id=$1 ORDER BY updated_at_ms DESC",
            &[&account_id],
        )?
        .into_iter()
        .map(|row| {
            parse_application_json(
                row.get(2),
                row.get(0),
                row.get(1),
                "Jobs representation application",
            )
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(PostingRepresentationInputs {
        profile,
        preferences,
        tracks,
        applications,
        reservations,
    })
}

fn posting_representation_mutable_inputs_sha256_postgres(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
) -> Result<String> {
    // One UNION statement observes both mutable tables under one statement
    // snapshot. PostgreSQL's tuple version (`xmin`) makes even an exact-value
    // ABA rewrite distinguishable. A second identical fingerprint immediately
    // before commit detects endpoint membership or tuple-version drift from
    // every current application/reservation writer. Account deletion, the only
    // current delete path, is excluded by the account fence already held. No
    // row lock is taken here: production writers legitimately use both
    // application -> reservation and reservation -> application order.
    let rows = tx
        .query(
            "SELECT input_kind,input_id,row_version,
                    value_1,value_2,value_3,value_4,value_5,value_6,value_7,value_8
               FROM (
                 SELECT 'application'::text AS input_kind,
                        application.id AS input_id,
                        application.xmin::text AS row_version,
                        application.job_id AS value_1,
                        application.application_json AS value_2,
                        ''::text AS value_3, ''::text AS value_4,
                        ''::text AS value_5, ''::text AS value_6,
                        ''::text AS value_7, ''::text AS value_8
                   FROM jobs_applications application
                  WHERE application.account_id=$1
                 UNION ALL
                 SELECT 'reservation'::text AS input_kind,
                        reservation.id AS input_id,
                        reservation.xmin::text AS row_version,
                        reservation.application_id AS value_1,
                        reservation.company_key AS value_2,
                        reservation.period_key AS value_3,
                        reservation.runner AS value_4,
                        reservation.status AS value_5,
                        reservation.reserved_at_ms::text AS value_6,
                        reservation.updated_at_ms::text AS value_7,
                        json_build_array(reservation.canonical_employer_id,
                                         reservation.canonical_employer_domain,
                                         reservation.employer_attestation_sha256)::text AS value_8
                   FROM jobs_attempt_reservations reservation
                  WHERE reservation.account_id=$1
               ) mutable_input
              ORDER BY input_kind,input_id",
            &[&account_id],
        )?
        .into_iter()
        .map(|row| {
            (0..11)
                .map(|index| row.get::<_, String>(index))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let encoded = serde_json::to_vec(&rows)
        .context("serialize Jobs representation mutable input fingerprint")?;
    Ok(hex::encode(Sha256::digest(encoded)))
}

fn representation_db_now_sqlite(tx: &rusqlite::Transaction<'_>) -> Result<i64> {
    tx.query_row(
        "SELECT CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER)",
        [],
        |row| row.get(0),
    )
    .context("read Jobs representation SQLite database time")
}

fn representation_db_now_postgres(tx: &mut postgres::Transaction<'_>) -> Result<i64> {
    tx.query_one(
        "SELECT floor(extract(epoch FROM clock_timestamp()) * 1000)::bigint",
        &[],
    )
    .map(|row| row.get(0))
    .context("read Jobs representation PostgreSQL database time")
}

fn represent_posting_page_sqlite(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    postings: Vec<JobPosting>,
    discovery: BTreeMap<String, Vec<JobDiscoveryAuthority>>,
    context: &PostingRepresentationContext<'_>,
    db_time_ms: i64,
) -> Result<Vec<JobPosting>> {
    let projections = resolve_composed_job_integrity_projection_page_sqlite_tx_at_ms(
        tx, account_id, &postings, db_time_ms,
    )?;
    postings
        .into_iter()
        .zip(projections)
        .map(|(posting, projection)| {
            let (mut projected, mut eligibility) = representation_before_ats(
                &posting,
                &projection,
                context,
                discovery.get(&posting.id).map(Vec::as_slice).unwrap_or(&[]),
            );
            if let Some(resolution) = projection.ats_certification.as_ref() {
                apply_ats_certification_resolution(&projected, &mut eligibility, resolution);
            }
            projected.eligibility = Some(eligibility);
            Ok(projected)
        })
        .collect()
}

fn represent_posting_page_postgres(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    postings: Vec<JobPosting>,
    discovery: BTreeMap<String, Vec<JobDiscoveryAuthority>>,
    context: &PostingRepresentationContext<'_>,
    db_time_ms: i64,
) -> Result<Vec<JobPosting>> {
    let projections =
        resolve_composed_job_integrity_projection_page_postgres_tx_after_prelock_at_ms(
            tx, account_id, &postings, db_time_ms,
        )?;
    let mut represented = Vec::with_capacity(postings.len());
    for (posting, projection) in postings.into_iter().zip(projections) {
        let (mut projected, mut eligibility) = representation_before_ats(
            &posting,
            &projection,
            context,
            discovery.get(&posting.id).map(Vec::as_slice).unwrap_or(&[]),
        );
        if let Some(resolution) = projection.ats_certification.as_ref() {
            apply_ats_certification_resolution(&projected, &mut eligibility, resolution);
        }
        projected.eligibility = Some(eligibility);
        represented.push(projected);
    }
    Ok(represented)
}

fn representation_before_ats(
    posting: &JobPosting,
    projection: &ComposedJobIntegrityProjection,
    context: &PostingRepresentationContext<'_>,
    discovery_authorities: &[JobDiscoveryAuthority],
) -> (JobPosting, JobEligibilityDecision) {
    let mut projected =
        posting_with_original_source_projection(posting, &projection.original_source);
    if let Some(binding) = projection.original_source.integrity_binding.as_ref() {
        projected.canonical_url = binding.canonical_application_url.clone();
    }
    projected.discovery_evidence = projection.discovery_evidence.clone();
    let mut eligibility = build_job_eligibility(
        &projected,
        context.profile,
        context.preferences,
        context.reservations,
        true,
        context
            .application_ids_by_job
            .get(projected.id.as_str())
            .copied(),
        context
            .tracks_by_id
            .get(projected.track_id.as_str())
            .copied(),
    );
    apply_discovery_authorities(discovery_authorities, &mut eligibility);
    (projected, eligibility)
}

fn application_context_job_ids(
    applications: &[JobApplication],
    matches: &[JobPosting],
) -> Vec<String> {
    let match_ids = matches
        .iter()
        .map(|posting| posting.id.as_str())
        .collect::<BTreeSet<_>>();
    applications
        .iter()
        .map(|application| application.job_id.as_str())
        .filter(|job_id| !match_ids.contains(job_id))
        .map(str::to_string)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn application_context_snapshots(
    applications: &[JobApplication],
    job_ids: &[String],
) -> BTreeMap<String, ApplicationJobContext> {
    let requested = job_ids.iter().map(String::as_str).collect::<BTreeSet<_>>();
    let mut snapshots = BTreeMap::new();
    for application in applications {
        if !requested.contains(application.job_id.as_str())
            || snapshots.contains_key(&application.job_id)
        {
            continue;
        }
        let Some(value) = application.receipt.pointer("/job_snapshot") else {
            continue;
        };
        let Ok(posting) = serde_json::from_value::<JobPosting>(value.clone()) else {
            continue;
        };
        if let Ok(context) = application_job_context(&posting, &application.job_id) {
            snapshots.insert(application.job_id.clone(), context);
        }
    }
    snapshots
}

fn application_job_context(
    posting: &JobPosting,
    expected_job_id: &str,
) -> Result<ApplicationJobContext> {
    if posting.id != expected_job_id
        || posting.id.is_empty()
        || posting.company.trim().is_empty()
        || posting.company.chars().count() > 200
        || posting.title.trim().is_empty()
        || posting.title.chars().count() > 500
        || posting.location.chars().count() > 500
        || posting.track_id.chars().count() > 500
    {
        anyhow::bail!("Jobs application job snapshot is invalid")
    }
    Ok(ApplicationJobContext {
        id: posting.id.clone(),
        company: posting.company.clone(),
        title: posting.title.clone(),
        location: posting.location.clone(),
        track_id: posting.track_id.clone(),
    })
}

fn finish_application_context_jobs(
    job_ids: Vec<String>,
    mut postings_by_id: BTreeMap<String, ApplicationJobContext>,
    fallback_batch_count: usize,
) -> Result<LoadedApplicationContextJobs> {
    let jobs = job_ids
        .into_iter()
        .map(|job_id| {
            postings_by_id
                .remove(&job_id)
                .ok_or_else(|| anyhow::anyhow!("Jobs workspace application context is incomplete"))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(LoadedApplicationContextJobs {
        jobs,
        fallback_batch_count,
    })
}

fn load_application_context_jobs_sqlite(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    applications: &[JobApplication],
    matches: &[JobPosting],
) -> Result<LoadedApplicationContextJobs> {
    let job_ids = application_context_job_ids(applications, matches);
    if job_ids.is_empty() {
        return Ok(LoadedApplicationContextJobs {
            jobs: Vec::new(),
            fallback_batch_count: 0,
        });
    }
    let mut postings_by_id = application_context_snapshots(applications, &job_ids);
    let missing = job_ids
        .iter()
        .filter(|job_id| !postings_by_id.contains_key(*job_id))
        .cloned()
        .collect::<Vec<_>>();
    let fallback_batch_count = missing
        .len()
        .div_ceil(JOBS_WORKSPACE_APPLICATION_CONTEXT_QUERY_CHUNK);
    for chunk in missing.chunks(JOBS_WORKSPACE_APPLICATION_CONTEXT_QUERY_CHUNK) {
        let placeholders = (0..chunk.len())
            .map(|index| format!("?{}", index + 2))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT id,posting_json,match_score,updated_at_ms
               FROM jobs_postings
              WHERE account_id=?1 AND id IN ({placeholders})
              ORDER BY id"
        );
        let values = std::iter::once(account_id).chain(chunk.iter().map(String::as_str));
        let rows = tx
            .prepare(&sql)?
            .query_map(rusqlite::params_from_iter(values), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        for (id, raw, score, updated) in rows {
            let posting = authoritative_representation_posting(id, raw, score, updated)?;
            let context = application_job_context(&posting, &posting.id)?;
            postings_by_id.insert(context.id.clone(), context);
        }
    }
    finish_application_context_jobs(job_ids, postings_by_id, fallback_batch_count)
}

fn load_application_context_jobs_postgres(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    applications: &[JobApplication],
    matches: &[JobPosting],
) -> Result<LoadedApplicationContextJobs> {
    let job_ids = application_context_job_ids(applications, matches);
    if job_ids.is_empty() {
        return Ok(LoadedApplicationContextJobs {
            jobs: Vec::new(),
            fallback_batch_count: 0,
        });
    }
    let mut postings_by_id = application_context_snapshots(applications, &job_ids);
    let missing = job_ids
        .iter()
        .filter(|job_id| !postings_by_id.contains_key(*job_id))
        .cloned()
        .collect::<Vec<_>>();
    let fallback_batch_count = missing
        .len()
        .div_ceil(JOBS_WORKSPACE_APPLICATION_CONTEXT_QUERY_CHUNK);
    for chunk in missing.chunks(JOBS_WORKSPACE_APPLICATION_CONTEXT_QUERY_CHUNK) {
        let rows = tx.query(
            "SELECT id,posting_json,match_score,updated_at_ms
               FROM jobs_postings
              WHERE account_id=$1 AND id=ANY($2)
              ORDER BY id",
            &[&account_id, &chunk],
        )?;
        for row in rows {
            let posting = authoritative_representation_posting(
                row.get(0),
                row.get(1),
                row.get(2),
                row.get(3),
            )?;
            let context = application_job_context(&posting, &posting.id)?;
            postings_by_id.insert(context.id.clone(), context);
        }
    }
    finish_application_context_jobs(job_ids, postings_by_id, fallback_batch_count)
}

fn load_representation_postings_sqlite(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    selection: PostingRepresentationSelection<'_>,
    applications: &[JobApplication],
) -> Result<LoadedRepresentationPostings> {
    match selection {
        PostingRepresentationSelection::Page { limit, after, .. } => {
            validate_jobs_match_page_size(limit)?;
            let application_job_ids = applications
                .iter()
                .map(|application| application.job_id.as_str())
                .collect::<BTreeSet<_>>();
            // The posting document is encrypted, so interactive membership cannot
            // be expressed by the order index alone. Inspect one bounded candidate
            // window plus a look-ahead row; do not chase an account's expired or
            // otherwise non-interactive history until a page happens to fill.
            // A sparse window may therefore return a short (or empty) page with a
            // continuation cursor, which preserves both bounded work and traversal.
            let candidate_scan_limit = limit + 1;
            let fetch_limit = candidate_scan_limit + 1;
            let mut selected =
                Vec::<(JobPosting, JobsMatchPageKey, Vec<JobDiscoveryAuthority>)>::new();
            // Keep the first-page and continuation predicates separate so the
            // continuation is an actual range seek on the composite index,
            // rather than an `OR parameter IS NULL` filter over sorted history.
            let rows = if let Some(after) = after {
                let mut statement = tx.prepare(
                    "SELECT id,posting_json,match_score,updated_at_ms
                       FROM jobs_postings
                      WHERE account_id=?1
                        AND (match_score < ?2
                          OR (match_score = ?2 AND updated_at_ms < ?3)
                          OR (match_score = ?2 AND updated_at_ms = ?3 AND id > ?4))
                      ORDER BY match_score DESC,updated_at_ms DESC,id ASC
                      LIMIT ?5",
                )?;
                let rows = statement
                    .query_map(
                        params![
                            account_id,
                            after.match_score,
                            after.updated_at_ms,
                            after.job_id,
                            fetch_limit as i64
                        ],
                        |row| {
                            Ok((
                                row.get::<_, String>(0)?,
                                row.get::<_, String>(1)?,
                                row.get::<_, i64>(2)?,
                                row.get::<_, i64>(3)?,
                            ))
                        },
                    )?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                rows
            } else {
                let mut statement = tx.prepare(
                    "SELECT id,posting_json,match_score,updated_at_ms
                       FROM jobs_postings
                      WHERE account_id=?1
                      ORDER BY match_score DESC,updated_at_ms DESC,id ASC
                      LIMIT ?2",
                )?;
                let rows = statement
                    .query_map(params![account_id, fetch_limit as i64], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, i64>(2)?,
                            row.get::<_, i64>(3)?,
                        ))
                    })?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                rows
            };
            let candidate_count = rows.len();
            let has_unscanned_candidates = rows.len() > candidate_scan_limit;
            let mut postings = rows
                .into_iter()
                .take(candidate_scan_limit)
                .map(|(id, raw, match_score, updated_at_ms)| {
                    authoritative_representation_posting(id, raw, match_score, updated_at_ms)
                })
                .collect::<Result<Vec<_>>>()?;
            let mut discovery = load_discovery_authorities_sqlite(tx, account_id, &postings)?;
            let mut scanned_after = None;
            for posting in postings.drain(..) {
                let key = JobsMatchPageKey {
                    match_score: posting.match_score,
                    updated_at_ms: posting.updated_at_ms,
                    job_id: posting.id.clone(),
                };
                scanned_after = Some(key.clone());
                let authorities = discovery.remove(&posting.id).unwrap_or_default();
                if posting_is_interactive(
                    &posting,
                    &authorities,
                    application_job_ids.contains(posting.id.as_str()),
                ) {
                    selected.push((posting, key, authorities));
                }
            }
            finish_loaded_representation_page(
                selected,
                candidate_count,
                limit,
                scanned_after,
                has_unscanned_candidates,
            )
        }
        PostingRepresentationSelection::LegacyAll => {
            let mut statement = tx.prepare(
                "SELECT id,posting_json,match_score,updated_at_ms
                   FROM jobs_postings
                  WHERE account_id=?1
                  ORDER BY match_score DESC,updated_at_ms DESC,id ASC
                  LIMIT ?2",
            )?;
            let rows = statement
                .query_map(
                    params![account_id, (JOBS_LEGACY_MATCHES_MAX_POSTINGS + 1) as i64],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, i64>(2)?,
                            row.get::<_, i64>(3)?,
                        ))
                    },
                )?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            if rows.len() > JOBS_LEGACY_MATCHES_MAX_POSTINGS {
                anyhow::bail!(
                    "Jobs legacy matches require the paginated matches endpoint above {} postings",
                    JOBS_LEGACY_MATCHES_MAX_POSTINGS
                )
            }
            let postings = rows
                .into_iter()
                .map(|(id, raw, score, updated)| {
                    authoritative_representation_posting(id, raw, score, updated)
                })
                .collect::<Result<Vec<_>>>()?;
            let discovery = load_discovery_authorities_sqlite(tx, account_id, &postings)?;
            Ok(LoadedRepresentationPostings {
                candidate_count: postings.len(),
                page_size: postings.len().max(1),
                postings,
                discovery,
                next_key: None,
            })
        }
        PostingRepresentationSelection::JobId(job_id) => {
            let row = tx
                .query_row(
                    "SELECT id,posting_json,match_score,updated_at_ms
                       FROM jobs_postings WHERE account_id=?1 AND id=?2",
                    params![account_id, job_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, i64>(2)?,
                            row.get::<_, i64>(3)?,
                        ))
                    },
                )
                .optional()?;
            let postings = row
                .map(|(id, raw, score, updated)| {
                    authoritative_representation_posting(id, raw, score, updated)
                })
                .transpose()?
                .into_iter()
                .collect::<Vec<_>>();
            let discovery = load_discovery_authorities_sqlite(tx, account_id, &postings)?;
            Ok(LoadedRepresentationPostings {
                candidate_count: postings.len(),
                postings,
                discovery,
                next_key: None,
                page_size: 1,
            })
        }
    }
}

fn load_representation_postings_postgres(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    selection: PostingRepresentationSelection<'_>,
    applications: &[JobApplication],
) -> Result<LoadedRepresentationPostings> {
    match selection {
        PostingRepresentationSelection::Page { limit, after, .. } => {
            validate_jobs_match_page_size(limit)?;
            let application_job_ids = applications
                .iter()
                .map(|application| application.job_id.as_str())
                .collect::<BTreeSet<_>>();
            // Keep the PostgreSQL path identical to SQLite: one bounded
            // candidate window plus look-ahead, never a history-length scan.
            let candidate_scan_limit = limit + 1;
            let fetch_limit = candidate_scan_limit + 1;
            let mut selected =
                Vec::<(JobPosting, JobsMatchPageKey, Vec<JobDiscoveryAuthority>)>::new();
            let rows = if let Some(after) = after {
                tx.query(
                    "SELECT id,posting_json,match_score,updated_at_ms
                       FROM jobs_postings
                      WHERE account_id=$1
                        AND ((-match_score),(-updated_at_ms),id)
                            > (-$2::bigint,-$3::bigint,$4::text)
                      ORDER BY (-match_score) ASC,(-updated_at_ms) ASC,id ASC
                      LIMIT $5",
                    &[
                        &account_id,
                        &after.match_score,
                        &after.updated_at_ms,
                        &after.job_id,
                        &(fetch_limit as i64),
                    ],
                )?
            } else {
                tx.query(
                    "SELECT id,posting_json,match_score,updated_at_ms
                       FROM jobs_postings
                      WHERE account_id=$1
                      ORDER BY (-match_score) ASC,(-updated_at_ms) ASC,id ASC
                      LIMIT $2",
                    &[&account_id, &(fetch_limit as i64)],
                )?
            };
            let candidate_count = rows.len();
            let has_unscanned_candidates = rows.len() > candidate_scan_limit;
            let mut postings = rows
                .into_iter()
                .take(candidate_scan_limit)
                .map(|row| {
                    authoritative_representation_posting(
                        row.get(0),
                        row.get(1),
                        row.get(2),
                        row.get(3),
                    )
                })
                .collect::<Result<Vec<_>>>()?;
            let mut discovery = load_discovery_authorities_postgres(tx, account_id, &postings)?;
            let mut scanned_after = None;
            for posting in postings.drain(..) {
                let key = JobsMatchPageKey {
                    match_score: posting.match_score,
                    updated_at_ms: posting.updated_at_ms,
                    job_id: posting.id.clone(),
                };
                scanned_after = Some(key.clone());
                let authorities = discovery.remove(&posting.id).unwrap_or_default();
                if posting_is_interactive(
                    &posting,
                    &authorities,
                    application_job_ids.contains(posting.id.as_str()),
                ) {
                    selected.push((posting, key, authorities));
                }
            }
            finish_loaded_representation_page(
                selected,
                candidate_count,
                limit,
                scanned_after,
                has_unscanned_candidates,
            )
        }
        PostingRepresentationSelection::LegacyAll => {
            let rows = tx.query(
                "SELECT id,posting_json,match_score,updated_at_ms
                   FROM jobs_postings
                  WHERE account_id=$1
                  ORDER BY match_score DESC,updated_at_ms DESC,id ASC
                  LIMIT $2",
                &[
                    &account_id,
                    &((JOBS_LEGACY_MATCHES_MAX_POSTINGS + 1) as i64),
                ],
            )?;
            if rows.len() > JOBS_LEGACY_MATCHES_MAX_POSTINGS {
                anyhow::bail!(
                    "Jobs legacy matches require the paginated matches endpoint above {} postings",
                    JOBS_LEGACY_MATCHES_MAX_POSTINGS
                )
            }
            let postings = rows
                .into_iter()
                .map(|row| {
                    authoritative_representation_posting(
                        row.get(0),
                        row.get(1),
                        row.get(2),
                        row.get(3),
                    )
                })
                .collect::<Result<Vec<_>>>()?;
            let discovery = load_discovery_authorities_postgres(tx, account_id, &postings)?;
            Ok(LoadedRepresentationPostings {
                candidate_count: postings.len(),
                page_size: postings.len().max(1),
                postings,
                discovery,
                next_key: None,
            })
        }
        PostingRepresentationSelection::JobId(job_id) => {
            let posting = tx
                .query_opt(
                    "SELECT id,posting_json,match_score,updated_at_ms
                       FROM jobs_postings WHERE account_id=$1 AND id=$2",
                    &[&account_id, &job_id],
                )?
                .map(|row| {
                    authoritative_representation_posting(
                        row.get(0),
                        row.get(1),
                        row.get(2),
                        row.get(3),
                    )
                })
                .transpose()?;
            let postings = posting.into_iter().collect::<Vec<_>>();
            let discovery = load_discovery_authorities_postgres(tx, account_id, &postings)?;
            Ok(LoadedRepresentationPostings {
                candidate_count: postings.len(),
                postings,
                discovery,
                next_key: None,
                page_size: 1,
            })
        }
    }
}

fn authoritative_representation_posting(
    relational_id: String,
    raw: String,
    relational_match_score: i64,
    relational_updated_at_ms: i64,
) -> Result<JobPosting> {
    let mut posting: JobPosting = parse_json(raw, "job posting")?;
    posting.id = relational_id;
    posting.match_score = relational_match_score;
    posting.updated_at_ms = relational_updated_at_ms;
    Ok(posting)
}

fn posting_is_interactive(
    posting: &JobPosting,
    discovery: &[JobDiscoveryAuthority],
    has_application: bool,
) -> bool {
    if !matches!(posting.availability_status.as_str(), "active" | "unknown") {
        return false;
    }
    discovery.is_empty()
        || has_application
        || discovery.iter().any(|authority| {
            authority.source_status == "active"
                && matches!(authority.source_health.as_str(), "waiting" | "healthy")
                && matches!(authority.membership_status.as_str(), "active" | "unknown")
        })
}

fn finish_loaded_representation_page(
    mut selected: Vec<(JobPosting, JobsMatchPageKey, Vec<JobDiscoveryAuthority>)>,
    candidate_count: usize,
    limit: usize,
    scanned_after: Option<JobsMatchPageKey>,
    has_unscanned_candidates: bool,
) -> Result<LoadedRepresentationPostings> {
    let selected_overflow = selected.len() > limit;
    let has_more = selected_overflow || has_unscanned_candidates;
    selected.truncate(limit);
    let next_key = if selected_overflow {
        selected.last().map(|(_, key, _)| key.clone())
    } else if has_unscanned_candidates {
        scanned_after
    } else {
        None
    };
    if has_more && next_key.is_none() {
        anyhow::bail!("Jobs match page continuation is missing")
    }
    let mut discovery = BTreeMap::new();
    let postings = selected
        .into_iter()
        .map(|(posting, _, authorities)| {
            if !authorities.is_empty() {
                discovery.insert(posting.id.clone(), authorities);
            }
            posting
        })
        .collect();
    Ok(LoadedRepresentationPostings {
        postings,
        discovery,
        candidate_count,
        next_key,
        page_size: limit,
    })
}

fn load_export_posting_page_sqlite(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    after_id: Option<&str>,
) -> Result<Vec<JobPosting>> {
    let mut statement = tx.prepare(
        "SELECT id, posting_json FROM jobs_postings
          WHERE account_id = ?1 AND (?2 IS NULL OR id > ?2)
          ORDER BY id
          LIMIT ?3",
    )?;
    let rows = statement
        .query_map(
            params![
                account_id,
                after_id,
                ACCOUNT_EXPORT_REPRESENTATION_PAGE_SIZE as i64
            ],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|(relational_id, raw)| {
            let mut posting: JobPosting = parse_json(raw, "account export job posting")?;
            posting.id = relational_id;
            Ok(posting)
        })
        .collect()
}

fn load_export_posting_page_postgres(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    after_id: Option<&str>,
) -> Result<Vec<JobPosting>> {
    tx.query(
        "SELECT id, posting_json FROM jobs_postings
          WHERE account_id = $1 AND ($2::text IS NULL OR id > $2)
          ORDER BY id
          LIMIT $3",
        &[
            &account_id,
            &after_id,
            &(ACCOUNT_EXPORT_REPRESENTATION_PAGE_SIZE as i64),
        ],
    )?
    .into_iter()
    .map(|row| {
        let relational_id: String = row.get(0);
        let mut posting: JobPosting = parse_json(row.get(1), "account export job posting")?;
        posting.id = relational_id;
        Ok(posting)
    })
    .collect()
}

fn export_current_authority_posting_representations(
    pool: &DbPool,
    account_id: &str,
    email: &str,
) -> Result<WorkspaceRepresentation> {
    for _ in 0..3 {
        match export_current_authority_posting_representations_once(pool, account_id, email) {
            Err(error) if error.is::<PostingRepresentationSnapshotChanged>() => continue,
            result => return result,
        }
    }
    Err(anyhow::Error::new(PostingRepresentationSnapshotChanged))
}

fn export_current_authority_posting_representations_once(
    pool: &DbPool,
    account_id: &str,
    email: &str,
) -> Result<WorkspaceRepresentation> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Deferred)?;
            let inputs = load_posting_representation_inputs_sqlite(&tx, account_id, email)?;
            let db_time_ms = representation_db_now_sqlite(&tx)?;
            let mut after_id = None::<String>;
            let mut represented = Vec::new();
            loop {
                let postings =
                    load_export_posting_page_sqlite(&tx, account_id, after_id.as_deref())?;
                if postings.is_empty() {
                    break;
                }
                let next_id = postings.last().map(|posting| posting.id.clone());
                let discovery = load_discovery_authorities_sqlite(&tx, account_id, &postings)?;
                represented.extend(represent_posting_page_from_inputs_sqlite_with_discovery(
                    &tx, account_id, postings, discovery, &inputs, db_time_ms,
                )?);
                after_id = next_id;
            }
            tx.commit()?;
            Ok(WorkspaceRepresentation {
                profile: inputs.profile,
                preferences: inputs.preferences,
                tracks: inputs.tracks,
                applications: inputs.applications,
                matches: represented,
                application_jobs: Vec::new(),
                matches_page: JobsMatchesPageInfo::omitted_from_exhaustive_export(),
                candidate_count: 0,
                authority_batch_count: 0,
                application_context_count: 0,
                application_context_fallback_batch_count: 0,
            })
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            lock_operational_hold_shared_postgres_tx(&mut tx).map_err(anyhow::Error::new)?;
            lock_managed_cloud_release_registry_shared_postgres_tx(&mut tx)?;
            lock_postgres_ats_certification(&mut tx).map_err(anyhow::Error::new)?;
            lock_discovery_account_shared_postgres(&mut tx, account_id)?;
            lock_job_integrity_publication_fence_shared_postgres_tx(&mut tx)
                .map_err(anyhow::Error::new)?;
            lock_account_policy_inputs_postgres(&mut tx, account_id, false)?;
            let mutable_inputs_sha256 =
                posting_representation_mutable_inputs_sha256_postgres(&mut tx, account_id)?;
            let inputs = load_posting_representation_inputs_postgres(&mut tx, account_id, email)?;
            let db_time_ms = representation_db_now_postgres(&mut tx)?;
            let mut after_id = None::<String>;
            let mut represented = Vec::new();
            loop {
                let postings =
                    load_export_posting_page_postgres(&mut tx, account_id, after_id.as_deref())?;
                if postings.is_empty() {
                    break;
                }
                let next_id = postings.last().map(|posting| posting.id.clone());
                let discovery =
                    load_discovery_authorities_postgres(&mut tx, account_id, &postings)?;
                represented.extend(represent_posting_page_from_inputs_postgres_with_discovery(
                    &mut tx, account_id, postings, discovery, &inputs, db_time_ms,
                )?);
                after_id = next_id;
            }
            if posting_representation_mutable_inputs_sha256_postgres(&mut tx, account_id)?
                != mutable_inputs_sha256
            {
                return Err(anyhow::Error::new(PostingRepresentationSnapshotChanged));
            }
            tx.commit()?;
            Ok(WorkspaceRepresentation {
                profile: inputs.profile,
                preferences: inputs.preferences,
                tracks: inputs.tracks,
                applications: inputs.applications,
                matches: represented,
                application_jobs: Vec::new(),
                matches_page: JobsMatchesPageInfo::omitted_from_exhaustive_export(),
                candidate_count: 0,
                authority_batch_count: 0,
                application_context_count: 0,
                application_context_fallback_batch_count: 0,
            })
        }
    })
}

fn load_discovery_authorities_sqlite(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    postings: &[JobPosting],
) -> Result<BTreeMap<String, Vec<JobDiscoveryAuthority>>> {
    if postings.is_empty() {
        return Ok(BTreeMap::new());
    }
    let placeholders = (0..postings.len())
        .map(|index| format!("?{}", index + 2))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT m.job_id, s.id, s.provider, s.status, s.health,
                m.availability_status, m.last_seen_at_ms, m.last_seen_run_id
           FROM jobs_discovery_memberships m
           JOIN jobs_discovery_sources s ON s.id = m.source_id
          WHERE m.account_id = ?1 AND m.job_id IN ({placeholders})
          ORDER BY m.job_id, m.last_seen_at_ms DESC"
    );
    let mut statement = tx.prepare(&sql)?;
    let values =
        std::iter::once(account_id).chain(postings.iter().map(|posting| posting.id.as_str()));
    let rows = statement
        .query_map(rusqlite::params_from_iter(values), |row| {
            Ok((
                row.get::<_, String>(0)?,
                JobDiscoveryAuthority {
                    source_id: row.get(1)?,
                    provider: row.get(2)?,
                    source_status: row.get(3)?,
                    source_health: row.get(4)?,
                    membership_status: row.get(5)?,
                    last_seen_at_ms: row.get(6)?,
                    last_seen_run_id: row.get(7)?,
                },
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(group_discovery_authorities(rows))
}

fn load_discovery_authorities_postgres(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    postings: &[JobPosting],
) -> Result<BTreeMap<String, Vec<JobDiscoveryAuthority>>> {
    if postings.is_empty() {
        return Ok(BTreeMap::new());
    }
    let job_ids = postings
        .iter()
        .map(|posting| posting.id.clone())
        .collect::<Vec<_>>();
    let rows = tx.query(
        "SELECT m.job_id, s.id, s.provider, s.status, s.health,
                m.availability_status, m.last_seen_at_ms, m.last_seen_run_id
           FROM jobs_discovery_memberships m
           JOIN jobs_discovery_sources s ON s.id = m.source_id
          WHERE m.account_id = $1 AND m.job_id = ANY($2)
          ORDER BY m.job_id, m.last_seen_at_ms DESC",
        &[&account_id, &job_ids],
    )?;
    Ok(group_discovery_authorities(rows.into_iter().map(|row| {
        (
            row.get(0),
            JobDiscoveryAuthority {
                source_id: row.get(1),
                provider: row.get(2),
                source_status: row.get(3),
                source_health: row.get(4),
                membership_status: row.get(5),
                last_seen_at_ms: row.get(6),
                last_seen_run_id: row.get(7),
            },
        )
    })))
}

fn group_discovery_authorities(
    rows: impl IntoIterator<Item = (String, JobDiscoveryAuthority)>,
) -> BTreeMap<String, Vec<JobDiscoveryAuthority>> {
    let mut grouped = BTreeMap::<String, Vec<JobDiscoveryAuthority>>::new();
    for (job_id, authority) in rows {
        grouped.entry(job_id).or_default().push(authority);
    }
    grouped
}

pub fn account_export(
    pool: &DbPool,
    account_id: &str,
    email: &str,
) -> Result<Option<JobsAccountExport>> {
    let has_profile = crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => pool
            .get()?
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM jobs_profiles WHERE account_id = ?1)",
                params![account_id],
                |row| row.get::<_, bool>(0),
            )
            .context("check Jobs export data"),
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query_one(
                "SELECT EXISTS(SELECT 1 FROM jobs_profiles WHERE account_id = $1)",
                &[&account_id],
            )
            .map(|row| row.get::<_, bool>(0))
            .context("check Jobs export data"),
    })?;
    let public_beta = crate::db::jobs_beta_access::public_beta_account_export(pool, account_id)?;
    if !has_profile && public_beta.is_empty() {
        return Ok(None);
    }

    let workspace = if has_profile {
        let _ = ensure_primary_application_identity(pool, account_id, email)?;
        let representation =
            export_current_authority_posting_representations(pool, account_id, email)?;
        let mut workspace = build_workspace(pool, account_id, email, representation)?;
        for session in &mut workspace.browser_sessions {
            session.takeover_url = None;
        }
        Some(workspace)
    } else {
        None
    };
    Ok(Some(JobsAccountExport {
        workspace,
        canonical_track_policy_ledger: export_canonical_track_policy_ledger(pool, account_id)?,
        resume_versions: list_resume_versions(pool, account_id)?,
        attempt_reservations: list_attempt_reservations(pool, account_id)?,
        run_events: list_account_run_events(pool, account_id)?,
        provider_messages: export_provider_messages(pool, account_id)?,
        communication_actions: export_communication_actions(pool, account_id)?,
        communication_evidence: export_communication_evidence(pool, account_id)?,
        communication_reconciliations: export_communication_reconciliations(pool, account_id)?,
        public_beta_enrollments: public_beta.enrollments,
        public_beta_overrides: public_beta.overrides,
    }))
}

#[cfg(test)]
mod workspace_representation_tests {
    use super::*;

    struct WorkspaceTestDirectory {
        path: std::path::PathBuf,
    }

    impl WorkspaceTestDirectory {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "bluey-workspace-{label}-{}-{}",
                std::process::id(),
                uuid::Uuid::new_v4()
            ));
            std::fs::create_dir(&path).expect("create isolated workspace test directory");
            Self { path }
        }

        fn database_path(&self) -> std::path::PathBuf {
            self.path.join("jobs.sqlite3")
        }
    }

    impl Drop for WorkspaceTestDirectory {
        fn drop(&mut self) {
            let valid_name = self
                .path
                .file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.starts_with("bluey-workspace-"));
            if valid_name && self.path.parent() == Some(std::env::temp_dir().as_path()) {
                let _ = std::fs::remove_dir_all(&self.path);
            }
        }
    }

    #[test]
    fn postgres_representation_freezes_integrity_publication_before_one_clock_sample() {
        fn section<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
            source
                .split_once(start)
                .unwrap_or_else(|| panic!("missing section start {start}"))
                .1
                .split_once(end)
                .unwrap_or_else(|| panic!("missing section end {end}"))
                .0
        }

        fn assert_ordered(source: &str, needles: &[&str]) {
            let mut previous = 0;
            for needle in needles {
                let position = source
                    .find(needle)
                    .unwrap_or_else(|| panic!("missing ordered operation {needle}"));
                assert!(position >= previous, "operation order inverted at {needle}");
                previous = position;
            }
        }

        let workspace_source = include_str!("workspace.rs");
        let current = section(
            workspace_source,
            "fn current_authority_posting_representations_once(",
            "fn represent_posting_page_from_inputs_sqlite(",
        );
        let export = section(
            workspace_source,
            "fn export_current_authority_posting_representations_once(",
            "fn load_discovery_authorities_sqlite(",
        );
        fn postgres_branch<'a>(operation: &'a str, label: &str) -> &'a str {
            operation
                .split_once("DbPool::Postgres(_) =>")
                .unwrap_or_else(|| panic!("missing PostgreSQL {label} branch"))
                .1
        }
        for representation in [
            postgres_branch(current, "current representation"),
            postgres_branch(export, "export representation"),
        ] {
            assert_ordered(
                representation,
                &[
                    "lock_operational_hold_shared_postgres_tx",
                    "lock_managed_cloud_release_registry_shared_postgres_tx",
                    "lock_postgres_ats_certification",
                    "lock_discovery_account_shared_postgres",
                    "lock_job_integrity_publication_fence_shared_postgres_tx",
                    "lock_account_policy_inputs_postgres",
                    "posting_representation_mutable_inputs_sha256_postgres",
                    "load_posting_representation_inputs_postgres",
                    "representation_db_now_postgres",
                    "represent_posting_page_from_inputs_postgres",
                ],
            );
            assert_eq!(
                representation
                    .matches("representation_db_now_postgres")
                    .count(),
                1,
                "representation must use one PostgreSQL database time"
            );
            let final_fingerprint = representation
                .rfind("posting_representation_mutable_inputs_sha256_postgres")
                .expect("missing final mutable-input fingerprint");
            let represented = representation
                .find("represent_posting_page_from_inputs_postgres")
                .expect("missing PostgreSQL representation");
            assert!(
                final_fingerprint > represented,
                "mutable inputs must be rechecked after representation"
            );
        }

        let input_loader = section(
            workspace_source,
            "fn load_posting_representation_inputs_postgres(",
            "fn posting_representation_mutable_inputs_sha256_postgres(",
        );
        let reservation_loader = section(input_loader, "let reservations =", "let applications =");
        let application_loader = section(
            input_loader,
            "let applications =",
            "Ok(PostingRepresentationInputs",
        );
        for mutable_loader in [reservation_loader, application_loader] {
            assert!(!mutable_loader.contains("FOR SHARE"));
            assert!(!mutable_loader.contains("FOR UPDATE"));
        }
        let mutable_fingerprint = section(
            workspace_source,
            "fn posting_representation_mutable_inputs_sha256_postgres(",
            "fn representation_db_now_sqlite(",
        );
        assert!(mutable_fingerprint.contains("application.xmin::text"));
        assert!(mutable_fingerprint.contains("reservation.xmin::text"));
        assert!(mutable_fingerprint.contains("UNION ALL"));
        assert!(!mutable_fingerprint.contains("FOR SHARE"));
        assert!(!mutable_fingerprint.contains("FOR UPDATE"));

        let composition_source = include_str!("job_integrity_composition.rs");
        let representation_composition = section(
            composition_source,
            "pub(crate) fn resolve_composed_job_integrity_projection_postgres_tx_after_prelock_at_ms(",
            "fn resolve_composed_job_integrity_projection_postgres_tx_with_source(",
        );
        let composition_call = representation_composition
            .split_once("resolve_composed_job_integrity_projection_postgres_tx_with_source(")
            .expect("representation composition call")
            .1;
        let source = composition_call
            .find("original_source")
            .expect("representation composition source");
        let db_time = composition_call
            .find("db_time_ms")
            .expect("representation composition database time");
        assert!(source < db_time);
        assert!(!representation_composition.contains("Some(db_time_ms)"));
        let composed = section(
            composition_source,
            "fn resolve_composed_job_integrity_projection_postgres_tx_with_source(",
            "pub fn resolve_composed_job_integrity_projection(",
        );
        assert!(composed.contains(
            "resolve_current_job_integrity_authority_postgres_tx_after_publication_fence_at_ms"
        ));

        let integrity_source = include_str!("job_integrity_authority.rs");
        let after_fence_resolver = section(
            integrity_source,
            "pub(crate) fn resolve_current_job_integrity_authority_postgres_tx_after_publication_fence_at_ms(",
            "fn evaluate_stored_job_integrity_authority_postgres(",
        );
        assert!(after_fence_resolver
            .contains("load_job_integrity_control_postgres_after_publication_fence"));
        assert!(after_fence_resolver
            .contains("load_job_integrity_policy_postgres_after_publication_fence"));
        assert!(!after_fence_resolver.contains("load_job_integrity_control_postgres(tx, false)"));
        assert!(!after_fence_resolver.contains("load_job_integrity_policy_postgres(tx"));
        let attestation_writer = section(
            integrity_source,
            "fn import_job_integrity_attestation_postgres_tx(",
            "#[allow(clippy::too_many_arguments)]\nfn insert_job_integrity_attestation_postgres(",
        );
        assert_ordered(
            attestation_writer,
            &[
                "load_job_integrity_control_postgres(tx, true)",
                "load_job_integrity_head_postgres",
            ],
        );
    }

    #[test]
    fn page_batching_does_not_replace_detail_or_action_authority_reads() {
        fn section<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
            source
                .split_once(start)
                .unwrap_or_else(|| panic!("missing section start {start}"))
                .1
                .split_once(end)
                .unwrap_or_else(|| panic!("missing section end {end}"))
                .0
        }

        let source_authority = include_str!("original_source_verification.rs");
        let sqlite_page = section(
            source_authority,
            "fn resolve_original_source_verification_projection_page_sqlite_tx_at_ms(",
            "fn original_source_verification_unavailable_projection(",
        );
        assert_eq!(
            sqlite_page
                .matches("sqlite_original_source_verification_authority_for_account_tx(")
                .count(),
            1
        );
        assert!(sqlite_page.contains("trusted_original_source_memberships_page_sqlite"));
        assert!(sqlite_page.contains("original_source_verification_heads_page_sqlite"));
        assert!(!sqlite_page.contains(
            "resolve_original_source_verification_projection_sqlite_tx_with_authority_at_ms("
        ));
        let postgres_page = section(
            source_authority,
            "fn resolve_original_source_verification_projection_page_postgres_tx_after_prelock_at_ms(",
            "fn original_source_validate_projection_db_time(",
        );
        assert_eq!(
            postgres_page
                .matches(
                    "postgres_original_source_verification_authority_for_account_tx_after_prelock(",
                )
                .count(),
            1
        );
        assert!(postgres_page.contains("trusted_original_source_memberships_page_postgres"));
        assert!(postgres_page.contains("original_source_verification_heads_page_postgres"));
        assert!(!postgres_page.contains(
            "resolve_original_source_verification_projection_postgres_tx_with_authority_at_ms("
        ));

        let composition = include_str!("job_integrity_composition.rs");
        let sqlite_composition = section(
            composition,
            "fn compose_job_integrity_page_after_sources_sqlite(",
            "fn compose_job_integrity_page_after_sources_postgres(",
        );
        assert!(sqlite_composition
            .contains("resolve_current_job_integrity_authority_page_sqlite_tx_at_ms("));
        assert!(!sqlite_composition
            .contains("resolve_current_job_integrity_authority_sqlite_tx_at_ms("));
        let postgres_composition = section(
            composition,
            "fn compose_job_integrity_page_after_sources_postgres(",
            "fn resolve_composed_job_integrity_projection_postgres_tx_with_source(",
        );
        assert!(postgres_composition.contains(
            "resolve_current_job_integrity_authority_page_postgres_tx_after_publication_fence_at_ms("
        ));
        assert!(!postgres_composition.contains(
            "resolve_current_job_integrity_authority_postgres_tx_after_publication_fence_at_ms("
        ));

        let workspace_source = include_str!("workspace.rs");
        let sqlite_action_representation = section(
            workspace_source,
            "fn represent_posting_page_from_inputs_sqlite(",
            "fn represent_posting_page_from_inputs_sqlite_with_discovery(",
        );
        assert!(sqlite_action_representation.contains("load_discovery_authorities_sqlite("));
        let postgres_action_representation = section(
            workspace_source,
            "fn represent_posting_page_from_inputs_postgres(",
            "fn represent_posting_page_from_inputs_postgres_with_discovery(",
        );
        assert!(postgres_action_representation.contains("load_discovery_authorities_postgres("));
        let detail = section(
            workspace_source,
            "pub fn get_current_posting_representation(",
            "fn current_authority_posting_representations(",
        );
        assert!(detail.contains("PostingRepresentationSelection::JobId(job_id)"));
        assert!(!detail.contains("JobsMatchCursor"));

        let applications = include_str!("applications.rs");
        assert!(applications.contains("represent_posting_page_from_inputs_sqlite("));
        assert!(applications.contains("represent_posting_page_from_inputs_postgres("));
        assert!(applications.contains("resolve_composed_job_integrity_projection_sqlite_tx_at_ms("));
        assert!(applications.contains(
            "resolve_composed_job_integrity_projection_postgres_tx_after_prelock_at_ms("
        ));
        assert!(!applications.contains("list_current_posting_representation_page("));
        assert!(!applications.contains("JobsMatchCursor"));
    }

    #[test]
    fn postgres_workspace_explain_fixture_covers_first_and_deep_tied_keysets() {
        let fixture = include_str!("../../../tests/fixtures/jobs_workspace_postgres_explain.sql");
        assert!(fixture.contains("generate_series(0, 5499)"));
        assert_eq!(
            fixture
                .matches("EXPLAIN (ANALYZE, BUFFERS, COSTS OFF)")
                .count(),
            2
        );
        assert!(fixture.contains("idx_jobs_postings_account_workspace_page"));
        assert!(fixture.contains("((-match_score), (-updated_at_ms), id)"));
        assert!(fixture.contains("'phase626-workspace-4999'"));
        assert!(fixture.contains("ROLLBACK;"));
    }

    #[test]
    fn cursor_preserves_exact_long_tiebreakers_and_never_emits_an_unusable_value() {
        let account_id = "acct-cursor-boundary";
        let exact_id = format!(" {} ", "x".repeat(241));
        let encoded = encode_jobs_match_cursor(
            account_id,
            JOBS_WORKSPACE_DEFAULT_PAGE_SIZE,
            &JobsMatchPageKey {
                match_score: 91,
                updated_at_ms: 1,
                job_id: exact_id.clone(),
            },
        )
        .expect("encode a valid persisted tiebreaker above the removed arbitrary bound");
        assert!(encoded.len() <= 2_048);
        assert_eq!(
            decode_jobs_match_cursor(Some(&encoded), account_id, JOBS_WORKSPACE_DEFAULT_PAGE_SIZE,)
                .expect("decode exact persisted tiebreaker")
                .expect("cursor key")
                .job_id,
            exact_id
        );

        let mut accepted = 1usize;
        let mut rejected = 8_192usize;
        while accepted + 1 < rejected {
            let candidate = accepted + (rejected - accepted) / 2;
            let result = encode_jobs_match_cursor(
                account_id,
                JOBS_WORKSPACE_DEFAULT_PAGE_SIZE,
                &JobsMatchPageKey {
                    match_score: 91,
                    updated_at_ms: 1,
                    job_id: "z".repeat(candidate),
                },
            );
            if result.is_ok() {
                accepted = candidate;
            } else {
                rejected = candidate;
            }
        }
        let largest = encode_jobs_match_cursor(
            account_id,
            JOBS_WORKSPACE_DEFAULT_PAGE_SIZE,
            &JobsMatchPageKey {
                match_score: 91,
                updated_at_ms: 1,
                job_id: "z".repeat(accepted),
            },
        )
        .expect("encode the largest usable cursor fixture");
        assert!(largest.len() <= 2_048);
        assert_eq!(
            encode_jobs_match_cursor(
                account_id,
                JOBS_WORKSPACE_DEFAULT_PAGE_SIZE,
                &JobsMatchPageKey {
                    match_score: 91,
                    updated_at_ms: 1,
                    job_id: "z".repeat(rejected),
                },
            )
            .expect_err("never emit a cursor the decoder will reject")
            .to_string(),
            INVALID_JOBS_MATCH_CURSOR
        );
    }

    fn legacy_positive_posting(at_ms: i64) -> JobPosting {
        let mut posting = JobPosting {
            id: "workspace-legacy-positive".to_string(),
            canonical_key: String::new(),
            source: "greenhouse_import".to_string(),
            external_id: "workspace-legacy-positive".to_string(),
            company: "Acme".to_string(),
            title: "Software Engineer".to_string(),
            location: "New York, NY".to_string(),
            workplace: "hybrid".to_string(),
            canonical_url: "https://boards.greenhouse.io/acme/jobs/workspace-legacy-positive"
                .to_string(),
            description: "Build reliable software.".to_string(),
            compensation: String::new(),
            employment_type: "full_time".to_string(),
            track_id: "track-workspace".to_string(),
            match_score: 100,
            matched_reasons: Vec::new(),
            missing_requirements: Vec::new(),
            posted_at_ms: Some(at_ms),
            last_verified_at_ms: Some(at_ms),
            availability_status: "active".to_string(),
            status: "matched".to_string(),
            created_at_ms: at_ms,
            updated_at_ms: at_ms,
            discovery_evidence: JobDiscoveryEvidence::default(),
            eligibility: None,
        };
        posting.canonical_key = canonical_job_key(&posting);
        posting.discovery_evidence = JobDiscoveryEvidence {
            provenance: "original_source".to_string(),
            canonical_status: "canonical".to_string(),
            canonical_job_id: Some(posting.canonical_key.clone()),
            employer_verification_status: "verified".to_string(),
            employer_id: Some("mutable-employer".to_string()),
            canonical_employer_domain: Some("acme.example".to_string()),
            application_domain: Some("boards.greenhouse.io".to_string()),
            scam_risk_status: "clear".to_string(),
            scam_signals: Vec::new(),
            original_source_status: "verified_open".to_string(),
            original_source_checked_at_ms: Some(at_ms),
            original_source_snapshot_expires_at_ms: Some(at_ms + DAY_MS),
            original_source_evidence_hash: Some("b".repeat(64)),
            original_source_mismatched_fields: Vec::new(),
            requires_original_revalidation: false,
        };
        posting
    }

    fn install_source_positive_export_fixture(
        pool: &DbPool,
        account_id: &str,
        email: &str,
    ) -> Vec<String> {
        let identity = ensure_primary_application_identity(pool, account_id, email)
            .expect("create source-positive export identity");
        let profile = save_profile(pool, account_id, &default_profile(email))
            .expect("save source-positive export profile");
        let preferences = save_preferences(
            pool,
            account_id,
            &JobPreferences {
                sponsorship: "not_required".to_string(),
                ..JobPreferences::default()
            },
        )
        .expect("save source-positive export preferences");
        let track = upsert_track(
            pool,
            account_id,
            &CareerTrack {
                id: format!("track-source-positive-export-{account_id}"),
                name: "Software engineering".to_string(),
                role: "Software Engineer".to_string(),
                locations: vec!["New York, NY".to_string()],
                remote_preference: "hybrid_ok".to_string(),
                application_identity_id: Some(identity.id),
                policy: CareerTrackPolicy {
                    role_family: "software_engineering".to_string(),
                    ..CareerTrackPolicy::default()
                },
                active: true,
                match_count: 0,
                created_at_ms: 0,
                updated_at_ms: 0,
            },
        )
        .expect("save source-positive export Track");
        let posting_for_index = |index: usize| {
            let now = now_ms();
            let suffix = format!("source-positive-export-{index:03}");
            JobPosting {
                id: String::new(),
                canonical_key: String::new(),
                source: "greenhouse_import".to_string(),
                external_id: suffix.clone(),
                company: "Acme".to_string(),
                title: "Software Engineer".to_string(),
                location: "New York, NY".to_string(),
                workplace: "hybrid".to_string(),
                canonical_url: format!("https://boards.greenhouse.io/acme/jobs/{suffix}"),
                description: "Build reliable products with Rust and TypeScript.".to_string(),
                compensation: "$170k-$200k".to_string(),
                employment_type: "full_time".to_string(),
                track_id: track.id.clone(),
                match_score: 90,
                matched_reasons: Vec::new(),
                missing_requirements: Vec::new(),
                posted_at_ms: Some(now - 60_000),
                last_verified_at_ms: Some(now),
                availability_status: "active".to_string(),
                status: "matched".to_string(),
                created_at_ms: 0,
                updated_at_ms: 0,
                discovery_evidence: JobDiscoveryEvidence::default(),
                eligibility: None,
            }
        };

        let first_input = posting_for_index(0);
        let (first, managed) =
            production_positive_authority_fixture::save_production_positive_verified_import(
                pool,
                account_id,
                &first_input,
                &profile,
                &preferences,
            );
        let mut postings = vec![first];
        for index in 1..102 {
            let input = posting_for_index(index);
            let source = discovery_source_input_from_verified_import(
                &input.source,
                &input.canonical_url,
                &input.company,
                &input.track_id,
            )
            .expect("derive source-positive export discovery source")
            .expect("greenhouse import must derive a discovery source");
            postings.push(
                save_verified_import_posting_with_source(
                    pool,
                    account_id,
                    &input,
                    &source,
                    &profile,
                    &preferences,
                )
                .expect("save source-positive export verified import"),
            );
        }
        let last = postings
            .last()
            .expect("source-positive postings include last")
            .clone();
        production_positive_authority_fixture::save_production_positive_verified_import_with_runtime(
            pool,
            account_id,
            &last,
            &profile,
            &preferences,
        );
        let postings = list_postings(pool, account_id)
            .expect("reload source-positive export postings after full snapshot");
        assert_eq!(
            postings.len(),
            102,
            "full public discovery snapshot must contain 102 postings"
        );
        // Batch publication makes all 102 assignments due together. Consume
        // them in the real scheduler's FIFO order, not workspace rank order.
        let assignment_job_ids = match pool {
            DbPool::Sqlite(_) => {
                let connection = pool.get().expect("read export fixture lease order");
                let mut statement = connection
                    .prepare(
                        "SELECT job_id FROM jobs_original_source_verification_assignments
                  WHERE account_id=?1 AND state='pending'
                  ORDER BY next_attempt_at_ms,created_at_ms,assignment_id",
                    )
                    .expect("prepare export fixture lease order");
                let rows = statement
                    .query_map(params![account_id], |row| row.get::<_, String>(0))
                    .expect("read export fixture assignment jobs");
                rows.collect::<rusqlite::Result<Vec<_>>>()
                    .expect("collect export fixture lease order")
            }
            DbPool::Postgres(_) => pool
                .get_pg()
                .expect("read PostgreSQL export fixture lease order")
                .query(
                    "SELECT job_id FROM jobs_original_source_verification_assignments
                      WHERE account_id=$1 AND state='pending'
                      ORDER BY next_attempt_at_ms,created_at_ms,assignment_id",
                    &[&account_id],
                )
                .expect("read PostgreSQL export fixture assignment jobs")
                .into_iter()
                .map(|row| row.get::<_, String>(0))
                .collect(),
        };
        assert_eq!(assignment_job_ids.len(), 102);
        let mut postings_by_id = postings
            .into_iter()
            .map(|posting| (posting.id.clone(), posting))
            .collect::<BTreeMap<_, _>>();
        let mut ids = Vec::with_capacity(postings_by_id.len());
        for (index, job_id) in assignment_job_ids.iter().enumerate() {
            let posting = postings_by_id
                .remove(job_id)
                .expect("every due assignment belongs to the published fixture");
            let authority =
                production_positive_authority_fixture::install_production_positive_job_authorities(
                    pool,
                    account_id,
                    &posting,
                    &managed,
                    "acme.com",
                    &format!("source-positive-export-{index:03}"),
                );
            ids.push(authority.posting.id);
        }
        assert!(postings_by_id.is_empty());
        ids.sort();
        ids
    }

    fn assert_source_positive_export(
        pool: &DbPool,
        account_id: &str,
        email: &str,
        expected_ids: &[String],
    ) {
        let exported = account_export(pool, account_id, email)
            .expect("build source-positive account export")
            .expect("source-positive account has export data");
        let matches = &exported
            .workspace
            .expect("source-positive export contains workspace")
            .matches;
        let actual_ids = matches
            .iter()
            .map(|posting| posting.id.clone())
            .collect::<Vec<_>>();
        assert_eq!(actual_ids, expected_ids);
        assert_eq!(matches.len(), 102);
        assert_eq!(matches[99].id, expected_ids[99]);
        assert_eq!(matches[100].id, expected_ids[100]);
        for posting in matches {
            assert_eq!(
                posting.discovery_evidence.employer_verification_status,
                "verified"
            );
            assert_eq!(posting.discovery_evidence.scam_risk_status, "clear");
            assert_eq!(
                posting
                    .discovery_evidence
                    .canonical_employer_domain
                    .as_deref(),
                Some("acme.com")
            );
            assert_eq!(
                posting.discovery_evidence.original_source_status,
                "verified_open"
            );
            assert!(posting
                .eligibility
                .as_ref()
                .expect("source-positive export eligibility")
                .passed_checks
                .iter()
                .any(|check| check == "ats_certified"));
        }
    }

    #[test]
    fn sqlite_account_export_chunks_102_source_positive_jobs_at_authority_cap() {
        let test_directory = WorkspaceTestDirectory::new("positive-export");
        let pool = crate::db::open_pool(&test_directory.database_path())
            .expect("open source-positive export pool");
        crate::db::run_migrations(&pool).expect("migrate source-positive export pool");
        let account_id = "acct-source-positive-export";
        let email = "source-positive-export@example.com";
        pool.get()
            .expect("source-positive export connection")
            .execute(
                "INSERT INTO accounts (id,email,password_hash,trial_seconds_remaining)
                 VALUES (?1,?2,'hash',0)",
                params![account_id, email],
            )
            .expect("insert source-positive export account");
        let ids = install_source_positive_export_fixture(&pool, account_id, email);

        assert_source_positive_export(&pool, account_id, email, &ids);
    }

    #[test]
    fn exhaustive_export_page_is_bounded_by_the_authority_resolver_envelope() {
        assert_eq!(
            ACCOUNT_EXPORT_REPRESENTATION_PAGE_SIZE,
            JOBS_WORKSPACE_MAX_PAGE_SIZE
        );
        const {
            assert!(ACCOUNT_EXPORT_REPRESENTATION_PAGE_SIZE <= JOBS_WORKSPACE_MAX_PAGE_SIZE + 1);
        }
    }

    #[test]
    fn workspace_projection_keeps_original_source_authority_review_first() {
        let at_ms = now_ms();
        let raw = legacy_positive_posting(at_ms);
        let mut evidence = JobDiscoveryEvidence::provider_verified_original_source(
            raw.canonical_key.clone(),
            "greenhouse:acme".to_string(),
            Some("boards.greenhouse.io".to_string()),
            at_ms,
            "c".repeat(64),
        );
        // Match the Phase 614 relational projection exactly: the hosted ATS
        // binding is source evidence, not employer-identity authority.
        evidence.employer_id = None;
        evidence.canonical_employer_domain = None;
        let projection = OriginalSourceVerificationProjection {
            feature_active: true,
            evidence: Some(evidence),
            expected_head: None,
            db_time_ms: at_ms,
            integrity_binding: None,
        };

        let projected = posting_with_original_source_projection(&raw, &projection);
        let mut hard_failures = Vec::new();
        let mut review_reasons = Vec::new();
        let mut passed_checks = Vec::new();
        let gate = apply_posting_discovery_evidence(
            &projected,
            at_ms,
            &mut hard_failures,
            &mut review_reasons,
            &mut passed_checks,
        );

        assert_eq!(
            projected.discovery_evidence.employer_verification_status,
            "ats_tenant_verified"
        );
        assert_eq!(
            projected.discovery_evidence.scam_risk_status,
            "source_screened"
        );
        assert_eq!(projected.discovery_evidence.employer_id, None);
        assert_eq!(projected.discovery_evidence.canonical_employer_domain, None);
        assert!(gate.can_prepare);
        assert!(!gate.can_queue);
        assert!(hard_failures.is_empty());
        assert!(review_reasons
            .iter()
            .any(|reason| reason.code == "employer_identity_review_required"));
        assert!(review_reasons
            .iter()
            .any(|reason| reason.code == "job_risk_review_required"));
    }

    #[test]
    fn workspace_eligibility_blocks_authoritative_ats_composition_mismatch() {
        let at_ms = now_ms();
        let raw = legacy_positive_posting(at_ms);
        let source_evidence = JobDiscoveryEvidence::provider_verified_original_source(
            raw.canonical_key.clone(),
            "greenhouse:acme".to_string(),
            Some("boards.greenhouse.io".to_string()),
            at_ms,
            "c".repeat(64),
        );
        let original_source = OriginalSourceVerificationProjection {
            feature_active: true,
            evidence: Some(source_evidence),
            expected_head: None,
            db_time_ms: at_ms,
            integrity_binding: None,
        };
        let mismatch = JobIntegrityResolution {
            status: JobIntegrityResolutionStatus::Mismatch,
            reason_code: "ats_source_binding_mismatch".to_string(),
            signal_codes: Vec::new(),
            authority: None,
        };
        let composed =
            compose_job_integrity_projection(&raw, original_source, None, mismatch, false);
        let mut projected = raw;
        projected.discovery_evidence = composed.discovery_evidence;
        let mut hard_failures = Vec::new();
        let mut review_reasons = Vec::new();
        let mut passed_checks = Vec::new();

        let gate = apply_posting_discovery_evidence(
            &projected,
            at_ms,
            &mut hard_failures,
            &mut review_reasons,
            &mut passed_checks,
        );

        assert!(!gate.can_prepare);
        assert!(!gate.can_queue);
        assert!(hard_failures
            .iter()
            .any(|reason| reason.code == "employer_identity_mismatch"));
        assert!(hard_failures
            .iter()
            .any(|reason| reason.code == "scam_risk_blocked"));
    }

    #[test]
    fn workspace_never_returns_raw_legacy_positive_or_embedded_queue_booleans() {
        let path = std::env::temp_dir().join(format!(
            "bluey-workspace-representation-{}-{}.sqlite3",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let pool = crate::db::open_pool(&path).expect("open workspace representation pool");
        crate::db::run_migrations(&pool).expect("migrate workspace representation pool");
        pool.get()
            .expect("workspace representation connection")
            .execute(
                "INSERT INTO accounts (id, email, password_hash, trial_seconds_remaining)
                 VALUES ('acct-workspace-representation', 'workspace@example.com', 'hash', 0)",
                [],
            )
            .expect("insert workspace representation account");
        let profile = default_profile("workspace@example.com");
        save_profile(&pool, "acct-workspace-representation", &profile)
            .expect("save workspace representation profile");
        let preferences = JobPreferences {
            sponsorship: "not_required".to_string(),
            ..JobPreferences::default()
        };
        save_preferences(&pool, "acct-workspace-representation", &preferences)
            .expect("save workspace representation preferences");

        let saved = upsert_posting(
            &pool,
            "acct-workspace-representation",
            &legacy_positive_posting(now_ms()),
            &profile,
            &preferences,
        )
        .expect("save sanitized workspace posting");
        let relational_id = saved.id.clone();
        let mut legacy = saved;
        legacy.id = "zzzz-mutated-workspace-embedded-id".to_string();
        legacy.discovery_evidence.employer_verification_status = "verified".to_string();
        legacy.discovery_evidence.employer_id = Some("mutable-employer".to_string());
        legacy.discovery_evidence.canonical_employer_domain = Some("acme.example".to_string());
        legacy.discovery_evidence.scam_risk_status = "clear".to_string();
        let embedded = JobEligibilityDecision {
            can_auto_submit: true,
            can_queue_local: true,
            can_queue_cloud: true,
            ..JobEligibilityDecision::default()
        };
        legacy.eligibility = Some(embedded);
        let payload = to_json(&legacy, "legacy workspace posting")
            .expect("serialize legacy workspace posting");
        pool.get()
            .expect("legacy workspace posting connection")
            .execute(
                "UPDATE jobs_postings SET posting_json = ?2 WHERE account_id = ?1",
                params!["acct-workspace-representation", payload],
            )
            .expect("inject legacy workspace posting");

        let workspace_matches = workspace(
            &pool,
            "acct-workspace-representation",
            "workspace@example.com",
        )
        .expect("build authoritative workspace")
        .matches;
        let response = workspace_matches
            .into_iter()
            .find(|posting| posting.id == relational_id)
            .expect("workspace posting");

        let listed = list_current_posting_representations(
            &pool,
            "acct-workspace-representation",
            "workspace@example.com",
        )
        .expect("list authoritative workspace postings");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, relational_id);
        let detailed = get_current_posting_representation(
            &pool,
            "acct-workspace-representation",
            "workspace@example.com",
            &relational_id,
        )
        .expect("get authoritative workspace posting")
        .expect("workspace detail posting");
        assert_eq!(detailed.id, relational_id);
        assert_ne!(detailed.id, legacy.id);

        assert_eq!(
            response.discovery_evidence.employer_verification_status,
            "unknown"
        );
        assert_eq!(response.discovery_evidence.employer_id, None);
        assert_eq!(response.discovery_evidence.canonical_employer_domain, None);
        assert_eq!(response.discovery_evidence.scam_risk_status, "unknown");
        let eligibility = response.eligibility.expect("workspace eligibility");
        assert!(!eligibility.can_auto_submit);
        assert!(!eligibility.can_queue_local);
        assert!(!eligibility.can_queue_cloud);

        for index in 1..51 {
            let mut extra = legacy_positive_posting(now_ms());
            extra.id = format!("workspace-legacy-exhaustive-{index:03}");
            extra.external_id = extra.id.clone();
            extra.canonical_url = format!(
                "https://boards.greenhouse.io/acme/jobs/workspace-legacy-exhaustive-{index:03}"
            );
            extra.canonical_key = canonical_job_key(&extra);
            upsert_posting(
                &pool,
                "acct-workspace-representation",
                &extra,
                &profile,
                &preferences,
            )
            .expect("save legacy exhaustive posting");
        }
        let legacy = list_current_posting_representations(
            &pool,
            "acct-workspace-representation",
            "workspace@example.com",
        )
        .expect("legacy endpoint remains exhaustive inside its safety envelope");
        assert_eq!(legacy.len(), 51);
    }

    #[test]
    fn workspace_keeps_501_application_jobs_available_without_ranked_list_inflation() {
        let test_directory = WorkspaceTestDirectory::new("application-context");
        let pool = crate::db::open_pool(&test_directory.database_path())
            .expect("open application-context pool");
        crate::db::run_migrations(&pool).expect("migrate application-context pool");
        let account_id = "acct-application-context";
        let email = "application-context@example.com";
        let mut conn = pool.get().expect("application-context connection");
        conn.execute(
            "INSERT INTO accounts (id,email,password_hash,trial_seconds_remaining)
             VALUES (?1,?2,'hash',0)",
            params![account_id, email],
        )
        .expect("insert application-context account");
        save_profile(&pool, account_id, &default_profile(email))
            .expect("save application-context profile");
        save_preferences(&pool, account_id, &JobPreferences::default())
            .expect("save application-context preferences");

        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .expect("start application-context seed");
        let fixed_at_ms = now_ms();
        for index in 0..551 {
            let mut posting = legacy_positive_posting(fixed_at_ms);
            posting.id = format!("application-context-{index:04}");
            posting.external_id = posting.id.clone();
            posting.title = format!("Current role {index:04}");
            posting.canonical_url =
                format!("https://boards.greenhouse.io/acme/jobs/application-context-{index:04}");
            posting.canonical_key = canonical_job_key(&posting);
            if index == 50 {
                posting.availability_status = "expired".to_string();
            }
            let posting_payload =
                to_json(&posting, "application-context posting").expect("serialize posting");
            tx.execute(
                "INSERT INTO jobs_postings (
                    id,account_id,canonical_key,posting_json,source,canonical_url,
                    company,title,location,match_score,status,created_at_ms,updated_at_ms
                 ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
                params![
                    posting.id,
                    account_id,
                    posting.canonical_key,
                    posting_payload,
                    posting.source,
                    posting.canonical_url,
                    posting.company,
                    posting.title,
                    posting.location,
                    posting.match_score,
                    posting.status,
                    posting.created_at_ms,
                    posting.updated_at_ms,
                ],
            )
            .expect("insert application-context posting");

            if index < 50 {
                continue;
            }
            let mut frozen = posting.clone();
            if index == 50 {
                frozen.title = "Frozen expired application role".to_string();
            }
            let receipt = if index == 51 {
                json!({"job_snapshot": {"id": "wrong-embedded-id"}})
            } else {
                json!({"job_snapshot": frozen})
            };
            let application = JobApplication {
                id: format!("application-context-app-{index:04}"),
                job_id: posting.id.clone(),
                resume_version_id: None,
                state: "matched".to_string(),
                submission_mode: "review".to_string(),
                match_score: posting.match_score,
                answers: Vec::new(),
                cover_letter: String::new(),
                receipt,
                run_id: None,
                created_at_ms: fixed_at_ms,
                updated_at_ms: fixed_at_ms + index as i64,
                submitted_at_ms: None,
            };
            tx.execute(
                "INSERT INTO jobs_applications (
                    id,account_id,job_id,state,application_json,created_at_ms,updated_at_ms
                 ) VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![
                    application.id,
                    account_id,
                    application.job_id,
                    application.state,
                    to_json(&application, "application-context application")
                        .expect("serialize application"),
                    application.created_at_ms,
                    application.updated_at_ms,
                ],
            )
            .expect("insert application-context application");
        }
        tx.commit().expect("commit application-context seed");
        drop(conn);

        let load = workspace_page(
            &pool,
            account_id,
            email,
            JOBS_WORKSPACE_DEFAULT_PAGE_SIZE,
            None,
        )
        .expect("load workspace with 501 application contexts");
        assert_eq!(load.workspace.matches.len(), 50);
        assert_eq!(load.workspace.application_jobs.len(), 501);
        assert_eq!(load.diagnostics.application_context_count, 501);
        assert_eq!(load.diagnostics.application_context_fallback_batch_count, 1);
        assert_eq!(
            load.workspace
                .application_jobs
                .iter()
                .find(|context| context.id == "application-context-0050")
                .expect("expired application context")
                .title,
            "Frozen expired application role"
        );
        assert_eq!(
            load.workspace
                .application_jobs
                .iter()
                .find(|context| context.id == "application-context-0051")
                .expect("malformed snapshot fallback context")
                .title,
            "Current role 0051"
        );
        assert!(load
            .workspace
            .application_jobs
            .iter()
            .any(|context| context.id == "application-context-0550"));
        assert!(!load
            .workspace
            .matches
            .iter()
            .any(|posting| posting.id == "application-context-0050"));

        let continuation = load_current_posting_representation_page(
            &pool,
            account_id,
            email,
            JOBS_WORKSPACE_DEFAULT_PAGE_SIZE,
            load.workspace.matches_page.next_cursor.as_deref(),
        )
        .expect("load continuation without application context reload");
        assert_eq!(continuation.diagnostics.application_context_count, 0);
        assert_eq!(
            continuation
                .diagnostics
                .application_context_fallback_batch_count,
            0
        );

        let legacy_error = list_current_posting_representations(&pool, account_id, email)
            .expect_err("legacy exhaustive endpoint must not silently truncate above 500");
        assert!(legacy_error
            .to_string()
            .contains("require the paginated matches endpoint above 500 postings"));
    }

    #[test]
    fn interactive_workspace_pages_5500_rows_without_gaps_and_keeps_export_exhaustive() {
        let test_directory = WorkspaceTestDirectory::new("representation-bound");
        let path = test_directory.database_path();
        let pool = crate::db::open_pool(&path).expect("open representation-bound pool");
        crate::db::run_migrations(&pool).expect("migrate representation-bound pool");
        let mut conn = pool.get().expect("representation-bound connection");
        conn.execute(
            "INSERT INTO accounts (id, email, password_hash, trial_seconds_remaining)
             VALUES ('acct-representation-bound', 'bound@example.com', 'hash', 0)",
            [],
        )
        .expect("insert representation-bound account");
        pool.get()
            .expect("representation-bound tenant connection")
            .execute(
                "INSERT INTO accounts (id, email, password_hash, trial_seconds_remaining)
                 VALUES ('acct-representation-other', 'other@example.com', 'hash', 0)",
                [],
            )
            .expect("insert other representation account");
        let profile = default_profile("bound@example.com");
        save_profile(&pool, "acct-representation-bound", &profile)
            .expect("save representation-bound profile");
        save_preferences(
            &pool,
            "acct-representation-bound",
            &JobPreferences::default(),
        )
        .expect("save representation-bound preferences");
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .expect("start representation-bound seed");
        let fixed_at_ms = now_ms();
        for index in 0..5_500 {
            let mut posting = legacy_positive_posting(fixed_at_ms);
            let relational_id = format!("bounded-posting-{index:04}");
            posting.id = relational_id.clone();
            posting.external_id = posting.id.clone();
            posting.canonical_url =
                format!("https://boards.greenhouse.io/acme/jobs/bounded-posting-{index:04}");
            posting.canonical_key = canonical_job_key(&posting);
            if index == 250 {
                posting.availability_status = "expired".to_string();
            }
            let embedded = JobEligibilityDecision {
                can_auto_submit: true,
                can_queue_local: true,
                can_queue_cloud: true,
                ..JobEligibilityDecision::default()
            };
            posting.eligibility = Some(embedded);
            if index == 5_499 {
                posting.id = "zzzz-mutated-embedded-id".to_string();
            }
            let payload = to_json(&posting, "bounded posting").expect("serialize bounded posting");
            tx.execute(
                "INSERT INTO jobs_postings (
                    id, account_id, canonical_key, posting_json, source, canonical_url,
                    company, title, location, match_score, status, created_at_ms, updated_at_ms
                 ) VALUES (?1, 'acct-representation-bound', ?2, ?3, ?4, ?5, ?6, ?7, ?8,
                           ?9, ?10, ?11, ?12)",
                params![
                    relational_id,
                    posting.canonical_key,
                    payload,
                    posting.source,
                    posting.canonical_url,
                    posting.company,
                    posting.title,
                    posting.location,
                    posting.match_score,
                    posting.status,
                    posting.created_at_ms,
                    posting.updated_at_ms,
                ],
            )
            .expect("insert bounded posting");
        }
        let mut other = legacy_positive_posting(now_ms());
        other.id = "other-tenant-posting".to_string();
        other.external_id = other.id.clone();
        other.canonical_url =
            "https://boards.greenhouse.io/acme/jobs/other-tenant-posting".to_string();
        other.canonical_key = canonical_job_key(&other);
        let other_payload =
            to_json(&other, "other tenant posting").expect("serialize other tenant posting");
        tx.execute(
            "INSERT INTO jobs_postings (
                id, account_id, canonical_key, posting_json, source, canonical_url,
                company, title, location, match_score, status, created_at_ms, updated_at_ms
             ) VALUES (?1, 'acct-representation-other', ?2, ?3, ?4, ?5, ?6, ?7, ?8,
                       ?9, ?10, ?11, ?12)",
            params![
                other.id,
                other.canonical_key,
                other_payload,
                other.source,
                other.canonical_url,
                other.company,
                other.title,
                other.location,
                other.match_score,
                other.status,
                other.created_at_ms,
                other.updated_at_ms,
            ],
        )
        .expect("insert other tenant posting");
        tx.commit().expect("commit representation-bound seed");
        let first_page_query_plan = {
            let mut statement = conn
                .prepare(
                    "EXPLAIN QUERY PLAN
                     SELECT id,posting_json,match_score,updated_at_ms
                       FROM jobs_postings
                      WHERE account_id=?1
                      ORDER BY match_score DESC,updated_at_ms DESC,id ASC
                      LIMIT ?2",
                )
                .expect("prepare workspace page query plan");
            statement
                .query_map(params!["acct-representation-bound", 52_i64], |row| {
                    row.get::<_, String>(3)
                })
                .expect("explain workspace page query")
                .collect::<std::result::Result<Vec<_>, _>>()
                .expect("collect workspace page query plan")
        };
        let continuation_query_plan = {
            let mut statement = conn
                .prepare(
                    "EXPLAIN QUERY PLAN
                     SELECT id,posting_json,match_score,updated_at_ms
                       FROM jobs_postings
                      WHERE account_id=?1
                        AND (match_score < ?2
                          OR (match_score = ?2 AND updated_at_ms < ?3)
                          OR (match_score = ?2 AND updated_at_ms = ?3 AND id > ?4))
                      ORDER BY match_score DESC,updated_at_ms DESC,id ASC
                      LIMIT ?5",
                )
                .expect("prepare workspace continuation query plan");
            statement
                .query_map(
                    params![
                        "acct-representation-bound",
                        100_i64,
                        fixed_at_ms,
                        "bounded-posting-0049",
                        52_i64,
                    ],
                    |row| row.get::<_, String>(3),
                )
                .expect("explain workspace continuation query")
                .collect::<std::result::Result<Vec<_>, _>>()
                .expect("collect workspace continuation query plan")
        };
        for query_plan in [first_page_query_plan, continuation_query_plan] {
            assert!(query_plan
                .iter()
                .any(|detail| { detail.contains("idx_jobs_postings_account_workspace_page") }));
            assert!(!query_plan
                .iter()
                .any(|detail| detail.contains("USE TEMP B-TREE")));
        }
        drop(conn);

        let first = workspace_page(
            &pool,
            "acct-representation-bound",
            "bound@example.com",
            JOBS_WORKSPACE_DEFAULT_PAGE_SIZE,
            None,
        )
        .expect("load first bounded workspace page");
        assert_eq!(first.workspace.matches.len(), 50);
        assert!(first.workspace.matches_page.has_more);
        assert!(first.workspace.matches_page.next_cursor.is_some());
        assert_eq!(first.diagnostics.returned_count, 50);
        assert!(first.diagnostics.candidate_count <= 52);
        assert_eq!(first.diagnostics.authority_batch_count, 4);

        let mut seen = BTreeSet::new();
        let mut ordered = Vec::new();
        for posting in &first.workspace.matches {
            assert!(seen.insert(posting.id.clone()));
            ordered.push(posting.id.clone());
        }
        let first_cursor = first
            .workspace
            .matches_page
            .next_cursor
            .clone()
            .expect("first page cursor");
        let mut cursor = Some(first_cursor.clone());
        let mut page_count = 1usize;
        while let Some(current) = cursor {
            let page = list_current_posting_representation_page(
                &pool,
                "acct-representation-bound",
                "bound@example.com",
                JOBS_WORKSPACE_DEFAULT_PAGE_SIZE,
                Some(&current),
            )
            .expect("load next bounded match page");
            page_count += 1;
            for posting in &page.matches {
                assert!(seen.insert(posting.id.clone()), "duplicate {}", posting.id);
                ordered.push(posting.id.clone());
            }
            cursor = page.page.next_cursor;
            assert!(page_count <= 111, "cursor traversal did not terminate");
        }
        assert_eq!(seen.len(), 5_499);
        assert!(ordered.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(!seen.contains("bounded-posting-0250"));
        assert!(seen.contains("bounded-posting-5499"));
        assert!(!seen.contains("zzzz-mutated-embedded-id"));
        assert!(!seen.contains("other-tenant-posting"));

        let legacy_error = list_current_posting_representations(
            &pool,
            "acct-representation-bound",
            "bound@example.com",
        )
        .expect_err("legacy list must require pagination above its prior safety envelope");
        assert!(legacy_error
            .to_string()
            .contains("require the paginated matches endpoint above 500 postings"));

        let mut tampered = first_cursor.clone().into_bytes();
        let tamper_index = ENCRYPTED_PAYLOAD_PREFIX.len() + 4;
        tampered[tamper_index] = if tampered[tamper_index] == b'A' {
            b'B'
        } else {
            b'A'
        };
        let tampered = String::from_utf8(tampered).expect("ASCII cursor");
        let tampered_error = list_current_posting_representation_page(
            &pool,
            "acct-representation-bound",
            "bound@example.com",
            JOBS_WORKSPACE_DEFAULT_PAGE_SIZE,
            Some(&tampered),
        )
        .expect_err("tampered cursor must fail closed");
        assert_eq!(tampered_error.to_string(), INVALID_JOBS_MATCH_CURSOR);
        let foreign_error = list_current_posting_representation_page(
            &pool,
            "acct-representation-other",
            "other@example.com",
            JOBS_WORKSPACE_DEFAULT_PAGE_SIZE,
            first.workspace.matches_page.next_cursor.as_deref(),
        )
        .expect_err("foreign-account cursor must fail closed");
        assert_eq!(foreign_error.to_string(), INVALID_JOBS_MATCH_CURSOR);
        let malformed_error = list_current_posting_representation_page(
            &pool,
            "acct-representation-bound",
            "bound@example.com",
            JOBS_WORKSPACE_DEFAULT_PAGE_SIZE,
            Some("not-a-cursor"),
        )
        .expect_err("malformed cursor must fail closed");
        assert_eq!(malformed_error.to_string(), INVALID_JOBS_MATCH_CURSOR);
        let mismatched_size_error = list_current_posting_representation_page(
            &pool,
            "acct-representation-bound",
            "bound@example.com",
            JOBS_WORKSPACE_DEFAULT_PAGE_SIZE + 1,
            Some(&first_cursor),
        )
        .expect_err("cursor page-size changes must fail closed");
        assert_eq!(mismatched_size_error.to_string(), INVALID_JOBS_MATCH_CURSOR);
        let issued_at_ms = now_ms() - JOBS_MATCH_CURSOR_TTL_MS - 1;
        let expired_cursor = to_json(
            &JobsMatchCursorV1 {
                schema_version: 1,
                purpose: JOBS_MATCH_CURSOR_PURPOSE.to_string(),
                account_sha256: private_lookup_hash(
                    JOBS_MATCH_CURSOR_PURPOSE,
                    "acct-representation-bound",
                )
                .expect("hash cursor account"),
                page_size: JOBS_WORKSPACE_DEFAULT_PAGE_SIZE as i64,
                match_score: 100,
                updated_at_ms: fixed_at_ms,
                job_id: "bounded-posting-0049".to_string(),
                issued_at_ms,
                expires_at_ms: issued_at_ms + JOBS_MATCH_CURSOR_TTL_MS,
            },
            "expired Jobs match cursor fixture",
        )
        .expect("encrypt expired cursor fixture");
        let expired_cursor_error = list_current_posting_representation_page(
            &pool,
            "acct-representation-bound",
            "bound@example.com",
            JOBS_WORKSPACE_DEFAULT_PAGE_SIZE,
            Some(&expired_cursor),
        )
        .expect_err("expired cursor must fail closed");
        assert_eq!(expired_cursor_error.to_string(), INVALID_JOBS_MATCH_CURSOR);
        let limit_error = list_current_posting_representation_page(
            &pool,
            "acct-representation-bound",
            "bound@example.com",
            JOBS_WORKSPACE_MAX_PAGE_SIZE + 1,
            None,
        )
        .expect_err("oversized page must fail closed");
        assert!(limit_error
            .to_string()
            .contains("page size must be between"));

        let detail = get_current_posting_representation(
            &pool,
            "acct-representation-bound",
            "bound@example.com",
            "bounded-posting-0250",
        )
        .expect("expired application-history detail remains available")
        .expect("expired detail posting");
        assert_eq!(detail.id, "bounded-posting-0250");
        assert_eq!(detail.availability_status, "expired");
        assert_eq!(
            detail.discovery_evidence.employer_verification_status,
            "unknown"
        );
        assert_eq!(detail.discovery_evidence.employer_id, None);
        assert_eq!(detail.discovery_evidence.canonical_employer_domain, None);
        assert_eq!(detail.discovery_evidence.scam_risk_status, "unknown");

        let exported = account_export(&pool, "acct-representation-bound", "bound@example.com")
            .expect("export oversized account")
            .expect("oversized account export");
        let exported_json = serde_json::to_value(&exported).expect("serialize oversized export");
        assert!(exported_json.pointer("/workspace/matches_page").is_none());
        let workspace = exported
            .workspace
            .as_ref()
            .expect("profile workspace export");
        assert_eq!(workspace.matches.len(), 5_500);
        assert!(workspace
            .matches
            .windows(2)
            .all(|pair| pair[0].id < pair[1].id));
        assert!(!workspace
            .matches
            .iter()
            .any(|posting| posting.id == "other-tenant-posting"));
        assert!(workspace
            .matches
            .iter()
            .any(|posting| posting.id == "bounded-posting-5499"));
        assert!(!workspace
            .matches
            .iter()
            .any(|posting| posting.id == "zzzz-mutated-embedded-id"));
        for posting in &workspace.matches {
            assert_eq!(
                posting.discovery_evidence.employer_verification_status,
                "unknown"
            );
            assert_eq!(posting.discovery_evidence.employer_id, None);
            assert_eq!(posting.discovery_evidence.canonical_employer_domain, None);
            assert_eq!(posting.discovery_evidence.scam_risk_status, "unknown");
            let eligibility = posting.eligibility.as_ref().expect("export eligibility");
            assert!(!eligibility.can_auto_submit);
            assert!(!eligibility.can_queue_local);
            assert!(!eligibility.can_queue_cloud);
        }
        let expired = workspace
            .matches
            .iter()
            .find(|posting| posting.id == "bounded-posting-0250")
            .expect("expired export posting");
        assert!(expired
            .eligibility
            .as_ref()
            .expect("hard-denied export eligibility")
            .hard_failures
            .iter()
            .any(|reason| reason.code == "job_closed"));

        // A page must not keep scanning across an account's entire expired
        // history merely to fill 50 visible rows. It returns a cursor at the
        // bounded candidate-window boundary and makes progress on the next
        // request instead.
        let sparse_scan_conn = pool.get().expect("sparse interactive-window connection");
        for index in 0..=JOBS_WORKSPACE_DEFAULT_PAGE_SIZE {
            let posting_id = format!("bounded-posting-{index:04}");
            let raw = sparse_scan_conn
                .query_row(
                    "SELECT posting_json FROM jobs_postings
                      WHERE account_id=?1 AND id=?2",
                    params!["acct-representation-bound", &posting_id],
                    |row| row.get::<_, String>(0),
                )
                .expect("load sparse interactive-window posting");
            let mut posting = parse_json::<JobPosting>(raw, "sparse interactive-window posting")
                .expect("parse sparse interactive-window posting");
            posting.availability_status = "expired".to_string();
            sparse_scan_conn
                .execute(
                    "UPDATE jobs_postings SET posting_json=?3
                      WHERE account_id=?1 AND id=?2",
                    params![
                        "acct-representation-bound",
                        &posting_id,
                        to_json(&posting, "sparse interactive-window posting")
                            .expect("serialize sparse interactive-window posting"),
                    ],
                )
                .expect("expire sparse interactive-window posting");
        }
        drop(sparse_scan_conn);

        let sparse = workspace_page(
            &pool,
            "acct-representation-bound",
            "bound@example.com",
            JOBS_WORKSPACE_DEFAULT_PAGE_SIZE,
            None,
        )
        .expect("load bounded sparse interactive window");
        assert!(sparse.workspace.matches.is_empty());
        assert!(sparse.workspace.matches_page.has_more);
        assert!(sparse.workspace.matches_page.next_cursor.is_some());
        assert!(
            sparse.diagnostics.candidate_count <= JOBS_WORKSPACE_DEFAULT_PAGE_SIZE + 2,
            "a sparse page must inspect only one bounded candidate window"
        );
        let sparse_next = list_current_posting_representation_page(
            &pool,
            "acct-representation-bound",
            "bound@example.com",
            JOBS_WORKSPACE_DEFAULT_PAGE_SIZE,
            sparse.workspace.matches_page.next_cursor.as_deref(),
        )
        .expect("continue bounded sparse interactive window");
        assert_eq!(sparse_next.matches.len(), JOBS_WORKSPACE_DEFAULT_PAGE_SIZE);
        assert_eq!(sparse_next.matches[0].id, "bounded-posting-0051");

        drop(pool);
    }

    #[test]
    fn postgres_workspace_list_and_detail_allow_lockable_snapshot_reads() {
        let Ok(database_url) = std::env::var("BLUEY_TEST_POSTGRES_URL") else {
            eprintln!(
                "skipped postgres workspace representation test: \
                 BLUEY_TEST_POSTGRES_URL is unavailable"
            );
            return;
        };
        let pool = crate::db::open_postgres_pool(&database_url)
            .expect("open PostgreSQL workspace representation pool");
        crate::db::run_migrations(&pool).expect("migrate PostgreSQL workspace representation pool");
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let account_id = format!("acct-workspace-pg-{suffix}");
        let email = format!("workspace-pg-{suffix}@example.com");
        pool.get_pg()
            .expect("PostgreSQL workspace seed connection")
            .execute(
                "INSERT INTO accounts (id, email, password_hash, trial_seconds_remaining)
                 VALUES ($1, $2, 'hash', 0)",
                &[&account_id, &email],
            )
            .expect("insert PostgreSQL workspace account");

        let result = (|| -> Result<()> {
            let profile = default_profile(&email);
            save_profile(&pool, &account_id, &profile)?;
            let preferences = JobPreferences::default();
            save_preferences(&pool, &account_id, &preferences)?;
            let mut posting = legacy_positive_posting(now_ms());
            posting.id = format!("workspace-pg-posting-{suffix}");
            posting.external_id = posting.id.clone();
            posting.canonical_url =
                format!("https://boards.greenhouse.io/acme/jobs/workspace-pg-posting-{suffix}");
            posting.canonical_key = canonical_job_key(&posting);
            upsert_posting(&pool, &account_id, &posting, &profile, &preferences)?;

            let mut snapshot_conn = pool.get_pg()?;
            let mut snapshot_tx = snapshot_conn.transaction()?;
            let initial_inputs = posting_representation_mutable_inputs_sha256_postgres(
                &mut snapshot_tx,
                &account_id,
            )?;
            let phantom_application_id = format!("workspace-pg-phantom-{suffix}");
            pool.get_pg()?.execute(
                "INSERT INTO jobs_applications (
                    id,account_id,job_id,state,application_json,created_at_ms,updated_at_ms
                 ) VALUES ($1,$2,$3,'matched','{}',1,1)",
                &[&phantom_application_id, &account_id, &posting.id],
            )?;
            let final_inputs = posting_representation_mutable_inputs_sha256_postgres(
                &mut snapshot_tx,
                &account_id,
            )?;
            assert_ne!(
                initial_inputs, final_inputs,
                "a committed application phantom must invalidate the representation snapshot"
            );
            snapshot_tx.rollback()?;

            let reservation_id = format!("workspace-pg-reservation-{suffix}");
            pool.get_pg()?.execute(
                "INSERT INTO jobs_attempt_reservations (
                    id,account_id,application_id,company_key,period_key,runner,status,
                    reserved_at_ms,updated_at_ms,canonical_employer_id,
                    canonical_employer_domain,employer_attestation_sha256
                 ) VALUES ($1,$2,$3,'acme','test-period','local','reserved',1,1,
                           'workspace-fixture-employer','workspace-fixture.example',$4)",
                &[&reservation_id, &account_id, &phantom_application_id, &"a".repeat(64)],
            )?;
            let mut aba_connection = pool.get_pg()?;
            let mut aba_tx = aba_connection.transaction()?;
            let before_aba =
                posting_representation_mutable_inputs_sha256_postgres(&mut aba_tx, &account_id)?;
            pool.get_pg()?.execute(
                "UPDATE jobs_attempt_reservations
                    SET status='running',updated_at_ms=2
                  WHERE account_id=$1 AND id=$2",
                &[&account_id, &reservation_id],
            )?;
            pool.get_pg()?.execute(
                "UPDATE jobs_attempt_reservations
                    SET status='reserved',updated_at_ms=1
                  WHERE account_id=$1 AND id=$2",
                &[&account_id, &reservation_id],
            )?;
            let after_aba =
                posting_representation_mutable_inputs_sha256_postgres(&mut aba_tx, &account_id)?;
            assert_ne!(
                before_aba, after_aba,
                "PostgreSQL xmin must expose an exact-value ABA rewrite"
            );
            aba_tx.rollback()?;
            pool.get_pg()?.execute(
                "DELETE FROM jobs_attempt_reservations WHERE account_id=$1 AND id=$2",
                &[&account_id, &reservation_id],
            )?;
            pool.get_pg()?.execute(
                "DELETE FROM jobs_applications WHERE account_id=$1 AND id=$2",
                &[&account_id, &phantom_application_id],
            )?;

            let workspace_matches = workspace(&pool, &account_id, &email)?.matches;
            assert!(workspace_matches
                .iter()
                .any(|candidate| candidate.id == posting.id));
            let listed = list_current_posting_representations(&pool, &account_id, &email)?;
            assert!(listed.iter().any(|candidate| candidate.id == posting.id));
            let detail =
                get_current_posting_representation(&pool, &account_id, &email, &posting.id)?
                    .expect("PostgreSQL workspace detail posting");
            assert_eq!(detail.id, posting.id);

            let mut corrupted = posting.clone();
            corrupted.id = "zzzz-mutated-pg-embedded-id".to_string();
            let corrupted_json = to_json(&corrupted, "corrupted PostgreSQL export posting")?;
            pool.get_pg()?.execute(
                "UPDATE jobs_postings SET posting_json=$3
                  WHERE account_id=$1 AND id=$2",
                &[&account_id, &posting.id, &corrupted_json],
            )?;
            let exported =
                account_export(&pool, &account_id, &email)?.expect("PostgreSQL account export");
            let workspace = exported
                .workspace
                .as_ref()
                .expect("PostgreSQL profile workspace export");
            assert!(workspace
                .matches
                .iter()
                .any(|candidate| candidate.id == posting.id));
            assert!(!workspace
                .matches
                .iter()
                .any(|candidate| candidate.id == "zzzz-mutated-pg-embedded-id"));
            Ok(())
        })();

        pool.get_pg()
            .expect("PostgreSQL workspace cleanup connection")
            .execute("DELETE FROM accounts WHERE id = $1", &[&account_id])
            .expect("delete isolated PostgreSQL workspace account");
        result.expect("exercise PostgreSQL workspace representations");
    }

    #[test]
    #[serial_test::serial]
    fn postgres_account_export_chunks_102_source_positive_jobs_at_authority_cap() {
        let Ok(database_url) = std::env::var("BLUEY_TEST_POSTGRES_URL") else {
            eprintln!(
                "skipped PostgreSQL source-positive export test: \
                 BLUEY_TEST_POSTGRES_URL is unavailable"
            );
            return;
        };
        let pool = crate::db::open_postgres_pool(&database_url)
            .expect("open PostgreSQL source-positive export pool");
        crate::db::run_migrations(&pool).expect("migrate PostgreSQL source-positive export pool");
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let account_id = format!("acct-source-positive-export-pg-{suffix}");
        let email = format!("source-positive-export-pg-{suffix}@example.com");
        pool.get_pg()
            .expect("PostgreSQL source-positive export connection")
            .execute(
                "INSERT INTO accounts (id,email,password_hash,trial_seconds_remaining)
                 VALUES ($1,$2,'hash',0)",
                &[&account_id, &email],
            )
            .expect("insert PostgreSQL source-positive export account");

        let result: Result<()> = {
            let ids = install_source_positive_export_fixture(&pool, &account_id, &email);
            assert_source_positive_export(&pool, &account_id, &email, &ids);
            Ok(())
        };

        pool.get_pg()
            .expect("PostgreSQL source-positive export cleanup connection")
            .execute("DELETE FROM accounts WHERE id=$1", &[&account_id])
            .expect("delete PostgreSQL source-positive export account");
        result.expect("exercise PostgreSQL source-positive export");
    }
}

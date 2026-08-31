const CURRENT_AUTHORITY_REPRESENTATION_MAX_POSTINGS: usize =
    GLOBAL_DISCOVERY_MAX_MATERIALIZED_PER_ACCOUNT;
const ACCOUNT_EXPORT_REPRESENTATION_PAGE_SIZE: usize =
    CURRENT_AUTHORITY_REPRESENTATION_MAX_POSTINGS;

#[derive(Clone, Copy)]
enum PostingRepresentationSelection<'a> {
    All,
    JobId(&'a str),
}

struct PostingRepresentationContext<'a> {
    profile: &'a CareerProfile,
    preferences: &'a JobPreferences,
    reservations: &'a [AttemptReservation],
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
}

pub fn workspace(pool: &DbPool, account_id: &str, email: &str) -> Result<JobsWorkspace> {
    let _ = ensure_primary_application_identity(pool, account_id, email)?;
    let representation = current_authority_posting_representations(
        pool,
        account_id,
        email,
        PostingRepresentationSelection::All,
    )?;
    build_workspace(pool, account_id, email, representation)
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
        applications: representation.applications,
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
        PostingRepresentationSelection::All,
    )
    .map(|representation| representation.matches)
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
    // used to represent them are read from one database snapshot. The all-
    // matches shape fails closed above the same 500-row account materialization
    // envelope instead of opening an unbounded three-authority N+1 scan or
    // returning a partially projected response.
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Deferred)?;
            let inputs = load_posting_representation_inputs_sqlite(&tx, account_id, email)?;
            let postings = load_representation_postings_sqlite(&tx, account_id, selection)?;
            let db_time_ms = representation_db_now_sqlite(&tx)?;
            let matches = represent_posting_page_from_inputs_sqlite(
                &tx, account_id, postings, &inputs, db_time_ms,
            )?;
            tx.commit()?;
            Ok(WorkspaceRepresentation {
                profile: inputs.profile,
                preferences: inputs.preferences,
                tracks: inputs.tracks,
                applications: inputs.applications,
                matches,
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
            let postings = load_representation_postings_postgres(&mut tx, account_id, selection)?;
            let db_time_ms = representation_db_now_postgres(&mut tx)?;
            let matches = represent_posting_page_from_inputs_postgres(
                &mut tx, account_id, postings, &inputs, db_time_ms,
            )?;
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
            })
        }
    })
}

fn represent_posting_page_from_inputs_sqlite(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    postings: Vec<JobPosting>,
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
        &PostingRepresentationContext {
            profile: &inputs.profile,
            preferences: &inputs.preferences,
            reservations: &inputs.reservations,
        },
        &application_ids_by_job,
        &tracks_by_id,
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
        &PostingRepresentationContext {
            profile: &inputs.profile,
            preferences: &inputs.preferences,
            reservations: &inputs.reservations,
        },
        &application_ids_by_job,
        &tracks_by_id,
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
                    reserved_at_ms,updated_at_ms
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
                    reserved_at_ms,updated_at_ms
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
                        ''::text AS value_8
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
    context: &PostingRepresentationContext<'_>,
    application_ids_by_job: &BTreeMap<&str, &str>,
    tracks_by_id: &BTreeMap<&str, &CareerTrack>,
    db_time_ms: i64,
) -> Result<Vec<JobPosting>> {
    let discovery = load_discovery_authorities_sqlite(tx, account_id, &postings)?;
    postings
        .into_iter()
        .map(|posting| {
            let projection = resolve_composed_job_integrity_projection_sqlite_tx_at_ms(
                tx, account_id, &posting, db_time_ms,
            )?;
            let (mut projected, mut eligibility) = representation_before_ats(
                &posting,
                &projection,
                context,
                application_ids_by_job,
                tracks_by_id,
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
    context: &PostingRepresentationContext<'_>,
    application_ids_by_job: &BTreeMap<&str, &str>,
    tracks_by_id: &BTreeMap<&str, &CareerTrack>,
    db_time_ms: i64,
) -> Result<Vec<JobPosting>> {
    let discovery = load_discovery_authorities_postgres(tx, account_id, &postings)?;
    let mut represented = Vec::with_capacity(postings.len());
    for posting in postings {
        let projection = resolve_composed_job_integrity_projection_postgres_tx_after_prelock_at_ms(
            tx, account_id, &posting, db_time_ms,
        )?;
        let (mut projected, mut eligibility) = representation_before_ats(
            &posting,
            &projection,
            context,
            application_ids_by_job,
            tracks_by_id,
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
    application_ids_by_job: &BTreeMap<&str, &str>,
    tracks_by_id: &BTreeMap<&str, &CareerTrack>,
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
        application_ids_by_job.get(projected.id.as_str()).copied(),
        tracks_by_id.get(projected.track_id.as_str()).copied(),
    );
    apply_discovery_authorities(discovery_authorities, &mut eligibility);
    (projected, eligibility)
}

fn ensure_representation_posting_bound(
    selection: PostingRepresentationSelection<'_>,
    len: usize,
) -> Result<()> {
    if matches!(selection, PostingRepresentationSelection::All)
        && len > CURRENT_AUTHORITY_REPRESENTATION_MAX_POSTINGS
    {
        anyhow::bail!(
            "Jobs current-authority representation requires pagination above {} postings",
            CURRENT_AUTHORITY_REPRESENTATION_MAX_POSTINGS
        )
    }
    Ok(())
}

fn load_representation_postings_sqlite(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    selection: PostingRepresentationSelection<'_>,
) -> Result<Vec<JobPosting>> {
    let (sql, values) = match selection {
        PostingRepresentationSelection::All => (
            "SELECT id, posting_json FROM jobs_postings
              WHERE account_id = ?1
              ORDER BY match_score DESC, updated_at_ms DESC
              LIMIT ?2",
            vec![
                rusqlite::types::Value::Text(account_id.to_string()),
                rusqlite::types::Value::Integer(
                    (CURRENT_AUTHORITY_REPRESENTATION_MAX_POSTINGS + 1) as i64,
                ),
            ],
        ),
        PostingRepresentationSelection::JobId(job_id) => (
            "SELECT id, posting_json FROM jobs_postings
              WHERE account_id = ?1 AND id = ?2",
            vec![
                rusqlite::types::Value::Text(account_id.to_string()),
                rusqlite::types::Value::Text(job_id.to_string()),
            ],
        ),
    };
    let mut statement = tx.prepare(sql)?;
    let rows = statement
        .query_map(rusqlite::params_from_iter(values), |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let postings = rows
        .into_iter()
        .map(|(relational_id, raw)| {
            let mut posting: JobPosting = parse_json(raw, "job posting")?;
            posting.id = relational_id;
            Ok(posting)
        })
        .collect::<Result<Vec<_>>>()?;
    ensure_representation_posting_bound(selection, postings.len())?;
    Ok(postings)
}

fn load_representation_postings_postgres(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    selection: PostingRepresentationSelection<'_>,
) -> Result<Vec<JobPosting>> {
    let rows = match selection {
        PostingRepresentationSelection::All => tx.query(
            "SELECT id, posting_json FROM jobs_postings
              WHERE account_id = $1
              ORDER BY match_score DESC, updated_at_ms DESC
              LIMIT $2",
            &[
                &account_id,
                &((CURRENT_AUTHORITY_REPRESENTATION_MAX_POSTINGS + 1) as i64),
            ],
        )?,
        PostingRepresentationSelection::JobId(job_id) => tx.query(
            "SELECT id, posting_json FROM jobs_postings
              WHERE account_id = $1 AND id = $2",
            &[&account_id, &job_id],
        )?,
    };
    let postings = rows
        .into_iter()
        .map(|row| {
            let relational_id: String = row.get(0);
            let mut posting: JobPosting = parse_json(row.get(1), "job posting")?;
            posting.id = relational_id;
            Ok(posting)
        })
        .collect::<Result<Vec<_>>>()?;
    ensure_representation_posting_bound(selection, postings.len())?;
    Ok(postings)
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
                represented.extend(represent_posting_page_from_inputs_sqlite(
                    &tx, account_id, postings, &inputs, db_time_ms,
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
                represented.extend(represent_posting_page_from_inputs_postgres(
                    &mut tx, account_id, postings, &inputs, db_time_ms,
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
    }

    #[test]
    fn current_authority_list_fails_closed_above_its_bound_without_blocking_detail() {
        let path = std::env::temp_dir().join(format!(
            "bluey-workspace-representation-bound-{}-{}.sqlite3",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
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
        for index in 0..=CURRENT_AUTHORITY_REPRESENTATION_MAX_POSTINGS {
            let mut posting = legacy_positive_posting(now_ms());
            let relational_id = format!("bounded-posting-{index:04}");
            posting.id = relational_id.clone();
            posting.external_id = posting.id.clone();
            posting.canonical_url =
                format!("https://boards.greenhouse.io/acme/jobs/bounded-posting-{index:04}");
            posting.canonical_key = canonical_job_key(&posting);
            if index == 250 {
                posting.availability_status = "closed".to_string();
            }
            let embedded = JobEligibilityDecision {
                can_auto_submit: true,
                can_queue_local: true,
                can_queue_cloud: true,
                ..JobEligibilityDecision::default()
            };
            posting.eligibility = Some(embedded);
            if index == CURRENT_AUTHORITY_REPRESENTATION_MAX_POSTINGS - 1 {
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
        drop(conn);

        let error = list_current_posting_representations(
            &pool,
            "acct-representation-bound",
            "bound@example.com",
        )
        .expect_err("oversized representation must fail closed");
        assert!(error
            .to_string()
            .contains("current-authority representation requires pagination above 500"));

        let detail = get_current_posting_representation(
            &pool,
            "acct-representation-bound",
            "bound@example.com",
            "bounded-posting-0500",
        )
        .expect("bounded detail remains available")
        .expect("bounded detail posting");
        assert_eq!(detail.id, "bounded-posting-0500");
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
        let workspace = exported
            .workspace
            .as_ref()
            .expect("profile workspace export");
        assert_eq!(
            workspace.matches.len(),
            CURRENT_AUTHORITY_REPRESENTATION_MAX_POSTINGS + 1
        );
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
            .any(|posting| posting.id == "bounded-posting-0499"));
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
        let denied = workspace
            .matches
            .iter()
            .find(|posting| posting.id == "bounded-posting-0250")
            .expect("hard-denied export posting");
        assert!(denied
            .eligibility
            .as_ref()
            .expect("hard-denied export eligibility")
            .hard_failures
            .iter()
            .any(|reason| reason.code == "job_closed"));

        drop(pool);
        let _ = std::fs::remove_file(path);
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
                    reserved_at_ms,updated_at_ms
                 ) VALUES ($1,$2,$3,'acme','test-period','local','reserved',1,1)",
                &[&reservation_id, &account_id, &phantom_application_id],
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
}

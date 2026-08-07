
pub fn workspace(pool: &DbPool, account_id: &str, email: &str) -> Result<JobsWorkspace> {
    let _ = ensure_primary_application_identity(pool, account_id, email)?;
    let profile = get_profile(pool, account_id, email)?;
    let preferences = get_preferences(pool, account_id)?;
    let tracks = list_tracks(pool, account_id)?;
    let applications = list_applications(pool, account_id)?;
    let reservations = list_attempt_reservations(pool, account_id)?;
    let mut matches = list_postings(pool, account_id)?;
    for posting in &mut matches {
        let existing_application_id = applications
            .iter()
            .find(|application| application.job_id == posting.id)
            .map(|application| application.id.as_str());
        let track = tracks.iter().find(|track| track.id == posting.track_id);
        let mut eligibility = build_job_eligibility(
            posting,
            &profile,
            &preferences,
            &reservations,
            true,
            existing_application_id,
            track,
        );
        apply_discovery_authority(pool, account_id, posting, &mut eligibility)?;
        posting.eligibility = Some(eligibility);
    }
    Ok(JobsWorkspace {
        profile,
        preferences,
        facts: list_facts(pool, account_id)?,
        tracks,
        matches,
        applications,
        application_evidence: list_application_evidence(pool, account_id, None)?,
        browser_sessions: list_browser_sessions(pool, account_id)?,
        interventions: list_interventions(pool, account_id)?,
        answer_memory: list_answer_memory(pool, account_id)?,
        candidate_events: list_candidate_events(pool, account_id)?,
        integrations: list_integrations(pool, account_id)?,
        application_identities: list_application_identities(pool, account_id)?,
        auto_submit_authorizations: list_auto_submit_authorizations(
            pool,
            account_id,
            email,
        )?,
        mailbox_connections: list_mailbox_connections(pool, account_id)?,
        discovery_sources: list_discovery_sources(pool, account_id)?
            .iter()
            .map(DiscoverySourceSummary::from)
            .collect(),
        entitlement: get_entitlement(pool, account_id)?,
        runner_availability: RunnerAvailability::default(),
    })
}

pub fn account_export(
    pool: &DbPool,
    account_id: &str,
    email: &str,
) -> Result<Option<JobsAccountExport>> {
    let exists = crate::db::run_blocking_db(|| match pool {
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
    if !exists {
        return Ok(None);
    }

    let mut workspace = workspace(pool, account_id, email)?;
    for session in &mut workspace.browser_sessions {
        session.takeover_url = None;
    }
    Ok(Some(JobsAccountExport {
        workspace,
        resume_versions: list_resume_versions(pool, account_id)?,
        attempt_reservations: list_attempt_reservations(pool, account_id)?,
        run_events: list_account_run_events(pool, account_id)?,
        provider_messages: export_provider_messages(pool, account_id)?,
        communication_actions: export_communication_actions(pool, account_id)?,
        communication_evidence: export_communication_evidence(pool, account_id)?,
        communication_reconciliations: export_communication_reconciliations(pool, account_id)?,
    }))
}


const DAY_MS: i64 = 24 * 60 * 60 * 1_000;
const LIVE_VERIFICATION_MAX_AGE_MS: i64 = DAY_MS;

pub fn get_job_discovery_authority(
    pool: &DbPool,
    account_id: &str,
    job_id: &str,
) -> Result<Vec<JobDiscoveryAuthority>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT s.id, s.provider, s.status, s.health,
                        m.availability_status, m.last_seen_at_ms, m.last_seen_run_id
                   FROM jobs_discovery_memberships m
                   JOIN jobs_discovery_sources s ON s.id = m.source_id
                  WHERE m.account_id = ?1 AND m.job_id = ?2
                  ORDER BY m.last_seen_at_ms DESC",
            )?;
            let rows = stmt.query_map(params![account_id, job_id], |row| {
                Ok(JobDiscoveryAuthority {
                    source_id: row.get(0)?,
                    provider: row.get(1)?,
                    source_status: row.get(2)?,
                    source_health: row.get(3)?,
                    membership_status: row.get(4)?,
                    last_seen_at_ms: row.get(5)?,
                    last_seen_run_id: row.get(6)?,
                })
            })?;
            rows.collect::<std::result::Result<Vec<_>, _>>()
                .context("get Jobs discovery authority")
        }
        DbPool::Postgres(_) => Ok(pool
            .get_pg()?
            .query(
                "SELECT s.id, s.provider, s.status, s.health,
                        m.availability_status, m.last_seen_at_ms, m.last_seen_run_id
                   FROM jobs_discovery_memberships m
                   JOIN jobs_discovery_sources s ON s.id = m.source_id
                  WHERE m.account_id = $1 AND m.job_id = $2
                  ORDER BY m.last_seen_at_ms DESC",
                &[&account_id, &job_id],
            )?
            .into_iter()
            .map(|row| JobDiscoveryAuthority {
                source_id: row.get(0),
                provider: row.get(1),
                source_status: row.get(2),
                source_health: row.get(3),
                membership_status: row.get(4),
                last_seen_at_ms: row.get(5),
                last_seen_run_id: row.get(6),
            })
            .collect()),
    })
}

fn apply_discovery_authority(
    pool: &DbPool,
    account_id: &str,
    posting: &JobPosting,
    decision: &mut JobEligibilityDecision,
) -> Result<()> {
    let authorities = get_job_discovery_authority(pool, account_id, &posting.id)?;
    apply_discovery_authorities(&authorities, decision);
    Ok(())
}

fn apply_discovery_authorities(
    authorities: &[JobDiscoveryAuthority],
    decision: &mut JobEligibilityDecision,
) {
    if authorities.is_empty() {
        return;
    }
    let now = now_ms();
    let runnable = authorities.iter().any(|authority| {
        authority.source_status == "active"
            && authority.source_health == "healthy"
            && authority.membership_status == "active"
            && authority.last_seen_at_ms >= now - LIVE_VERIFICATION_MAX_AGE_MS
    });
    if runnable {
        decision
            .passed_checks
            .push("discovery_source_healthy".to_string());
        return;
    }

    decision.can_auto_submit = false;
    decision.can_queue_local = false;
    decision.can_queue_cloud = false;
    let message = if authorities
        .iter()
        .any(|authority| authority.source_health == "paused")
    {
        "This job source is paused after repeated failures. Bluey will not run applications until it recovers."
    } else if authorities
        .iter()
        .any(|authority| authority.source_health == "degraded")
    {
        "This job source is degraded. Bluey will keep the packet in Review until a healthy refresh succeeds."
    } else {
        "Bluey must refresh this job source before an application can enter a runner."
    };
    push_reason(
        &mut decision.review_reasons,
        "discovery_source_unhealthy",
        message,
    );
}

fn enforce_application_finalization_eligibility(
    application: &JobApplication,
    posting: &JobPosting,
    profile: &CareerProfile,
    preferences: &JobPreferences,
    track: Option<&CareerTrack>,
    reservations: &[AttemptReservation],
    authorities: &[JobDiscoveryAuthority],
) -> Result<()> {
    let mut decision = build_job_eligibility(
        posting,
        profile,
        preferences,
        reservations,
        false,
        Some(application.id.as_str()),
        track,
    );
    apply_discovery_authorities(authorities, &mut decision);
    if !decision.can_prepare {
        anyhow::bail!("job eligibility changed while the application packet was generated")
    }
    if application.state == "queued" && !decision.can_auto_submit {
        anyhow::bail!("auto-submit eligibility changed while the application packet was generated")
    }
    Ok(())
}

pub fn posting_age_days(posting: &JobPosting, at_ms: i64) -> i64 {
    let published_or_first_seen = posting.posted_at_ms.unwrap_or(posting.created_at_ms);
    at_ms.saturating_sub(published_or_first_seen).max(0) / DAY_MS
}

pub fn evaluate_job_eligibility(
    pool: &DbPool,
    account_id: &str,
    posting: &JobPosting,
    require_live_verification: bool,
    existing_application_id: Option<&str>,
) -> Result<JobEligibilityDecision> {
    let profile = get_profile(pool, account_id, "")?;
    let preferences = get_preferences(pool, account_id)?;
    let tracks = list_tracks(pool, account_id)?;
    let track = tracks.iter().find(|track| track.id == posting.track_id);
    let reservations = list_attempt_reservations(pool, account_id)?;
    let mut decision = build_job_eligibility(
        posting,
        &profile,
        &preferences,
        &reservations,
        require_live_verification,
        existing_application_id,
        track,
    );
    apply_discovery_authority(pool, account_id, posting, &mut decision)?;
    Ok(decision)
}

fn build_job_eligibility(
    posting: &JobPosting,
    profile: &CareerProfile,
    preferences: &JobPreferences,
    reservations: &[AttemptReservation],
    require_live_verification: bool,
    existing_application_id: Option<&str>,
    track: Option<&CareerTrack>,
) -> JobEligibilityDecision {
    let now = now_ms();
    let capability = submission_capability(posting);
    let mut hard_failures = Vec::new();
    let mut review_reasons = Vec::new();
    let mut passed_checks = Vec::new();
    let experience_evidence = role_experience_evidence(profile, track, posting);
    let experience_requirement = experience_requirement(posting);

    match track {
        None => push_reason(
            &mut hard_failures,
            "career_track_missing",
            "Choose an active Career Track before Bluey prepares this application.",
        ),
        Some(track) if !track.active => push_reason(
            &mut hard_failures,
            "career_track_inactive",
            "This Career Track is paused.",
        ),
        Some(track) if track.application_identity_id.is_none() => push_reason(
            &mut hard_failures,
            "application_identity_required",
            "Choose a verified application identity for this Career Track.",
        ),
        Some(track) => {
            passed_checks.push("career_track_active".to_string());
            passed_checks.push("application_identity_bound".to_string());
            let posting_family = posting_role_family(posting);
            if posting_family != ROLE_FAMILY_GENERIC
                && experience_evidence.role_family != ROLE_FAMILY_GENERIC
                && posting_family != experience_evidence.role_family
            {
                push_reason(
                    &mut hard_failures,
                    "role_family_mismatch",
                    "This role belongs to a different Career Track.",
                );
            } else if posting.track_id != track.id {
                push_reason(
                    &mut hard_failures,
                    "career_track_binding_changed",
                    "This job is not bound to the selected Career Track.",
                );
            } else {
                passed_checks.push("role_family_aligned".to_string());
            }
        }
    }

    match posting.availability_status.as_str() {
        "active" => passed_checks.push("job_active".to_string()),
        "unknown" => push_reason(
            &mut review_reasons,
            "availability_unverified",
            "Bluey has not verified that this pasted job is still accepting applications.",
        ),
        _ => push_reason(
            &mut hard_failures,
            "job_closed",
            "This job is no longer accepting applications.",
        ),
    }

    let age_days = posting_age_days(posting, now);
    if age_days > preferences.max_posting_age_days {
        push_reason(
            &mut hard_failures,
            "job_too_old",
            &format!(
                "Posted {age_days} days ago; your limit is {} days.",
                preferences.max_posting_age_days
            ),
        );
    } else {
        passed_checks.push("job_fresh".to_string());
    }

    let live_verification_missing = posting
        .last_verified_at_ms
        .is_none_or(|verified_at| verified_at < now - LIVE_VERIFICATION_MAX_AGE_MS);
    if require_live_verification && live_verification_missing {
        push_reason(
            &mut review_reasons,
            "live_verification_required",
            "Bluey must confirm this job is still open before a runner starts.",
        );
    } else if !live_verification_missing {
        passed_checks.push("job_recently_verified".to_string());
    }

    let company = posting.company.to_lowercase();
    if preferences.excluded_companies.iter().any(|excluded| {
        let excluded = excluded.trim().to_lowercase();
        !excluded.is_empty() && (company.contains(&excluded) || excluded.contains(&company))
    }) {
        push_reason(
            &mut hard_failures,
            "company_excluded",
            "This company is excluded by your Jobs settings.",
        );
    } else {
        passed_checks.push("company_allowed".to_string());
    }

    let title = posting.title.to_lowercase();
    if preferences.excluded_titles.iter().any(|excluded| {
        let excluded = excluded.trim().to_lowercase();
        !excluded.is_empty() && title.contains(&excluded)
    }) {
        push_reason(
            &mut hard_failures,
            "title_excluded",
            "This title is excluded by your Jobs settings.",
        );
    } else {
        passed_checks.push("title_allowed".to_string());
    }

    if let Some(minimum) = preferences.minimum_compensation {
        match compensation_range(&posting.compensation) {
            Some((_, maximum)) if maximum < minimum => push_reason(
                &mut hard_failures,
                "salary_below_floor",
                "The listed compensation is below your minimum compensation setting.",
            ),
            Some(_) => passed_checks.push("salary_floor_passed".to_string()),
            None => push_reason(
                &mut review_reasons,
                "salary_not_listed",
                "Compensation is not clear enough to verify your salary floor.",
            ),
        }
    }

    if let Some(reason) = location_failure(posting, profile, preferences) {
        if preferences.location_policy == "ask" {
            push_reason(&mut review_reasons, "location_needs_confirmation", &reason);
        } else {
            push_reason(&mut hard_failures, "location_mismatch", &reason);
        }
    } else {
        passed_checks.push("location_allowed".to_string());
    }

    if !preferences.employment_types.is_empty() {
        match candidate_employment_type(posting) {
            Some(kind)
                if preferences.employment_types.iter().any(|allowed| {
                    normalize_candidate_employment_type(allowed) == Some(kind)
                }) =>
            {
                passed_checks.push("employment_type_allowed".to_string());
            }
            Some(_) => push_reason(
                &mut hard_failures,
                "employment_type_mismatch",
                "This job uses an employment type you did not select.",
            ),
            None => push_reason(
                &mut review_reasons,
                "employment_type_unverified",
                "Bluey must confirm this job's employment type before Auto-submit.",
            ),
        }
    }

    if !preferences.engagement_types.is_empty() {
        match candidate_engagement_type(posting) {
            Some(kind)
                if preferences.engagement_types.iter().any(|allowed| {
                    normalize_candidate_engagement_type(allowed) == Some(kind)
                }) =>
            {
                passed_checks.push("engagement_type_allowed".to_string());
            }
            Some(_) => push_reason(
                &mut hard_failures,
                "engagement_type_mismatch",
                "This job uses an engagement type you did not select.",
            ),
            None => push_reason(
                &mut review_reasons,
                "engagement_type_unverified",
                "Bluey must confirm whether this role is W2, C2C, 1099, or direct hire before Auto-submit.",
            ),
        }
    }

    if let Some(reason) = track_employment_type_failure(posting, track) {
        push_reason(
            &mut hard_failures,
            "track_employment_type_mismatch",
            &reason,
        );
    } else if track.is_some() {
        passed_checks.push("track_employment_type_allowed".to_string());
    }
    if let Some(reason) = track_engagement_type_failure(posting, track) {
        push_reason(
            &mut hard_failures,
            "track_engagement_type_mismatch",
            &reason,
        );
    } else if track.is_some() {
        passed_checks.push("track_engagement_type_allowed".to_string());
    }
    if let Some(reason) = track_work_authorization_failure(profile, posting, track) {
        push_reason(
            &mut hard_failures,
            "work_authorization_mismatch",
            &reason,
        );
    } else if track.is_some() {
        passed_checks.push("work_authorization_allowed".to_string());
    }

    let required_minimum = experience_requirement
        .required_min_months
        .into_iter()
        .chain(experience_requirement.title_floor_months)
        .max();
    let required_maximum = experience_requirement.required_max_months;
    let misses_required_minimum = required_minimum
        .is_some_and(|months| months > experience_evidence.target_max_months);
    let exceeds_required_maximum = required_maximum
        .is_some_and(|months| months < experience_evidence.target_min_months);
    if misses_required_minimum || exceeds_required_maximum {
        push_reason(
            &mut hard_failures,
            "experience_outside_target_range",
            &format!(
                "This role's required level is outside this Career Track's {}-{} year target range.",
                experience_evidence.target_min_months / 12,
                (experience_evidence.target_max_months + 11) / 12,
            ),
        );
    } else if required_minimum.is_some() || required_maximum.is_some() {
        passed_checks.push("experience_aligned".to_string());
    }
    if experience_requirement
        .preferred_min_months
        .is_some_and(|months| months > experience_evidence.target_max_months)
    {
        push_reason(
            &mut review_reasons,
            "preferred_experience_above_target",
            "The preferred experience is above this Career Track's target range.",
        );
    }

    if preferences.sponsorship == "required" {
        if clearly_blocks_sponsorship(posting) {
            push_reason(
                &mut hard_failures,
                "sponsorship_unavailable",
                "This job appears to reject sponsorship.",
            );
        } else if clearly_offers_sponsorship(posting) {
            passed_checks.push("sponsorship_available".to_string());
        } else {
            push_reason(
                &mut review_reasons,
                "sponsorship_needs_confirmation",
                "Sponsorship support must be confirmed before Auto-submit.",
            );
        }
    } else if preferences.sponsorship == "ask" {
        if clearly_offers_sponsorship(posting) {
            passed_checks.push("sponsorship_available".to_string());
        } else {
            push_reason(
                &mut review_reasons,
                "sponsorship_answer_required",
                "Confirm the sponsorship answer before Auto-submit.",
            );
        }
    } else {
        passed_checks.push("sponsorship_policy_passed".to_string());
    }

    let active_company_key = normalize_company_key(&posting.company);
    let has_active_company_application = reservations.iter().any(|reservation| {
        existing_application_id.is_none_or(|existing_id| reservation.application_id != existing_id)
            && active_attempt_status(&reservation.status)
            && reservation.company_key == active_company_key
    });
    if has_active_company_application {
        push_reason(
            &mut hard_failures,
            "company_application_exists",
            "Bluey already has an in-progress or submitted application for this company. Another Career Track, resume, or application email does not create a second candidate.",
        );
    } else {
        passed_checks.push("company_application_clear".to_string());
    }

    let period_key = attempt_period_key(now, preferences.time_zone_offset_minutes);
    let daily_count = reservations
        .iter()
        .filter(|reservation| {
            existing_application_id
                .is_none_or(|existing_id| reservation.application_id != existing_id)
                && reservation.period_key == period_key
                && active_attempt_status(&reservation.status)
        })
        .count() as i64;
    if daily_count >= preferences.daily_limit.clamp(1, 50) {
        push_reason(
            &mut hard_failures,
            "daily_limit_reached",
            "Today's application limit has been reached.",
        );
    } else {
        passed_checks.push("daily_limit_available".to_string());
    }

    if !posting.missing_requirements.is_empty() {
        push_reason(
            &mut review_reasons,
            "missing_requirements",
            "This application still has required details to review.",
        );
    }
    if posting.match_score < profile.auto_submit_threshold.clamp(60, 100) {
        push_reason(
            &mut review_reasons,
            "below_auto_submit_threshold",
            "The match score is below your Auto-submit threshold.",
        );
    }

    match capability.as_str() {
        "certified" => passed_checks.push("ats_certified".to_string()),
        "beta_review" => push_reason(
            &mut review_reasons,
            "ats_review_required",
            "This application system is in beta and requires packet review.",
        ),
        "handoff" => push_reason(
            &mut review_reasons,
            "site_handoff_required",
            "Bluey can prepare the packet, but you complete submission on this site.",
        ),
        "unknown_review" => push_reason(
            &mut review_reasons,
            "unknown_ats_review_required",
            "This application system is not certified for runner submission.",
        ),
        _ => push_reason(
            &mut hard_failures,
            "site_blocked",
            "This job link cannot be opened by Bluey Jobs.",
        ),
    }

    let can_prepare = capability != "blocked"
        && hard_failures
            .iter()
            .all(|reason| reason.code == "daily_limit_reached")
        && (!require_live_verification || !live_verification_missing);
    let queue_capable = matches!(capability.as_str(), "certified" | "beta_review");
    let can_queue =
        can_prepare && hard_failures.is_empty() && queue_capable && !live_verification_missing;
    let can_auto_submit = can_queue
        && capability == "certified"
        && review_reasons.is_empty()
        && posting.missing_requirements.is_empty()
        && posting.match_score >= profile.auto_submit_threshold.clamp(60, 100);

    JobEligibilityDecision {
        capability,
        can_prepare,
        can_auto_submit,
        can_queue_local: can_queue,
        can_queue_cloud: can_queue,
        hard_failures,
        review_reasons,
        passed_checks,
        base_profile_fit: posting.match_score,
        tailored_packet_coverage: None,
        experience_requirement,
        experience_evidence,
        career_track_id: track.map(|value| value.id.clone()).unwrap_or_default(),
        application_identity_id: track.and_then(|value| value.application_identity_id.clone()),
        evidence_revision_id: None,
        evaluated_at_ms: now,
    }
}

fn push_reason(reasons: &mut Vec<EligibilityReason>, code: &str, message: &str) {
    if reasons.iter().any(|reason| reason.code == code) {
        return;
    }
    reasons.push(EligibilityReason {
        code: code.to_string(),
        message: message.to_string(),
    });
}

fn submission_capability(posting: &JobPosting) -> String {
    let url = posting.canonical_url.trim().to_ascii_lowercase();
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return "blocked".to_string();
    }
    if url.contains("linkedin.com") || url.contains("indeed.com") {
        return "handoff".to_string();
    }
    if known_review_only_ats(&url) {
        return "beta_review".to_string();
    }
    "unknown_review".to_string()
}

fn known_review_only_ats(url: &str) -> bool {
    [
        "greenhouse.io",
        "boards.greenhouse.io",
        "lever.co",
        "jobs.lever.co",
        "ashbyhq.com",
        "jobs.ashbyhq.com",
        "smartrecruiters.com",
        "workday.com",
        "myworkdayjobs.com",
    ]
    .iter()
    .any(|host| url.contains(host))
}

fn location_failure(
    posting: &JobPosting,
    profile: &CareerProfile,
    preferences: &JobPreferences,
) -> Option<String> {
    let workplace = format!("{} {}", posting.workplace, posting.location).to_ascii_lowercase();
    let is_remote = workplace.contains("remote");
    let is_hybrid = workplace.contains("hybrid");
    let is_onsite = workplace.contains("on-site")
        || workplace.contains("onsite")
        || workplace.contains("on site");

    if (preferences.location_policy == "remote_only"
        || preferences.remote_preference == "remote_only")
        && !is_remote
    {
        return Some("Your settings allow remote jobs only.".to_string());
    }
    if preferences.remote_preference == "remote_or_hybrid" && is_onsite && !is_hybrid {
        return Some(
            "Your settings allow remote or hybrid jobs, not on-site-only jobs.".to_string(),
        );
    }

    if preferences.desired_locations.is_empty() || is_remote {
        return None;
    }
    let posting_location = normalized_location(&posting.location);
    let mut allowed_locations = preferences.desired_locations.clone();
    if preferences.location_policy == "local" && !profile.current_location.trim().is_empty() {
        allowed_locations.push(profile.current_location.clone());
    }
    let matches_location = allowed_locations.iter().any(|candidate| {
        let candidate = normalized_location(candidate);
        !candidate.is_empty()
            && (posting_location.contains(&candidate) || candidate.contains(&posting_location))
    });
    (!matches_location).then(|| {
        format!(
            "{} is outside your selected job locations.",
            if posting.location.trim().is_empty() {
                "This location"
            } else {
                posting.location.trim()
            }
        )
    })
}

fn normalized_location(value: &str) -> String {
    value
        .to_ascii_lowercase()
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect()
}

fn eligibility_error_message(decision: &JobEligibilityDecision) -> String {
    decision
        .hard_failures
        .first()
        .or_else(|| decision.review_reasons.first())
        .map(|reason| reason.message.clone())
        .unwrap_or_else(|| "This application is not eligible for that action.".to_string())
}

fn normalize_company_key(company: &str) -> String {
    const LEGAL_SUFFIXES: &[&str] = &[
        "ag",
        "co",
        "company",
        "corp",
        "corporation",
        "gmbh",
        "inc",
        "incorporated",
        "limited",
        "llc",
        "ltd",
        "plc",
        "pte",
        "pty",
    ];
    let mut tokens: Vec<String> = company
        .to_ascii_lowercase()
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect();
    let fallback = tokens.concat();
    if tokens.first().is_some_and(|token| token == "the") {
        tokens.remove(0);
    }
    while tokens
        .last()
        .is_some_and(|token| LEGAL_SUFFIXES.contains(&token.as_str()))
    {
        tokens.pop();
    }
    let normalized = tokens.concat();
    if normalized.is_empty() {
        fallback
    } else {
        normalized
    }
}

fn active_attempt_status(status: &str) -> bool {
    matches!(
        status,
        "reserved" | "running" | "side_effect_unknown" | "submitted"
    )
}

fn attempt_period_key(at_ms: i64, offset_minutes: i64) -> String {
    let adjusted = at_ms.saturating_add(offset_minutes.clamp(-840, 840) * 60_000);
    Utc.timestamp_millis_opt(adjusted)
        .single()
        .map(|timestamp| {
            format!(
                "{:04}-{:02}-{:02}",
                timestamp.year(),
                timestamp.month(),
                timestamp.day()
            )
        })
        .unwrap_or_else(|| "1970-01-01".to_string())
}

fn compensation_range(compensation: &str) -> Option<(i64, i64)> {
    let normalized = compensation.replace([',', '$'], "").to_lowercase();
    let mut amounts = Vec::new();
    for token in normalized
        .split(|character: char| !(character.is_ascii_digit() || character == 'k'))
        .filter(|token| !token.is_empty())
    {
        if let Some(raw) = token.strip_suffix('k') {
            if let Ok(value) = raw.parse::<i64>() {
                amounts.push(value * 1_000);
            }
        } else if let Ok(value) = token.parse::<i64>() {
            amounts.push(if value < 1_000 { value * 1_000 } else { value });
        }
    }
    if amounts.is_empty() {
        None
    } else {
        Some((
            *amounts.iter().min().unwrap_or(&0),
            *amounts.iter().max().unwrap_or(&0),
        ))
    }
}

fn clearly_blocks_sponsorship(posting: &JobPosting) -> bool {
    let text = format!("{} {}", posting.title, posting.description).to_lowercase();
    [
        "no sponsorship",
        "not sponsor",
        "does not sponsor",
        "do not sponsor",
        "don't sponsor",
        "not able to sponsor",
        "without sponsorship",
        "must be authorized to work",
        "will not sponsor",
        "cannot sponsor",
        "unable to sponsor",
        "not eligible for visa sponsorship",
        "not eligible for sponsorship",
        "ineligible for visa sponsorship",
        "ineligible for sponsorship",
        "no visa sponsorship available",
        "no visa sponsorship is available",
        "no sponsorship available",
        "no sponsorship is available",
        "visa sponsorship is not available",
        "visa sponsorship not available",
        "sponsorship is not available",
        "sponsorship not available",
        "sponsorship unavailable",
        "u.s. citizenship required",
        "us citizenship required",
        "must be a u.s. citizen",
        "must be a us citizen",
        "must be u.s. citizen",
        "must be us citizen",
    ]
    .iter()
    .any(|phrase| text.contains(phrase))
}

fn clearly_offers_sponsorship(posting: &JobPosting) -> bool {
    if clearly_blocks_sponsorship(posting) {
        return false;
    }
    let text = format!("{} {}", posting.title, posting.description).to_lowercase();
    [
        "eligible for visa sponsorship",
        "eligible for sponsorship",
        "visa sponsorship is available",
        "visa sponsorship available",
        "sponsorship is available",
        "sponsorship available",
        "we provide visa sponsorship",
        "we offer visa sponsorship",
        "will sponsor visas",
        "can sponsor visas",
    ]
    .iter()
    .any(|phrase| text.contains(phrase))
}

fn score_posting(
    posting: &JobPosting,
    profile: &CareerProfile,
    preferences: &JobPreferences,
    track: Option<&CareerTrack>,
) -> (i64, Vec<String>, Vec<String>) {
    let mut score = 25i64;
    let mut reasons = Vec::new();
    let mut missing = Vec::new();
    let title = posting.title.to_lowercase();
    let description = posting.description.to_lowercase();
    let location = posting.location.to_lowercase();

    let target_roles = track
        .map(|value| vec![value.role.as_str()])
        .unwrap_or_else(|| preferences.desired_roles.iter().map(String::as_str).collect());
    let role_text_matches = target_roles.iter().any(|role| {
        let role = role.to_lowercase();
        !role.trim().is_empty() && (title.contains(&role) || role.contains(&title))
    });
    let target_family = canonical_role_family(track, posting);
    let posting_family = posting_role_family(posting);
    if role_text_matches {
        score += 25;
        reasons.push("Role matches this Career Track".to_string());
    } else if target_family == posting_family && target_family != ROLE_FAMILY_GENERIC {
        score += 15;
        reasons.push("Role family matches this Career Track".to_string());
    } else if target_family != ROLE_FAMILY_GENERIC && posting_family != ROLE_FAMILY_GENERIC {
        score -= 20;
        missing.push("Role belongs to a different Career Track".to_string());
    }

    let matching_skills: Vec<String> = profile
        .skills
        .iter()
        .filter(|skill| description.contains(&skill.to_lowercase()))
        .take(5)
        .cloned()
        .collect();
    if !matching_skills.is_empty() {
        score += (matching_skills.len() as i64 * 5).min(25);
        reasons.push(format!("Matches {} profile skills", matching_skills.len()));
    } else if !profile.skills.is_empty() && !description.is_empty() {
        missing.push("No direct skill overlap found yet".to_string());
    }

    if preferences.desired_locations.iter().any(|candidate| {
        let candidate = candidate.to_lowercase();
        location.contains(&candidate) || candidate.contains(&location)
    }) || (preferences.remote_preference.contains("remote")
        && (posting.workplace.eq_ignore_ascii_case("remote") || location.contains("remote")))
    {
        score += 15;
        reasons.push("Location preference fits".to_string());
    }

    let evidence = role_experience_evidence(profile, track, posting);
    let requirement = experience_requirement(posting);
    let required_minimum = requirement
        .required_min_months
        .into_iter()
        .chain(requirement.title_floor_months)
        .max();
    let required_maximum = requirement.required_max_months;
    let required_aligned = required_minimum
        .is_none_or(|months| months <= evidence.target_max_months)
        && required_maximum.is_none_or(|months| months >= evidence.target_min_months);
    if required_minimum.is_some() || required_maximum.is_some() {
        if required_aligned {
            score += 15;
            reasons.push(format!(
                "Required experience fits this Track's {}-{} year range",
                evidence.target_min_months / 12,
                (evidence.target_max_months + 11) / 12,
            ));
        } else {
            score -= 25;
            missing.push(format!(
                "Required experience is outside this Track's {}-{} year range",
                evidence.target_min_months / 12,
                (evidence.target_max_months + 11) / 12,
            ));
        }
    }
    if requirement
        .preferred_min_months
        .is_some_and(|months| months <= evidence.target_max_months)
    {
        score += 5;
        reasons.push("Preferred experience also fits".to_string());
    }

    if posting.compensation.is_empty() || preferences.minimum_compensation.is_none() {
        reasons.push("Compensation needs confirmation".to_string());
    }

    (score.clamp(0, 99), reasons, missing)
}

fn candidate_truth_fingerprint(profile: &CareerProfile) -> String {
    // Fingerprint the exact candidate snapshot used to build the packet. The
    // login/application email is intentionally excluded because the verified
    // application identity is fenced separately, and `updated_at_ms` is not a
    // candidate fact. Everything else can affect resume prose, form answers,
    // eligibility, or user-visible contact data and must invalidate stale work.
    let mut snapshot = profile.clone();
    snapshot.email.clear();
    snapshot.updated_at_ms = 0;
    let encoded = serde_json::to_vec(&snapshot)
        .expect("CareerProfile contains no serialization-fallible values");
    hex::encode(Sha256::digest(encoded))
}

fn confirmed_facts_fingerprint(facts: &[CareerFact]) -> String {
    let mut confirmed = facts
        .iter()
        .filter(|fact| fact.verification_status == "confirmed")
        .cloned()
        .collect::<Vec<_>>();
    confirmed.sort_by(|left, right| left.id.cmp(&right.id));
    let encoded = serde_json::to_vec(&confirmed)
        .expect("CareerFact contains no serialization-fallible values");
    hex::encode(Sha256::digest(encoded))
}

fn selected_application_identity<'a>(
    track_identity_id: Option<&str>,
    identities: &'a [ApplicationIdentity],
) -> Option<&'a ApplicationIdentity> {
    track_identity_id
        .and_then(|identity_id| identities.iter().find(|item| item.id == identity_id))
        .or_else(|| identities.iter().find(|item| item.is_default))
        .filter(|item| item.verification_status == "verified")
}

fn posting_snapshot_fingerprint(posting: &JobPosting) -> Result<String> {
    let encoded = serde_json::to_vec(posting).context("encode Jobs posting snapshot")?;
    Ok(hex::encode(Sha256::digest(encoded)))
}

pub fn list_attempt_reservations(
    pool: &DbPool,
    account_id: &str,
) -> Result<Vec<AttemptReservation>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT id, application_id, company_key, period_key, runner, status,
                        reserved_at_ms, updated_at_ms
                   FROM jobs_attempt_reservations
                  WHERE account_id = ?1 ORDER BY reserved_at_ms DESC",
            )?;
            let rows = stmt
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
            Ok(rows)
        }
        DbPool::Postgres(_) => Ok(pool
            .get_pg()?
            .query(
                "SELECT id, application_id, company_key, period_key, runner, status,
                        reserved_at_ms, updated_at_ms
                   FROM jobs_attempt_reservations
                  WHERE account_id = $1 ORDER BY reserved_at_ms DESC",
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
            .collect()),
    })
}

pub fn reserve_application_attempt(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    runner: &str,
) -> Result<AttemptReservation> {
    let application = get_application(pool, account_id, application_id)?
        .ok_or_else(|| anyhow::anyhow!("application not found"))?;
    let posting = get_posting(pool, account_id, &application.job_id)?
        .ok_or_else(|| anyhow::anyhow!("job not found"))?;
    let preferences = get_preferences(pool, account_id)?;
    let _ = get_entitlement(pool, account_id)?;
    let company_key = normalize_company_key(&posting.company);
    let period_key = attempt_period_key(now_ms(), preferences.time_zone_offset_minutes);
    let daily_limit = preferences.daily_limit.clamp(1, 50);
    let now = now_ms();
    let id = format!("attempt-{application_id}");

    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            if let Some(existing) = tx
                .query_row(
                    "SELECT id, application_id, company_key, period_key, runner, status,
                            reserved_at_ms, updated_at_ms
                       FROM jobs_attempt_reservations
                      WHERE account_id = ?1 AND application_id = ?2",
                    params![account_id, application_id],
                    |row| {
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
                    },
                )
                .optional()?
            {
                if active_attempt_status(&existing.status) {
                    tx.execute(
                        "UPDATE jobs_attempt_reservations SET runner = ?3, updated_at_ms = ?4
                          WHERE account_id = ?1 AND application_id = ?2",
                        params![account_id, application_id, runner, now],
                    )?;
                    tx.commit()?;
                    return Ok(AttemptReservation {
                        runner: runner.to_string(),
                        updated_at_ms: now,
                        ..existing
                    });
                }
            }
            let discovered: i64 = tx.query_row(
                "SELECT COUNT(*) FROM jobs_discovery_memberships
                  WHERE account_id = ?1 AND job_id = ?2",
                params![account_id, posting.id],
                |row| row.get(0),
            )?;
            if discovered > 0 {
                let healthy: i64 = tx.query_row(
                    "SELECT COUNT(*)
                       FROM jobs_discovery_memberships m
                       JOIN jobs_discovery_sources s ON s.id = m.source_id
                      WHERE m.account_id = ?1 AND m.job_id = ?2
                        AND m.availability_status = 'active'
                        AND m.last_seen_at_ms >= ?3
                        AND s.status = 'active' AND s.health = 'healthy'",
                    params![account_id, posting.id, now - LIVE_VERIFICATION_MAX_AGE_MS],
                    |row| row.get(0),
                )?;
                if healthy == 0 {
                    anyhow::bail!(
                        "the discovery source must be healthy before reserving this application"
                    )
                }
            }
            let used: i64 = tx.query_row(
                "SELECT COUNT(*) FROM jobs_attempt_reservations
                  WHERE account_id = ?1 AND period_key = ?2
                    AND status IN ('reserved', 'running', 'side_effect_unknown', 'submitted')",
                params![account_id, period_key],
                |row| row.get(0),
            )?;
            if used >= daily_limit {
                anyhow::bail!("today's application attempt limit has been reached")
            }
            let company_in_use: i64 = tx.query_row(
                "SELECT COUNT(*) FROM jobs_attempt_reservations
                  WHERE account_id = ?1 AND company_key = ?2 AND application_id <> ?3
                    AND status IN ('reserved', 'running', 'side_effect_unknown', 'submitted')",
                params![account_id, company_key, application_id],
                |row| row.get(0),
            )?;
            if company_in_use > 0 {
                anyhow::bail!(
                    "an in-progress or submitted application already exists for this company"
                )
            }
            tx.execute(
                "INSERT INTO jobs_attempt_reservations (
                    id, account_id, application_id, company_key, period_key, runner, status,
                    reserved_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'reserved', ?7, ?7)
                 ON CONFLICT(account_id, application_id) DO UPDATE SET
                    company_key = excluded.company_key, period_key = excluded.period_key,
                    runner = excluded.runner, status = 'reserved', reserved_at_ms = excluded.reserved_at_ms,
                    updated_at_ms = excluded.updated_at_ms",
                params![id, account_id, application_id, company_key, period_key, runner, now],
            )?;
            tx.commit()?;
            Ok(AttemptReservation {
                id,
                application_id: application_id.to_string(),
                company_key,
                period_key,
                runner: runner.to_string(),
                status: "reserved".to_string(),
                reserved_at_ms: now,
                updated_at_ms: now,
            })
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            tx.query_one(
                "SELECT account_id FROM jobs_entitlements WHERE account_id = $1 FOR UPDATE",
                &[&account_id],
            )?;
            if let Some(row) = tx.query_opt(
                "SELECT id, application_id, company_key, period_key, runner, status,
                        reserved_at_ms, updated_at_ms
                   FROM jobs_attempt_reservations
                  WHERE account_id = $1 AND application_id = $2",
                &[&account_id, &application_id],
            )? {
                let existing = AttemptReservation {
                    id: row.get(0),
                    application_id: row.get(1),
                    company_key: row.get(2),
                    period_key: row.get(3),
                    runner: row.get(4),
                    status: row.get(5),
                    reserved_at_ms: row.get(6),
                    updated_at_ms: row.get(7),
                };
                if active_attempt_status(&existing.status) {
                    tx.execute(
                        "UPDATE jobs_attempt_reservations SET runner = $3, updated_at_ms = $4
                          WHERE account_id = $1 AND application_id = $2",
                        &[&account_id, &application_id, &runner, &now],
                    )?;
                    tx.commit()?;
                    return Ok(AttemptReservation {
                        runner: runner.to_string(),
                        updated_at_ms: now,
                        ..existing
                    });
                }
            }
            let authorities = tx.query(
                "SELECT s.status, s.health, m.availability_status, m.last_seen_at_ms
                   FROM jobs_discovery_memberships m
                   JOIN jobs_discovery_sources s ON s.id = m.source_id
                  WHERE m.account_id = $1 AND m.job_id = $2
                  FOR UPDATE OF s, m",
                &[&account_id, &posting.id],
            )?;
            if !authorities.is_empty() {
                let healthy = authorities.iter().any(|row| {
                    row.get::<_, String>(0) == "active"
                        && row.get::<_, String>(1) == "healthy"
                        && row.get::<_, String>(2) == "active"
                        && row.get::<_, i64>(3) >= now - LIVE_VERIFICATION_MAX_AGE_MS
                });
                if !healthy {
                    anyhow::bail!(
                        "the discovery source must be healthy before reserving this application"
                    )
                }
            }
            let used: i64 = tx
                .query_one(
                    "SELECT COUNT(*) FROM jobs_attempt_reservations
                      WHERE account_id = $1 AND period_key = $2
                        AND status IN ('reserved', 'running', 'side_effect_unknown', 'submitted')",
                    &[&account_id, &period_key],
                )?
                .get(0);
            if used >= daily_limit {
                anyhow::bail!("today's application attempt limit has been reached")
            }
            let company_in_use: i64 = tx
                .query_one(
                    "SELECT COUNT(*) FROM jobs_attempt_reservations
                      WHERE account_id = $1 AND company_key = $2 AND application_id <> $3
                        AND status IN ('reserved', 'running', 'side_effect_unknown', 'submitted')",
                    &[&account_id, &company_key, &application_id],
                )?
                .get(0);
            if company_in_use > 0 {
                anyhow::bail!(
                    "an in-progress or submitted application already exists for this company"
                )
            }
            tx.execute(
                "INSERT INTO jobs_attempt_reservations (
                    id, account_id, application_id, company_key, period_key, runner, status,
                    reserved_at_ms, updated_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, 'reserved', $7, $7)
                 ON CONFLICT(account_id, application_id) DO UPDATE SET
                    company_key = EXCLUDED.company_key, period_key = EXCLUDED.period_key,
                    runner = EXCLUDED.runner, status = 'reserved', reserved_at_ms = EXCLUDED.reserved_at_ms,
                    updated_at_ms = EXCLUDED.updated_at_ms",
                &[&id, &account_id, &application_id, &company_key, &period_key, &runner, &now],
            )?;
            tx.commit()?;
            Ok(AttemptReservation {
                id,
                application_id: application_id.to_string(),
                company_key,
                period_key,
                runner: runner.to_string(),
                status: "reserved".to_string(),
                reserved_at_ms: now,
                updated_at_ms: now,
            })
        }
    })
}

pub fn update_attempt_reservation_status(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    status: &str,
) -> Result<bool> {
    if !matches!(
        status,
        "reserved" | "running" | "released" | "side_effect_unknown" | "submitted"
    ) {
        anyhow::bail!("invalid application attempt status")
    }
    let now = now_ms();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            if status == "running" {
                let job_id: String = tx.query_row(
                    "SELECT job_id FROM jobs_applications
                      WHERE account_id = ?1 AND id = ?2",
                    params![account_id, application_id],
                    |row| row.get(0),
                )?;
                ensure_discovery_authority_in_sqlite_tx(&tx, account_id, &job_id, now)?;
            }
            let changed = tx.execute(
                "UPDATE jobs_attempt_reservations SET status = ?3, updated_at_ms = ?4
                  WHERE account_id = ?1 AND application_id = ?2",
                params![account_id, application_id, status, now],
            )? > 0;
            tx.commit()?;
            Ok(changed)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            if status == "running" {
                let job_id: String = tx
                    .query_one(
                        "SELECT job_id FROM jobs_applications
                          WHERE account_id = $1 AND id = $2 FOR UPDATE",
                        &[&account_id, &application_id],
                    )?
                    .get(0);
                ensure_discovery_authority_in_pg_tx(&mut tx, account_id, &job_id, now)?;
            }
            let changed = tx.execute(
                "UPDATE jobs_attempt_reservations SET status = $3, updated_at_ms = $4
                  WHERE account_id = $1 AND application_id = $2",
                &[&account_id, &application_id, &status, &now],
            )? > 0;
            tx.commit()?;
            Ok(changed)
        }
    })
}

fn ensure_discovery_authority_in_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    job_id: &str,
    now: i64,
) -> Result<()> {
    let discovered: i64 = tx.query_row(
        "SELECT COUNT(*) FROM jobs_discovery_memberships
          WHERE account_id = ?1 AND job_id = ?2",
        params![account_id, job_id],
        |row| row.get(0),
    )?;
    if discovered == 0 {
        return Ok(());
    }
    let healthy: i64 = tx.query_row(
        "SELECT COUNT(*)
           FROM jobs_discovery_memberships m
           JOIN jobs_discovery_sources s ON s.id = m.source_id
          WHERE m.account_id = ?1 AND m.job_id = ?2
            AND m.availability_status = 'active'
            AND m.last_seen_at_ms >= ?3
            AND s.status = 'active' AND s.health = 'healthy'",
        params![account_id, job_id, now - LIVE_VERIFICATION_MAX_AGE_MS],
        |row| row.get(0),
    )?;
    if healthy == 0 {
        anyhow::bail!("the discovery source must be healthy before the runner starts")
    }
    Ok(())
}

fn ensure_discovery_authority_in_pg_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    job_id: &str,
    now: i64,
) -> Result<()> {
    let authorities = tx.query(
        "SELECT s.status, s.health, m.availability_status, m.last_seen_at_ms
           FROM jobs_discovery_memberships m
           JOIN jobs_discovery_sources s ON s.id = m.source_id
          WHERE m.account_id = $1 AND m.job_id = $2
          FOR UPDATE OF s, m",
        &[&account_id, &job_id],
    )?;
    if authorities.is_empty() {
        return Ok(());
    }
    let healthy = authorities.iter().any(|row| {
        row.get::<_, String>(0) == "active"
            && row.get::<_, String>(1) == "healthy"
            && row.get::<_, String>(2) == "active"
            && row.get::<_, i64>(3) >= now - LIVE_VERIFICATION_MAX_AGE_MS
    });
    if !healthy {
        anyhow::bail!("the discovery source must be healthy before the runner starts")
    }
    Ok(())
}

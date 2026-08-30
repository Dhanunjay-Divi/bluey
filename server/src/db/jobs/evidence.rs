const JOBS_EVIDENCE_SCHEMA_VERSION: i64 = 1;

fn build_profile_evidence_revision(
    account_id: &str,
    profile: &CareerProfile,
    facts: &[CareerFact],
    track: &CareerTrack,
    identity: &ApplicationIdentity,
    experience: &RoleExperienceEvidence,
) -> Result<ProfileEvidenceRevision> {
    // Keep evidence stable across a no-op profile save. The application email
    // is fenced by the verified ApplicationIdentity below, and the persistence
    // timestamp is not a candidate fact.
    let mut candidate_profile = profile.clone();
    candidate_profile.email.clear();
    candidate_profile.updated_at_ms = 0;
    // Track timestamps and match counts are transport/derived projection data.
    // They can advance during a semantic no-op read/write and must not revoke a
    // prepared application's immutable candidate-evidence binding.
    let mut candidate_track = track.clone();
    candidate_track.match_count = 0;
    candidate_track.created_at_ms = 0;
    candidate_track.updated_at_ms = 0;
    let mut confirmed_facts = facts
        .iter()
        .filter(|fact| fact.verification_status == "confirmed")
        .cloned()
        .collect::<Vec<_>>();
    confirmed_facts.sort_by(|left, right| left.id.cmp(&right.id));

    let snapshot = json!({
        "schema_version": JOBS_EVIDENCE_SCHEMA_VERSION,
        "profile": candidate_profile,
        "confirmed_facts": confirmed_facts,
        "career_track": candidate_track,
        "application_identity": {
            "id": identity.id,
            "email": identity.email,
            "label": identity.label,
            "verification_status": identity.verification_status,
        },
        "role_experience": experience,
    });
    let encoded = serde_json::to_vec(&snapshot).context("encode Jobs evidence snapshot")?;
    let content_hash = hex::encode(Sha256::digest(encoded));
    let scoped = Sha256::digest(format!("{account_id}:{content_hash}").as_bytes());
    Ok(ProfileEvidenceRevision {
        id: format!("evidence-{}", &hex::encode(scoped)[..32]),
        career_track_id: track.id.clone(),
        revision_no: 0,
        content_hash,
        snapshot,
        created_at_ms: now_ms(),
    })
}

struct ResumeClaimEvidenceContext<'a> {
    account_id: &'a str,
    job_id: &'a str,
    resume_version_id: &'a str,
    content: &'a Value,
    profile: &'a CareerProfile,
    facts: &'a [CareerFact],
    track: &'a CareerTrack,
    identity: &'a ApplicationIdentity,
    evidence_revision: &'a ProfileEvidenceRevision,
}

fn build_resume_claim_evidence(
    context: ResumeClaimEvidenceContext<'_>,
) -> Result<Vec<ResumeClaimEvidence>> {
    let source_index = claim_source_index(context.profile, context.facts, context.identity);
    let rewrite_sources = resume_rewrite_source_map(context.content, context.profile)?;
    let mut raw_claims = Vec::new();
    collect_resume_claims(context.content, "", &mut raw_claims);
    let relevant_sources = context
        .track
        .policy
        .relevant_employment_ids
        .iter()
        .map(|id| format!("employment:{id}"))
        .collect::<Vec<_>>();
    let created_at_ms = now_ms();
    let mut claims = raw_claims
        .into_iter()
        .filter_map(|(path, text)| {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return None;
            }
            let normalized = normalize_candidate_text(trimmed);
            let mut source_ids = source_index.get(&normalized).cloned().unwrap_or_default();
            if let Some(explicit_sources) = rewrite_sources.get(&path) {
                source_ids.clone_from(explicit_sources);
            }
            if path == "/headline" || path == "/summary" {
                source_ids.extend(relevant_sources.iter().cloned());
            }
            if source_ids.is_empty() {
                source_ids.push(format!(
                    "evidence_revision:{}",
                    context.evidence_revision.id
                ));
            }
            source_ids.sort();
            source_ids.dedup();
            let digest = Sha256::digest(
                format!(
                    "{}|{}|{}|{}",
                    context.evidence_revision.id, context.job_id, path, trimmed
                )
                .as_bytes(),
            );
            let claim_id = format!("claim-{}", &hex::encode(digest)[..32]);
            let row_digest = Sha256::digest(
                format!(
                    "{}|{}|{claim_id}",
                    context.account_id, context.resume_version_id
                )
                .as_bytes(),
            );
            Some(ResumeClaimEvidence {
                id: format!("claim-evidence-{}", &hex::encode(row_digest)[..32]),
                resume_version_id: context.resume_version_id.to_string(),
                claim_id,
                evidence_revision_id: context.evidence_revision.id.clone(),
                source_ids,
                claim: json!({ "path": path, "text": trimmed }),
                created_at_ms,
            })
        })
        .collect::<Vec<_>>();
    claims.sort_by(|left, right| left.claim_id.cmp(&right.claim_id));
    claims.dedup_by(|left, right| left.claim_id == right.claim_id);
    if claims.is_empty() {
        anyhow::bail!("prepared resume has no evidence-backed claims")
    }
    Ok(claims)
}

fn resume_rewrite_source_map(
    content: &Value,
    profile: &CareerProfile,
) -> Result<BTreeMap<String, Vec<String>>> {
    let Some(value) = content.pointer("/provenance/resume_generation/rewrite_sources") else {
        return Ok(BTreeMap::new());
    };
    if value.is_null() {
        return Ok(BTreeMap::new());
    }
    let values = value
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("resume rewrite sources must be an object"))?;
    let mut result = BTreeMap::new();
    for (path, encoded_sources) in values {
        let (output_entry_index, _) = parse_resume_highlight_path(path)
            .ok_or_else(|| anyhow::anyhow!("resume rewrite source path is not an employment highlight"))?;
        let output_entry = content
            .pointer(&format!("/employment/{output_entry_index}"))
            .ok_or_else(|| anyhow::anyhow!("resume rewrite source path has no employment entry"))?;
        if content.pointer(path).and_then(Value::as_str).is_none() {
            anyhow::bail!("resume rewrite source path does not reference a text claim")
        }
        let sources = encoded_sources
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("resume rewrite source IDs must be an array"))?;
        if sources.is_empty() || sources.len() > 4 {
            anyhow::bail!("resume rewrite must cite between 1 and 4 source bullets")
        }
        let mut parsed_entry = None;
        let mut normalized_sources = Vec::with_capacity(sources.len());
        for source in sources {
            let source = source
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("resume rewrite source ID must be text"))?;
            let Some((entry_index, highlight_index)) = parse_resume_highlight_source_id(source)
            else {
                anyhow::bail!("resume rewrite source is not an employment highlight")
            };
            if parsed_entry.is_some_and(|existing| existing != entry_index) {
                anyhow::bail!("resume rewrite cannot combine evidence from different roles")
            }
            if profile
                .employment
                .get(entry_index)
                .and_then(|entry| entry.highlights.get(highlight_index))
                .is_none()
            {
                anyhow::bail!("resume rewrite source is outside the evidence revision")
            }
            parsed_entry = Some(entry_index);
            if normalized_sources.iter().any(|existing| existing == source) {
                anyhow::bail!("resume rewrite contains a duplicate source ID")
            }
            normalized_sources.push(source.to_string());
        }
        let source_entry_index = parsed_entry
            .ok_or_else(|| anyhow::anyhow!("resume rewrite has no source employment entry"))?;
        let source_entry = &profile.employment[source_entry_index];
        if !same_employment_entry(output_entry, source_entry) {
            anyhow::bail!("resume rewrite sources do not belong to the rendered employment entry")
        }
        result.insert(path.clone(), normalized_sources);
    }
    Ok(result)
}

fn parse_resume_highlight_path(path: &str) -> Option<(usize, usize)> {
    let parts = path.split('/').collect::<Vec<_>>();
    if parts.len() != 5
        || !parts[0].is_empty()
        || parts[1] != "employment"
        || parts[3] != "highlights"
    {
        return None;
    }
    Some((parts[2].parse().ok()?, parts[4].parse().ok()?))
}

fn same_employment_entry(rendered: &Value, source: &EmploymentEntry) -> bool {
    if !source.id.trim().is_empty() {
        return rendered.get("id").and_then(Value::as_str) == Some(source.id.as_str());
    }
    [
        ("company", source.company.as_str()),
        ("title", source.title.as_str()),
        ("start_date", source.start_date.as_str()),
        ("end_date", source.end_date.as_str()),
    ]
    .into_iter()
    .all(|(field, expected)| rendered.get(field).and_then(Value::as_str) == Some(expected))
}

fn parse_resume_highlight_source_id(id: &str) -> Option<(usize, usize)> {
    let parts = id.split(':').collect::<Vec<_>>();
    if parts.len() != 4 || parts[0] != "employment" || parts[2] != "highlight" {
        return None;
    }
    Some((parts[1].parse().ok()?, parts[3].parse().ok()?))
}

fn collect_resume_claims(value: &Value, path: &str, claims: &mut Vec<(String, String)>) {
    match value {
        Value::String(text)
            if !path.starts_with("/target")
                && !path.starts_with("/provenance")
                && path != "/source_resume_name" =>
        {
            claims.push((path.to_string(), text.clone()));
        }
        Value::String(_) => {}
        Value::Array(values) => {
            for (index, item) in values.iter().enumerate() {
                collect_resume_claims(item, &format!("{path}/{index}"), claims);
            }
        }
        Value::Object(values) => {
            for (key, item) in values {
                collect_resume_claims(item, &format!("{path}/{key}"), claims);
            }
        }
        _ => {}
    }
}

fn claim_source_index(
    profile: &CareerProfile,
    facts: &[CareerFact],
    identity: &ApplicationIdentity,
) -> BTreeMap<String, Vec<String>> {
    let mut index: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut add = |text: &str, source: String| {
        let key = normalize_candidate_text(text);
        if !key.is_empty() {
            index.entry(key).or_default().push(source);
        }
    };

    add(&profile.full_name, "profile:full_name".to_string());
    add(&identity.email, format!("identity:{}", identity.id));
    add(&profile.phone, "profile:phone".to_string());
    add(&profile.current_location, "profile:current_location".to_string());
    add(&profile.linkedin_url, "profile:linkedin_url".to_string());
    add(&profile.portfolio_url, "profile:portfolio_url".to_string());
    add(&profile.headline, "profile:headline".to_string());
    add(&profile.summary, "profile:summary".to_string());
    for skill in &profile.skills {
        add(skill, format!("profile:skill:{}", normalize_candidate_text(skill)));
    }
    for certification in &profile.certifications {
        add(
            certification,
            format!(
                "profile:certification:{}",
                normalize_candidate_text(certification)
            ),
        );
    }
    for (entry_index, employment) in profile.employment.iter().enumerate() {
        let source = format!(
            "employment:{}",
            stable_profile_entry_id(&employment.id, &employment.company, &employment.title)
        );
        add(&employment.company, source.clone());
        add(&employment.title, source.clone());
        add(&employment.location, source.clone());
        add(&employment.start_date, source.clone());
        add(&employment.end_date, source.clone());
        for (highlight_index, highlight) in employment.highlights.iter().enumerate() {
            add(highlight, source.clone());
            add(
                highlight,
                format!("employment:{entry_index}:highlight:{highlight_index}"),
            );
        }
    }
    for education in &profile.education {
        let source = format!("education:{}", stable_profile_entry_id(&education.id, &education.school, &education.degree));
        add(&education.school, source.clone());
        add(&education.degree, source.clone());
        add(&education.field, source.clone());
        add(&education.location, source.clone());
        add(&education.start_date, source.clone());
        add(&education.end_date, source.clone());
    }
    for project in &profile.projects {
        let source = format!("project:{}", stable_profile_entry_id(&project.id, &project.name, &project.role));
        add(&project.name, source.clone());
        add(&project.role, source.clone());
        add(&project.summary, source.clone());
        for technology in &project.technologies {
            add(technology, source.clone());
        }
        add(&project.url, source);
    }
    for fact in facts
        .iter()
        .filter(|fact| fact.verification_status == "confirmed")
    {
        add(&fact.label, format!("fact:{}", fact.id));
        if let Some(value) = fact.value.as_str() {
            add(value, format!("fact:{}", fact.id));
        }
    }
    for sources in index.values_mut() {
        sources.sort();
        sources.dedup();
    }
    index
}

fn stable_profile_entry_id(id: &str, primary: &str, secondary: &str) -> String {
    if !id.trim().is_empty() {
        return id.to_string();
    }
    let digest = Sha256::digest(
        format!(
            "{}|{}",
            normalize_candidate_text(primary),
            normalize_candidate_text(secondary)
        )
        .as_bytes(),
    );
    format!("legacy-{}", &hex::encode(digest)[..16])
}

fn normalize_candidate_text(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_ascii_lowercase()
}

fn persist_evidence_revision_sqlite(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    evidence: &ProfileEvidenceRevision,
) -> Result<ProfileEvidenceRevision> {
    if let Some((id, revision_no, snapshot_json, created_at_ms)) = tx
        .query_row(
            "SELECT id, revision_no, snapshot_json, created_at_ms
               FROM jobs_profile_evidence_revisions
              WHERE account_id = ?1 AND career_track_id = ?2 AND content_hash = ?3",
            params![account_id, evidence.career_track_id, evidence.content_hash],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?, row.get::<_, i64>(3)?)),
        )
        .optional()?
    {
        return Ok(ProfileEvidenceRevision {
            id,
            career_track_id: evidence.career_track_id.clone(),
            revision_no,
            content_hash: evidence.content_hash.clone(),
            snapshot: parse_json(snapshot_json, "Jobs evidence revision")?,
            created_at_ms,
        });
    }
    let revision_no = tx.query_row(
        "SELECT COALESCE(MAX(revision_no), 0) + 1
           FROM jobs_profile_evidence_revisions
          WHERE account_id = ?1 AND career_track_id = ?2",
        params![account_id, evidence.career_track_id],
        |row| row.get::<_, i64>(0),
    )?;
    tx.execute(
        "INSERT INTO jobs_profile_evidence_revisions (
            id, account_id, career_track_id, revision_no, content_hash,
            snapshot_json, created_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            evidence.id,
            account_id,
            evidence.career_track_id,
            revision_no,
            evidence.content_hash,
            to_json(&evidence.snapshot, "Jobs evidence revision")?,
            evidence.created_at_ms,
        ],
    )?;
    let mut stored = evidence.clone();
    stored.revision_no = revision_no;
    Ok(stored)
}

fn lock_profile_evidence_revision_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    career_track_id: &str,
) -> Result<()> {
    tx.query_one(
        "SELECT pg_advisory_xact_lock(hashtextextended('jobs-evidence:' || $1 || ':' || $2, 0))",
        &[&account_id, &career_track_id],
    )?;
    Ok(())
}

fn persist_evidence_revision_postgres(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    evidence: &ProfileEvidenceRevision,
) -> Result<ProfileEvidenceRevision> {
    // Prepared finalization prelocks this exact namespace before evaluating any
    // expiring authority. Reacquiring the transaction lock is immediate and
    // proves that persistence cannot silently drift to a different namespace.
    lock_profile_evidence_revision_postgres_tx(tx, account_id, &evidence.career_track_id)?;
    if let Some(row) = tx.query_opt(
        "SELECT id, revision_no, snapshot_json, created_at_ms
           FROM jobs_profile_evidence_revisions
          WHERE account_id = $1 AND career_track_id = $2 AND content_hash = $3",
        &[&account_id, &evidence.career_track_id, &evidence.content_hash],
    )? {
        return Ok(ProfileEvidenceRevision {
            id: row.get(0),
            career_track_id: evidence.career_track_id.clone(),
            revision_no: row.get(1),
            content_hash: evidence.content_hash.clone(),
            snapshot: parse_json(row.get(2), "Jobs evidence revision")?,
            created_at_ms: row.get(3),
        });
    }
    let revision_no: i64 = tx
        .query_one(
            "SELECT COALESCE(MAX(revision_no), 0) + 1
               FROM jobs_profile_evidence_revisions
              WHERE account_id = $1 AND career_track_id = $2",
            &[&account_id, &evidence.career_track_id],
        )?
        .get(0);
    tx.execute(
        "INSERT INTO jobs_profile_evidence_revisions (
            id, account_id, career_track_id, revision_no, content_hash,
            snapshot_json, created_at_ms
         ) VALUES ($1, $2, $3, $4, $5, $6, $7)",
        &[
            &evidence.id,
            &account_id,
            &evidence.career_track_id,
            &revision_no,
            &evidence.content_hash,
            &to_json(&evidence.snapshot, "Jobs evidence revision")?,
            &evidence.created_at_ms,
        ],
    )?;
    let mut stored = evidence.clone();
    stored.revision_no = revision_no;
    Ok(stored)
}

fn persist_claim_evidence_sqlite(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    resume_version_id: &str,
    claims: &[ResumeClaimEvidence],
) -> Result<()> {
    for claim in claims {
        let row_digest = Sha256::digest(
            format!("{account_id}|{resume_version_id}|{}", claim.claim_id).as_bytes(),
        );
        tx.execute(
            "INSERT INTO jobs_resume_claim_evidence (
                id, account_id, resume_version_id, claim_id, evidence_revision_id,
                source_ids_json, claim_json, created_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(account_id, resume_version_id, claim_id) DO NOTHING",
            params![
                format!("claim-evidence-{}", &hex::encode(row_digest)[..32]),
                account_id,
                resume_version_id,
                claim.claim_id,
                claim.evidence_revision_id,
                to_json(&claim.source_ids, "Jobs claim sources")?,
                to_json(&claim.claim, "Jobs claim evidence")?,
                claim.created_at_ms,
            ],
        )?;
    }
    Ok(())
}

fn persist_claim_evidence_postgres(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    resume_version_id: &str,
    claims: &[ResumeClaimEvidence],
) -> Result<()> {
    for claim in claims {
        let row_digest = Sha256::digest(
            format!("{account_id}|{resume_version_id}|{}", claim.claim_id).as_bytes(),
        );
        tx.execute(
            "INSERT INTO jobs_resume_claim_evidence (
                id, account_id, resume_version_id, claim_id, evidence_revision_id,
                source_ids_json, claim_json, created_at_ms
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT(account_id, resume_version_id, claim_id) DO NOTHING",
            &[
                &format!("claim-evidence-{}", &hex::encode(row_digest)[..32]),
                &account_id,
                &resume_version_id,
                &claim.claim_id,
                &claim.evidence_revision_id,
                &to_json(&claim.source_ids, "Jobs claim sources")?,
                &to_json(&claim.claim, "Jobs claim evidence")?,
                &claim.created_at_ms,
            ],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod evidence_tests {
    use super::*;

    fn evidence_profile() -> CareerProfile {
        CareerProfile {
            full_name: "Example Candidate".into(),
            headline: "Software Engineer".into(),
            employment: vec![
                EmploymentEntry {
                    id: "work-1".into(),
                    company: "Example Health".into(),
                    title: "Software Engineer".into(),
                    highlights: vec!["Built reliable distributed systems.".into()],
                    ..Default::default()
                },
                EmploymentEntry {
                    id: "work-2".into(),
                    company: "Other Company".into(),
                    title: "Platform Engineer".into(),
                    highlights: vec!["Managed a migration.".into()],
                    ..Default::default()
                },
            ],
            ..Default::default()
        }
    }

    fn resume_validation_fixture() -> (CareerProfile, ApplicationIdentity, JobPosting, Value) {
        let profile = CareerProfile {
            full_name: "Example Candidate".into(),
            phone: "+1 555 0100".into(),
            current_location: "Arlington, VA".into(),
            linkedin_url: "https://www.linkedin.com/in/example-candidate".into(),
            portfolio_url: "https://example.test".into(),
            headline: "Software Engineer".into(),
            summary: "Builds reliable distributed systems.".into(),
            skills: vec!["Rust".into(), "PostgreSQL".into()],
            employment: vec![EmploymentEntry {
                id: "work-1".into(),
                company: "Example Health".into(),
                title: "Software Engineer".into(),
                location: "Arlington, VA".into(),
                start_date: "2022-01".into(),
                end_date: "2024-06".into(),
                current: false,
                highlights: vec![
                    "Built reliable distributed systems.".into(),
                    "Improved database reliability with PostgreSQL.".into(),
                ],
            }],
            education: vec![EducationEntry {
                id: "education-1".into(),
                school: "Example University".into(),
                degree: "BS".into(),
                field: "Computer Science".into(),
                location: "Example, VA".into(),
                start_date: "2018-08".into(),
                end_date: "2022-05".into(),
            }],
            projects: vec![ProjectEntry {
                id: "project-1".into(),
                name: "Reliability Toolkit".into(),
                role: "Maintainer".into(),
                summary: "Built operational tooling.".into(),
                technologies: vec!["Rust".into()],
                url: "https://example.test/toolkit".into(),
            }],
            certifications: vec!["AWS Certified Developer".into()],
            source_resume_name: "candidate-resume.docx".into(),
            ..Default::default()
        };
        let identity = ApplicationIdentity {
            id: "identity-1".into(),
            email: "candidate@example.test".into(),
            label: "Primary".into(),
            verification_status: "verified".into(),
            is_default: true,
            created_at_ms: 1,
            updated_at_ms: 1,
        };
        let posting = JobPosting {
            id: "job-1".into(),
            canonical_key: "target-company:backend-engineer:remote".into(),
            source: "greenhouse".into(),
            external_id: "target-company-job-1".into(),
            company: "Target Company".into(),
            title: "Backend Engineer".into(),
            location: "Remote".into(),
            workplace: "remote".into(),
            canonical_url: "https://boards.greenhouse.io/target/jobs/1".into(),
            description: "Build reliable backend systems with Rust and PostgreSQL.".into(),
            compensation: "$150k-$180k".into(),
            employment_type: "full_time".into(),
            track_id: "track-1".into(),
            match_score: 90,
            matched_reasons: vec!["Verified skill fit".into()],
            missing_requirements: Vec::new(),
            posted_at_ms: Some(1),
            last_verified_at_ms: Some(1),
            availability_status: "active".into(),
            status: "matched".into(),
            created_at_ms: 1,
            updated_at_ms: 1,
            discovery_evidence: JobDiscoveryEvidence::default(),
            eligibility: None,
        };
        let baseline = json!({
            "target": {
                "job_id": posting.id,
                "company": posting.company,
                "title": posting.title,
                "location": posting.location,
            },
            "contact": {
                "name": profile.full_name,
                "email": identity.email,
                "phone": profile.phone,
                "location": profile.current_location,
                "linkedin_url": profile.linkedin_url,
                "portfolio_url": profile.portfolio_url,
            },
            "headline": profile.headline,
            "summary": profile.summary,
            "skills": profile.skills,
            "employment": profile.employment,
            "education": profile.education,
            "projects": profile.projects,
            "certifications": profile.certifications,
            "source_resume_name": profile.source_resume_name,
            "provenance": {},
        });
        (profile, identity, posting, baseline)
    }

    #[test]
    fn rewritten_claim_persists_its_exact_source_bullet() {
        let profile = evidence_profile();
        let content = json!({
            "headline": "Software Engineer",
            "employment": [{
                "id": "work-1",
                "company": "Example Health",
                "title": "Software Engineer",
                "start_date": "",
                "end_date": "",
                "highlights": ["Engineered reliable distributed systems."]
            }],
            "provenance": {
                "resume_generation": {
                    "rewrite_sources": {
                        "/employment/0/highlights/0": ["employment:0:highlight:0"]
                    }
                }
            }
        });
        let track = CareerTrack {
            id: "track-1".into(),
            name: "Software Engineering".into(),
            role: "Software Engineer".into(),
            locations: Vec::new(),
            remote_preference: String::new(),
            application_identity_id: Some("identity-1".into()),
            policy: CareerTrackPolicy {
                relevant_employment_ids: vec!["work-1".into()],
                ..Default::default()
            },
            active: true,
            match_count: 0,
            created_at_ms: 1,
            updated_at_ms: 1,
        };
        let identity = ApplicationIdentity {
            id: "identity-1".into(),
            email: "candidate@example.test".into(),
            label: "Primary".into(),
            verification_status: "verified".into(),
            is_default: true,
            created_at_ms: 1,
            updated_at_ms: 1,
        };
        let revision = ProfileEvidenceRevision {
            id: "evidence-1".into(),
            career_track_id: track.id.clone(),
            revision_no: 1,
            content_hash: "hash".into(),
            snapshot: json!({}),
            created_at_ms: 1,
        };
        let claims = build_resume_claim_evidence(ResumeClaimEvidenceContext {
            account_id: "account-1",
            job_id: "job-1",
            resume_version_id: "resume-1",
            content: &content,
            profile: &profile,
            facts: &[],
            track: &track,
            identity: &identity,
            evidence_revision: &revision,
        })
        .unwrap();
        let rewritten = claims
            .iter()
            .find(|claim| {
                claim.claim["path"] == "/employment/0/highlights/0"
                    && claim.claim["text"] == "Engineered reliable distributed systems."
            })
            .unwrap();
        assert_eq!(
            rewritten.source_ids,
            vec!["employment:0:highlight:0".to_string()]
        );
    }

    #[test]
    fn rewrite_source_map_rejects_cross_role_evidence() {
        let profile = evidence_profile();
        let content = json!({
            "employment": [{"highlights": ["Combined claim"]}],
            "provenance": {
                "resume_generation": {
                    "rewrite_sources": {
                        "/employment/0/highlights/0": [
                            "employment:0:highlight:0",
                            "employment:1:highlight:0"
                        ]
                    }
                }
            }
        });
        assert!(resume_rewrite_source_map(&content, &profile)
            .unwrap_err()
            .to_string()
            .contains("different roles"));
    }

    #[test]
    fn rewrite_source_map_rejects_sources_attached_to_the_wrong_rendered_role() {
        let profile = evidence_profile();
        let content = json!({
            "employment": [{
                "id": "work-2",
                "company": "Other Company",
                "title": "Platform Engineer",
                "start_date": "",
                "end_date": "",
                "highlights": ["Engineered reliable distributed systems."]
            }],
            "provenance": {
                "resume_generation": {
                    "rewrite_sources": {
                        "/employment/0/highlights/0": ["employment:0:highlight:0"]
                    }
                }
            }
        });
        assert!(resume_rewrite_source_map(&content, &profile)
            .unwrap_err()
            .to_string()
            .contains("rendered employment entry"));
    }

    #[test]
    fn prepared_resume_validator_accepts_a_same_role_evidence_backed_rewrite() {
        let (profile, identity, posting, baseline) = resume_validation_fixture();
        let mut generated = baseline.clone();
        generated["employment"][0]["highlights"][0] =
            json!("Engineered reliable distributed systems for production workloads.");
        generated["provenance"]["resume_generation"] = json!({
            "rewrite_sources": {
                "/employment/0/highlights/0": ["employment:0:highlight:0"]
            }
        });

        validate_prepared_resume_content(
            &generated,
            &baseline,
            &profile,
            &identity,
            &posting,
        )
        .unwrap();
    }

    #[test]
    fn prepared_resume_validator_rejects_forged_identity_and_candidate_facts() {
        let (profile, identity, posting, baseline) = resume_validation_fixture();
        for (pointer, replacement, expected_error) in [
            (
                "/contact/name",
                json!("Different Candidate"),
                "candidate name",
            ),
            (
                "/employment/0/company",
                json!("Fabricated Employer"),
                "employment company",
            ),
            (
                "/employment/0/title",
                json!("Principal Engineer"),
                "employment title",
            ),
            (
                "/employment/0/start_date",
                json!("2019-01"),
                "employment start_date",
            ),
        ] {
            let mut generated = baseline.clone();
            *generated.pointer_mut(pointer).unwrap() = replacement;
            let error = validate_prepared_resume_content(
                &generated,
                &baseline,
                &profile,
                &identity,
                &posting,
            )
            .unwrap_err();
            assert!(error.to_string().contains(expected_error), "{error:#}");
        }
    }

    #[test]
    fn prepared_resume_validator_rejects_unsupported_skills_and_unproven_rewrites() {
        let (profile, identity, posting, baseline) = resume_validation_fixture();
        let mut unsupported_skill = baseline.clone();
        unsupported_skill["skills"] = json!(["Rust", "ImaginaryDB"]);
        assert!(validate_prepared_resume_content(
            &unsupported_skill,
            &baseline,
            &profile,
            &identity,
            &posting,
        )
        .unwrap_err()
        .to_string()
        .contains("unsupported skill"));

        let mut unproven_rewrite = baseline.clone();
        unproven_rewrite["employment"][0]["highlights"][0] =
            json!("Created a new unsupported achievement.");
        assert!(validate_prepared_resume_content(
            &unproven_rewrite,
            &baseline,
            &profile,
            &identity,
            &posting,
        )
        .unwrap_err()
        .to_string()
        .contains("without rewrite evidence"));
    }

    #[test]
    fn prepared_resume_validator_rejects_an_evidence_backed_invented_claim() {
        let (profile, identity, posting, baseline) = resume_validation_fixture();
        let mut generated = baseline.clone();
        generated["employment"][0]["highlights"][0] =
            json!("Engineered ImaginaryDB systems that reduced latency by 99%.");
        generated["provenance"]["resume_generation"] = json!({
            "rewrite_sources": {
                "/employment/0/highlights/0": ["employment:0:highlight:0"]
            }
        });

        let error = validate_prepared_resume_content(
            &generated,
            &baseline,
            &profile,
            &identity,
            &posting,
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("unsupported protected claims"),
            "{error:#}"
        );
    }

    #[test]
    fn prepared_resume_validator_rejects_unknown_nested_fields() {
        let (profile, identity, posting, baseline) = resume_validation_fixture();
        for (parent, field, value, expected_error) in [
            (
                "contact",
                "preferred_name",
                json!("Invented Alias"),
                "contact contains an unsupported field",
            ),
            (
                "target",
                "internal_score",
                json!(100),
                "target contains an unsupported field",
            ),
        ] {
            let mut generated = baseline.clone();
            generated[parent][field] = value;
            let error = validate_prepared_resume_content(
                &generated,
                &baseline,
                &profile,
                &identity,
                &posting,
            )
            .unwrap_err();
            assert!(error.to_string().contains(expected_error), "{error:#}");
        }

        let mut generated = baseline.clone();
        generated["employment"][0]["invented_metric"] = json!("99%");
        let error = validate_prepared_resume_content(
            &generated,
            &baseline,
            &profile,
            &identity,
            &posting,
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("employment record contains an unsupported field"),
            "{error:#}"
        );
    }
}

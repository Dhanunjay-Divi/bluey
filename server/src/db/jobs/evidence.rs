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
        "career_track": track,
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
    for employment in &profile.employment {
        let source = format!("employment:{}", stable_profile_entry_id(&employment.id, &employment.company, &employment.title));
        add(&employment.company, source.clone());
        add(&employment.title, source.clone());
        add(&employment.location, source.clone());
        add(&employment.start_date, source.clone());
        add(&employment.end_date, source.clone());
        for highlight in &employment.highlights {
            add(highlight, source.clone());
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

fn persist_evidence_revision_postgres(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    evidence: &ProfileEvidenceRevision,
) -> Result<ProfileEvidenceRevision> {
    tx.query_one(
        "SELECT pg_advisory_xact_lock(hashtextextended('jobs-evidence:' || $1 || ':' || $2, 0))",
        &[&account_id, &evidence.career_track_id],
    )?;
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

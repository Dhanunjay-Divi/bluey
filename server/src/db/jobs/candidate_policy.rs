const ROLE_FAMILY_SOFTWARE: &str = "software_engineering";
const ROLE_FAMILY_DATA: &str = "data_engineering";
const ROLE_FAMILY_ML: &str = "data_science_ml";
const ROLE_FAMILY_PRODUCT: &str = "product_management";
const ROLE_FAMILY_CLINICAL: &str = "clinical_research";
const ROLE_FAMILY_GENERIC: &str = "generic";

fn canonical_role_family(track: Option<&CareerTrack>, posting: &JobPosting) -> String {
    if let Some(configured) = track
        .map(|value| value.policy.role_family.trim())
        .filter(|value| !value.is_empty())
    {
        return normalize_role_family(configured);
    }
    let track_role = track.map(|value| value.role.as_str()).unwrap_or_default();
    infer_role_family(&format!("{track_role} {}", posting.title))
}

fn posting_role_family(posting: &JobPosting) -> String {
    infer_role_family(&posting.title)
}

fn normalize_role_family(value: &str) -> String {
    let normalized = candidate_normalize(value);
    match normalized.as_str() {
        "software engineering" | "software engineer" | "software developer" | "swe" | "sde" => {
            ROLE_FAMILY_SOFTWARE.to_string()
        }
        "data engineering" | "data engineer" | "de" => ROLE_FAMILY_DATA.to_string(),
        "data science" | "machine learning" | "machine learning engineering" | "ml" | "ai" => {
            ROLE_FAMILY_ML.to_string()
        }
        "product management" | "product manager" | "pm" => ROLE_FAMILY_PRODUCT.to_string(),
        "clinical research" | "clinical operations" => ROLE_FAMILY_CLINICAL.to_string(),
        "generic" | "other" => ROLE_FAMILY_GENERIC.to_string(),
        _ => infer_role_family(value),
    }
}

fn infer_role_family(value: &str) -> String {
    let text = format!(" {} ", candidate_normalize(value));
    if candidate_contains_any(
        &text,
        &[
            " data engineer ",
            " data platform ",
            " analytics engineer ",
            " etl ",
            " data warehouse ",
        ],
    ) {
        ROLE_FAMILY_DATA.to_string()
    } else if candidate_contains_any(
        &text,
        &[
            " machine learning ",
            " data scientist ",
            " applied scientist ",
            " artificial intelligence ",
            " ml engineer ",
        ],
    ) {
        ROLE_FAMILY_ML.to_string()
    } else if candidate_contains_any(
        &text,
        &[
            " product manager ",
            " product management ",
            " product owner ",
        ],
    ) {
        ROLE_FAMILY_PRODUCT.to_string()
    } else if candidate_contains_any(
        &text,
        &[
            " clinical research ",
            " clinical operations ",
            " clinical trial ",
            " research coordinator ",
        ],
    ) {
        ROLE_FAMILY_CLINICAL.to_string()
    } else if candidate_contains_any(
        &text,
        &[
            " software ",
            " developer ",
            " frontend ",
            " front end ",
            " backend ",
            " back end ",
            " full stack ",
            " fullstack ",
            " devops ",
            " site reliability ",
            " cloud engineer ",
            " application engineer ",
        ],
    ) {
        ROLE_FAMILY_SOFTWARE.to_string()
    } else {
        ROLE_FAMILY_GENERIC.to_string()
    }
}

fn role_experience_evidence(
    profile: &CareerProfile,
    track: Option<&CareerTrack>,
    posting: &JobPosting,
) -> RoleExperienceEvidence {
    let role_family = canonical_role_family(track, posting);
    let explicit_ids: BTreeSet<&str> = track
        .map(|value| {
            value
                .policy
                .relevant_employment_ids
                .iter()
                .map(String::as_str)
                .collect()
        })
        .unwrap_or_default();
    let use_explicit = !explicit_ids.is_empty();
    let relevant: Vec<&EmploymentEntry> = profile
        .employment
        .iter()
        .filter(|entry| {
            if use_explicit {
                explicit_ids.contains(entry.id.as_str())
            } else {
                employment_matches_role_family(entry, &role_family)
            }
        })
        .collect();
    let total_months = non_overlapping_employment_months(&relevant);
    RoleExperienceEvidence {
        role_family,
        relevant_employment_ids: relevant
            .iter()
            .filter(|entry| !entry.id.trim().is_empty())
            .map(|entry| entry.id.clone())
            .collect(),
        total_months,
        target_min_months: total_months.saturating_sub(12),
        target_max_months: total_months.saturating_add(24),
    }
}

fn employment_matches_role_family(entry: &EmploymentEntry, role_family: &str) -> bool {
    if role_family == ROLE_FAMILY_GENERIC {
        return true;
    }
    let family = infer_role_family(&format!("{} {}", entry.title, entry.highlights.join(" ")));
    family == role_family
}

fn non_overlapping_employment_months(entries: &[&EmploymentEntry]) -> i64 {
    let now = Utc::now();
    let current_month = i64::from(now.year()) * 12 + i64::from(now.month0());
    let mut intervals: Vec<(i64, i64)> = entries
        .iter()
        .filter_map(|entry| {
            let start = parse_candidate_year_month(&entry.start_date)?;
            let end = if entry.current || entry.end_date.trim().is_empty() {
                current_month.saturating_add(1)
            } else {
                parse_candidate_year_month(&entry.end_date)?.saturating_add(1)
            };
            (end > start).then_some((start, end))
        })
        .collect();
    if intervals.is_empty() {
        return 0;
    }
    intervals.sort_unstable_by_key(|interval| interval.0);
    let mut total = 0i64;
    let mut merged = intervals[0];
    for interval in intervals.into_iter().skip(1) {
        if interval.0 <= merged.1 {
            merged.1 = merged.1.max(interval.1);
        } else {
            total = total.saturating_add(merged.1 - merged.0);
            merged = interval;
        }
    }
    total.saturating_add(merged.1 - merged.0)
}

fn parse_candidate_year_month(value: &str) -> Option<i64> {
    let mut parts = value.trim().split('-');
    let year = parts.next()?.parse::<i64>().ok()?;
    if !(1900..=2200).contains(&year) {
        return None;
    }
    let month = parts
        .next()
        .and_then(|part| part.parse::<i64>().ok())
        .unwrap_or(1);
    (1..=12).contains(&month).then_some(year * 12 + month - 1)
}

fn experience_requirement(posting: &JobPosting) -> ExperienceRequirement {
    let text = format!("{}\n{}", posting.title, posting.description).to_ascii_lowercase();
    let mut required: Vec<(i64, Option<i64>)> = Vec::new();
    let mut preferred: Vec<(i64, Option<i64>)> = Vec::new();
    for clause in text.split(['\n', '.', ';']) {
        let Some(range) = clause_experience_range(clause) else {
            continue;
        };
        if candidate_contains_any(
            clause,
            &["preferred", "ideally", "nice to have", "bonus", "plus"],
        ) {
            preferred.push(range);
        } else {
            required.push(range);
        }
    }
    let required_min_months = required.iter().map(|value| value.0).max();
    let required_max_months = required.iter().filter_map(|value| value.1).max();
    let preferred_min_months = preferred.iter().map(|value| value.0).max();
    let preferred_max_months = preferred.iter().filter_map(|value| value.1).max();
    ExperienceRequirement {
        required_min_months,
        required_max_months,
        preferred_min_months,
        preferred_max_months,
        title_floor_months: title_seniority_floor_months(&posting.title),
    }
}

fn clause_experience_range(clause: &str) -> Option<(i64, Option<i64>)> {
    let normalized: String = clause
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '+' | '-') {
                character
            } else {
                ' '
            }
        })
        .collect();
    let tokens: Vec<&str> = normalized.split_whitespace().collect();
    for (index, token) in tokens.iter().enumerate() {
        if !matches!(*token, "year" | "years" | "yr" | "yrs") {
            continue;
        }
        let start = index.saturating_sub(4);
        for candidate in tokens[start..index].iter().rev() {
            if let Some((minimum, maximum)) = parse_candidate_year_range(candidate) {
                return Some((minimum.saturating_mul(12), maximum.map(|value| value * 12)));
            }
        }
    }
    None
}

fn parse_candidate_year_range(token: &str) -> Option<(i64, Option<i64>)> {
    let plus = token.ends_with('+');
    let value = token.trim_end_matches('+');
    if let Some((left, right)) = value.split_once('-') {
        let minimum = parse_candidate_number(left)?;
        let maximum = parse_candidate_number(right)?;
        return (maximum >= minimum).then_some((minimum, Some(maximum)));
    }
    let minimum = parse_candidate_number(value)?;
    Some((minimum, (!plus).then_some(minimum)))
}

fn parse_candidate_number(value: &str) -> Option<i64> {
    value.parse::<i64>().ok().or(match value {
        "one" => Some(1),
        "two" => Some(2),
        "three" => Some(3),
        "four" => Some(4),
        "five" => Some(5),
        "six" => Some(6),
        "seven" => Some(7),
        "eight" => Some(8),
        "nine" => Some(9),
        "ten" => Some(10),
        _ => None,
    })
}

fn title_seniority_floor_months(title: &str) -> Option<i64> {
    let text = format!(" {} ", candidate_normalize(title));
    if text.contains(" principal ") {
        Some(108)
    } else if text.contains(" director ") {
        Some(96)
    } else if text.contains(" staff ") {
        Some(84)
    } else if candidate_contains_any(&text, &[" senior ", " sr ", " lead "]) {
        Some(60)
    } else {
        None
    }
}

fn track_employment_type_failure(
    posting: &JobPosting,
    track: Option<&CareerTrack>,
) -> Option<String> {
    let selected = track?.policy.employment_types.as_slice();
    if selected.is_empty() {
        return None;
    }
    let actual = candidate_employment_type(posting)?;
    (!selected
        .iter()
        .any(|value| normalize_candidate_employment_type(value) == Some(actual)))
    .then(|| format!("This Career Track does not allow {actual} roles."))
}

fn track_engagement_type_failure(
    posting: &JobPosting,
    track: Option<&CareerTrack>,
) -> Option<String> {
    let selected = track?.policy.engagement_types.as_slice();
    if selected.is_empty() {
        return None;
    }
    let actual = candidate_engagement_type(posting)?;
    (!selected
        .iter()
        .any(|value| normalize_candidate_engagement_type(value) == Some(actual)))
        .then(|| format!("This Career Track does not allow {actual} engagements."))
}

fn track_work_authorization_failure(
    profile: &CareerProfile,
    posting: &JobPosting,
    track: Option<&CareerTrack>,
) -> Option<String> {
    let profile_authorization = candidate_normalize(&profile.work_authorization).replace(' ', "_");
    if let Some(track) = track {
        if !track.policy.work_authorizations.is_empty()
            && !track.policy.work_authorizations.iter().any(|value| {
                candidate_normalize(value).replace(' ', "_") == profile_authorization
            })
        {
            return Some(
                "The verified work authorization is not enabled for this Career Track."
                    .to_string(),
            );
        }
    }
    let text = candidate_normalize(&format!("{} {}", posting.title, posting.description));
    if candidate_contains_any(
        &text,
        &[
            "us citizenship required",
            "u s citizenship required",
            "must be a us citizen",
            "must be a u s citizen",
        ],
    ) && !candidate_contains_any(
        &profile_authorization,
        &["citizen", "us_citizen", "u_s_citizen"],
    ) {
        return Some("This role requires U.S. citizenship that is not in the verified profile.".to_string());
    }
    None
}

fn candidate_employment_type(posting: &JobPosting) -> Option<&'static str> {
    let text = candidate_normalize(&format!(
        "{} {} {}",
        posting.employment_type, posting.title, posting.description
    ));
    [
        ("internship", ["internship", "intern"].as_slice()),
        ("apprenticeship", ["apprenticeship", "apprentice"].as_slice()),
        ("part_time", ["part time", "parttime"].as_slice()),
        ("temporary", ["temporary", "temp role", "temp position"].as_slice()),
        ("seasonal", ["seasonal"].as_slice()),
        ("per_diem", ["per diem", "perdiem"].as_slice()),
        ("contract", ["contract", "contractor"].as_slice()),
        ("full_time", ["full time", "fulltime", "permanent"].as_slice()),
    ]
    .into_iter()
    .find_map(|(kind, markers)| candidate_contains_any(&text, markers).then_some(kind))
}

pub(crate) fn normalize_candidate_employment_type(value: &str) -> Option<&'static str> {
    let normalized = candidate_normalize(value).replace(' ', "_");
    match normalized.as_str() {
        "full_time" | "fulltime" | "ft" | "permanent" | "direct_hire" => {
            Some("full_time")
        }
        "part_time" => Some("part_time"),
        "contract" | "contractor" => Some("contract"),
        "temporary" | "temp" => Some("temporary"),
        "intern" | "internship" => Some("internship"),
        "apprentice" | "apprenticeship" => Some("apprenticeship"),
        "seasonal" => Some("seasonal"),
        "per_diem" | "perdiem" => Some("per_diem"),
        _ => None,
    }
}

pub(crate) fn normalize_candidate_engagement_type(value: &str) -> Option<&'static str> {
    let normalized = candidate_normalize(value).replace(' ', "_");
    match normalized.as_str() {
        "w2" | "w_2" => Some("w2"),
        "c2c" | "c_2_c" | "corp_to_corp" | "corporation_to_corporation" => Some("c2c"),
        "1099" | "independent_contractor" => Some("1099"),
        "direct_hire" | "permanent_hire" => Some("direct_hire"),
        _ => None,
    }
}

fn candidate_engagement_type(posting: &JobPosting) -> Option<&'static str> {
    let text = candidate_normalize(&format!(
        "{} {} {}",
        posting.employment_type, posting.title, posting.description
    ));
    if candidate_contains_any(&text, &["corp to corp", "c2c", "c 2 c"]) {
        Some("c2c")
    } else if candidate_contains_any(&text, &["1099", "independent contractor"]) {
        Some("1099")
    } else if candidate_contains_any(&text, &["w2", "w 2"]) {
        Some("w2")
    } else if candidate_contains_any(&text, &["direct hire", "permanent hire"]) {
        Some("direct_hire")
    } else {
        None
    }
}

fn tailored_packet_coverage(
    posting: &JobPosting,
    profile: &CareerProfile,
    content: &Value,
) -> i64 {
    let posting_text = candidate_normalize(&format!("{} {}", posting.title, posting.description));
    let packet_text = candidate_normalize(&content.to_string());
    let mut evidence_phrases: BTreeSet<String> = BTreeSet::new();
    for phrase in profile
        .skills
        .iter()
        .chain(profile.certifications.iter())
        .chain(profile.employment.iter().map(|entry| &entry.title))
    {
        let normalized = candidate_normalize(phrase);
        if normalized.len() >= 2 && posting_text.contains(&normalized) {
            evidence_phrases.insert(normalized);
        }
    }
    if evidence_phrases.is_empty() {
        return 0;
    }
    let covered = evidence_phrases
        .iter()
        .filter(|phrase| packet_text.contains(phrase.as_str()))
        .count() as i64;
    (covered * 100 / evidence_phrases.len() as i64).clamp(0, 100)
}

fn candidate_normalize(value: &str) -> String {
    value
        .to_ascii_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn candidate_contains_any(text: &str, markers: &[&str]) -> bool {
    markers.iter().any(|marker| text.contains(marker))
}

#[cfg(test)]
mod candidate_policy_tests {
    use super::*;

    fn posting(title: &str, description: &str) -> JobPosting {
        JobPosting {
            id: "job-1".to_string(),
            canonical_key: "job-1".to_string(),
            source: "greenhouse".to_string(),
            external_id: "job-1".to_string(),
            company: "Example".to_string(),
            title: title.to_string(),
            location: "Remote".to_string(),
            workplace: "remote".to_string(),
            canonical_url: "https://boards.greenhouse.io/example/jobs/1".to_string(),
            description: description.to_string(),
            compensation: String::new(),
            employment_type: "full_time".to_string(),
            track_id: "track-1".to_string(),
            match_score: 0,
            matched_reasons: Vec::new(),
            missing_requirements: Vec::new(),
            posted_at_ms: None,
            last_verified_at_ms: None,
            availability_status: "active".to_string(),
            status: "matched".to_string(),
            created_at_ms: 0,
            updated_at_ms: 0,
            discovery_evidence: JobDiscoveryEvidence::default(),
            eligibility: None,
        }
    }

    #[test]
    fn role_experience_ignores_unrelated_and_merges_overlapping_months() {
        let profile = CareerProfile {
            employment: vec![
                EmploymentEntry {
                    id: "software-a".to_string(),
                    title: "Software Engineer".to_string(),
                    start_date: "2020-01".to_string(),
                    end_date: "2022-12".to_string(),
                    ..EmploymentEntry::default()
                },
                EmploymentEntry {
                    id: "software-b".to_string(),
                    title: "Backend Developer".to_string(),
                    start_date: "2022-01".to_string(),
                    end_date: "2023-12".to_string(),
                    ..EmploymentEntry::default()
                },
                EmploymentEntry {
                    id: "clinical".to_string(),
                    title: "Clinical Research Coordinator".to_string(),
                    start_date: "2015-01".to_string(),
                    end_date: "2019-12".to_string(),
                    ..EmploymentEntry::default()
                },
            ],
            ..CareerProfile::default()
        };
        let evidence = role_experience_evidence(
            &profile,
            None,
            &posting("Software Engineer", "Build backend services"),
        );
        assert_eq!(evidence.role_family, ROLE_FAMILY_SOFTWARE);
        assert_eq!(evidence.total_months, 48);
        assert_eq!(
            evidence.relevant_employment_ids,
            vec!["software-a".to_string(), "software-b".to_string()]
        );
    }

    #[test]
    fn required_and_preferred_experience_are_separate_from_title_floor() {
        let requirement = experience_requirement(&posting(
            "Senior Software Engineer",
            "Requires 3+ years of experience. 5 years preferred.",
        ));
        assert_eq!(requirement.required_min_months, Some(36));
        assert_eq!(requirement.required_max_months, None);
        assert_eq!(requirement.preferred_min_months, Some(60));
        assert_eq!(requirement.title_floor_months, Some(60));
    }

    #[test]
    fn track_filters_contract_engagement_without_guessing_unknown_values() {
        let track = CareerTrack {
            id: "track-1".to_string(),
            name: "Software".to_string(),
            role: "Software Engineer".to_string(),
            locations: Vec::new(),
            remote_preference: "remote_or_hybrid".to_string(),
            application_identity_id: None,
            policy: CareerTrackPolicy {
                employment_types: vec!["full_time".to_string()],
                engagement_types: vec!["w2".to_string()],
                ..CareerTrackPolicy::default()
            },
            active: true,
            match_count: 0,
            created_at_ms: 0,
            updated_at_ms: 0,
        };
        let contract = posting("Software Engineer", "Six month C2C contract");
        assert!(track_employment_type_failure(&contract, Some(&track)).is_some());
        assert!(track_engagement_type_failure(&contract, Some(&track)).is_some());
    }
}

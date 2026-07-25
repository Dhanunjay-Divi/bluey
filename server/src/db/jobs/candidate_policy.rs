const ROLE_FAMILY_SOFTWARE: &str = "software_engineering";
const ROLE_FAMILY_DATA: &str = "data_engineering";
const ROLE_FAMILY_ML: &str = "data_science_ml";
const ROLE_FAMILY_PRODUCT: &str = "product_management";
const ROLE_FAMILY_CLINICAL: &str = "clinical_research";
const ROLE_FAMILY_GENERIC: &str = "generic";

fn canonical_role_family(track: Option<&CareerTrack>, posting: &JobPosting) -> String {
    track
        .map(|value| value.role.trim())
        .filter(|value| !value.is_empty())
        .map(infer_role_family)
        .unwrap_or_else(|| infer_role_family(&posting.title))
}

fn posting_role_family(posting: &JobPosting) -> String {
    infer_role_family(&posting.title)
}

pub(crate) fn normalize_career_track(track: &CareerTrack) -> CareerTrack {
    let mut value = track.clone();
    value.name = value.name.trim().to_string();
    value.role = canonical_target_role(&value.role);
    value.locations = trim_dedupe(&value.locations);
    value.remote_preference = match candidate_normalize(&value.remote_preference).as_str() {
        "remote only" => "remote_only",
        "hybrid ok" => "hybrid_ok",
        "onsite ok" | "on site ok" => "onsite_ok",
        _ => "remote_or_hybrid",
    }
    .to_string();
    value.application_identity_id = value
        .application_identity_id
        .as_deref()
        .map(str::trim)
        .filter(|identity_id| !identity_id.is_empty())
        .map(str::to_string);
    value.source_resume_asset_id = value.source_resume_asset_id.trim().to_string();
    value.policy.role_family = infer_role_family(&value.role);
    value.policy.relevant_employment_ids = trim_dedupe(&value.policy.relevant_employment_ids);
    value.policy.employment_types = normalize_employment_types(&value.policy.employment_types);
    value.policy.engagement_types = normalize_engagement_types(&value.policy.engagement_types);
    value.policy.work_authorizations = normalize_work_authorizations(
        &value.policy.work_authorizations,
    );
    value
}

fn canonical_target_role(value: &str) -> String {
    let role_markers = [
        " engineer ",
        " developer ",
        " manager ",
        " director ",
        " analyst ",
        " scientist ",
        " designer ",
        " consultant ",
        " specialist ",
        " architect ",
        " research ",
        " coordinator ",
        " recruiter ",
        " accountant ",
    ];
    let role_only = value
        .split(',')
        .map(str::trim)
        .find(|part| {
            let normalized = format!(" {} ", candidate_normalize(part));
            candidate_contains_any(&normalized, &role_markers)
        })
        .unwrap_or_else(|| value.trim());
    let clean = strip_target_seniority(role_only);
    let normalized = candidate_normalize(&clean);
    match normalized.as_str() {
        "sde" | "swe" | "software developer" | "application developer"
        | "application engineer" | "software engineer" => "Software Engineer".to_string(),
        "frontend" | "frontend developer" | "front end developer" | "frontend engineer"
        | "front end engineer" => "Frontend Engineer".to_string(),
        "backend" | "backend developer" | "back end developer" | "backend engineer"
        | "back end engineer" => "Backend Engineer".to_string(),
        "fullstack" | "full stack" | "fullstack developer" | "full stack developer"
        | "fullstack engineer" | "full stack engineer" => "Full Stack Engineer".to_string(),
        "mobile developer" | "mobile engineer" => "Mobile Engineer".to_string(),
        "devops" | "devops developer" | "devops engineer" => "DevOps Engineer".to_string(),
        "sre" | "site reliability engineer" => "Site Reliability Engineer".to_string(),
        "cloud engineer" => "Cloud Engineer".to_string(),
        "security engineer" | "cybersecurity engineer" => "Security Engineer".to_string(),
        "de" | "data developer" | "data engineer" => "Data Engineer".to_string(),
        "data analyst" => "Data Analyst".to_string(),
        "data scientist" => "Data Scientist".to_string(),
        "ml engineer" | "mle" | "machine learning engineer" => {
            "Machine Learning Engineer".to_string()
        }
        "ai engineer" | "artificial intelligence engineer" => "AI Engineer".to_string(),
        "business intelligence analyst" | "bi analyst" => {
            "Business Intelligence Analyst".to_string()
        }
        "pm" | "product owner" | "product manager" => "Product Manager".to_string(),
        "tpm" | "technical product manager" => "Technical Product Manager".to_string(),
        "program manager" => "Program Manager".to_string(),
        "project manager" => "Project Manager".to_string(),
        "qa" | "qa engineer" | "test engineer" | "quality assurance engineer" => {
            "Quality Assurance Engineer".to_string()
        }
        "solutions architect" => "Solutions Architect".to_string(),
        "solutions engineer" => "Solutions Engineer".to_string(),
        "cra" | "clinical research associate" => "Clinical Research Associate".to_string(),
        "crc" | "clinical research coordinator" => {
            "Clinical Research Coordinator".to_string()
        }
        "clinical research analyst" => "Clinical Research Analyst".to_string(),
        "clinical operations manager" | "director of clinical operations" => {
            "Clinical Operations Manager".to_string()
        }
        _ => clean,
    }
}

fn strip_target_seniority(value: &str) -> String {
    let words: Vec<&str> = value.split_whitespace().collect();
    if words.is_empty() {
        return String::new();
    }
    let first = candidate_normalize(words[0]);
    let mut start = if matches!(
        first.as_str(),
        "junior" | "jr" | "senior" | "sr" | "staff" | "lead" | "principal"
    ) || matches!(first.as_str(), "entry level" | "mid level")
    {
        1
    } else if matches!(first.as_str(), "entry" | "mid")
        && words
            .get(1)
            .is_some_and(|word| candidate_normalize(word) == "level")
    {
        2
    } else {
        0
    };
    start = start.min(words.len());
    let mut end = words.len();
    if end > start
        && matches!(
            candidate_normalize(words[end - 1]).as_str(),
            "i" | "ii" | "iii" | "iv" | "1" | "2" | "3" | "4"
        )
    {
        end -= 1;
    }
    words[start..end].join(" ").trim().to_string()
}

fn trim_dedupe(values: &[String]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    values
        .iter()
        .filter_map(|value| {
            let clean = value.trim();
            if clean.is_empty() || !seen.insert(clean.to_lowercase()) {
                None
            } else {
                Some(clean.to_string())
            }
        })
        .collect()
}

fn normalize_employment_types(values: &[String]) -> Vec<String> {
    let mut normalized = trim_dedupe(values)
        .into_iter()
        .map(|value| {
            normalize_candidate_employment_type(&value)
                .map(str::to_string)
                .unwrap_or(value)
        })
        .collect::<Vec<_>>();
    normalized = trim_dedupe(&normalized);
    if normalized.is_empty() {
        normalized.push("full_time".to_string());
    }
    normalized
}

fn normalize_engagement_types(values: &[String]) -> Vec<String> {
    let normalized = trim_dedupe(values)
        .into_iter()
        .map(|value| {
            normalize_candidate_engagement_type(&value)
                .map(str::to_string)
                .unwrap_or(value)
        })
        .collect::<Vec<_>>();
    trim_dedupe(&normalized)
}

fn normalize_work_authorizations(values: &[String]) -> Vec<String> {
    let normalized = trim_dedupe(values)
        .into_iter()
        .map(|value| candidate_normalize(&value).replace(' ', "_"))
        .collect::<Vec<_>>();
    trim_dedupe(&normalized)
}

fn infer_role_family(value: &str) -> String {
    let canonical = canonical_target_role(value);
    let text = format!(" {} ", candidate_normalize(&canonical));
    if candidate_contains_any(
        &text,
        &[
            " data engineer ",
            " data platform ",
            " data analyst ",
            " analytics engineer ",
            " business intelligence ",
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
            " ai engineer ",
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
            " mobile engineer ",
            " quality assurance ",
            " test engineer ",
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
    let now = Utc::now();
    let current_month = i64::from(now.year()) * 12 + i64::from(now.month0());
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
            let matches_role = if use_explicit {
                explicit_ids.contains(entry.id.as_str())
                    && employment_matches_role_family(entry, &role_family)
            } else {
                employment_matches_role_family(entry, &role_family)
            };
            matches_role && employment_month_interval(entry, current_month).is_some()
        })
        .collect();
    let total_months = non_overlapping_employment_months(&relevant, current_month);
    let max_verified_title_level = relevant
        .iter()
        .map(|entry| candidate_title_level(&entry.title))
        .max()
        .unwrap_or(0);
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
        max_verified_title_level,
        target_title_level: candidate_title_level(&posting.title),
    }
}

fn employment_matches_role_family(entry: &EmploymentEntry, role_family: &str) -> bool {
    if role_family == ROLE_FAMILY_GENERIC {
        return true;
    }
    let family = infer_role_family(&format!("{} {}", entry.title, entry.highlights.join(" ")));
    family == role_family
}

fn non_overlapping_employment_months(
    entries: &[&EmploymentEntry],
    current_month: i64,
) -> i64 {
    let mut intervals: Vec<(i64, i64)> = entries
        .iter()
        .filter_map(|entry| employment_month_interval(entry, current_month))
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

fn employment_month_interval(
    entry: &EmploymentEntry,
    current_month: i64,
) -> Option<(i64, i64)> {
    let start = parse_candidate_year_month(&entry.start_date)?;
    let end = if entry.current {
        current_month.saturating_add(1)
    } else {
        parse_candidate_year_month(&entry.end_date)?.saturating_add(1)
    };
    (end > start).then_some((start, end))
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
        for (range, is_preferred) in clause_experience_ranges(clause) {
            if is_preferred {
                preferred.push(range);
            } else {
                required.push(range);
            }
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

fn clause_experience_ranges(clause: &str) -> Vec<((i64, Option<i64>), bool)> {
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
    let mut ranges = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        if !matches!(*token, "year" | "years" | "yr" | "yrs") {
            continue;
        }
        let start = index.saturating_sub(4);
        for (offset, candidate) in tokens[start..index].iter().enumerate().rev() {
            if let Some((minimum, maximum)) = parse_candidate_year_range(candidate) {
                let number_index = start + offset;
                ranges.push((
                    (
                        minimum.saturating_mul(12),
                        maximum.map(|value| value * 12),
                    ),
                    experience_marker_is_preferred(&tokens, number_index, index),
                ));
                break;
            }
        }
    }
    ranges
}

fn experience_marker_is_preferred(
    tokens: &[&str],
    number_index: usize,
    year_index: usize,
) -> bool {
    let preferred_markers = ["preferred", "ideally", "bonus", "plus"];
    let required_markers = [
        "require",
        "requires",
        "required",
        "minimum",
        "must",
        "need",
        "needed",
    ];
    let start = number_index.saturating_sub(5);
    let end = (year_index + 6).min(tokens.len());
    let nearest_preferred = tokens[start..end]
        .iter()
        .enumerate()
        .filter(|(_, token)| preferred_markers.contains(token))
        .map(|(offset, _)| (start + offset).abs_diff(year_index))
        .min();
    let nearest_required = tokens[start..end]
        .iter()
        .enumerate()
        .filter(|(_, token)| required_markers.contains(token))
        .map(|(offset, _)| (start + offset).abs_diff(year_index))
        .min();
    let context = tokens[start..end].join(" ");
    let preferred_phrase = context.contains("nice to have");
    match (nearest_preferred, nearest_required) {
        (Some(preferred), Some(required)) => preferred < required,
        (Some(_), None) => true,
        (None, _) => preferred_phrase,
    }
}

fn parse_candidate_year_range(token: &str) -> Option<(i64, Option<i64>)> {
    let value = token.trim_end_matches('+');
    if let Some((left, right)) = value.split_once('-') {
        let minimum = parse_candidate_number(left)?;
        let maximum = parse_candidate_number(right)?;
        return (maximum >= minimum).then_some((minimum, Some(maximum)));
    }
    let minimum = parse_candidate_number(value)?;
    Some((minimum, None))
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

fn candidate_title_level(title: &str) -> i64 {
    let text = format!(" {} ", candidate_normalize(title));
    if candidate_contains_any(&text, &[" director ", " head ", " vice president ", " vp "]) {
        5
    } else if text.contains(" principal ") {
        4
    } else if text.contains(" staff ") {
        3
    } else if candidate_contains_any(
        &text,
        &[
            " senior ",
            " sr ",
            " lead ",
            " engineer iii ",
            " engineer 3 ",
        ],
    ) {
        2
    } else if candidate_contains_any(
        &text,
        &[" intern ", " internship ", " apprentice ", " junior ", " jr ", " entry "],
    ) {
        0
    } else {
        1
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
    let Some(actual) = candidate_employment_type(posting) else {
        return Some(
            "Bluey could not verify this job's employment type for this Career Track."
                .to_string(),
        );
    };
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
    let Some(actual) = candidate_engagement_type(posting) else {
        return Some(
            "Bluey could not verify whether this role is W2, C2C, 1099, or direct hire."
                .to_string(),
        );
    };
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
    ) && !matches!(
        profile_authorization.as_str(),
        "citizen" | "us_citizen" | "u_s_citizen" | "united_states_citizen"
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
            eligibility: None,
        }
    }

    fn track(role: &str) -> CareerTrack {
        CareerTrack {
            id: "track-1".to_string(),
            name: "  Primary track  ".to_string(),
            role: role.to_string(),
            locations: vec![
                " Arlington, VA ".to_string(),
                "arlington, va".to_string(),
                "San Francisco, CA".to_string(),
            ],
            remote_preference: "Remote Only".to_string(),
            application_identity_id: Some("  identity-1  ".to_string()),
            source_resume_asset_id: String::new(),
            policy: CareerTrackPolicy {
                role_family: ROLE_FAMILY_CLINICAL.to_string(),
                relevant_employment_ids: vec![
                    " software-a ".to_string(),
                    "software-a".to_string(),
                ],
                employment_types: vec!["Full Time".to_string()],
                engagement_types: vec!["W-2".to_string()],
                work_authorizations: vec!["US Citizen".to_string()],
            },
            active: true,
            match_count: 99,
            created_at_ms: 1,
            updated_at_ms: 2,
        }
    }

    #[test]
    fn career_track_normalization_uses_full_form_roles_and_server_owned_family() {
        let normalized = normalize_career_track(&track(
            "Capital One, Senior Software Engineer II",
        ));

        assert_eq!(normalized.name, "Primary track");
        assert_eq!(normalized.role, "Software Engineer");
        assert_eq!(normalized.policy.role_family, ROLE_FAMILY_SOFTWARE);
        assert_eq!(
            normalized.locations,
            vec![
                "Arlington, VA".to_string(),
                "San Francisco, CA".to_string()
            ]
        );
        assert_eq!(normalized.remote_preference, "remote_only");
        assert_eq!(
            normalized.application_identity_id.as_deref(),
            Some("identity-1")
        );
        assert_eq!(
            normalized.policy.relevant_employment_ids,
            vec!["software-a".to_string()]
        );
        assert_eq!(
            normalized.policy.employment_types,
            vec!["full_time".to_string()]
        );
        assert_eq!(
            normalized.policy.engagement_types,
            vec!["w2".to_string()]
        );
        assert_eq!(
            normalized.policy.work_authorizations,
            vec!["us_citizen".to_string()]
        );
    }

    #[test]
    fn career_track_role_aliases_resolve_to_canonical_full_forms() {
        assert_eq!(canonical_target_role("SDE"), "Software Engineer");
        assert_eq!(canonical_target_role("DE"), "Data Engineer");
        assert_eq!(canonical_target_role("PM"), "Product Manager");
        assert_eq!(
            canonical_target_role("CRA"),
            "Clinical Research Associate"
        );
    }

    #[test]
    fn posting_role_aliases_resolve_to_the_correct_role_family() {
        assert_eq!(infer_role_family("SDE"), ROLE_FAMILY_SOFTWARE);
        assert_eq!(infer_role_family("DE"), ROLE_FAMILY_DATA);
        assert_eq!(infer_role_family("MLE"), ROLE_FAMILY_ML);
        assert_eq!(infer_role_family("PM"), ROLE_FAMILY_PRODUCT);
        assert_eq!(infer_role_family("CRA"), ROLE_FAMILY_CLINICAL);
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
        assert_eq!(evidence.max_verified_title_level, 1);
        assert_eq!(evidence.target_title_level, 1);
    }

    #[test]
    fn explicit_track_employment_cannot_cross_role_families() {
        let profile = CareerProfile {
            employment: vec![
                EmploymentEntry {
                    id: "software".to_string(),
                    title: "Software Engineer".to_string(),
                    start_date: "2022-01".to_string(),
                    end_date: "2023-12".to_string(),
                    ..EmploymentEntry::default()
                },
                EmploymentEntry {
                    id: "clinical".to_string(),
                    title: "Clinical Research Coordinator".to_string(),
                    start_date: "2015-01".to_string(),
                    end_date: "2021-12".to_string(),
                    ..EmploymentEntry::default()
                },
            ],
            ..CareerProfile::default()
        };
        let mut software_track = track("Software Engineer");
        software_track.policy.relevant_employment_ids =
            vec!["software".to_string(), "clinical".to_string()];

        let evidence = role_experience_evidence(
            &profile,
            Some(&software_track),
            &posting("Software Engineer", "Build backend services"),
        );

        assert_eq!(evidence.total_months, 24);
        assert_eq!(
            evidence.relevant_employment_ids,
            vec!["software".to_string()]
        );
    }

    #[test]
    fn blank_end_date_is_not_treated_as_current_without_current_flag() {
        let profile = CareerProfile {
            employment: vec![EmploymentEntry {
                id: "software".to_string(),
                title: "Software Engineer".to_string(),
                start_date: "2022-01".to_string(),
                end_date: String::new(),
                current: false,
                ..EmploymentEntry::default()
            }],
            ..CareerProfile::default()
        };

        let evidence = role_experience_evidence(
            &profile,
            None,
            &posting("Software Engineer", "Build backend services"),
        );

        assert_eq!(evidence.total_months, 0);
        assert!(evidence.relevant_employment_ids.is_empty());
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
    fn mixed_required_and_preferred_experience_in_one_clause_stays_separate() {
        let requirement = experience_requirement(&posting(
            "Software Engineer",
            "Candidates need 3 years required, while 5 years is preferred",
        ));
        assert_eq!(requirement.required_min_months, Some(36));
        assert_eq!(requirement.preferred_min_months, Some(60));
    }

    #[test]
    fn scalar_experience_requirement_is_a_minimum_not_an_exact_ceiling() {
        let requirement = experience_requirement(&posting(
            "Software Engineer",
            "Requires 10 years of software engineering experience.",
        ));
        assert_eq!(requirement.required_min_months, Some(120));
        assert_eq!(requirement.required_max_months, None);
    }

    #[test]
    fn track_filters_fail_closed_without_guessing_unknown_values() {
        let track = CareerTrack {
            id: "track-1".to_string(),
            name: "Software".to_string(),
            role: "Software Engineer".to_string(),
            locations: Vec::new(),
            remote_preference: "remote_or_hybrid".to_string(),
            application_identity_id: None,
            source_resume_asset_id: String::new(),
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

        let mut unknown = posting("Software Engineer", "Build reliable products");
        unknown.employment_type.clear();
        assert!(track_employment_type_failure(&unknown, Some(&track)).is_some());
        assert!(track_engagement_type_failure(&unknown, Some(&track)).is_some());
    }

    #[test]
    fn negated_citizenship_does_not_satisfy_citizenship_requirement() {
        let profile = CareerProfile {
            work_authorization: "Not a citizen".to_string(),
            ..CareerProfile::default()
        };
        let restricted = posting(
            "Software Engineer",
            "Applicants must be a U.S. citizen for this role.",
        );

        assert!(track_work_authorization_failure(&profile, &restricted, None).is_some());
    }

    #[test]
    fn title_levels_are_independent_from_elapsed_experience() {
        assert_eq!(candidate_title_level("Software Engineer"), 1);
        assert_eq!(candidate_title_level("Senior Software Engineer"), 2);
        assert_eq!(candidate_title_level("Staff Software Engineer"), 3);
        assert_eq!(candidate_title_level("Principal Software Engineer"), 4);
    }
}

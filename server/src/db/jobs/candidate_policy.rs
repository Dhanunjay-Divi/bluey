const ROLE_FAMILY_SOFTWARE: &str = "software-engineering";
const ROLE_FAMILY_DATA: &str = "data-and-ai";
const ROLE_FAMILY_ML: &str = "data-and-ai";
const ROLE_FAMILY_PRODUCT: &str = "product-management";
const ROLE_FAMILY_CLINICAL: &str = "clinical-research";
const ROLE_FAMILY_GENERIC: &str = "generic";

fn canonical_role_family(track: Option<&CareerTrack>, posting: &JobPosting) -> String {
    if let Some(track) = track {
        let authoritative = track.policy.authority.canonical_role_family_id.trim();
        if !authoritative.is_empty() {
            return authoritative.to_string();
        }
        let configured = track.policy.role_family.trim();
        if !configured.is_empty() {
            return normalize_role_family(configured);
        }
        return match crate::jobs_taxonomy::resolve_target_role(&track.role) {
            crate::jobs_taxonomy::TargetRoleResolution::Known { family_id, .. } => family_id,
            _ => ROLE_FAMILY_GENERIC.to_string(),
        };
    }
    posting_role_family(posting)
}

fn posting_role_family(posting: &JobPosting) -> String {
    match crate::jobs_taxonomy::classify_posting_role(&posting.title) {
        crate::jobs_taxonomy::PostingRoleClassification::Known { family_id, .. } => family_id,
        _ => ROLE_FAMILY_GENERIC.to_string(),
    }
}

fn normalize_role_family(value: &str) -> String {
    let normalized = candidate_normalize(value);
    match normalized.as_str() {
        "software engineering" | "software engineer" | "software developer" | "swe" | "sde" => {
            ROLE_FAMILY_SOFTWARE.to_string()
        }
        "data engineering" | "data engineer" | "de" | "data and ai" => ROLE_FAMILY_DATA.to_string(),
        "data science" | "machine learning" | "machine learning engineering" | "ml" | "ai" => {
            ROLE_FAMILY_ML.to_string()
        }
        "product management" | "product manager" => ROLE_FAMILY_PRODUCT.to_string(),
        "clinical research" | "clinical operations" => ROLE_FAMILY_CLINICAL.to_string(),
        "generic" | "other" => ROLE_FAMILY_GENERIC.to_string(),
        _ => infer_role_family(value),
    }
}

fn infer_role_family(value: &str) -> String {
    match crate::jobs_taxonomy::classify_posting_role(value) {
        crate::jobs_taxonomy::PostingRoleClassification::Known { family_id, .. } => family_id,
        _ => ROLE_FAMILY_GENERIC.to_string(),
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
                    && (role_family == ROLE_FAMILY_GENERIC
                        || employment_matches_role_family(entry, &role_family))
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
        return false;
    }
    let family = infer_role_family(&entry.title);
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CandidateCategoryUnknownReason {
    Empty,
    Unsupported,
    Negated,
    Mixed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CandidateCategoryClassification {
    Known(&'static str),
    Unknown(CandidateCategoryUnknownReason),
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum CandidatePolicyFilterDecision {
    Allowed,
    Mismatch(String),
    ReviewRequired(String),
}

fn track_employment_type_decision(
    posting: &JobPosting,
    track: Option<&CareerTrack>,
) -> Option<CandidatePolicyFilterDecision> {
    let selected = track?.policy.employment_types.as_slice();
    if selected.is_empty() {
        return Some(CandidatePolicyFilterDecision::Allowed);
    }
    Some(match candidate_employment_type(posting) {
        CandidateCategoryClassification::Known(actual)
            if selected
                .iter()
                .any(|value| normalize_candidate_employment_type(value) == Some(actual)) =>
        {
            CandidatePolicyFilterDecision::Allowed
        }
        CandidateCategoryClassification::Known(actual) => CandidatePolicyFilterDecision::Mismatch(
            format!("This Career Track does not allow {actual} roles."),
        ),
        CandidateCategoryClassification::Unknown(_) => {
            CandidatePolicyFilterDecision::ReviewRequired(
                "Bluey could not prove this posting's employment type for the Career Track."
                    .to_string(),
            )
        }
    })
}

fn track_engagement_type_decision(
    posting: &JobPosting,
    track: Option<&CareerTrack>,
) -> Option<CandidatePolicyFilterDecision> {
    let selected = track?.policy.engagement_types.as_slice();
    if selected.is_empty() {
        return Some(CandidatePolicyFilterDecision::Allowed);
    }
    Some(match candidate_engagement_type(posting) {
        CandidateCategoryClassification::Known(actual)
            if selected
                .iter()
                .any(|value| normalize_candidate_engagement_type(value) == Some(actual)) =>
        {
            CandidatePolicyFilterDecision::Allowed
        }
        CandidateCategoryClassification::Known(actual) => CandidatePolicyFilterDecision::Mismatch(
            format!("This Career Track does not allow {actual} engagements."),
        ),
        CandidateCategoryClassification::Unknown(_) => {
            CandidatePolicyFilterDecision::ReviewRequired(
                "Bluey could not prove this posting's engagement type for the Career Track."
                    .to_string(),
            )
        }
    })
}

fn track_work_authorization_failure(
    profile: &CareerProfile,
    posting: &JobPosting,
    track: Option<&CareerTrack>,
) -> Option<String> {
    let profile_authorization = candidate_normalize(&profile.work_authorization).replace(' ', "_");
    if let Some(track) = track {
        if !track.policy.work_authorizations.is_empty()
            && !track
                .policy
                .work_authorizations
                .iter()
                .any(|value| candidate_normalize(value).replace(' ', "_") == profile_authorization)
        {
            return Some(
                "The verified work authorization is not enabled for this Career Track.".to_string(),
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
        return Some(
            "This role requires U.S. citizenship that is not in the verified profile.".to_string(),
        );
    }
    None
}

fn candidate_employment_type(posting: &JobPosting) -> CandidateCategoryClassification {
    let evidence = candidate_normalize(&posting.employment_type);
    let context = candidate_normalize(&format!(
        "{} {}",
        posting.employment_type, posting.description
    ));
    classify_candidate_category(
        &evidence,
        &context,
        &[
            ("internship", ["internship", "intern"].as_slice()),
            (
                "apprenticeship",
                ["apprenticeship", "apprentice"].as_slice(),
            ),
            ("part_time", ["part time", "parttime"].as_slice()),
            (
                "temporary",
                ["temporary", "temp role", "temp position"].as_slice(),
            ),
            ("seasonal", ["seasonal"].as_slice()),
            ("per_diem", ["per diem", "perdiem"].as_slice()),
            ("contract", ["contract", "contractor"].as_slice()),
            (
                "full_time",
                ["full time", "fulltime", "permanent", "direct hire"].as_slice(),
            ),
        ],
        normalize_candidate_employment_type(&posting.employment_type),
    )
}

pub(crate) fn normalize_candidate_employment_type(value: &str) -> Option<&'static str> {
    let normalized = candidate_normalize(value).replace(' ', "_");
    match normalized.as_str() {
        "full_time" | "fulltime" | "ft" | "permanent" | "direct_hire" => Some("full_time"),
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

fn candidate_engagement_type(posting: &JobPosting) -> CandidateCategoryClassification {
    let evidence = candidate_normalize(&posting.employment_type);
    let context = candidate_normalize(&format!(
        "{} {}",
        posting.employment_type, posting.description
    ));
    classify_candidate_category(
        &evidence,
        &context,
        &[
            ("c2c", ["corp to corp", "c2c", "c 2 c"].as_slice()),
            ("1099", ["1099", "independent contractor"].as_slice()),
            ("w2", ["w2", "w 2"].as_slice()),
            ("direct_hire", ["direct hire", "permanent hire"].as_slice()),
        ],
        normalize_candidate_engagement_type(&posting.employment_type),
    )
}

fn classify_candidate_category(
    evidence: &str,
    context: &str,
    categories: &[(&'static str, &[&str])],
    explicit_kind: Option<&'static str>,
) -> CandidateCategoryClassification {
    if evidence.trim().is_empty() && context.trim().is_empty() {
        return CandidateCategoryClassification::Unknown(CandidateCategoryUnknownReason::Empty);
    }
    if categories.iter().any(|(_, markers)| {
        markers
            .iter()
            .any(|marker| candidate_marker_is_negated(context, marker))
    }) {
        return CandidateCategoryClassification::Unknown(CandidateCategoryUnknownReason::Negated);
    }
    let context_matches: BTreeSet<_> = categories
        .iter()
        .filter_map(|(kind, markers)| {
            markers
                .iter()
                .any(|marker| candidate_contains_phrase(context, marker))
                .then_some(*kind)
        })
        .collect();
    if context_matches.len() > 1 {
        return CandidateCategoryClassification::Unknown(CandidateCategoryUnknownReason::Mixed);
    }
    let mut matches: BTreeSet<_> = categories
        .iter()
        .filter_map(|(kind, markers)| {
            markers
                .iter()
                .any(|marker| candidate_contains_phrase(evidence, marker))
                .then_some(*kind)
        })
        .collect();
    matches.extend(explicit_kind);
    match matches.len() {
        0 => CandidateCategoryClassification::Unknown(CandidateCategoryUnknownReason::Unsupported),
        1 => CandidateCategoryClassification::Known(matches.into_iter().next().unwrap_or_default()),
        _ => CandidateCategoryClassification::Unknown(CandidateCategoryUnknownReason::Mixed),
    }
}

fn candidate_marker_is_negated(text: &str, marker: &str) -> bool {
    // Temporal qualifiers do not turn a denial into positive evidence. Strip
    // them before matching so "not currently accepted" and "currently no
    // contract roles" remain fail-closed while positive "currently available"
    // still has no negation marker.
    let mut temporal_neutral = format!(" {text} ");
    for phrase in ["at this time", "at the moment", "right now", "for now"] {
        temporal_neutral = temporal_neutral.replace(&format!(" {phrase} "), " ");
    }
    let temporal_neutral = temporal_neutral
        .split_whitespace()
        .filter(|token| !matches!(*token, "currently" | "presently" | "temporarily"))
        .collect::<Vec<_>>()
        .join(" ");
    let text = temporal_neutral.as_str();
    [
        "no",
        "not",
        "not a",
        "not an",
        "non",
        "without",
        "not eligible for",
        "do not accept",
        "does not accept",
        "can not accept",
        "cannot accept",
        "will not accept",
        "do not allow",
        "does not allow",
        "cannot allow",
        "do not permit",
        "does not permit",
        "cannot permit",
        "do not support",
        "does not support",
        "cannot support",
        "no longer",
        "no longer accept",
        "no longer accepting",
        "no longer allow",
        "no longer allowing",
        "no longer offer",
        "no longer offering",
        "no longer permit",
        "no longer permitting",
        "no longer support",
        "no longer supporting",
    ]
    .iter()
    .any(|prefix| candidate_contains_phrase(text, &format!("{prefix} {marker}")))
        || [
            "not allowed",
            "not accepted",
            "not available",
            "not offered",
            "not permitted",
            "not supported",
            "not eligible",
            "not possible",
            "unavailable",
            "excluded",
            "prohibited",
            "disallowed",
            "is not allowed",
            "is not accepted",
            "is not available",
            "is not offered",
            "is not permitted",
            "is not supported",
            "is not eligible",
            "is not possible",
            "is unavailable",
            "are not allowed",
            "are not accepted",
            "are not available",
            "are not offered",
            "are not permitted",
            "are not supported",
            "are not eligible",
            "are not possible",
            "are unavailable",
            "no longer accepted",
            "no longer allowed",
            "no longer available",
            "no longer eligible",
            "no longer offered",
            "no longer permitted",
            "no longer supported",
        ]
        .iter()
        .any(|suffix| candidate_contains_phrase(text, &format!("{marker} {suffix}")))
        || [
            "candidate",
            "candidates",
            "application",
            "applications",
            "role",
            "roles",
            "position",
            "positions",
            "job",
            "jobs",
            "work",
            "option",
            "options",
        ]
        .iter()
        .any(|subject| {
            [
                "unavailable",
                "not available",
                "not offered",
                "not permitted",
                "not supported",
                "not eligible",
                "prohibited",
                "disallowed",
                "excluded",
                "not accepted",
                "are not accepted",
                "not allowed",
                "are not allowed",
                "are unavailable",
                "are not available",
                "are not offered",
                "are not permitted",
                "are not supported",
                "are not eligible",
                "are prohibited",
                "are disallowed",
                "are excluded",
                "are no longer accepted",
                "are no longer allowed",
                "are no longer available",
                "are no longer eligible",
                "are no longer offered",
                "are no longer permitted",
                "are no longer supported",
                "is no longer accepted",
                "is no longer allowed",
                "is no longer available",
                "is no longer eligible",
                "is no longer offered",
                "is no longer permitted",
                "is no longer supported",
            ]
            .iter()
            .any(|suffix| candidate_contains_phrase(text, &format!("{marker} {subject} {suffix}")))
        })
}

fn candidate_contains_phrase(text: &str, marker: &str) -> bool {
    format!(" {text} ").contains(&format!(" {marker} "))
}

fn tailored_packet_coverage(posting: &JobPosting, profile: &CareerProfile, content: &Value) -> i64 {
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
    fn role_experience_uses_employment_title_not_highlight_mentions() {
        let profile = CareerProfile {
            employment: vec![
                EmploymentEntry {
                    id: "recruiter".to_string(),
                    title: "Recruiter".to_string(),
                    highlights: vec!["Partnered with Software Engineer teams".to_string()],
                    start_date: "2015-01".to_string(),
                    end_date: "2019-12".to_string(),
                    ..EmploymentEntry::default()
                },
                EmploymentEntry {
                    id: "backend".to_string(),
                    title: "Backend Developer".to_string(),
                    start_date: "2020-01".to_string(),
                    end_date: "2022-12".to_string(),
                    ..EmploymentEntry::default()
                },
            ],
            ..CareerProfile::default()
        };
        let track = CareerTrack {
            id: "track-software".to_string(),
            name: "Software".to_string(),
            role: "Software Engineer".to_string(),
            locations: Vec::new(),
            remote_preference: "remote_or_hybrid".to_string(),
            application_identity_id: None,
            policy: CareerTrackPolicy {
                role_family: ROLE_FAMILY_SOFTWARE.to_string(),
                ..CareerTrackPolicy::default()
            },
            active: true,
            match_count: 0,
            created_at_ms: 0,
            updated_at_ms: 0,
        };
        let evidence = role_experience_evidence(
            &profile,
            Some(&track),
            &posting("Software Engineer", "Build backend services"),
        );
        assert_eq!(
            evidence.relevant_employment_ids,
            vec!["backend".to_string()]
        );
        assert_eq!(evidence.total_months, 36);
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
        let mut contract = posting("Software Engineer", "Six month C2C contract");
        contract.employment_type = "contract c2c".to_string();
        assert!(matches!(
            track_employment_type_decision(&contract, Some(&track)),
            Some(CandidatePolicyFilterDecision::Mismatch(_))
        ));
        assert!(matches!(
            track_engagement_type_decision(&contract, Some(&track)),
            Some(CandidatePolicyFilterDecision::Mismatch(_))
        ));

        let unknown = posting("Software Engineer", "Build reliable services");
        assert!(matches!(
            track_engagement_type_decision(&unknown, Some(&track)),
            Some(CandidatePolicyFilterDecision::ReviewRequired(_))
        ));
    }

    #[test]
    fn employment_and_engagement_classification_rejects_negated_or_mixed_evidence() {
        let mut exact_employment = posting("Software Engineer", "Build reliable services");
        exact_employment.employment_type = "full_time".to_string();
        assert_eq!(
            candidate_employment_type(&exact_employment),
            CandidateCategoryClassification::Known("full_time")
        );
        let mut exact_engagement = posting("Software Engineer", "Build reliable services");
        exact_engagement.employment_type = "w2".to_string();
        assert_eq!(
            candidate_engagement_type(&exact_engagement),
            CandidateCategoryClassification::Known("w2")
        );

        let mut exact = posting("Software Engineer", "Build reliable services");
        exact.employment_type = "full_time w2".to_string();
        assert_eq!(
            candidate_employment_type(&exact),
            CandidateCategoryClassification::Known("full_time")
        );
        assert_eq!(
            candidate_engagement_type(&exact),
            CandidateCategoryClassification::Known("w2")
        );

        let mut negated = posting("Software Engineer", "No C2C; W2 only.");
        negated.employment_type = "contract c2c".to_string();
        assert_eq!(
            candidate_engagement_type(&negated),
            CandidateCategoryClassification::Unknown(CandidateCategoryUnknownReason::Negated)
        );

        let mut mixed = posting("Software Engineer", "W2 or C2C");
        mixed.employment_type = "contract".to_string();
        assert_eq!(
            candidate_engagement_type(&mixed),
            CandidateCategoryClassification::Unknown(CandidateCategoryUnknownReason::Mixed)
        );

        let mut unknown = posting("Software Engineer", "Build reliable services");
        unknown.employment_type.clear();
        assert!(matches!(
            candidate_employment_type(&unknown),
            CandidateCategoryClassification::Unknown(_)
        ));
        assert!(matches!(
            candidate_engagement_type(&unknown),
            CandidateCategoryClassification::Unknown(_)
        ));

        for title in ["Contract Administrator", "1099 Compliance Analyst"] {
            let mut title_only = posting(title, "Build reliable services");
            title_only.employment_type.clear();
            assert!(matches!(
                candidate_employment_type(&title_only),
                CandidateCategoryClassification::Unknown(_)
            ));
            assert!(matches!(
                candidate_engagement_type(&title_only),
                CandidateCategoryClassification::Unknown(_)
            ));
        }

        for description in [
            "Not an internship.",
            "Internship is not offered.",
            "Full-time roles unavailable.",
            "Contract positions prohibited.",
            "C2C prohibited.",
            "C2C disallowed.",
            "C2C jobs excluded.",
            "We do not accept C2C.",
            "C2C is not supported.",
            "C2C is not eligible.",
            "C2C candidates are not accepted.",
            "Contract roles are not currently available.",
            "C2C is not currently accepted.",
            "Currently, we do not accept C2C.",
            "Contract positions are temporarily unavailable.",
            "No longer C2C.",
            "We are no longer accepting C2C.",
            "C2C no longer accepted.",
            "Contract roles are no longer available.",
            "We are no longer offering contract positions.",
        ] {
            let mut denied = posting("Software Engineer", description);
            let lowercase = description.to_ascii_lowercase();
            let is_internship = lowercase.contains("internship");
            let is_employment = is_internship
                || lowercase.contains("full-time")
                || lowercase.contains("contract positions")
                || lowercase.contains("contract roles");
            denied.employment_type = if is_internship {
                "internship".to_string()
            } else if lowercase.contains("full-time") {
                "full_time".to_string()
            } else if lowercase.contains("contract positions")
                || lowercase.contains("contract roles")
            {
                "contract".to_string()
            } else {
                "contract c2c".to_string()
            };
            let classification = if is_employment {
                candidate_employment_type(&denied)
            } else {
                candidate_engagement_type(&denied)
            };
            assert_eq!(
                classification,
                CandidateCategoryClassification::Unknown(CandidateCategoryUnknownReason::Negated),
                "negated evidence was accepted: {description}"
            );
        }
    }
}

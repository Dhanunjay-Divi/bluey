use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

use super::jobs::{CareerProfile, EmploymentEntry, JobPosting, ProjectEntry};

const MAX_RESUME_SKILLS: usize = 16;

pub(super) struct TailoredResume {
    pub headline: String,
    pub summary: String,
    pub skills: Vec<String>,
    pub employment: Vec<EmploymentEntry>,
    pub projects: Vec<ProjectEntry>,
    pub diff: Value,
}

pub(super) fn tailor_resume(
    profile: &CareerProfile,
    posting: &JobPosting,
    mode: &str,
) -> TailoredResume {
    let terms = JobTerms::new(posting);
    let ranked_skills = rank_skills(&profile.skills, &terms);
    let matched_skills: Vec<String> = ranked_skills
        .iter()
        .filter(|item| item.score > 0)
        .map(|item| item.value.clone())
        .collect();
    let skills: Vec<String> = ranked_skills
        .into_iter()
        .take(MAX_RESUME_SKILLS)
        .map(|item| item.value)
        .collect();
    let employment = emphasize_employment(&profile.employment, &terms);
    let projects = emphasize_projects(&profile.projects, &terms);
    let headline = tailored_headline(profile, mode, &matched_skills);
    let summary = tailored_summary(profile, mode, &matched_skills);
    let diff = structural_diff(
        profile,
        &headline,
        &summary,
        &skills,
        &employment,
        &projects,
    );

    TailoredResume {
        headline,
        summary,
        skills,
        employment,
        projects,
        diff,
    }
}

#[derive(Debug)]
struct RankedValue {
    value: String,
    score: i64,
    original_index: usize,
}

struct JobTerms {
    title_tokens: BTreeSet<String>,
    description_token_counts: BTreeMap<String, i64>,
    normalized_corpus: String,
}

impl JobTerms {
    fn new(posting: &JobPosting) -> Self {
        let title_tokens = meaningful_tokens(&posting.title);
        let description_token_counts = meaningful_token_counts(&posting.description);
        let normalized_corpus = format!(
            " {} {} ",
            normalize_phrase(&posting.title),
            normalize_phrase(&posting.description)
        );
        Self {
            title_tokens,
            description_token_counts,
            normalized_corpus,
        }
    }

    fn score(&self, value: &str) -> i64 {
        let normalized = normalize_phrase(value);
        if normalized.is_empty() {
            return 0;
        }

        let mut score = 0;
        let mut phrase_relevance = phrase_count(&self.normalized_corpus, &normalized) * 40;
        // Skills are often written differently in profiles and job descriptions
        // (for example, "Amazon Web Services" versus "AWS"). Treat the best
        // matching alias as the same evidence, without stacking overlapping
        // aliases such as Node.js + JavaScript.
        for aliases in ALIAS_GROUPS {
            if aliases
                .iter()
                .any(|alias| contains_phrase(&format!(" {normalized} "), alias))
            {
                let alias_relevance = aliases
                    .iter()
                    .map(|alias| phrase_count(&self.normalized_corpus, alias))
                    .max()
                    .unwrap_or_default()
                    * 40;
                phrase_relevance = phrase_relevance.max(alias_relevance);
            }
        }
        score += phrase_relevance;
        for token in meaningful_tokens(value) {
            if self.title_tokens.contains(&token) {
                score += 12;
            }
            score += self
                .description_token_counts
                .get(&token)
                .copied()
                .unwrap_or_default()
                .min(5)
                * 4;
        }
        score
    }
}

const ALIAS_GROUPS: &[&[&str]] = &[
    &["software engineer", "software developer", "swe", "sde"],
    &["data engineer", "data engineering", "de"],
    &["product manager", "product management", "pm"],
    &["amazon web services", "aws"],
    &["google cloud platform", "google cloud", "gcp"],
    &["microsoft azure", "azure"],
    &["kubernetes", "k8s"],
    &["postgresql", "postgres"],
    &["react js", "reactjs", "react"],
    &["node js", "nodejs", "node"],
    &["typescript", "ts"],
    &["javascript", "js"],
    &["generative ai", "gen ai", "genai"],
    &["machine learning", "ml"],
    &[
        "large language model",
        "large language models",
        "llm",
        "llms",
    ],
    &[
        "continuous integration",
        "continuous delivery",
        "ci cd",
        "cicd",
    ],
    &["extract transform load", "etl"],
    &["extract load transform", "elt"],
    &["natural language processing", "nlp"],
    &["quality assurance", "qa"],
];

fn rank_skills(skills: &[String], terms: &JobTerms) -> Vec<RankedValue> {
    let mut ranked: Vec<RankedValue> = skills
        .iter()
        .enumerate()
        .map(|(original_index, value)| RankedValue {
            value: value.clone(),
            score: terms.score(value),
            original_index,
        })
        .collect();
    ranked.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.original_index.cmp(&right.original_index))
    });
    ranked
}

fn emphasize_employment(employment: &[EmploymentEntry], terms: &JobTerms) -> Vec<EmploymentEntry> {
    employment
        .iter()
        .cloned()
        .map(|mut entry| {
            entry.highlights = rank_strings(&entry.highlights, terms);
            if is_placeholder_value(&entry.location) {
                entry.location.clear();
            }
            entry
        })
        .collect()
}

fn emphasize_projects(projects: &[ProjectEntry], terms: &JobTerms) -> Vec<ProjectEntry> {
    let mut ranked: Vec<(i64, usize, ProjectEntry)> = projects
        .iter()
        .cloned()
        .enumerate()
        .map(|(index, mut project)| {
            project.technologies = rank_strings(&project.technologies, terms);
            let searchable = format!(
                "{} {} {} {}",
                project.name,
                project.role,
                project.summary,
                project.technologies.join(" ")
            );
            (terms.score(&searchable), index, project)
        })
        .collect();
    ranked.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    ranked.into_iter().map(|(_, _, project)| project).collect()
}

fn rank_strings(values: &[String], terms: &JobTerms) -> Vec<String> {
    let mut ranked: Vec<(i64, usize, String)> = values
        .iter()
        .cloned()
        .enumerate()
        .map(|(index, value)| (terms.score(&value), index, value))
        .collect();
    ranked.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    ranked.into_iter().map(|(_, _, value)| value).collect()
}

fn tailored_headline(profile: &CareerProfile, mode: &str, matched_skills: &[String]) -> String {
    let cleaned = factual_headline(profile);
    let base = cleaned.trim();
    if base.is_empty() || mode != "enhance" {
        return base.to_string();
    }
    let additions: Vec<&str> = matched_skills
        .iter()
        .filter(|skill| {
            !contains_phrase(
                &format!(" {} ", normalize_phrase(base)),
                &normalize_phrase(skill),
            )
        })
        .take(2)
        .map(String::as_str)
        .collect();
    if additions.is_empty() {
        base.to_string()
    } else {
        format!("{base} | {}", additions.join(" | "))
    }
}

fn tailored_summary(profile: &CareerProfile, mode: &str, matched_skills: &[String]) -> String {
    let base = profile.summary.trim();
    if base.is_empty() || mode != "enhance" {
        return base.to_string();
    }
    let additions: Vec<&str> = matched_skills.iter().take(3).map(String::as_str).collect();
    if additions.is_empty() {
        base.to_string()
    } else {
        format!(
            "{} Relevant strengths include {}.",
            sentence(base),
            human_join(&additions)
        )
    }
}

fn structural_diff(
    profile: &CareerProfile,
    headline: &str,
    summary: &str,
    skills: &[String],
    employment: &[EmploymentEntry],
    projects: &[ProjectEntry],
) -> Value {
    let mut diff = serde_json::Map::new();
    if profile.headline.trim() != headline.trim() {
        diff.insert(
            "headline".to_string(),
            json!({ "before": profile.headline, "after": headline }),
        );
    }
    if profile.summary.trim() != summary.trim() {
        diff.insert(
            "summary".to_string(),
            json!({ "before": profile.summary, "after": summary }),
        );
    }
    if profile.skills != skills {
        diff.insert(
            "skill_emphasis".to_string(),
            json!({ "before": profile.skills, "after": skills }),
        );
    }

    let experience_changes: Vec<Value> = profile
        .employment
        .iter()
        .zip(employment)
        .filter_map(|(before, after)| {
            if before.highlights.first() == after.highlights.first() {
                return None;
            }
            Some(json!({
                "role": employment_label(after),
                "previously_first": before.highlights.first(),
                "moved_to_top": after.highlights.first(),
            }))
        })
        .collect();
    if !experience_changes.is_empty() {
        diff.insert(
            "experience_emphasis".to_string(),
            Value::Array(experience_changes),
        );
    }

    let data_quality: Vec<Value> = profile
        .employment
        .iter()
        .zip(employment)
        .filter(|(before, after)| !before.location.is_empty() && after.location.is_empty())
        .map(|(_, after)| {
            json!({
                "role": employment_label(after),
                "change": "Omitted an invalid placeholder location from this resume version.",
            })
        })
        .collect();
    if !data_quality.is_empty() {
        diff.insert("data_quality".to_string(), Value::Array(data_quality));
    }

    let before_projects: Vec<&str> = profile
        .projects
        .iter()
        .map(|item| item.name.as_str())
        .collect();
    let after_projects: Vec<&str> = projects.iter().map(|item| item.name.as_str()).collect();
    if before_projects != after_projects {
        diff.insert(
            "project_emphasis".to_string(),
            json!({ "before": before_projects, "after": after_projects }),
        );
    }
    diff.insert(
        "evidence_policy".to_string(),
        json!("Existing profile facts only; Bluey moved the strongest evidence first."),
    );
    diff.insert("claims_added".to_string(), json!([]));
    Value::Object(diff)
}

fn employment_label(entry: &EmploymentEntry) -> String {
    match (entry.title.trim(), entry.company.trim()) {
        ("", "") => "Experience".to_string(),
        ("", company) => company.to_string(),
        (title, "") => title.to_string(),
        (title, company) => format!("{title} at {company}"),
    }
}

fn human_join(values: &[&str]) -> String {
    match values {
        [] => String::new(),
        [only] => (*only).to_string(),
        [left, right] => format!("{left} and {right}"),
        _ => format!(
            "{}, and {}",
            values[..values.len() - 1].join(", "),
            values[values.len() - 1]
        ),
    }
}

fn factual_headline(profile: &CareerProfile) -> String {
    let headline = profile.headline.trim();
    for entry in &profile.employment {
        let company = entry.company.trim();
        if company.is_empty() {
            continue;
        }
        for separator in [", ", " - ", " | "] {
            let prefix = format!("{company}{separator}");
            if let Some(role) = headline.strip_prefix(&prefix) {
                if !role.trim().is_empty() {
                    return role.trim().to_string();
                }
            }
        }
    }
    headline.to_string()
}

fn sentence(value: &str) -> String {
    let value = value.trim();
    if value.ends_with(['.', '!', '?']) {
        value.to_string()
    } else {
        format!("{value}.")
    }
}

fn is_placeholder_value(value: &str) -> bool {
    let normalized = normalize_phrase(value);
    if matches!(
        normalized.as_str(),
        "n a" | "none" | "unknown" | "test" | "placeholder" | "xxx"
    ) {
        return true;
    }
    let compact: Vec<char> = normalized
        .chars()
        .filter(|character| character.is_alphanumeric())
        .collect();
    compact.len() >= 3 && compact.iter().all(|character| *character == compact[0])
}

fn meaningful_tokens(value: &str) -> BTreeSet<String> {
    meaningful_token_list(value).into_iter().collect()
}

fn meaningful_token_counts(value: &str) -> BTreeMap<String, i64> {
    let mut counts = BTreeMap::new();
    for token in meaningful_token_list(value) {
        *counts.entry(token).or_default() += 1;
    }
    counts
}

fn meaningful_token_list(value: &str) -> Vec<String> {
    normalize_phrase(value)
        .split_whitespace()
        .filter(|token| token.len() > 1 && !STOP_WORDS.contains(token))
        .map(str::to_string)
        .collect()
}

fn normalize_phrase(value: &str) -> String {
    let mut normalized = String::new();
    let mut separator = true;
    for character in value.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            normalized.push(character);
            separator = false;
        } else if !separator && !normalized.is_empty() {
            normalized.push(' ');
            separator = true;
        }
    }
    normalized.trim().to_string()
}

fn contains_phrase(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    let padded_haystack = if haystack.starts_with(' ') && haystack.ends_with(' ') {
        haystack.to_string()
    } else {
        format!(" {haystack} ")
    };
    padded_haystack.contains(&format!(" {needle} "))
}

fn phrase_count(haystack: &str, needle: &str) -> i64 {
    if needle.is_empty() {
        return 0;
    }
    let haystack_tokens: Vec<&str> = haystack.split_whitespace().collect();
    let needle_tokens: Vec<&str> = needle.split_whitespace().collect();
    if needle_tokens.is_empty() || needle_tokens.len() > haystack_tokens.len() {
        return 0;
    }
    haystack_tokens
        .windows(needle_tokens.len())
        .filter(|window| *window == needle_tokens.as_slice())
        .count()
        .min(5) as i64
}

const STOP_WORDS: &[&str] = &[
    "a",
    "about",
    "all",
    "an",
    "and",
    "are",
    "as",
    "at",
    "be",
    "build",
    "by",
    "can",
    "company",
    "for",
    "from",
    "have",
    "in",
    "is",
    "it",
    "job",
    "of",
    "on",
    "or",
    "our",
    "role",
    "that",
    "the",
    "their",
    "this",
    "to",
    "using",
    "we",
    "will",
    "with",
    "work",
    "you",
    "your",
    "years",
    "experience",
    "required",
    "preferred",
    "responsibilities",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn posting(title: &str, description: &str) -> JobPosting {
        JobPosting {
            id: String::new(),
            canonical_key: String::new(),
            source: "fixture".to_string(),
            external_id: String::new(),
            title: title.to_string(),
            description: description.to_string(),
            company: "Example Labs".to_string(),
            location: "Remote".to_string(),
            workplace: "remote".to_string(),
            canonical_url: "https://example.test/jobs/1".to_string(),
            compensation: String::new(),
            employment_type: "full_time".to_string(),
            track_id: String::new(),
            match_score: 90,
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

    fn profile() -> CareerProfile {
        CareerProfile {
            headline: "Software Engineer".to_string(),
            summary: "Builds reliable products for customers.".to_string(),
            skills: vec![
                "Java".to_string(),
                "Amazon Web Services".to_string(),
                "PostgreSQL".to_string(),
                "React.js".to_string(),
            ],
            employment: vec![EmploymentEntry {
                company: "Northstar".to_string(),
                title: "Software Engineer".to_string(),
                highlights: vec![
                    "Built React interfaces for customer workflows.".to_string(),
                    "Designed AWS data services backed by Postgres.".to_string(),
                ],
                ..EmploymentEntry::default()
            }],
            projects: vec![
                ProjectEntry {
                    name: "Frontend console".to_string(),
                    summary: "React user interface".to_string(),
                    technologies: vec!["React.js".to_string()],
                    ..ProjectEntry::default()
                },
                ProjectEntry {
                    name: "Data platform".to_string(),
                    summary: "AWS data pipeline".to_string(),
                    technologies: vec!["Amazon Web Services".to_string()],
                    ..ProjectEntry::default()
                },
            ],
            ..CareerProfile::default()
        }
    }

    #[test]
    fn aliases_rank_existing_skills_without_creating_new_ones() {
        let profile = profile();
        let result = tailor_resume(
            &profile,
            &posting(
                "Cloud Engineer",
                "Build services with AWS, Postgres, and Kubernetes.",
            ),
            "factual",
        );
        assert_eq!(
            &result.skills[..2],
            &["Amazon Web Services".to_string(), "PostgreSQL".to_string()]
        );
        assert!(result
            .skills
            .iter()
            .all(|skill| profile.skills.contains(skill)));
    }

    #[test]
    fn different_jobs_move_different_factual_evidence_first() {
        let profile = profile();
        let cloud = tailor_resume(
            &profile,
            &posting("Cloud Engineer", "AWS Postgres data services"),
            "factual",
        );
        let frontend = tailor_resume(
            &profile,
            &posting("Frontend Engineer", "React user interfaces"),
            "factual",
        );
        assert!(cloud.employment[0].highlights[0].contains("AWS"));
        assert!(frontend.employment[0].highlights[0].contains("React"));
        assert_eq!(cloud.projects[0].name, "Data platform");
        assert_eq!(frontend.projects[0].name, "Frontend console");

        let mut original = profile.employment[0].highlights.clone();
        let mut tailored = cloud.employment[0].highlights.clone();
        original.sort();
        tailored.sort();
        assert_eq!(original, tailored);
    }

    #[test]
    fn enhance_uses_only_existing_skills_and_empty_fields_stay_empty() {
        let profile = profile();
        let result = tailor_resume(
            &profile,
            &posting("Cloud Engineer", "AWS and PostgreSQL"),
            "enhance",
        );
        assert!(result.headline.contains("Amazon Web Services"));
        assert!(result.summary.contains("Amazon Web Services"));
        assert!(!result.summary.contains("Example Labs"));

        let empty = tailor_resume(
            &CareerProfile::default(),
            &posting("Cloud Engineer", "AWS and PostgreSQL"),
            "enhance",
        );
        assert!(empty.headline.is_empty());
        assert!(empty.summary.is_empty());
    }

    #[test]
    fn enhance_keeps_sentences_and_removes_employer_prefix_from_headline() {
        let mut profile = profile();
        profile.headline = "Northstar, Software Engineer".to_string();
        profile.summary = "M.S. in Computer Science".to_string();
        let result = tailor_resume(
            &profile,
            &posting("Full Stack Engineer", "Java Java AWS React"),
            "enhance",
        );
        assert!(result.headline.starts_with("Software Engineer"));
        assert!(!result.headline.contains("Northstar"));
        assert!(result
            .summary
            .contains("Computer Science. Relevant strengths"));
    }

    #[test]
    fn repeated_job_terms_rank_relevant_existing_skills_first() {
        let mut profile = profile();
        profile.skills = vec![
            "Node.js".to_string(),
            "Python".to_string(),
            "Java".to_string(),
            "Amazon Web Services".to_string(),
        ];
        let result = tailor_resume(
            &profile,
            &posting("Full Stack Engineer", "Java Java Java AWS AWS Node.js"),
            "factual",
        );
        assert_eq!(result.skills[0], "Java");
        assert_eq!(result.skills[1], "Amazon Web Services");
    }

    #[test]
    fn diff_only_claims_a_highlight_moved_when_the_first_item_changed() {
        let profile = profile();
        let result = tailor_resume(
            &profile,
            &posting("Frontend Engineer", "React interfaces AWS"),
            "factual",
        );
        assert!(result.diff.get("experience_emphasis").is_none());
    }

    #[test]
    fn placeholder_employment_locations_are_omitted_and_disclosed() {
        let mut profile = profile();
        profile.employment[0].location = "aaaa".to_string();
        let result = tailor_resume(
            &profile,
            &posting("Cloud Engineer", "AWS Postgres"),
            "factual",
        );
        assert!(result.employment[0].location.is_empty());
        assert!(result.diff.get("data_quality").is_some());
    }
}

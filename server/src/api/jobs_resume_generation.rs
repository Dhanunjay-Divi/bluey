//! Evidence-grounded model planning for one job-specific resume version.
//!
//! The model may rank and rewrite verified profile evidence, but it cannot add
//! skills, employers, dates, metrics, or other candidate facts. A strict
//! validator rejects unsupported output and falls back to the deterministic
//! tailoring path. Provider provenance and Bluey's upstream cost stay in the
//! server-side generation ledger; the public packet receives only the truth
//! policy and generation kind.

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, time::Duration};

use super::AppState;
use crate::{
    db::{
        jobs::{self, CareerProfile, JobPosting, ResumeVersion},
        jobs_generation::{self, ResumeGenerationReservation},
        usage::{self, UsageEvent},
    },
    pricing, routing,
};

const GENERATION_SCHEMA_VERSION: i64 = 1;
const MAX_RESUME_SKILLS: usize = 16;
const MAX_HEADLINE_CHARS: usize = 180;
const MAX_SUMMARY_CHARS: usize = 700;
const MODEL_TIMEOUT_SECS: u64 = 75;

#[derive(Debug, Clone)]
pub struct GeneratedResume {
    pub content: Value,
    pub diff: Value,
    pub public_provenance: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ResumePlan {
    headline: String,
    summary: String,
    #[serde(default)]
    headline_evidence_ids: Vec<String>,
    #[serde(default)]
    summary_evidence_ids: Vec<String>,
    skill_order: Vec<String>,
    employment_order: Vec<usize>,
    employment_highlight_order: Vec<HighlightOrder>,
    project_order: Vec<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct HighlightOrder {
    entry_index: usize,
    highlight_indices: Vec<usize>,
}

#[derive(Debug, Clone)]
struct EvidenceCatalog {
    values: BTreeMap<String, String>,
}

impl EvidenceCatalog {
    fn from_profile(profile: &CareerProfile) -> Self {
        let mut values = BTreeMap::new();
        insert_evidence(&mut values, "profile:headline", &profile.headline);
        insert_evidence(&mut values, "profile:summary", &profile.summary);
        for (index, skill) in profile.skills.iter().enumerate() {
            insert_evidence(&mut values, &format!("skill:{index}"), skill);
        }
        for (index, certification) in profile.certifications.iter().enumerate() {
            insert_evidence(
                &mut values,
                &format!("certification:{index}"),
                certification,
            );
        }
        for (entry_index, entry) in profile.employment.iter().enumerate() {
            insert_evidence(
                &mut values,
                &format!("employment:{entry_index}:title"),
                &entry.title,
            );
            insert_evidence(
                &mut values,
                &format!("employment:{entry_index}:company"),
                &entry.company,
            );
            for (highlight_index, highlight) in entry.highlights.iter().enumerate() {
                insert_evidence(
                    &mut values,
                    &format!("employment:{entry_index}:highlight:{highlight_index}"),
                    highlight,
                );
            }
        }
        for (project_index, project) in profile.projects.iter().enumerate() {
            insert_evidence(
                &mut values,
                &format!("project:{project_index}:name"),
                &project.name,
            );
            insert_evidence(
                &mut values,
                &format!("project:{project_index}:role"),
                &project.role,
            );
            insert_evidence(
                &mut values,
                &format!("project:{project_index}:summary"),
                &project.summary,
            );
        }
        Self { values }
    }

    fn selected_text(&self, ids: &[String]) -> Result<String> {
        let mut selected = Vec::with_capacity(ids.len());
        for id in ids {
            let value = self
                .values
                .get(id)
                .ok_or_else(|| anyhow!("unknown evidence id: {id}"))?;
            selected.push(value.as_str());
        }
        Ok(selected.join(" "))
    }
}

fn insert_evidence(values: &mut BTreeMap<String, String>, id: &str, value: &str) {
    let value = value.trim();
    if !value.is_empty() {
        values.insert(id.to_string(), value.to_string());
    }
}

pub async fn generate(
    state: &AppState,
    account_id: &str,
    profile: &CareerProfile,
    posting: &JobPosting,
    baseline: &ResumeVersion,
) -> Result<GeneratedResume> {
    let generation_key = generation_key(account_id, profile, posting, baseline)?;
    match jobs_generation::reserve(&state.pool, account_id, &posting.id, &generation_key)? {
        ResumeGenerationReservation::Ready(record) => {
            if record.provider.as_deref() == Some("bluey") {
                return deterministic_fallback(baseline);
            }
            let plan: ResumePlan = serde_json::from_value(
                record
                    .output
                    .ok_or_else(|| anyhow!("completed generation has no output"))?,
            )
            .context("decode cached resume plan")?;
            materialize(profile, baseline, &plan, "model")
        }
        ResumeGenerationReservation::Pending => {
            Err(anyhow!("resume generation is already in progress"))
        }
        ResumeGenerationReservation::Start(record) => {
            generate_reserved(
                state,
                account_id,
                profile,
                posting,
                baseline,
                &generation_key,
                &record.reservation_token,
            )
            .await
        }
    }
}

async fn generate_reserved(
    state: &AppState,
    account_id: &str,
    profile: &CareerProfile,
    posting: &JobPosting,
    baseline: &ResumeVersion,
    generation_key: &str,
    reservation_token: &str,
) -> Result<GeneratedResume> {
    let catalog = EvidenceCatalog::from_profile(profile);
    let system = system_prompt();
    let user = user_prompt(profile, posting, &catalog)?;
    let estimated_input_tokens = ((system.len() + user.len()) as i64 / 4).max(1);
    let started = std::time::Instant::now();

    if model_generation_enabled() {
        for (provider, model) in routing::resolve_route_candidates_with_seed("deep", generation_key)
        {
            if state
                .config
                .upstream
                .key_candidates(provider, generation_key)
                .is_empty()
            {
                continue;
            }
            if provider_capacity(state, provider, generation_key)
                .await
                .is_err()
            {
                continue;
            }
            let attempt = tokio::time::timeout(
                Duration::from_secs(MODEL_TIMEOUT_SECS),
                routing::complete(
                    &state.config.upstream,
                    provider,
                    model,
                    system,
                    &user,
                    Some(1_800),
                    Some(0.1),
                    routing::ThinkingBudget::off(),
                    Some(estimated_input_tokens),
                    &[],
                ),
            )
            .await;
            let completion = match attempt {
                Ok(Ok(completion)) => completion,
                Ok(Err(error)) => {
                    tracing::warn!(provider, model, error = %error, "Jobs resume generation route failed");
                    continue;
                }
                Err(_) => {
                    tracing::warn!(provider, model, "Jobs resume generation route timed out");
                    continue;
                }
            };
            let plan = match parse_plan(&completion.text)
                .and_then(|plan| validate_plan(profile, &catalog, plan))
            {
                Ok(plan) => plan,
                Err(error) => {
                    tracing::warn!(provider, model, error = %error, "Jobs resume generation output rejected");
                    continue;
                }
            };
            let bluey_cost = pricing::lookup(&completion.provider, &completion.model)
                .map(|price| {
                    pricing::compute_cost(price, completion.input_tokens, completion.output_tokens)
                        .0
                })
                .unwrap_or(0);
            let output = serde_json::to_value(&plan)?;
            jobs_generation::complete(
                &state.pool,
                account_id,
                generation_key,
                reservation_token,
                &output,
                &completion.provider,
                &completion.model,
                completion.input_tokens,
                completion.output_tokens,
                bluey_cost,
            )?;
            let _ = usage::record(
                &state.pool,
                account_id,
                &UsageEvent {
                    request_id: format!("jobs-resume-{generation_key}"),
                    kind: "jobs_resume_generation".to_string(),
                    task_type: Some("resume_tailoring".to_string()),
                    lane: Some("deep".to_string()),
                    provider: Some(completion.provider),
                    model: Some(completion.model),
                    input_tokens: completion.input_tokens,
                    output_tokens: completion.output_tokens,
                    latency_ms: started.elapsed().as_millis().try_into().unwrap_or(i64::MAX),
                    cost_cents_to_bluey: bluey_cost,
                    cost_cents_to_customer: 0,
                    was_speculative: false,
                    was_fallback: false,
                },
            );
            return materialize(profile, baseline, &plan, "model");
        }
    }

    let output = json!({"fallback": "deterministic_baseline"});
    jobs_generation::complete(
        &state.pool,
        account_id,
        generation_key,
        reservation_token,
        &output,
        "bluey",
        "deterministic-fallback-v1",
        0,
        0,
        0,
    )?;
    deterministic_fallback(baseline)
}

async fn provider_capacity(
    state: &AppState,
    provider: &str,
    generation_key: &str,
) -> std::result::Result<(), u64> {
    let limiter = match provider {
        "openai" => &state.rate_limiters.provider_openai_llm,
        "anthropic" => &state.rate_limiters.provider_anthropic_llm,
        "gemini" => &state.rate_limiters.provider_gemini_llm,
        "deepseek" => &state.rate_limiters.provider_deepseek_llm,
        "zai" => &state.rate_limiters.provider_zai_llm,
        _ => return Err(60),
    };
    limiter.check(generation_key).await
}

fn model_generation_enabled() -> bool {
    std::env::var("BLUEY_JOBS_MODEL_GENERATION_ENABLED")
        .map(|value| {
            !matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "0" | "false" | "off"
            )
        })
        .unwrap_or(true)
}

fn generation_key(
    account_id: &str,
    profile: &CareerProfile,
    posting: &JobPosting,
    baseline: &ResumeVersion,
) -> Result<String> {
    let input = json!({
        "schema_version": GENERATION_SCHEMA_VERSION,
        "account_id": account_id,
        "profile": profile,
        "posting": posting,
        "baseline_checksum": baseline.checksum,
        "mode": baseline.mode,
    });
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(&input)?)))
}

fn system_prompt() -> &'static str {
    r#"You create one job-specific resume plan from verified candidate evidence.
Return JSON only. Never invent or infer a skill, employer, title, degree, date, metric, certification, authorization fact, or location. Never copy candidate requirements from the job description into the candidate's history. Use exact skill strings and exact evidence IDs from the input.

The output schema is:
{
  "headline": "concise evidence-grounded headline",
  "summary": "concise evidence-grounded summary",
  "headline_evidence_ids": ["evidence:id"],
  "summary_evidence_ids": ["evidence:id"],
  "skill_order": ["exact candidate skill"],
  "employment_order": [0],
  "employment_highlight_order": [{"entry_index":0,"highlight_indices":[0]}],
  "project_order": [0]
}

Include every employment and project index exactly once. Include every highlight index exactly once for every employment entry. Choose at most 16 skills. The headline and summary may rewrite selected evidence for clarity, but every meaningful word and every number must come from the selected evidence. Prefer the evidence that best answers the job description."#
}

fn user_prompt(
    profile: &CareerProfile,
    posting: &JobPosting,
    catalog: &EvidenceCatalog,
) -> Result<String> {
    let input = json!({
        "job": {
            "company": posting.company,
            "title": posting.title,
            "location": posting.location,
            "description": posting.description,
            "employment_type": posting.employment_type,
        },
        "candidate": {
            "skills": profile.skills,
            "employment": profile.employment,
            "projects": profile.projects,
            "certifications": profile.certifications,
        },
        "evidence_catalog": catalog.values,
    });
    serde_json::to_string(&input).context("encode resume generation prompt")
}

fn parse_plan(raw: &str) -> Result<ResumePlan> {
    let trimmed = raw.trim();
    let json = if trimmed.starts_with("```") {
        let without_open = trimmed
            .strip_prefix("```json")
            .or_else(|| trimmed.strip_prefix("```JSON"))
            .or_else(|| trimmed.strip_prefix("```"))
            .unwrap_or(trimmed);
        without_open
            .strip_suffix("```")
            .unwrap_or(without_open)
            .trim()
    } else {
        trimmed
    };
    serde_json::from_str(json).context("decode model resume plan")
}

fn validate_plan(
    profile: &CareerProfile,
    catalog: &EvidenceCatalog,
    plan: ResumePlan,
) -> Result<ResumePlan> {
    if plan.headline.chars().count() > MAX_HEADLINE_CHARS {
        return Err(anyhow!("headline is too long"));
    }
    if plan.summary.chars().count() > MAX_SUMMARY_CHARS {
        return Err(anyhow!("summary is too long"));
    }
    validate_permutation(
        &plan.employment_order,
        profile.employment.len(),
        "employment",
    )?;
    validate_permutation(&plan.project_order, profile.projects.len(), "project")?;
    if plan.employment_highlight_order.len() != profile.employment.len() {
        return Err(anyhow!("highlight plans must cover every employment entry"));
    }
    let mut highlight_entries = Vec::with_capacity(plan.employment_highlight_order.len());
    for order in &plan.employment_highlight_order {
        if order.entry_index >= profile.employment.len() {
            return Err(anyhow!("employment highlight entry is out of range"));
        }
        highlight_entries.push(order.entry_index);
        validate_permutation(
            &order.highlight_indices,
            profile.employment[order.entry_index].highlights.len(),
            "employment highlight",
        )?;
    }
    validate_permutation(
        &highlight_entries,
        profile.employment.len(),
        "employment highlight entry",
    )?;
    if plan.skill_order.len() > MAX_RESUME_SKILLS {
        return Err(anyhow!("too many selected skills"));
    }
    let mut seen_skills = Vec::new();
    for skill in &plan.skill_order {
        if !profile.skills.iter().any(|candidate| candidate == skill) {
            return Err(anyhow!("unknown candidate skill: {skill}"));
        }
        if seen_skills.contains(&skill) {
            return Err(anyhow!("duplicate candidate skill: {skill}"));
        }
        seen_skills.push(skill);
    }
    validate_narrative(
        &plan.headline,
        &catalog.selected_text(&plan.headline_evidence_ids)?,
        "headline",
    )?;
    validate_narrative(
        &plan.summary,
        &catalog.selected_text(&plan.summary_evidence_ids)?,
        "summary",
    )?;
    Ok(plan)
}

fn validate_permutation(values: &[usize], expected_len: usize, label: &str) -> Result<()> {
    if values.len() != expected_len {
        return Err(anyhow!("{label} order must include every item"));
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    if sorted != (0..expected_len).collect::<Vec<_>>() {
        return Err(anyhow!(
            "{label} order has duplicates or out-of-range items"
        ));
    }
    Ok(())
}

fn validate_narrative(value: &str, evidence: &str, label: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Ok(());
    }
    if evidence.trim().is_empty() {
        return Err(anyhow!("{label} has no selected evidence"));
    }
    let evidence_tokens = normalized_tokens(evidence);
    for token in normalized_tokens(value) {
        if is_stop_word(&token) || token.len() <= 3 {
            continue;
        }
        if !evidence_tokens
            .iter()
            .any(|candidate| token_matches(&token, candidate))
        {
            return Err(anyhow!("{label} contains unsupported word: {token}"));
        }
    }
    let evidence_numbers: Vec<String> = evidence_tokens
        .iter()
        .filter(|token| token.chars().any(|character| character.is_ascii_digit()))
        .cloned()
        .collect();
    for token in normalized_tokens(value)
        .into_iter()
        .filter(|token| token.chars().any(|character| character.is_ascii_digit()))
    {
        if !evidence_numbers.contains(&token) {
            return Err(anyhow!("{label} contains an unsupported number"));
        }
    }
    Ok(())
}

fn normalized_tokens(value: &str) -> Vec<String> {
    let mut normalized = String::with_capacity(value.len());
    for character in value.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            normalized.push(character);
        } else {
            normalized.push(' ');
        }
    }
    normalized.split_whitespace().map(str::to_string).collect()
}

fn token_matches(left: &str, right: &str) -> bool {
    left == right || stem(left) == stem(right)
}

fn stem(value: &str) -> &str {
    for suffix in ["ing", "ed", "es", "s"] {
        if value.len() > suffix.len() + 4 && value.ends_with(suffix) {
            return &value[..value.len() - suffix.len()];
        }
    }
    value
}

fn is_stop_word(value: &str) -> bool {
    matches!(
        value,
        "a" | "an"
            | "and"
            | "as"
            | "at"
            | "by"
            | "for"
            | "from"
            | "in"
            | "into"
            | "of"
            | "on"
            | "or"
            | "the"
            | "to"
            | "with"
            | "across"
            | "using"
            | "through"
            | "who"
            | "that"
            | "this"
            | "their"
            | "its"
            | "is"
            | "are"
            | "was"
            | "were"
    )
}

fn deterministic_fallback(baseline: &ResumeVersion) -> Result<GeneratedResume> {
    let mut content = baseline.content.clone();
    let public_provenance = json!({
        "kind": "deterministic_fallback",
        "schema_version": GENERATION_SCHEMA_VERSION,
        "truth_guard": "deterministic",
        "claims_added": 0,
    });
    content["provenance"]["resume_generation"] = public_provenance.clone();

    let mut diff = baseline.diff.clone();
    let diff_object = diff
        .as_object_mut()
        .ok_or_else(|| anyhow!("deterministic resume diff must be an object"))?;
    diff_object.insert(
        "evidence_policy".to_string(),
        json!("Verified profile facts only; Bluey used deterministic relevance ranking because model generation was unavailable."),
    );
    diff_object.insert("claims_added".to_string(), json!([]));

    Ok(GeneratedResume {
        content,
        diff,
        public_provenance,
    })
}

fn materialize(
    profile: &CareerProfile,
    baseline: &ResumeVersion,
    plan: &ResumePlan,
    kind: &str,
) -> Result<GeneratedResume> {
    if kind == "model" {
        validate_plan(
            profile,
            &EvidenceCatalog::from_profile(profile),
            plan.clone(),
        )?;
    }
    let mut content = baseline.content.clone();
    content["headline"] = json!(plan.headline);
    content["summary"] = json!(plan.summary);
    content["skills"] = json!(plan.skill_order);

    let highlight_orders: BTreeMap<usize, &HighlightOrder> = plan
        .employment_highlight_order
        .iter()
        .map(|order| (order.entry_index, order))
        .collect();
    let mut employment = Vec::with_capacity(profile.employment.len());
    for entry_index in &plan.employment_order {
        let mut entry = profile.employment[*entry_index].clone();
        let order = highlight_orders
            .get(entry_index)
            .ok_or_else(|| anyhow!("missing employment highlight order"))?;
        entry.highlights = order
            .highlight_indices
            .iter()
            .map(|index| entry.highlights[*index].clone())
            .collect();
        employment.push(entry);
    }
    content["employment"] = serde_json::to_value(&employment)?;
    let projects = plan
        .project_order
        .iter()
        .map(|index| profile.projects[*index].clone())
        .collect::<Vec<_>>();
    content["projects"] = serde_json::to_value(&projects)?;

    let public_provenance = json!({
        "kind": kind,
        "schema_version": GENERATION_SCHEMA_VERSION,
        "truth_guard": if kind == "model" { "passed" } else { "deterministic" },
        "claims_added": 0,
    });
    content["provenance"]["resume_generation"] = public_provenance.clone();
    let diff = build_diff(profile, plan, &employment, &projects, kind);
    Ok(GeneratedResume {
        content,
        diff,
        public_provenance,
    })
}

fn build_diff(
    profile: &CareerProfile,
    plan: &ResumePlan,
    employment: &[jobs::EmploymentEntry],
    projects: &[jobs::ProjectEntry],
    kind: &str,
) -> Value {
    let mut diff = serde_json::Map::new();
    if profile.headline.trim() != plan.headline.trim() {
        diff.insert(
            "headline".to_string(),
            json!({"before": profile.headline, "after": plan.headline}),
        );
    }
    if profile.summary.trim() != plan.summary.trim() {
        diff.insert(
            "summary".to_string(),
            json!({"before": profile.summary, "after": plan.summary}),
        );
    }
    if profile.skills != plan.skill_order {
        diff.insert(
            "skill_emphasis".to_string(),
            json!({"before": profile.skills, "after": plan.skill_order}),
        );
    }
    let experience_changes = profile
        .employment
        .iter()
        .filter_map(|before| {
            employment
                .iter()
                .find(|after| after.id == before.id)
                .filter(|after| after.highlights.first() != before.highlights.first())
                .map(|after| {
                    json!({
                        "role": if after.company.trim().is_empty() { after.title.clone() } else { format!("{} at {}", after.title, after.company) },
                        "previously_first": before.highlights.first(),
                        "moved_to_top": after.highlights.first(),
                    })
                })
        })
        .collect::<Vec<_>>();
    if !experience_changes.is_empty() {
        diff.insert("experience_emphasis".to_string(), json!(experience_changes));
    }
    let before_projects = profile
        .projects
        .iter()
        .map(|project| project.name.as_str())
        .collect::<Vec<_>>();
    let after_projects = projects
        .iter()
        .map(|project| project.name.as_str())
        .collect::<Vec<_>>();
    if before_projects != after_projects {
        diff.insert(
            "project_emphasis".to_string(),
            json!({"before": before_projects, "after": after_projects}),
        );
    }
    diff.insert(
        "evidence_policy".to_string(),
        json!(if kind == "model" {
            "A managed model ranked and rewrote verified profile evidence. No new factual claims were added."
        } else {
            "Verified profile facts only; Bluey used deterministic relevance ranking because model generation was unavailable."
        }),
    );
    diff.insert("claims_added".to_string(), json!([]));
    Value::Object(diff)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::jobs::{EmploymentEntry, ProjectEntry};

    fn profile() -> CareerProfile {
        CareerProfile {
            headline: "Software Engineer".into(),
            summary: "Built reliable distributed systems for healthcare teams.".into(),
            skills: vec!["Rust".into(), "PostgreSQL".into(), "React".into()],
            employment: vec![EmploymentEntry {
                id: "work-1".into(),
                company: "Example Health".into(),
                title: "Software Engineer".into(),
                highlights: vec![
                    "Built reliable distributed systems.".into(),
                    "Reduced deployment time by 30 percent.".into(),
                ],
                ..Default::default()
            }],
            projects: vec![ProjectEntry {
                id: "project-1".into(),
                name: "Care Platform".into(),
                summary: "Created healthcare workflow software.".into(),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    fn valid_plan() -> ResumePlan {
        ResumePlan {
            headline: "Software Engineer".into(),
            summary: "Built reliable distributed systems for healthcare teams.".into(),
            headline_evidence_ids: vec!["profile:headline".into()],
            summary_evidence_ids: vec!["profile:summary".into()],
            skill_order: vec!["Rust".into(), "PostgreSQL".into()],
            employment_order: vec![0],
            employment_highlight_order: vec![HighlightOrder {
                entry_index: 0,
                highlight_indices: vec![1, 0],
            }],
            project_order: vec![0],
        }
    }

    #[test]
    fn parses_fenced_json() {
        let raw = format!(
            "```json\n{}\n```",
            serde_json::to_string(&valid_plan()).unwrap()
        );
        assert_eq!(parse_plan(&raw).unwrap(), valid_plan());
    }

    #[test]
    fn rejects_unknown_skill_and_new_number() {
        let profile = profile();
        let catalog = EvidenceCatalog::from_profile(&profile);
        let mut unknown_skill = valid_plan();
        unknown_skill.skill_order.push("Kubernetes".into());
        assert!(validate_plan(&profile, &catalog, unknown_skill).is_err());

        let mut invented_metric = valid_plan();
        invented_metric.summary = "Built reliable systems with 99 percent uptime.".into();
        assert!(validate_plan(&profile, &catalog, invented_metric).is_err());
    }

    #[test]
    fn rejects_duplicate_or_missing_indexes() {
        let profile = profile();
        let catalog = EvidenceCatalog::from_profile(&profile);
        let mut invalid = valid_plan();
        invalid.employment_highlight_order[0].highlight_indices = vec![0, 0];
        assert!(validate_plan(&profile, &catalog, invalid).is_err());
    }

    #[test]
    fn materialized_resume_reorders_only_verified_evidence() {
        let profile = profile();
        let baseline = ResumeVersion {
            id: "resume-1".into(),
            job_id: "job-1".into(),
            version_no: 1,
            mode: "factual".into(),
            content: json!({"provenance": {}}),
            diff: json!({}),
            claim_ids: Vec::new(),
            checksum: "checksum".into(),
            created_at_ms: 1,
        };
        let generated = materialize(&profile, &baseline, &valid_plan(), "model").unwrap();
        assert_eq!(
            generated.content["employment"][0]["highlights"][0],
            "Reduced deployment time by 30 percent."
        );
        assert_eq!(
            generated.content["provenance"]["resume_generation"]["kind"],
            "model"
        );
        assert_eq!(generated.diff["claims_added"], json!([]));
    }

    #[test]
    fn deterministic_fallback_preserves_the_exact_baseline_order_and_diff() {
        let baseline = ResumeVersion {
            id: String::new(),
            job_id: "job-1".into(),
            version_no: 0,
            mode: "factual".into(),
            content: json!({
                "headline": "Software Engineer",
                "employment": [{"id": "work-2"}, {"id": "work-1"}],
                "provenance": {"candidate_truth_fingerprint": "truth"},
            }),
            diff: json!({"experience_emphasis": [{"moved_to_top": "Verified result"}]}),
            claim_ids: vec!["fact-1".into()],
            checksum: "checksum".into(),
            created_at_ms: 1,
        };
        let generated = deterministic_fallback(&baseline).unwrap();
        assert_eq!(
            generated.content["employment"],
            baseline.content["employment"]
        );
        assert_eq!(
            generated.diff["experience_emphasis"],
            baseline.diff["experience_emphasis"]
        );
        assert_eq!(
            generated.content["provenance"]["resume_generation"]["kind"],
            "deterministic_fallback"
        );
    }
}

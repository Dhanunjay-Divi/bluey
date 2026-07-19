//! Evidence-grounded model planning for one job-specific resume version.
//!
//! The model may rank verified profile evidence, but it never authors resume
//! prose. Headline and summary text are composed deterministically from exact
//! evidence records selected by ID, so short credentials and metrics cannot be
//! invented or reassigned across facts. Provider provenance and Bluey's
//! upstream cost stay in the server-side generation ledger; the public packet
//! receives only the truth policy and generation kind.

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
        jobs_generation_allowance::{self, AllowanceReservation},
        jobs_provider_cost_holds::{self, CostHoldReservation},
        usage::UsageEvent,
    },
    pricing, routing,
};

const GENERATION_SCHEMA_VERSION: i64 = 3;
const MAX_RESUME_SKILLS: usize = 16;
const MAX_HEADLINE_CHARS: usize = 180;
const MAX_SUMMARY_CHARS: usize = 700;
const MAX_MODEL_OUTPUT_TOKENS: u32 = 1_200;
const MAX_CANDIDATE_PROMPT_BYTES: usize = 48 * 1024;
const MAX_JOB_PROMPT_BYTES: usize = 32 * 1024;
const MAX_USER_PROMPT_BYTES: usize = 96 * 1024;
const MAX_PROVIDER_ATTEMPTS: usize = 3;
const MAX_ATTEMPT_BLUEY_COST_CENTS: i64 = 10;
const MAX_GENERATION_BLUEY_COST_CENTS: i64 = 20;
const MODEL_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(22);
const MODEL_GENERATION_DEADLINE: Duration = Duration::from_secs(75);

const _: () = assert!(
    MODEL_GENERATION_DEADLINE.as_secs() + 30 < jobs_generation::RESERVATION_TTL.as_secs(),
    "model generation needs a settlement margin below its reservation TTL"
);

#[derive(Debug, Clone)]
pub struct GeneratedResume {
    pub content: Value,
    pub diff: Value,
    pub public_provenance: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ResumePlan {
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
#[serde(deny_unknown_fields)]
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
        let mut seen = Vec::with_capacity(ids.len());
        for id in ids {
            if seen.contains(id) {
                return Err(anyhow!("duplicate evidence id: {id}"));
            }
            let value = self
                .values
                .get(id)
                .ok_or_else(|| anyhow!("unknown evidence id: {id}"))?;
            selected.push(value.as_str());
            seen.push(id.clone());
        }
        Ok(selected.join(" • "))
    }

    fn compose_headline(&self, ids: &[String]) -> Result<String> {
        if ids.len() > 1 {
            return Err(anyhow!(
                "headline must select exactly one indivisible evidence record"
            ));
        }
        if ids.iter().any(|id| !headline_evidence_id(id)) {
            return Err(anyhow!("headline selected non-title evidence"));
        }
        let value = self.selected_text(ids)?;
        if value.chars().count() > MAX_HEADLINE_CHARS {
            return Err(anyhow!("selected headline evidence is too long"));
        }
        Ok(value)
    }

    fn compose_summary(&self, ids: &[String]) -> Result<String> {
        if ids.iter().any(|id| !summary_evidence_id(id)) {
            return Err(anyhow!("summary selected non-narrative evidence"));
        }
        let value = self.selected_text(ids)?;
        if value.chars().count() > MAX_SUMMARY_CHARS {
            return Err(anyhow!("selected summary evidence is too long"));
        }
        Ok(value)
    }
}

fn headline_evidence_id(id: &str) -> bool {
    id == "profile:headline"
        || (id.starts_with("employment:") && id.ends_with(":title"))
        || (id.starts_with("project:") && id.ends_with(":role"))
}

fn summary_evidence_id(id: &str) -> bool {
    id == "profile:summary"
        || id.starts_with("certification:")
        || (id.starts_with("employment:") && id.contains(":highlight:"))
        || (id.starts_with("project:") && id.ends_with(":summary"))
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
    let managed_generation = model_generation_ready(state);
    let generation_key =
        generation_key(account_id, profile, posting, baseline, managed_generation)?;
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
                managed_generation,
            )
            .await
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn generate_reserved(
    state: &AppState,
    account_id: &str,
    profile: &CareerProfile,
    posting: &JobPosting,
    baseline: &ResumeVersion,
    generation_key: &str,
    reservation_token: &str,
    managed_generation: bool,
) -> Result<GeneratedResume> {
    let mut reservation = GenerationReservationGuard::new(
        state.pool.clone(),
        account_id,
        &posting.id,
        generation_key,
        reservation_token,
    );
    let catalog = EvidenceCatalog::from_profile(profile);
    let system = system_prompt();
    let user = match user_prompt(profile, posting, &catalog) {
        Ok(user) => user,
        Err(error) => {
            tracing::warn!(
                account_id_hash = %cue_core::account_id_hash_prefix(account_id),
                generation_ref = %generation_log_ref(generation_key),
                error = %error,
                "Jobs resume model prompt rejected at the size boundary"
            );
            return finish_deterministic_fallback(
                state,
                account_id,
                baseline,
                generation_key,
                reservation_token,
                &mut reservation,
            );
        }
    };
    let estimated_input_tokens = pricing::utf8_input_token_upper_bound([system, user.as_str()]);
    if managed_generation {
        match jobs_generation_allowance::reserve(
            &state.pool,
            account_id,
            &posting.id,
            generation_key,
            reservation_token,
        )? {
            AllowanceReservation::Reserved => reservation.arm_allowance(),
            AllowanceReservation::AlreadyMetered
            | AllowanceReservation::Busy
            | AllowanceReservation::Exhausted => {
                tracing::warn!(
                    account_id_hash = %cue_core::account_id_hash_prefix(account_id),
                    generation_ref = %generation_log_ref(generation_key),
                    "Jobs resume model generation has no available packet allowance"
                );
                return finish_transient_deterministic_fallback(baseline, &mut reservation);
            }
        }
        let generation = tokio::time::timeout(
            MODEL_GENERATION_DEADLINE,
            try_model_generation(
                state,
                account_id,
                profile,
                &catalog,
                system,
                &user,
                generation_key,
                reservation_token,
                estimated_input_tokens,
            ),
        )
        .await;
        match generation {
            Ok(Ok(Some(accounted))) => {
                let output = serde_json::to_value(&accounted.plan)?;
                jobs_generation::complete(
                    &state.pool,
                    account_id,
                    generation_key,
                    reservation_token,
                    &output,
                    &accounted.completion.provider,
                    &accounted.completion.model,
                    accounted.completion.input_tokens,
                    accounted.completion.output_tokens,
                    accounted.bluey_cost,
                )?;
                reservation.disarm();
                return materialize(profile, baseline, &accounted.plan, "model");
            }
            Ok(Ok(None)) => {}
            Ok(Err(error)) => {
                reservation.fail("accounting_or_generation_failed")?;
                return Err(error);
            }
            Err(_) => tracing::warn!(
                account_id_hash = %cue_core::account_id_hash_prefix(account_id),
                generation_ref = %generation_log_ref(generation_key),
                deadline_ms = MODEL_GENERATION_DEADLINE.as_millis(),
                "Jobs resume model generation reached its overall deadline"
            ),
        }
    }

    if managed_generation {
        finish_transient_deterministic_fallback(baseline, &mut reservation)
    } else {
        finish_deterministic_fallback(
            state,
            account_id,
            baseline,
            generation_key,
            reservation_token,
            &mut reservation,
        )
    }
}

fn model_generation_enabled() -> bool {
    model_generation_enabled_value(
        std::env::var("BLUEY_JOBS_MODEL_GENERATION_ENABLED")
            .ok()
            .as_deref(),
    )
}

fn model_generation_enabled_value(value: Option<&str>) -> bool {
    value.is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "on" | "yes"
        )
    })
}

fn model_generation_ready(state: &AppState) -> bool {
    model_generation_prerequisites_ready(
        model_generation_enabled(),
        state.rate_limiters.account_llm.is_some(),
        state.config.upstream_spend_guard.is_some(),
    )
}

fn model_generation_prerequisites_ready(
    explicitly_enabled: bool,
    account_limiter_configured: bool,
    spend_guard_configured: bool,
) -> bool {
    explicitly_enabled && account_limiter_configured && spend_guard_configured
}

struct AccountedGeneration {
    plan: ResumePlan,
    completion: routing::Completion,
    bluey_cost: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AttemptOutcome {
    Accepted,
    RejectedTruth,
    RejectedCost,
    RejectedDeadline,
    RejectedProviderBoundary,
    RejectedProviderError,
    RejectedTimeout,
    RejectedCancelled,
}

impl AttemptOutcome {
    fn task_type(self) -> &'static str {
        match self {
            Self::Accepted => "jobs_resume_tailoring_accepted",
            Self::RejectedTruth => "jobs_resume_tailoring_rejected_truth",
            Self::RejectedCost => "jobs_resume_tailoring_rejected_cost",
            Self::RejectedDeadline => "jobs_resume_tailoring_rejected_deadline",
            Self::RejectedProviderBoundary => "jobs_resume_tailoring_rejected_provider_boundary",
            Self::RejectedProviderError => "jobs_resume_tailoring_rejected_provider_error",
            Self::RejectedTimeout => "jobs_resume_tailoring_rejected_timeout",
            Self::RejectedCancelled => "jobs_resume_tailoring_rejected_cancelled",
        }
    }
}

struct GenerationReservationGuard {
    pool: crate::db::DbPool,
    account_id: String,
    job_id: String,
    generation_key: String,
    reservation_token: String,
    allowance_armed: bool,
    armed: bool,
}

impl GenerationReservationGuard {
    fn new(
        pool: crate::db::DbPool,
        account_id: &str,
        job_id: &str,
        generation_key: &str,
        reservation_token: &str,
    ) -> Self {
        Self {
            pool,
            account_id: account_id.to_string(),
            job_id: job_id.to_string(),
            generation_key: generation_key.to_string(),
            reservation_token: reservation_token.to_string(),
            allowance_armed: false,
            armed: true,
        }
    }

    fn arm_allowance(&mut self) {
        self.allowance_armed = true;
    }

    fn disarm(&mut self) {
        self.armed = false;
        self.allowance_armed = false;
    }

    fn fail(&mut self, code: &str) -> Result<()> {
        let generation_result = if self.armed {
            jobs_generation::fail(
                &self.pool,
                &self.account_id,
                &self.generation_key,
                &self.reservation_token,
                code,
            )
        } else {
            Ok(())
        };
        if generation_result.is_ok() {
            self.armed = false;
        }
        let allowance_result = if self.allowance_armed {
            jobs_generation_allowance::release(
                &self.pool,
                &self.account_id,
                &self.job_id,
                &self.generation_key,
                &self.reservation_token,
            )
            .map(|_| ())
        } else {
            Ok(())
        };
        if allowance_result.is_ok() {
            self.allowance_armed = false;
        }
        generation_result.and(allowance_result)
    }
}

impl Drop for GenerationReservationGuard {
    fn drop(&mut self) {
        if !self.armed && !self.allowance_armed {
            return;
        }
        if self.armed {
            if let Err(error) = jobs_generation::fail(
                &self.pool,
                &self.account_id,
                &self.generation_key,
                &self.reservation_token,
                "cancelled",
            ) {
                tracing::error!(
                    account_id_hash = %cue_core::account_id_hash_prefix(&self.account_id),
                    generation_ref = %generation_log_ref(&self.generation_key),
                    error = %error,
                    "failed to close a cancelled Jobs resume generation reservation"
                );
            }
        }
        if self.allowance_armed {
            if let Err(error) = jobs_generation_allowance::release(
                &self.pool,
                &self.account_id,
                &self.job_id,
                &self.generation_key,
                &self.reservation_token,
            ) {
                tracing::error!(
                    account_id_hash = %cue_core::account_id_hash_prefix(&self.account_id),
                    job_id_hash = %generation_log_ref(&self.job_id),
                    error = %error,
                    "failed to release a cancelled Jobs generation allowance"
                );
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn try_model_generation(
    state: &AppState,
    account_id: &str,
    profile: &CareerProfile,
    catalog: &EvidenceCatalog,
    system: &str,
    user: &str,
    generation_key: &str,
    reservation_token: &str,
    estimated_input_tokens: i64,
) -> Result<Option<AccountedGeneration>> {
    if jobs_provider_cost_holds::has_generation_exposure(&state.pool, account_id, generation_key)? {
        tracing::warn!(
            account_id_hash = %cue_core::account_id_hash_prefix(account_id),
            generation_ref = %generation_log_ref(generation_key),
            "Jobs resume generation has prior ambiguous provider exposure; refusing redispatch"
        );
        return Ok(None);
    }
    if let Err(denied) = state.rate_limiters.check_account_llm(account_id).await {
        tracing::warn!(
            account_id_hash = %cue_core::account_id_hash_prefix(account_id),
            generation_ref = %generation_log_ref(generation_key),
            reason = denied.reason,
            retry_after_secs = denied.retry_after_secs,
            "Jobs resume generation stopped by the account LLM limiter"
        );
        return Ok(None);
    }

    let spend_guard = state
        .config
        .upstream_spend_guard
        .ok_or_else(|| anyhow!("Jobs managed generation requires an upstream spend guard"))?;
    let generation_started = tokio::time::Instant::now();
    let mut dispatched_attempts = 0_usize;
    'routes: for (provider, model) in
        routing::resolve_route_candidates_with_seed("deep", generation_key)
    {
        if dispatched_attempts >= MAX_PROVIDER_ATTEMPTS
            || generation_started.elapsed() >= MODEL_GENERATION_DEADLINE
        {
            break;
        }
        let key_candidates = state.config.upstream.key_candidates(
            provider,
            &format!("jobs:{generation_key}:{provider}:{model}"),
        );
        if key_candidates.is_empty() {
            continue;
        }
        let Some(estimated_bluey_cost) = estimated_route_bluey_cost(
            provider,
            model,
            estimated_input_tokens,
            i64::from(MAX_MODEL_OUTPUT_TOKENS),
        ) else {
            tracing::warn!(provider, model, "Jobs resume route has no pricing entry");
            continue;
        };
        if estimated_bluey_cost > MAX_ATTEMPT_BLUEY_COST_CENTS {
            tracing::warn!(
                provider,
                model,
                estimated_bluey_cost,
                "Jobs resume route exceeded the hard upstream-cost ceiling"
            );
            continue;
        }
        loop {
            if dispatched_attempts >= MAX_PROVIDER_ATTEMPTS
                || generation_started.elapsed() >= MODEL_GENERATION_DEADLINE
            {
                break 'routes;
            }
            let selected_key = match state
                .provider_health
                .choose_key(provider, model, &key_candidates)
                .await
            {
                Ok(selected) => selected,
                Err(denied) => {
                    tracing::warn!(
                        provider,
                        model,
                        reason = denied.reason,
                        retry_after_secs = denied.retry_after_secs,
                        "Jobs resume route has no healthy provider key"
                    );
                    break;
                }
            };
            if let Err(denied) = state
                .rate_limiters
                .check_provider_llm(provider, model)
                .await
            {
                tracing::warn!(
                    provider,
                    model,
                    reason = denied.reason,
                    retry_after_secs = denied.retry_after_secs,
                    "Jobs resume route skipped by the provider/model limiter"
                );
                break;
            }
            let attempt_index = dispatched_attempts;
            let request_id = attempt_request_id(
                generation_key,
                reservation_token,
                attempt_index,
                provider,
                model,
            );
            let hold_reservation_token = match jobs_provider_cost_holds::reserve(
                &state.pool,
                account_id,
                generation_key,
                reservation_token,
                &request_id,
                provider,
                model,
                estimated_bluey_cost,
                MAX_GENERATION_BLUEY_COST_CENTS,
                spend_guard,
            )? {
                CostHoldReservation::Held { reservation_token } => reservation_token,
                CostHoldReservation::RecoveredAmbiguous { reservation_token } => {
                    dispatched_attempts += 1;
                    drop(ProviderAttemptGuard::new(
                        state.pool.clone(),
                        account_id,
                        request_id,
                        &reservation_token,
                        provider,
                        model,
                        attempt_index,
                        estimated_input_tokens,
                        estimated_bluey_cost,
                    ));
                    continue;
                }
                CostHoldReservation::GenerationLimit => break 'routes,
                CostHoldReservation::GlobalLimit => break 'routes,
            };
            dispatched_attempts += 1;
            let remaining = MODEL_GENERATION_DEADLINE.saturating_sub(generation_started.elapsed());
            if remaining.is_zero() {
                let hold = ProviderAttemptGuard::new(
                    state.pool.clone(),
                    account_id,
                    request_id,
                    &hold_reservation_token,
                    provider,
                    model,
                    attempt_index,
                    estimated_input_tokens,
                    estimated_bluey_cost,
                );
                drop(hold);
                break 'routes;
            }
            let mut hold = ProviderAttemptGuard::new(
                state.pool.clone(),
                account_id,
                request_id,
                &hold_reservation_token,
                provider,
                model,
                attempt_index,
                estimated_input_tokens,
                estimated_bluey_cost,
            );
            let attempt = tokio::time::timeout(
                MODEL_ATTEMPT_TIMEOUT.min(remaining),
                routing::complete_with_key(
                    &selected_key.secret,
                    provider,
                    model,
                    system,
                    user,
                    Some(MAX_MODEL_OUTPUT_TOKENS),
                    Some(0.1),
                    routing::ThinkingBudget::off(),
                    Some(estimated_input_tokens),
                    &[],
                ),
            )
            .await;
            let mut completion = match attempt {
                Ok(Ok(completion)) => completion,
                Ok(Err(error)) => {
                    hold.settle_uncertain(AttemptOutcome::RejectedProviderError)?;
                    tracing::warn!(provider, model, error = %error, "Jobs resume generation route failed");
                    if let Some(retry_after_secs) = routing::upstream_retry_after(&error) {
                        state
                            .provider_health
                            .record_cooldown(
                                provider,
                                model,
                                &selected_key.fingerprint,
                                retry_after_secs,
                            )
                            .await;
                        continue;
                    }
                    break;
                }
                Err(_) => {
                    hold.settle_uncertain(AttemptOutcome::RejectedTimeout)?;
                    tracing::warn!(provider, model, "Jobs resume generation route timed out");
                    break;
                }
            };
            normalize_missing_completion_usage(&mut completion, estimated_input_tokens);
            let computed_bluey_cost = pricing::lookup(&completion.provider, &completion.model)
                .map(|price| completed_bluey_cost(price, &completion))
                .unwrap_or(estimated_bluey_cost);
            let settled_bluey_cost = if completion.usage_provenance.is_exact() {
                computed_bluey_cost
            } else {
                estimated_bluey_cost.max(computed_bluey_cost)
            };
            let provider_boundary_rejected =
                completion.provider != provider || completion.model != model;
            let cost_rejected = settled_bluey_cost > MAX_ATTEMPT_BLUEY_COST_CENTS;
            let deadline_rejected = generation_started.elapsed() >= MODEL_GENERATION_DEADLINE;
            let plan =
                parse_plan(&completion.text).and_then(|plan| validate_plan(profile, catalog, plan));
            let outcome = if provider_boundary_rejected {
                AttemptOutcome::RejectedProviderBoundary
            } else if cost_rejected {
                AttemptOutcome::RejectedCost
            } else if deadline_rejected {
                AttemptOutcome::RejectedDeadline
            } else if plan.is_ok() {
                AttemptOutcome::Accepted
            } else {
                AttemptOutcome::RejectedTruth
            };
            hold.settle(&completion, computed_bluey_cost, outcome)?;
            if provider_boundary_rejected {
                tracing::error!(
                    requested_provider = provider,
                    requested_model = model,
                    completed_provider = %completion.provider,
                    completed_model = %completion.model,
                    "Jobs resume completion crossed its routed provider/model boundary"
                );
                break 'routes;
            }
            if cost_rejected || deadline_rejected {
                break 'routes;
            }
            let plan = match plan {
                Ok(plan) => plan,
                Err(error) => {
                    tracing::warn!(
                        provider = %completion.provider,
                        model = %completion.model,
                        error = %error,
                        "Jobs resume generation output rejected"
                    );
                    break;
                }
            };
            return Ok(Some(AccountedGeneration {
                plan,
                completion,
                bluey_cost: settled_bluey_cost,
            }));
        }
    }
    Ok(None)
}

fn estimated_route_bluey_cost(
    provider: &str,
    model: &str,
    estimated_input_tokens: i64,
    max_output_tokens: i64,
) -> Option<i64> {
    pricing::lookup(provider, model).map(|price| {
        pricing::estimate_bluey_cost_ceiling(
            price,
            estimated_input_tokens.max(0),
            max_output_tokens.max(0),
        )
    })
}

fn completed_bluey_cost(price: &pricing::ModelPricing, completion: &routing::Completion) -> i64 {
    // Provider usage is untrusted JSON. Calculate in i128 so a malformed token
    // count cannot overflow the accounting path and accidentally become cheap.
    const MICROCENTS_PER_CENT: i128 = 10_000;
    const TOKENS_PER_MILLION: i128 = 1_000_000;
    let input_tokens = i128::from(completion.input_tokens.max(0));
    let output_tokens = i128::from(completion.output_tokens.max(0));
    let input_microcents = i128::from(price.upstream_in_microcents_per_1m)
        .saturating_mul(input_tokens)
        / TOKENS_PER_MILLION;
    let output_microcents = i128::from(price.upstream_out_microcents_per_1m)
        .saturating_mul(output_tokens)
        / TOKENS_PER_MILLION;
    let microcents = input_microcents.saturating_add(output_microcents).max(0);
    let cents = microcents.saturating_add(MICROCENTS_PER_CENT - 1) / MICROCENTS_PER_CENT;
    cents.min(i128::from(i64::MAX)) as i64
}

fn normalize_missing_completion_usage(
    completion: &mut routing::Completion,
    estimated_input_tokens: i64,
) {
    let mut repaired = false;
    if completion.input_tokens <= 0 {
        completion.input_tokens = estimated_input_tokens.max(1);
        repaired = true;
    }
    if completion.output_tokens <= 0 && !completion.text.is_empty() {
        // One token per UTF-8 byte is a conservative tokenizer-independent
        // ceiling. Providers occasionally omit output usage; recording zero
        // would let rejected attempts bypass the generation spend boundary.
        completion.output_tokens = i64::try_from(completion.text.len()).unwrap_or(i64::MAX);
        repaired = true;
    }
    if repaired && completion.usage_provenance.is_exact() {
        completion.usage_provenance = pricing::UsageProvenance::Estimated;
    }
}

struct ProviderAttemptGuard {
    pool: crate::db::DbPool,
    account_id: String,
    request_id: String,
    reservation_token: String,
    requested_provider: String,
    requested_model: String,
    attempt_index: usize,
    estimated_input_tokens: i64,
    estimated_output_tokens: i64,
    projected_bluey_cost: i64,
    fallback_bluey_cost: i64,
    pending_outcome: AttemptOutcome,
    started: std::time::Instant,
    armed: bool,
}

impl ProviderAttemptGuard {
    #[allow(clippy::too_many_arguments)]
    fn new(
        pool: crate::db::DbPool,
        account_id: &str,
        request_id: String,
        reservation_token: &str,
        requested_provider: &str,
        requested_model: &str,
        attempt_index: usize,
        estimated_input_tokens: i64,
        projected_bluey_cost: i64,
    ) -> Self {
        Self {
            pool,
            account_id: account_id.to_string(),
            request_id,
            reservation_token: reservation_token.to_string(),
            requested_provider: requested_provider.to_string(),
            requested_model: requested_model.to_string(),
            attempt_index,
            estimated_input_tokens,
            estimated_output_tokens: i64::from(MAX_MODEL_OUTPUT_TOKENS),
            projected_bluey_cost,
            fallback_bluey_cost: projected_bluey_cost,
            pending_outcome: AttemptOutcome::RejectedCancelled,
            started: std::time::Instant::now(),
            armed: true,
        }
    }

    fn settle(
        &mut self,
        completion: &routing::Completion,
        bluey_cost: i64,
        outcome: AttemptOutcome,
    ) -> Result<()> {
        let route_matches = completion.provider == self.requested_provider
            && completion.model == self.requested_model;
        let usage_provenance = if route_matches {
            completion.usage_provenance
        } else {
            pricing::UsageProvenance::Missing
        };
        let reported_bluey_cost =
            bluey_cost.clamp(0, crate::db::usage::MAX_AUTHORITATIVE_EVENT_COST_CENTS);
        let settled_bluey_cost = if usage_provenance.is_exact() {
            reported_bluey_cost
        } else {
            self.projected_bluey_cost.max(reported_bluey_cost)
        };
        // If persistence fails after a known completion, Drop retries with a
        // conservative Missing-provenance value. Keep it separate from the
        // immutable projection so trusted Exact usage can still settle lower.
        self.fallback_bluey_cost = self
            .fallback_bluey_cost
            .max(self.projected_bluey_cost)
            .max(reported_bluey_cost);
        self.estimated_input_tokens = completion.input_tokens.max(0);
        self.estimated_output_tokens = completion.output_tokens.max(0);
        self.pending_outcome = outcome;
        let event = UsageEvent {
            request_id: self.request_id.clone(),
            kind: "jobs_resume_generation_attempt".to_string(),
            task_type: Some(outcome.task_type().to_string()),
            lane: Some("deep".to_string()),
            // Durable accounting stays bound to the immutable requested route
            // even when a provider returns crossed identity metadata.
            provider: Some(self.requested_provider.clone()),
            model: Some(self.requested_model.clone()),
            input_tokens: completion.input_tokens.max(0),
            output_tokens: completion.output_tokens.max(0),
            latency_ms: self
                .started
                .elapsed()
                .as_millis()
                .try_into()
                .unwrap_or(i64::MAX),
            cost_cents_to_bluey: settled_bluey_cost,
            cost_cents_to_customer: 0,
            was_speculative: false,
            was_fallback: self.attempt_index > 0,
        };
        jobs_provider_cost_holds::settle_with_usage(
            &self.pool,
            &self.account_id,
            &self.request_id,
            &self.reservation_token,
            reported_bluey_cost,
            usage_provenance,
            &event,
        )?;
        self.armed = false;
        if usage_provenance.is_exact() && reported_bluey_cost > self.projected_bluey_cost {
            tracing::error!(
                account_id_hash = %cue_core::account_id_hash_prefix(&self.account_id),
                request_id = %self.request_id,
                provider = %self.requested_provider,
                model = %self.requested_model,
                projected_cost_cents = self.projected_bluey_cost,
                exact_cost_cents = reported_bluey_cost,
                "Jobs exact provider cost exceeded its pre-dispatch upper bound"
            );
            anyhow::bail!("Jobs exact provider cost exceeded pre-dispatch upper bound")
        }
        Ok(())
    }

    fn settle_uncertain(&mut self, outcome: AttemptOutcome) -> Result<()> {
        let completion = routing::Completion {
            text: String::new(),
            provider: self.requested_provider.clone(),
            model: self.requested_model.clone(),
            input_tokens: self.estimated_input_tokens.max(1),
            output_tokens: self.estimated_output_tokens.max(0),
            usage_provenance: pricing::UsageProvenance::Missing,
        };
        self.settle(&completion, self.fallback_bluey_cost, outcome)
    }
}

impl Drop for ProviderAttemptGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        if let Err(error) = self.settle_uncertain(self.pending_outcome) {
            tracing::error!(
                account_id_hash = %cue_core::account_id_hash_prefix(&self.account_id),
                request_id = %self.request_id,
                error = %error,
                "failed to conservatively settle a cancelled Jobs provider attempt"
            );
        }
    }
}

fn attempt_request_id(
    generation_key: &str,
    reservation_token: &str,
    attempt_index: usize,
    provider: &str,
    model: &str,
) -> String {
    let input = format!("{generation_key}|{reservation_token}|{attempt_index}|{provider}|{model}");
    format!("jobs-resume-attempt-{}", hex::encode(Sha256::digest(input)))
}

fn finish_deterministic_fallback(
    state: &AppState,
    account_id: &str,
    baseline: &ResumeVersion,
    generation_key: &str,
    reservation_token: &str,
    reservation: &mut GenerationReservationGuard,
) -> Result<GeneratedResume> {
    let output = json!({"fallback": "deterministic_baseline"});
    jobs_generation::complete(
        &state.pool,
        account_id,
        generation_key,
        reservation_token,
        &output,
        "bluey",
        "deterministic-fallback-v2",
        0,
        0,
        0,
    )?;
    reservation.disarm();
    deterministic_fallback(baseline)
}

fn finish_transient_deterministic_fallback(
    baseline: &ResumeVersion,
    reservation: &mut GenerationReservationGuard,
) -> Result<GeneratedResume> {
    // Managed routing was explicitly enabled, so limiter pressure, provider
    // outages, and spend-guard denials must remain retryable. The caller still
    // receives the deterministic review-first resume for this request.
    reservation.fail("managed_generation_unavailable")?;
    deterministic_fallback(baseline)
}

fn generation_log_ref(generation_key: &str) -> String {
    hex::encode(Sha256::digest(generation_key.as_bytes()))
        .chars()
        .take(12)
        .collect()
}

fn generation_key(
    account_id: &str,
    profile: &CareerProfile,
    posting: &JobPosting,
    baseline: &ResumeVersion,
    managed_generation: bool,
) -> Result<String> {
    let input = json!({
        "schema_version": GENERATION_SCHEMA_VERSION,
        "generation_mode": if managed_generation { "managed" } else { "deterministic" },
        "account_id": account_id,
        "profile": profile,
        "posting": posting,
        "baseline_checksum": baseline.checksum,
        "mode": baseline.mode,
    });
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(&input)?)))
}

fn system_prompt() -> &'static str {
    r#"You rank verified candidate evidence for one job-specific resume.
Return JSON only. Do not write or rewrite the headline, summary, skills, employers, titles, degrees, dates, metrics, certifications, authorization facts, or locations. Select only exact skill strings, exact evidence IDs, and existing array indexes from the input. Never copy candidate requirements from the job description into candidate evidence.

The output schema is:
{
  "headline_evidence_ids": ["evidence:id"],
  "summary_evidence_ids": ["evidence:id"],
  "skill_order": ["exact candidate skill"],
  "employment_order": [0],
  "employment_highlight_order": [{"entry_index":0,"highlight_indices":[0]}],
  "project_order": [0]
}

Select zero or one headline evidence ID and any non-duplicated summary evidence IDs whose exact text fits the resume. Bluey composes those exact records deterministically; do not return headline or summary text. An empty headline or summary selection preserves Bluey's deterministic baseline instead of deleting it. Include every employment and project index exactly once. Include every highlight index exactly once for every employment entry. Choose at most 16 skills. Prefer the evidence that best answers the job description."#
}

fn user_prompt(
    profile: &CareerProfile,
    posting: &JobPosting,
    catalog: &EvidenceCatalog,
) -> Result<String> {
    let job = json!({
        "company": posting.company,
        "title": posting.title,
        "location": posting.location,
        "description": posting.description,
        "employment_type": posting.employment_type,
    });
    let candidate = json!({
        "skills": profile.skills,
        "employment": profile.employment,
        "projects": profile.projects,
        "certifications": profile.certifications,
    });
    ensure_json_size(&candidate, MAX_CANDIDATE_PROMPT_BYTES, "candidate profile")?;
    ensure_json_size(&job, MAX_JOB_PROMPT_BYTES, "job posting")?;
    let input = json!({
        "job": job,
        "candidate": candidate,
        "evidence_catalog": catalog.values,
    });
    let prompt = serde_json::to_string(&input).context("encode resume generation prompt")?;
    if prompt.len() > MAX_USER_PROMPT_BYTES {
        return Err(anyhow!(
            "resume generation prompt exceeds {MAX_USER_PROMPT_BYTES} bytes"
        ));
    }
    Ok(prompt)
}

fn ensure_json_size(value: &Value, max_bytes: usize, label: &str) -> Result<()> {
    let size = serde_json::to_vec(value)
        .with_context(|| format!("measure {label} for resume generation"))?
        .len();
    if size > max_bytes {
        return Err(anyhow!("{label} exceeds {max_bytes} bytes"));
    }
    Ok(())
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
    catalog.compose_headline(&plan.headline_evidence_ids)?;
    catalog.compose_summary(&plan.summary_evidence_ids)?;
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
    let catalog = EvidenceCatalog::from_profile(profile);
    if kind == "model" {
        validate_plan(profile, &catalog, plan.clone())?;
    }
    let headline = if plan.headline_evidence_ids.is_empty() {
        baseline
            .content
            .get("headline")
            .and_then(Value::as_str)
            .unwrap_or(&profile.headline)
            .to_string()
    } else {
        catalog.compose_headline(&plan.headline_evidence_ids)?
    };
    let summary = if plan.summary_evidence_ids.is_empty() {
        baseline
            .content
            .get("summary")
            .and_then(Value::as_str)
            .unwrap_or(&profile.summary)
            .to_string()
    } else {
        catalog.compose_summary(&plan.summary_evidence_ids)?
    };
    let mut content = baseline.content.clone();
    content["headline"] = json!(headline);
    content["summary"] = json!(summary);
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
    let diff = build_diff(
        profile,
        plan,
        &headline,
        &summary,
        &employment,
        &projects,
        kind,
    );
    Ok(GeneratedResume {
        content,
        diff,
        public_provenance,
    })
}

fn build_diff(
    profile: &CareerProfile,
    plan: &ResumePlan,
    headline: &str,
    summary: &str,
    employment: &[jobs::EmploymentEntry],
    projects: &[jobs::ProjectEntry],
    kind: &str,
) -> Value {
    let mut diff = serde_json::Map::new();
    if profile.headline.trim() != headline.trim() {
        diff.insert(
            "headline".to_string(),
            json!({"before": profile.headline, "after": headline}),
        );
    }
    if profile.summary.trim() != summary.trim() {
        diff.insert(
            "summary".to_string(),
            json!({"before": profile.summary, "after": summary}),
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
            "A managed model ranked verified profile evidence. Bluey composed exact evidence records without model-authored facts."
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
    use crate::db::{
        self,
        jobs::{EmploymentEntry, ProjectEntry},
    };

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

    fn posting() -> JobPosting {
        serde_json::from_value(json!({
            "company": "Example",
            "title": "Engineer",
            "description": "Build reliable systems"
        }))
        .unwrap()
    }

    fn generation_pool_with_job() -> (crate::db::DbPool, String, String) {
        let path = std::env::temp_dir().join(format!(
            "bluey-jobs-resume-generation-boundary-{}-{}.sqlite3",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let pool = db::open_pool(&path).unwrap();
        db::run_migrations(&pool).unwrap();
        let account_id = uuid::Uuid::new_v4().to_string();
        let job_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO accounts (id, email, password_hash, trial_seconds_remaining) \
                 VALUES (?1, 'generation-boundary@bluey.test', 'hash', 0)",
                rusqlite::params![account_id],
            )
            .unwrap();
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO jobs_postings (id, account_id, canonical_key, posting_json, source, \
                 canonical_url, company, title, location, match_score, status, created_at_ms, updated_at_ms) \
                 VALUES (?1, ?2, ?3, '{}', 'test', NULL, 'Example', 'Engineer', NULL, 0, \
                 'matched', ?4, ?4)",
                rusqlite::params![job_id, account_id, format!("test:{job_id}"), now],
            )
            .unwrap();
        (pool, account_id, job_id)
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
    fn rejects_unknown_skill_and_duplicate_evidence() {
        let profile = profile();
        let catalog = EvidenceCatalog::from_profile(&profile);
        let mut unknown_skill = valid_plan();
        unknown_skill.skill_order.push("Kubernetes".into());
        assert!(validate_plan(&profile, &catalog, unknown_skill).is_err());

        let mut duplicate_evidence = valid_plan();
        duplicate_evidence
            .summary_evidence_ids
            .push("profile:summary".into());
        assert!(validate_plan(&profile, &catalog, duplicate_evidence).is_err());

        let mut company_as_title = valid_plan();
        company_as_title.headline_evidence_ids = vec!["employment:0:company".into()];
        assert!(validate_plan(&profile, &catalog, company_as_title).is_err());
    }

    #[test]
    fn rejects_model_authored_short_credentials_titles_and_metrics() {
        let mut attempted = serde_json::to_value(valid_plan()).unwrap();
        attempted["headline"] = json!("CEO, PhD, AWS");
        attempted["summary"] = json!("Increased revenue by 30 percent using AI and ML.");
        assert!(parse_plan(&attempted.to_string()).is_err());
    }

    #[test]
    fn exact_composition_cannot_reassign_a_metric_between_evidence_records() {
        let mut profile = profile();
        profile.summary = "Increased revenue for the platform.".into();
        profile.employment[0].highlights[0] = "Reduced latency by 30 percent.".into();
        let mut plan = valid_plan();
        plan.summary_evidence_ids =
            vec!["profile:summary".into(), "employment:0:highlight:0".into()];
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
        let generated = materialize(&profile, &baseline, &plan, "model").unwrap();
        assert_eq!(
            generated.content["summary"],
            "Increased revenue for the platform. • Reduced latency by 30 percent."
        );
        assert_ne!(
            generated.content["summary"],
            "Increased revenue by 30 percent for the platform."
        );
    }

    #[test]
    fn preserves_legitimate_short_credentials_only_when_the_evidence_contains_them() {
        let mut profile = profile();
        profile.headline = "VP".into();
        profile.summary = "AWS and GCP certified engineer.".into();
        let catalog = EvidenceCatalog::from_profile(&profile);
        let plan = valid_plan();
        assert_eq!(
            catalog
                .compose_headline(&plan.headline_evidence_ids)
                .unwrap(),
            "VP"
        );
        assert_eq!(
            catalog.compose_summary(&plan.summary_evidence_ids).unwrap(),
            "AWS and GCP certified engineer."
        );
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
    fn prompt_caps_reject_oversized_profile_and_job_text() {
        let mut oversized_profile = profile();
        oversized_profile.employment[0].highlights = vec!["x".repeat(MAX_CANDIDATE_PROMPT_BYTES)];
        let posting = posting();
        let catalog = EvidenceCatalog::from_profile(&oversized_profile);
        assert!(user_prompt(&oversized_profile, &posting, &catalog).is_err());

        let profile = profile();
        let mut oversized_posting = posting;
        oversized_posting.description = "x".repeat(MAX_JOB_PROMPT_BYTES);
        let catalog = EvidenceCatalog::from_profile(&profile);
        assert!(user_prompt(&profile, &oversized_posting, &catalog).is_err());
    }

    #[test]
    fn model_generation_is_default_off_and_requires_an_explicit_true_value() {
        assert!(!model_generation_enabled_value(None));
        assert!(!model_generation_enabled_value(Some("")));
        assert!(!model_generation_enabled_value(Some("false")));
        assert!(!model_generation_enabled_value(Some("surprise")));
        assert!(model_generation_enabled_value(Some("1")));
        assert!(model_generation_enabled_value(Some("YES")));
    }

    #[test]
    fn cost_and_attempt_boundaries_are_deterministic() {
        let known = estimated_route_bluey_cost("openai", "gpt-5.4-mini", 1_000, 500);
        assert!(known.is_some_and(|cost| cost > 0));
        assert!(estimated_route_bluey_cost("openai", "missing", 1_000, 500).is_none());
        let price = pricing::lookup("openai", "gpt-5.4-mini").unwrap();
        let completion = routing::Completion {
            text: String::new(),
            provider: "openai".into(),
            model: "gpt-5.4-mini".into(),
            input_tokens: 1_000,
            output_tokens: 500,
            usage_provenance: pricing::UsageProvenance::Exact,
        };
        assert_eq!(
            completed_bluey_cost(price, &completion),
            pricing::compute_cost(price, 1_000, 500).0
        );
        assert!(
            completed_bluey_cost(
                price,
                &routing::Completion {
                    input_tokens: i64::MAX,
                    output_tokens: i64::MAX,
                    ..completion
                }
            ) > MAX_ATTEMPT_BLUEY_COST_CENTS
        );
        assert_ne!(
            attempt_request_id("generation", "lease-a", 0, "openai", "model"),
            attempt_request_id("generation", "lease-b", 0, "openai", "model")
        );
        assert!(MODEL_GENERATION_DEADLINE < jobs_generation::RESERVATION_TTL);
    }

    #[test]
    fn provider_deadlines_leave_a_settlement_margin_before_the_lease_ttl() {
        assert!(
            MODEL_ATTEMPT_TIMEOUT * u32::try_from(MAX_PROVIDER_ATTEMPTS).unwrap()
                <= MODEL_GENERATION_DEADLINE
        );
        assert!(
            MODEL_GENERATION_DEADLINE + Duration::from_secs(30) < jobs_generation::RESERVATION_TTL
        );
    }

    #[test]
    fn cancellation_guard_marks_the_generation_failed_and_immediately_restartable() {
        let (pool, account_id, job_id) = generation_pool_with_job();
        let ResumeGenerationReservation::Start(first) =
            jobs_generation::reserve(&pool, &account_id, &job_id, "cancelled-generation").unwrap()
        else {
            panic!("first worker should own the generation")
        };
        {
            let _guard = GenerationReservationGuard::new(
                pool.clone(),
                &account_id,
                &job_id,
                "cancelled-generation",
                &first.reservation_token,
            );
        }
        let (status, failure_code): (String, Option<String>) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT status, failure_code FROM jobs_resume_generations \
                 WHERE account_id = ?1 AND generation_key = ?2",
                rusqlite::params![account_id, "cancelled-generation"],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(status, "failed");
        assert_eq!(failure_code.as_deref(), Some("cancelled"));
        let ResumeGenerationReservation::Start(second) =
            jobs_generation::reserve(&pool, &account_id, &job_id, "cancelled-generation").unwrap()
        else {
            panic!("cancelled generation should be immediately restartable")
        };
        assert_ne!(first.reservation_token, second.reservation_token);
    }

    #[test]
    fn provider_spend_keeps_the_jobs_packet_slot_consumed_after_generation_failure() {
        let (pool, account_id, job_id) = generation_pool_with_job();
        let ResumeGenerationReservation::Start(generation) =
            jobs_generation::reserve(&pool, &account_id, &job_id, "spent-generation").unwrap()
        else {
            panic!("generation reservation must start")
        };
        assert_eq!(
            jobs_generation_allowance::reserve(
                &pool,
                &account_id,
                &job_id,
                "spent-generation",
                &generation.reservation_token,
            )
            .unwrap(),
            AllowanceReservation::Reserved
        );
        let attempt_request_id = "spent-generation:attempt:0";
        let hold_token = match jobs_provider_cost_holds::reserve(
            &pool,
            &account_id,
            "spent-generation",
            &generation.reservation_token,
            attempt_request_id,
            "openai",
            "gpt-5.4-mini",
            2,
            MAX_GENERATION_BLUEY_COST_CENTS,
            crate::config::UpstreamSpendGuard {
                limit_cents: 100,
                window_hours: 24,
            },
        )
        .unwrap()
        {
            CostHoldReservation::Held { reservation_token } => reservation_token,
            other => panic!("expected provider hold, got {other:?}"),
        };
        jobs_provider_cost_holds::settle_with_usage(
            &pool,
            &account_id,
            attempt_request_id,
            &hold_token,
            2,
            pricing::UsageProvenance::Exact,
            &UsageEvent {
                request_id: attempt_request_id.into(),
                kind: "jobs_resume_generation_attempt".into(),
                task_type: Some("jobs_resume_tailoring_rejected_provider_error".into()),
                lane: Some("deep".into()),
                provider: Some("openai".into()),
                model: Some("gpt-5.4-mini".into()),
                input_tokens: 10,
                output_tokens: 0,
                latency_ms: 1,
                cost_cents_to_bluey: 2,
                cost_cents_to_customer: 0,
                was_speculative: false,
                was_fallback: false,
            },
        )
        .unwrap();
        jobs_generation::fail(
            &pool,
            &account_id,
            "spent-generation",
            &generation.reservation_token,
            "provider_failed",
        )
        .unwrap();

        assert!(
            !jobs_generation_allowance::release(
                &pool,
                &account_id,
                &job_id,
                "spent-generation",
                &generation.reservation_token,
            )
            .unwrap(),
            "a provider-billed generation must not refund its included packet slot"
        );
        let (used_packets, allowance_status): (i64, String) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT e.used_packets, r.status
                   FROM jobs_entitlements e
                   JOIN jobs_generation_allowance_reservations r
                     ON r.account_id = e.account_id
                  WHERE e.account_id = ?1 AND r.job_id = ?2",
                rusqlite::params![account_id, job_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(used_packets, 1);
        assert_eq!(allowance_status, "reserved");
    }

    #[test]
    fn crossing_the_lease_ttl_cannot_commit_late_model_output() {
        let (pool, account_id, job_id) = generation_pool_with_job();
        let ResumeGenerationReservation::Start(first) =
            jobs_generation::reserve(&pool, &account_id, &job_id, "expired-generation").unwrap()
        else {
            panic!("first worker should own the generation")
        };
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_resume_generations SET updated_at_ms = 0 \
                 WHERE account_id = ?1 AND generation_key = ?2",
                rusqlite::params![account_id, "expired-generation"],
            )
            .unwrap();
        assert!(jobs_generation::complete(
            &pool,
            &account_id,
            "expired-generation",
            &first.reservation_token,
            &json!({"late": true}),
            "openai",
            "gpt-5.4-mini",
            10,
            10,
            1,
        )
        .is_err());
        let ResumeGenerationReservation::Start(second) =
            jobs_generation::reserve(&pool, &account_id, &job_id, "expired-generation").unwrap()
        else {
            panic!("expired generation should be reclaimable")
        };
        assert_ne!(first.reservation_token, second.reservation_token);
    }

    #[test]
    fn rejected_provider_output_is_durably_settled_before_retry() {
        let path = std::env::temp_dir().join(format!(
            "bluey-jobs-resume-attempt-accounting-{}-{}.sqlite3",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let pool = db::open_pool(&path).unwrap();
        db::run_migrations(&pool).unwrap();
        let account = db::accounts::Account::create(&pool, "attempt@bluey.test", "hash").unwrap();
        let completion = routing::Completion {
            text: "rejected".into(),
            provider: "openai".into(),
            model: "gpt-5.4-mini".into(),
            input_tokens: 100,
            output_tokens: 50,
            usage_provenance: pricing::UsageProvenance::Exact,
        };
        let request_id = attempt_request_id(
            "generation",
            "reservation",
            0,
            &completion.provider,
            &completion.model,
        );
        assert!(matches!(
            jobs_provider_cost_holds::reserve(
                &pool,
                &account.id,
                "generation",
                "reservation",
                &request_id,
                &completion.provider,
                &completion.model,
                3,
                MAX_GENERATION_BLUEY_COST_CENTS,
                crate::config::UpstreamSpendGuard {
                    limit_cents: 4,
                    window_hours: 24,
                },
            )
            .unwrap(),
            CostHoldReservation::Held { .. }
        ));
        let mut attempt = ProviderAttemptGuard::new(
            pool.clone(),
            &account.id,
            request_id.clone(),
            "reservation",
            &completion.provider,
            &completion.model,
            0,
            100,
            3,
        );
        attempt
            .settle(&completion, 3, AttemptOutcome::RejectedTruth)
            .unwrap();
        let (task_type, provider, model, cost): (String, String, String, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT task_type, provider, model, cost_cents_to_bluey FROM usage_events \
                 WHERE account_id = ?1 AND kind = 'jobs_resume_generation_attempt'",
                rusqlite::params![account.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(task_type, AttemptOutcome::RejectedTruth.task_type());
        assert_eq!(provider, "openai");
        assert_eq!(model, "gpt-5.4-mini");
        assert_eq!(cost, 3);
        assert!(matches!(
            jobs_provider_cost_holds::reserve(
                &pool,
                &account.id,
                "generation-2",
                "reservation-2",
                "second-attempt",
                "openai",
                "gpt-5.4-mini",
                1,
                MAX_GENERATION_BLUEY_COST_CENTS,
                crate::config::UpstreamSpendGuard {
                    limit_cents: 4,
                    window_hours: 24,
                },
            )
            .unwrap(),
            CostHoldReservation::Held { .. }
        ));
        // The provider attempt event and durable hold are one exposure, not
        // two. The exact boundary admits cost 3 + 1, then denies another cent.
        assert_eq!(
            jobs_provider_cost_holds::reserve(
                &pool,
                &account.id,
                "generation-3",
                "reservation-3",
                "third-attempt",
                "openai",
                "gpt-5.4-mini",
                1,
                MAX_GENERATION_BLUEY_COST_CENTS,
                crate::config::UpstreamSpendGuard {
                    limit_cents: 4,
                    window_hours: 24,
                },
            )
            .unwrap(),
            CostHoldReservation::GlobalLimit
        );
    }

    #[test]
    fn jobs_provider_settlement_failure_stops_before_customer_root_and_reconciles_conservatively() {
        let path = std::env::temp_dir().join(format!(
            "bluey-jobs-resume-settlement-failure-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let pool = db::open_pool(&path).unwrap();
        db::run_migrations(&pool).unwrap();
        let account =
            db::accounts::Account::create(&pool, "settlement-failure@bluey.test", "hash").unwrap();
        let request_id = "settlement-failure:attempt:0";
        let reservation_token = "settlement-failure-token";
        assert!(matches!(
            jobs_provider_cost_holds::reserve(
                &pool,
                &account.id,
                "settlement-failure-generation",
                reservation_token,
                request_id,
                "openai",
                "gpt-5.4-mini",
                5,
                MAX_GENERATION_BLUEY_COST_CENTS,
                crate::config::UpstreamSpendGuard {
                    limit_cents: 100,
                    window_hours: 24,
                },
            )
            .unwrap(),
            CostHoldReservation::Held { .. }
        ));
        let completion = routing::Completion {
            text: "valid provider result".into(),
            provider: "openai".into(),
            model: "gpt-5.4-mini".into(),
            input_tokens: 10,
            output_tokens: 5,
            usage_provenance: pricing::UsageProvenance::Exact,
        };
        let mut attempt = ProviderAttemptGuard::new(
            pool.clone(),
            &account.id,
            request_id.into(),
            reservation_token,
            "openai",
            "gpt-5.4-mini",
            0,
            10,
            5,
        );
        jobs_provider_cost_holds::fail_next_settlement_for_test();
        assert!(attempt
            .settle(&completion, 2, AttemptOutcome::Accepted)
            .is_err());

        let (status_before_drop, root_events_before_drop): (String, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT h.status,
                        (SELECT COUNT(*) FROM usage_events e
                          WHERE e.account_id = ?1 AND e.kind = 'jobs_resume_generation')
                   FROM jobs_provider_cost_holds h",
                rusqlite::params![account.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(status_before_drop, "held");
        assert_eq!(root_events_before_drop, 0);

        drop(attempt);
        let (status, settled_cost, provenance, root_events): (String, i64, String, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT h.status, h.settled_cost_cents, h.usage_provenance,
                        (SELECT COUNT(*) FROM usage_events e
                          WHERE e.account_id = ?1 AND e.kind = 'jobs_resume_generation')
                   FROM jobs_provider_cost_holds h",
                rusqlite::params![account.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(status, "settled");
        assert_eq!(settled_cost, 5);
        assert_eq!(provenance, pricing::UsageProvenance::Missing.as_str());
        assert_eq!(root_events, 0);
    }

    #[test]
    fn jobs_usage_provenance_controls_hold_shrink_overrun_and_next_admission() {
        let cases = [
            (
                "exact-lower",
                pricing::UsageProvenance::Exact,
                "openai",
                2,
                2,
                5,
                3,
                true,
                pricing::UsageProvenance::Exact,
            ),
            (
                "estimated-lower",
                pricing::UsageProvenance::Estimated,
                "openai",
                2,
                5,
                5,
                1,
                false,
                pricing::UsageProvenance::Estimated,
            ),
            (
                "missing-zero",
                pricing::UsageProvenance::Missing,
                "openai",
                0,
                5,
                5,
                1,
                false,
                pricing::UsageProvenance::Missing,
            ),
            (
                "route-mismatch",
                pricing::UsageProvenance::Exact,
                "crossed-provider",
                2,
                5,
                5,
                1,
                false,
                pricing::UsageProvenance::Missing,
            ),
            (
                "exact-overrun",
                pricing::UsageProvenance::Exact,
                "openai",
                7,
                7,
                7,
                1,
                false,
                pricing::UsageProvenance::Exact,
            ),
        ];
        for (
            case,
            provenance,
            returned_provider,
            reported_cost,
            expected_settled,
            next_limit,
            next_cost,
            expect_next_held,
            expected_provenance,
        ) in cases
        {
            let path = std::env::temp_dir().join(format!(
                "bluey-jobs-provenance-{case}-{}.sqlite3",
                uuid::Uuid::new_v4()
            ));
            let pool = db::open_pool(&path).unwrap();
            db::run_migrations(&pool).unwrap();
            let account =
                db::accounts::Account::create(&pool, &format!("{case}@bluey.test"), "hash")
                    .unwrap();
            let request_id = format!("{case}:attempt:0");
            let reservation_token = format!("{case}-token");
            assert!(matches!(
                jobs_provider_cost_holds::reserve(
                    &pool,
                    &account.id,
                    case,
                    &reservation_token,
                    &request_id,
                    "openai",
                    "gpt-5.4-mini",
                    5,
                    MAX_GENERATION_BLUEY_COST_CENTS,
                    crate::config::UpstreamSpendGuard {
                        limit_cents: 100,
                        window_hours: 24,
                    },
                )
                .unwrap(),
                CostHoldReservation::Held { .. }
            ));
            let completion = routing::Completion {
                text: "result".into(),
                provider: returned_provider.into(),
                model: "gpt-5.4-mini".into(),
                input_tokens: if provenance == pricing::UsageProvenance::Missing {
                    0
                } else {
                    10
                },
                output_tokens: if provenance == pricing::UsageProvenance::Missing {
                    0
                } else {
                    5
                },
                usage_provenance: provenance,
            };
            let mut attempt = ProviderAttemptGuard::new(
                pool.clone(),
                &account.id,
                request_id,
                &reservation_token,
                "openai",
                "gpt-5.4-mini",
                0,
                10,
                5,
            );
            let result = attempt.settle(&completion, reported_cost, AttemptOutcome::RejectedTruth);
            assert_eq!(result.is_err(), case == "exact-overrun", "{case}");

            let (settled_cost, stored_provenance): (i64, String) = pool
                .get()
                .unwrap()
                .query_row(
                    "SELECT settled_cost_cents, usage_provenance
                       FROM jobs_provider_cost_holds",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap();
            assert_eq!(settled_cost, expected_settled, "{case}");
            assert_eq!(stored_provenance, expected_provenance.as_str(), "{case}");

            let next = jobs_provider_cost_holds::reserve(
                &pool,
                &account.id,
                &format!("{case}-next"),
                &format!("{case}-next-token"),
                &format!("{case}:attempt:1"),
                "openai",
                "gpt-5.4-mini",
                next_cost,
                MAX_GENERATION_BLUEY_COST_CENTS,
                crate::config::UpstreamSpendGuard {
                    limit_cents: next_limit,
                    window_hours: 24,
                },
            )
            .unwrap();
            assert_eq!(
                matches!(next, CostHoldReservation::Held { .. }),
                expect_next_held,
                "{case}"
            );
        }
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

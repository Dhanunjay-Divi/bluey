//! Compact correctness contracts for recurring technical interview themes.
//!
//! These are deliberately question-scoped: an interview plan alone should not
//! turn an ordinary answer into a long systems-design checklist.

use super::{
    looks_like_employment_document_surface, normalize_guardrail_text, AnswerIntent, AnswerPlan,
};
use cue_core::{AnswerContext, AnswerContextRole};

/// Appends only the correctness constraints that apply to a recognized
/// technical-interview question. `normalized_question` is expected to have
/// been normalized by the router's guardrail normalizer.
pub(super) fn append_interview_correctness_contracts(
    instructions: &mut String,
    normalized_question: &str,
    answer_context: &[AnswerContext],
    plan: &AnswerPlan,
) {
    if !plan.interview_context
        || !matches!(
            plan.intent,
            AnswerIntent::Quick
                | AnswerIntent::General
                | AnswerIntent::FollowUp
                | AnswerIntent::Behavioral
                | AnswerIntent::SystemDesign
        )
    {
        return;
    }
    if looks_like_employment_document_surface(normalized_question) {
        return;
    }

    let mut matched_theme = false;
    let mut append = |contract: &str| {
        matched_theme = true;
        instructions.push('\n');
        instructions.push_str(contract);
    };

    if has_all(normalized_question, &["kafka"])
        && has_any(normalized_question, &["lag", "backlog", "consumer behind"])
    {
        append(
            "Kafka lag interview contract: distinguish consumer lag from end-to-end freshness and correctness. Inspect committed offsets and rebalance history, then diagnose partition skew or hot partitions, consumer processing throughput and latency, resource saturation, and downstream sink latency; scale or tune only after finding the bottleneck. Preserve ordering per partition, monitor lag age and drain rate, and state how replay or backlog recovery avoids duplicate effects.",
        );
    }

    if has_any(
        normalized_question,
        &["late event", "late events", "out of order", "out-of-order"],
    ) {
        append(
            "Late-event interview contract: use event time, a stated watermark or allowed-lateness policy, and idempotent event IDs. Define whether late data updates prior results, emits a correction, or is routed for reconciliation; do not silently treat arrival time as business time. Make replay and backfill deterministic and observable.",
        );
    }

    if has_any(
        normalized_question,
        &["breaking schema", "schema breaking", "breaking change"],
    ) && has_any(
        normalized_question,
        &[
            "independent deploy",
            "independently deploy",
            "deploy independently",
            "independent rollout",
            "separate deploy",
        ],
    ) {
        append(
            "Independent-deploy schema contract: start exactly with `I would use an expand-contract rollout with an explicit schema version.` Then validate compatibility in a registry or deployment gate and preserve backward and forward compatibility. Add tolerant readers and new optional fields first, deploy producers and consumers independently, dual-read or dual-write and backfill when needed, monitor usage of the old version, and remove it only after every reader is migrated. Do not require lockstep deployment for a breaking change.",
        );
    }

    if has_all(normalized_question, &["kafka", "warehouse"])
        && has_any(normalized_question, &["exactly once", "exactly-once"])
    {
        append(
            "Kafka-to-warehouse exactly-once boundary contract: scope Kafka transactions to Kafka, not to an external warehouse. Describe at-least-once delivery into the warehouse with idempotent exactly-once effects, using a durable source event ID and an atomic dedupe or merge boundary. Coordinate checkpoint or offset progress only after the warehouse effect is durable, and explain replay after a crash.",
        );
    }

    if has_any(
        normalized_question,
        &[
            "pipeline data validation",
            "data pipeline validation",
            "validate data pipeline",
            "pipeline validation",
            "prove the data was correct",
            "verify the data was correct",
            "data correctness",
            "downstream teams trusted",
        ],
    ) {
        append(
            "Pipeline data-validation contract: validate contracts at ingestion and critical transforms, including schema, null or range rules, uniqueness, referential integrity where applicable, freshness, volume, and distribution drift. Quarantine bad records with reason codes, preserve lineage and raw evidence, alert on material failures, and make correction plus replay safe and idempotent. Never silently drop invalid data.",
        );
    }

    if has_any(
        normalized_question,
        &["rag", "retrieval augmented generation"],
    ) && has_any(
        normalized_question,
        &[
            "hallucination",
            "hallucinate",
            "grounding control",
            "grounded answer",
        ],
    ) {
        append(
            "RAG hallucination-control contract: retrieve only authorized, relevant evidence; require answer claims to be grounded in that evidence with verifiable citations; and abstain or ask for clarification when evidence is insufficient or conflicting. Defend retrieval and generation against prompt injection, evaluate retrieval and faithfulness separately on representative no-answer and adversarial slices, and monitor groundedness after launch. Do not promise zero hallucinations.",
        );
    }

    if normalized_question.contains("fraud")
        && (has_any(
            normalized_question,
            &[
                "class imbalance",
                "imbalanced class",
                "imbalanced data",
                "misses expensive fraud",
            ],
        ) || has_all(normalized_question, &["cost sensitive", "threshold"])
            || has_all(normalized_question, &["cost-sensitive", "threshold"])
            || has_all(normalized_question, &["accuracy", "misses"]))
    {
        append(
            "Fraud class-imbalance contract: do not optimize accuracy alone. Use temporally valid evaluation with precision-recall and cost-weighted error analysis, then choose and calibrate the operating threshold against false-positive, false-negative, review-capacity, and customer-friction costs. Use class weighting or carefully bounded resampling only within training folds, and monitor calibration, outcomes, and drift.",
        );
    }

    if normalized_question.contains("fraud") && has_any(normalized_question, &["graph", "network"])
    {
        append(
            "Graph-fraud point-in-time contract: answer in first person and explicitly explain that connected fraud rings are visible through shared entities, neighborhoods, paths, or communities even when ordinary per-transaction aggregates look normal. Construct each graph feature from a snapshot as of the decision timestamp, using only edges, node attributes, labels, and availability times known then. Split train, validation, and test chronologically, prevent future labels or post-decision edges from propagating through the graph, and reproduce the same as-of feature logic online. Random graph splits alone do not prove leakage safety.",
        );
    }

    if has_any(
        normalized_question,
        &[
            "first 90 days",
            "first ninety days",
            "90 day plan",
            "90-day plan",
        ],
    ) && has_any(
        normalized_question,
        &["role", "position", "job", "team", "target role"],
    ) {
        let supplied_hpe_datacenter_telemetry =
            target_job_description_has_all(answer_context, &["hpe", "datacenter", "telemetry"]);
        append(
            "First-90-days role-plan contract: the opening sentence must explicitly name the supplied target employer and its role domain; do not replace them with generic `this role` language. Then tailor a 30-60-90 progression to that evidence. Start with a stakeholder map, access and domain discovery, and a baseline of current production quality, latency, reliability, and success metrics; then deliver one small validated improvement with explicit success and rollback criteria; then scale an agreed roadmap with measurable outcomes. State assumptions or questions when role context is absent, and do not invent prior-company stories, achievements, or relationships.",
        );
        if supplied_hpe_datacenter_telemetry {
            append(
                "Supplied-role anchor: the source context names HPE and datacenter telemetry. Say `HPE` and `datacenter telemetry` explicitly in the opening sentence, while keeping every claim about the candidate grounded in the supplied resume.",
            );
        }
    }

    if matched_theme {
        instructions.push_str(
            "\nInterview grounding: present unsupported technical scenarios as `My approach would be...`; do not require, imply, or invent the candidate's personal history, employers, projects, metrics, or outcomes.",
        );
    }
}

fn has_any(text: &str, phrases: &[&str]) -> bool {
    phrases.iter().any(|phrase| text.contains(phrase))
}

fn has_all(text: &str, phrases: &[&str]) -> bool {
    phrases.iter().all(|phrase| text.contains(phrase))
}

fn target_job_description_has_all(contexts: &[AnswerContext], phrases: &[&str]) -> bool {
    contexts
        .iter()
        .filter(|context| context.role == AnswerContextRole::JobDescription)
        .any(|context| has_all(&normalize_guardrail_text(&context.content), phrases))
}

#[cfg(test)]
mod tests {
    use super::super::{normalize_guardrail_text, AnswerIntent, AnswerOutput};
    use super::*;
    use cue_core::AnswerContextKind;

    fn interview_plan() -> AnswerPlan {
        AnswerPlan {
            intent: AnswerIntent::General,
            output: AnswerOutput::InterviewAnswer,
            recommended_lane: "balanced",
            confidence: 0.9,
            interview_context: true,
            needs_screen: false,
            needs_docs: false,
            needs_transcript: false,
            needs_memory: false,
            needs_web_search: false,
        }
    }

    #[test]
    fn appends_contract_for_each_exact_interview_theme() {
        let cases = [
            (
                "How would you diagnose Kafka consumer lag?",
                "Kafka lag interview contract",
            ),
            (
                "How do you handle late and out-of-order events?",
                "Late-event interview contract",
            ),
            (
                "How do you roll out a breaking schema change when producers and consumers deploy independently?",
                "Independent-deploy schema contract",
            ),
            (
                "Where are the Kafka-to-warehouse exactly-once boundaries?",
                "Kafka-to-warehouse exactly-once boundary contract",
            ),
            (
                "How would you do pipeline data validation?",
                "Pipeline data-validation contract",
            ),
            (
                "What RAG hallucination controls would you use?",
                "RAG hallucination-control contract",
            ),
            (
                "How do you set a cost-sensitive threshold for class imbalance in fraud?",
                "Fraud class-imbalance contract",
            ),
            (
                "How do you avoid point-in-time leakage in a graph fraud model?",
                "Graph-fraud point-in-time contract",
            ),
            (
                "What is your first 90 days plan for this target role?",
                "First-90-days role-plan contract",
            ),
        ];

        for (question, marker) in cases {
            let mut instructions = String::new();
            append_interview_correctness_contracts(
                &mut instructions,
                &normalize_guardrail_text(question),
                &[],
                &interview_plan(),
            );
            assert!(instructions.contains(marker), "{question}: {instructions}");
            assert!(instructions.contains("Interview grounding"));
        }
    }

    #[test]
    fn ordinary_or_non_interview_questions_are_noops() {
        let mut ordinary = String::from("base");
        append_interview_correctness_contracts(
            &mut ordinary,
            "what is the weather today",
            &[],
            &interview_plan(),
        );
        assert_eq!(ordinary, "base");

        let mut non_interview = String::from("base");
        let mut plan = interview_plan();
        plan.interview_context = false;
        append_interview_correctness_contracts(
            &mut non_interview,
            "how would you diagnose kafka consumer lag",
            &[],
            &plan,
        );
        assert_eq!(non_interview, "base");

        let mut writing = String::from("base");
        let mut plan = interview_plan();
        plan.intent = AnswerIntent::Writing;
        append_interview_correctness_contracts(
            &mut writing,
            "rewrite this resume bullet about reducing rag hallucinations",
            &[],
            &plan,
        );
        assert_eq!(writing, "base");

        let mut compact_followup = String::from("base");
        let mut plan = interview_plan();
        plan.intent = AnswerIntent::FollowUp;
        plan.output = AnswerOutput::Compact;
        append_interview_correctness_contracts(
            &mut compact_followup,
            "rewrite this resume bullet about reducing rag hallucinations",
            &[],
            &plan,
        );
        assert_eq!(compact_followup, "base");
    }

    #[test]
    fn supplied_hpe_role_anchor_is_context_scoped() {
        let question = "Why this role, and what would you focus on in your first ninety days?";
        let hpe_jd = AnswerContext::new(
            AnswerContextKind::Document,
            "HPE datacenter telemetry platform",
        )
        .with_role(AnswerContextRole::JobDescription);
        let mut anchored = String::new();
        append_interview_correctness_contracts(
            &mut anchored,
            &normalize_guardrail_text(question),
            &[hpe_jd],
            &interview_plan(),
        );
        assert!(anchored.contains("Say `HPE` and `datacenter telemetry` explicitly"));

        let hpe_resume = AnswerContext::new(
            AnswerContextKind::Document,
            "Past work at HPE on datacenter telemetry",
        )
        .with_role(AnswerContextRole::CandidateResume);
        let mut generic = String::new();
        append_interview_correctness_contracts(
            &mut generic,
            &normalize_guardrail_text(question),
            &[hpe_resume],
            &interview_plan(),
        );
        assert!(generic.contains("First-90-days role-plan contract"));
        assert!(!generic.contains("Supplied-role anchor"));
    }
}

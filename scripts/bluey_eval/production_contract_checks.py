"""Deterministic production-answer contract self-checks."""

from __future__ import annotations

from typing import Callable, List

from .payment_contracts import (
    has_unsafe_ambiguous_payment_outcome,
    payment_operation_semantic_issues,
)
from .system_contracts import (
    feature_store_consistency_issues,
    payment_timeout_followup_completeness_issues,
    rag_evaluation_plan_issues,
    url_shortener_safety_issues,
)


def self_check_production_answer_contracts(
    q38_required_group_issues: Callable[[str], List[str]],
) -> None:
    shallow_rag = (
        "Use a golden dataset and score retrieval precision and faithfulness. "
        "Review the top 10% with humans. Target p95 latency under 2 seconds."
    )
    assert {
        "missing_retrieval_recall_or_ranking_metric",
        "missing_citation_correctness_evaluation",
        "missing_no_answer_refusal_evaluation",
        "unsafe_top_score_only_human_review",
        "unlabeled_numeric_rag_launch_target",
    }.issubset(set(rag_evaluation_plan_issues(shallow_rag)))
    assert "unlabeled_numeric_rag_launch_target" not in rag_evaluation_plan_issues(
        "Measure cost per 1,000 tokens and p95 latency against a named baseline."
    )
    complete_rag = (
        "Use a versioned golden set sliced by common, rare, no-answer, adversarial "
        "prompt-injection, ACL permission leakage, and PII privacy cases. Measure "
        "recall@k and nDCG, faithfulness, citation correctness, correct refusal, latency, "
        "and cost. Compare the champion baseline with predeclared per-slice launch gates "
        "and regression checks. Calibrate the judge against blinded human labels, report "
        "inter-rater agreement, and use stratified risk-weighted human review."
    )
    assert not rag_evaluation_plan_issues(complete_rag)
    assert not rag_evaluation_plan_issues(
        complete_rag.replace(
            "predeclared per-slice launch gates and regression checks",
            "a predeclared gate or threshold for every slice and regression checks",
        )
    )
    assert not rag_evaluation_plan_issues(
        complete_rag.replace(
            "Compare the champion baseline with predeclared per-slice launch gates and "
            "regression checks.",
            "Compare the candidate against a named champion on every slice, with an "
            "explicit failure gate on any critical-slice regression.",
        )
    )
    missing_critical_gate = complete_rag.replace(
        "Compare the champion baseline with predeclared per-slice launch gates and "
        "regression checks.",
        "Compare the candidate against a named champion on every slice.",
    )
    assert "missing_per_slice_launch_gates" in rag_evaluation_plan_issues(
        missing_critical_gate
    )
    unsafe_aggregate_gate = (
        "Use recall@k, citation correctness, no-answer refusal, adversarial prompt "
        "injection, ACL permissions, PII privacy, a champion baseline, judge calibration, "
        "human review, latency, cost, and regression. Per-slice thresholds are not "
        "required; an aggregate launch gate is enough."
    )
    assert "unsafe_negated_or_aggregate_only_slice_gate" in (
        rag_evaluation_plan_issues(unsafe_aggregate_gate)
    )
    saved_round539_q27 = (
        "Use a versioned, representative golden evaluation set with blinded human "
        "labels and slices for common queries, rare queries, no-answer or unanswerable "
        "prompts, adversarial prompt-injection, ACL or cross-tenant permission checks, "
        "and PII or privacy cases, then compare the candidate against a named champion "
        "on every slice. Measure retrieval recall@k plus MRR or nDCG, answer faithfulness "
        "and citation correctness, end-to-end task success, correct abstention, safety, "
        "latency, and cost, with an explicit failure gate on any critical-slice regression. "
        "Calibrate any automated judge against the human labels, report inter-rater "
        "agreement, and use stratified, risk-weighted human review. Require a documented "
        "baseline comparison and block rollout on a regression."
    )
    assert not rag_evaluation_plan_issues(saved_round539_q27)
    biased_review = complete_rag + " Review only the top 10% with humans."
    assert "unsafe_top_score_only_human_review" in rag_evaluation_plan_issues(
        biased_review
    )
    safely_rejected_bias = complete_rag + " Never review only the top 10% with humans."
    assert "unsafe_top_score_only_human_review" not in rag_evaluation_plan_issues(
        safely_rejected_bias
    )
    omitted_safety = complete_rag + (
        " Do not evaluate ACL permission leakage or PII privacy cases."
    )
    assert "unsafe_omitted_rag_safety_slice" in rag_evaluation_plan_issues(
        omitted_safety
    )
    for unsafe_suffix in (
        "However, a critical-slice regression does not block launch.",
        "We do not need per-slice thresholds; use the overall score.",
    ):
        assert "unsafe_negated_or_aggregate_only_slice_gate" in (
            rag_evaluation_plan_issues(complete_rag + " " + unsafe_suffix)
        ), unsafe_suffix
    for unsafe_suffix in (
        "Do not measure citation correctness.",
        "PII and privacy cases are unnecessary.",
    ):
        assert "unsafe_omitted_rag_safety_slice" in (
            rag_evaluation_plan_issues(complete_rag + " " + unsafe_suffix)
        ), unsafe_suffix
    for safe_suffix in (
        "Do not omit PII privacy cases.",
        "Never skip ACL permission leakage tests.",
        "Do not use only an aggregate launch gate; every slice gets a threshold.",
        "Per-slice gates are optional for noncritical informational slices, but "
        "mandatory for every critical slice.",
    ):
        assert not {
            "unsafe_negated_or_aggregate_only_slice_gate",
            "unsafe_omitted_rag_safety_slice",
        }.intersection(rag_evaluation_plan_issues(complete_rag + " " + safe_suffix)), safe_suffix

    shallow_store = (
        "Use online and offline stores, a stream processor, point-in-time training, "
        "backfills, and a registry with the same schema and transformations."
    )
    assert {
        "missing_shared_executable_feature_transformations",
        "missing_feature_event_time",
        "missing_feature_availability_time",
        "missing_point_in_time_join_mechanics",
        "missing_late_event_correction_policy",
        "missing_idempotent_feature_replay",
        "missing_online_offline_feature_skew_check",
    } == set(feature_store_consistency_issues(shallow_store))
    complete_store = (
        "One versioned executable feature code package is compiled for streaming and "
        "batch training jobs. Persist event-time and availability-time, then build "
        "training rows with an as-of join admitting both event-time and availability-time "
        "at or before prediction-time. A watermark defines late events; corrections "
        "trigger an idempotent backfill replay deduplicated by event-id and materialization "
        "version. Continuously compare online and offline values for skew and parity."
    )
    assert not feature_store_consistency_issues(complete_store)
    for contradiction in (
        "Do not use the same executable transformation code in streaming and batch.",
        "Both event-time and availability-time are not at or before decision-time.",
        "Batch and streaming must not use identical feature logic.",
    ):
        contradiction_issues = set(
            feature_store_consistency_issues(complete_store + " " + contradiction)
        )
        assert contradiction_issues, contradiction
        assert (
            "unsafe_independent_feature_transformations" in contradiction_issues
            or "unsafe_negated_feature_store_correctness" in contradiction_issues
        ), (contradiction, contradiction_issues)
    future_training = complete_store + (
        " Training may include records where event-time and availability-time exceed "
        "decision-time."
    )
    assert "unsafe_future_feature_availability_or_label_cutoff" in (
        feature_store_consistency_issues(future_training)
    )
    for unsafe_suffix in (
        "Online/offline parity checks are unnecessary.",
        "Correcting late events is unnecessary.",
    ):
        assert "unsafe_negated_feature_store_correctness" in (
            feature_store_consistency_issues(complete_store + " " + unsafe_suffix)
        ), unsafe_suffix
    for safe_suffix in (
        "Batch and streaming do not use different code.",
        "Streaming and batch transformations must not be implemented independently.",
        "Streaming and batch use different code artifacts compiled from the same "
        "versioned DSL.",
    ):
        safe_issues = set(feature_store_consistency_issues(complete_store + " " + safe_suffix))
        assert "unsafe_independent_feature_transformations" not in safe_issues, safe_suffix
        assert "missing_shared_executable_feature_transformations" not in safe_issues, safe_suffix
    live_like_as_of_only = (
        "Use one versioned executable feature definition for streaming and batch. "
        "Persist event_time and availability_time. The online path admits values when "
        "both event_time and availability_time are at or before serving time. "
        "Online cache metadata and entity lookup behavior stay isolated. Build training rows with an "
        "as-of join at the decision timestamp. A watermark corrects late events with "
        "idempotent replay by event ID and version, and parity checks compare online "
        "values with offline recomputation."
    )
    assert "missing_point_in_time_join_mechanics" in feature_store_consistency_issues(
        live_like_as_of_only
    )
    explicit_training_predicate = (
        live_like_as_of_only
        + " For every training row, include a feature only when both its event-time and "
        "availability-time are at or before that row's decision timestamp."
    )
    assert not feature_store_consistency_issues(explicit_training_predicate)
    saved_round539_q38_mechanics = (
        "The online streaming path and offline batch path rebuild data from the same "
        "executable feature definitions. Persist event-time and availability-time. "
        "Training rows are built with an as-of join against the decision, prediction, "
        "or observation timestamp. A feature is eligible only if both event-time and "
        "availability-time are at or before that timestamp. A watermark handles late "
        "events through correction and idempotent replay by event ID and feature version. "
        "Continuously compare streaming and batch values with equivalence tests for "
        "parity and skew."
    )
    assert not feature_store_consistency_issues(saved_round539_q38_mechanics)
    snake_case_store = (
        "One versioned executable feature transformation compiles for streaming and "
        "batch training. Persist event_time and availability_time. Build rows with an "
        "as-of join on decision_time, admitting values only when event_time <= "
        "decision_time and availability_time <= decision_time. A watermark corrects "
        "late events through idempotent replay by event_id and feature version. "
        "Continuously compare online and offline values for parity and skew."
    )
    assert not feature_store_consistency_issues(snake_case_store)
    unsafe_snake_case_store = snake_case_store.replace(
        "availability_time <= decision_time",
        "availability_time may be after decision_time",
    )
    unsafe_snake_case_issues = set(
        feature_store_consistency_issues(unsafe_snake_case_store)
    )
    assert "unsafe_future_feature_availability_or_label_cutoff" in (
        unsafe_snake_case_issues
    )
    assert "missing_point_in_time_join_mechanics" in unsafe_snake_case_issues
    unsafe_unicode_comparator_store = snake_case_store.replace(
        "availability_time <= decision_time",
        "availability_time ≥ decision_time",
    )
    unsafe_unicode_issues = set(
        feature_store_consistency_issues(unsafe_unicode_comparator_store)
    )
    assert "unsafe_future_feature_availability_or_label_cutoff" in unsafe_unicode_issues
    assert "missing_point_in_time_join_mechanics" in unsafe_unicode_issues
    assert not q38_required_group_issues(saved_round539_q38_mechanics)
    leaking_anchored_store = saved_round539_q38_mechanics.replace(
        "both event-time and availability-time are at or before that timestamp",
        "event-time and availability-time may be after that timestamp",
    )
    leaking_anchored_issues = set(
        feature_store_consistency_issues(leaking_anchored_store)
    )
    assert "unsafe_future_feature_availability_or_label_cutoff" in (
        leaking_anchored_issues
    )
    assert "missing_point_in_time_join_mechanics" in leaking_anchored_issues
    coordinated_timestamp_store = (
        "Use versioned executable transformations for streaming and batch jobs. "
        "Persist event time and availability time. The training builder performs an "
        "as-of join using only values whose event and availability times are at or "
        "before the prediction cutoff. A watermark handles late events through "
        "idempotent correction and replay by event ID. Continuously compare online "
        "and offline values for parity and skew."
    )
    assert not feature_store_consistency_issues(coordinated_timestamp_store)
    paraphrased_store = (
        "A shared versioned transformation definition compiles into live serving and "
        "offline batch training. Persist the source event timestamp and knowledge "
        "timestamp. Build rows with a point-in-time join: source event timestamp and "
        "knowledge timestamp must each be no later than the decision timestamp. A "
        "watermark quarantines late events, then an idempotent replay deduplicates by "
        "event ID. Continuously run online/offline equivalence and skew checks."
    )
    assert not feature_store_consistency_issues(paraphrased_store)
    label_equals_decision_store = complete_store.replace(
        "prediction-time",
        "label timestamp, which is explicitly identical to the decision timestamp",
    )
    assert not feature_store_consistency_issues(label_equals_decision_store)
    for safe_label_equivalence in (
        "label timestamp, which equals the decision timestamp",
        "label timestamp, explicitly defined as the decision timestamp",
        "label timestamp, defined to be exactly the decision timestamp",
        "label timestamp, which serves as the decision timestamp",
        "label timestamp, which coincides exactly with the decision timestamp",
    ):
        assert not feature_store_consistency_issues(
            complete_store.replace("prediction-time", safe_label_equivalence)
        ), safe_label_equivalence
    snapshot_store = (
        "A shared versioned executable transformation compiles for live serving and "
        "offline batch training. Persist the source event timestamp and knowledge "
        "timestamp. A snapshot join requires the source event timestamp to precede "
        "request time and the knowledge timestamp to be earlier than request time. A "
        "watermark corrects late events through an idempotent replay deduplicated by "
        "event ID. Continuously compare online/offline values for parity and skew."
    )
    assert not feature_store_consistency_issues(snapshot_store)
    negated_coordinated_timestamps = coordinated_timestamp_store.replace(
        "event and availability times are at or before the prediction cutoff",
        "event and availability times are not required to be at or before the prediction cutoff",
    )
    negated_coordinated_issues = set(
        feature_store_consistency_issues(negated_coordinated_timestamps)
    )
    assert "missing_point_in_time_join_mechanics" in negated_coordinated_issues
    assert "unsafe_negated_feature_store_correctness" in negated_coordinated_issues
    for unsafe_wording in (
        "event and availability times need not be at or before the prediction cutoff",
        "event and availability times are not necessarily at or before the prediction cutoff",
        "event and availability times can be before or after the prediction cutoff",
    ):
        unsafe_store = coordinated_timestamp_store.replace(
            "event and availability times are at or before the prediction cutoff",
            unsafe_wording,
        )
        unsafe_issues = set(feature_store_consistency_issues(unsafe_store))
        assert "missing_point_in_time_join_mechanics" in unsafe_issues
        assert "unsafe_negated_feature_store_correctness" in unsafe_issues
    for unsafe_wording in (
        "The availability timestamp may be after the prediction timestamp.",
        "Include features whose ingestion time is later than decision time.",
        "Use features not available until after the observation timestamp.",
        "Availability-time is unconstrained.",
        "Future availability-time is allowed in training rows.",
        "Availability time need not be before prediction time.",
        "Availability timestamps are not required to precede decision time.",
        "Feature values may arrive after prediction and are still included.",
        "Values remain eligible even when unavailable at scoring time.",
        "Availability timestamp may lag prediction timestamp and we still include it.",
        "Future feature values are allowed in training rows.",
        "We do not exclude values unavailable at prediction time.",
    ):
        unsafe_issues = set(
            feature_store_consistency_issues(complete_store + " " + unsafe_wording)
        )
        assert "unsafe_future_feature_availability_or_label_cutoff" in unsafe_issues
        assert "missing_point_in_time_join_mechanics" in unsafe_issues
    for unsafe_label_boundary in (
        "The as-of join admits event-time and availability-time at or before label cutoff.",
        "Both event-time and availability-time are bounded by the outcome timestamp.",
        "The as-of join admits event-time and availability-time at or before label "
        "timestamp, which is not the decision timestamp.",
        "The as-of join admits event-time and availability-time at or before label "
        "timestamp, which is not identical to the decision timestamp.",
        "The as-of join admits event-time and availability-time at or before label "
        "timestamp, which differs from the decision timestamp.",
        "The as-of join admits event-time and availability-time at or before label "
        "timestamp, which is unrelated to the decision timestamp.",
        "The as-of join admits event-time and availability-time at or before label "
        "timestamp, which is never identical to the decision timestamp.",
        "The as-of join admits event-time and availability-time at or before label "
        "timestamp, which is approximately the decision timestamp.",
    ):
        unsafe_issues = set(
            feature_store_consistency_issues(
                complete_store.replace(
                    "admitting both event-time and availability-time at or before prediction-time",
                    unsafe_label_boundary,
                )
            )
        )
        assert "unsafe_future_feature_availability_or_label_cutoff" in unsafe_issues
        assert "missing_point_in_time_join_mechanics" in unsafe_issues
    assert "unsafe_future_feature_availability_or_label_cutoff" not in (
        feature_store_consistency_issues(
            complete_store
            + " Never use a feature whose availability time is after prediction time."
        )
    )
    for safe_future_rejection in (
        "Never include values whose availability time is after prediction time.",
        "Exclude values unavailable at prediction time.",
    ):
        assert "unsafe_future_feature_availability_or_label_cutoff" not in (
            feature_store_consistency_issues(complete_store + " " + safe_future_rejection)
        ), safe_future_rejection
    for independent_wording in (
        "Streaming and batch transformations are implemented independently.",
        "Duplicate transformation logic separately.",
        "The registry shares schemas only; each path has its own implementation.",
        "Batch and streaming use different code as long as schemas match.",
        "We do not compile or share executable transformations between paths.",
    ):
        independent_issues = set(
            feature_store_consistency_issues(complete_store + " " + independent_wording)
        )
        assert "unsafe_independent_feature_transformations" in independent_issues
        assert "missing_shared_executable_feature_transformations" in independent_issues
    alternate_store_wording = (
        "We define executable feature transformations once and compile them for batch "
        "training and stream serving. Run an equivalence test between batch training "
        "outputs and live serving outputs."
    )
    alternate_store_issues = set(feature_store_consistency_issues(alternate_store_wording))
    assert "missing_shared_executable_feature_transformations" not in alternate_store_issues
    assert "missing_online_offline_feature_skew_check" not in alternate_store_issues
    dsl_store_wording = (
        "Define every feature once in a versioned DSL and compile that definition into "
        "both streaming and batch jobs."
    )
    assert "missing_shared_executable_feature_transformations" not in (
        feature_store_consistency_issues(dsl_store_wording)
    )
    invented_store_slos = (
        "Serve at p99 under 50ms and ingest 10k events per second with the feature store."
    )
    assert "unlabeled_numeric_feature_store_target" in feature_store_consistency_issues(
        invented_store_slos
    )
    assert "unlabeled_numeric_feature_store_target" not in feature_store_consistency_issues(
        "Assumption: serve at p99 under 50ms and ingest 10k events per second."
    )
    negated_store = complete_store + (
        " Do not share executable transformation code. Do not persist event-time or "
        "availability-time, skip the as-of join, do not correct late events, make replay "
        "and backfill non-idempotent without dedup, and do not compare online and offline "
        "values for skew or parity."
    )
    assert "unsafe_negated_feature_store_correctness" in (
        feature_store_consistency_issues(negated_store)
    )
    safe_join_warning = complete_store + " Never skip the as-of join."
    assert "unsafe_negated_feature_store_correctness" not in (
        feature_store_consistency_issues(safe_join_warning)
    )
    leaking_store = complete_store.replace(
        "admitting both event-time and availability-time at or before prediction-time",
        "where event-time is before prediction-time but availability-time is not filtered",
    )
    leaking_issues = set(feature_store_consistency_issues(leaking_store))
    assert "missing_point_in_time_join_mechanics" in leaking_issues
    assert "unsafe_negated_feature_store_correctness" in leaking_issues

    safe_url_design = (
        "Active mutable mappings use 302 or 307 with bounded cache freshness. "
        "Revocable redirects return Cache-Control: no-store so browsers, clients, "
        "intermediaries, and CDNs cannot retain a positive redirect. "
        "Deleted or expired mappings return 404 or 410, and abuse-blocked mappings "
        "return 403 or a safe warning interstitial. A court-ordered legal block returns "
        "451. Deletion, expiration, abuse blocking, and legal blocking synchronously "
        "commit a deny tombstone and purge internal caches before acknowledgment; "
        "never redirect inactive mappings to the stored destination. Public links use "
        "302 or 307 even when the destination is immutable; never use 301 or 308 because "
        "browser caches are outside the revocation boundary."
    )
    assert not url_shortener_safety_issues(
        safe_url_design,
        require_revocation_completeness=True,
    )
    saved_round548_redirect_wording = (
        "Revocable public links use 302 or 307 with Cache-Control: no-store, "
        "never 301 or 308. Deleted links synchronously publish a deny tombstone "
        "before acknowledgement, and every redirect worker fails closed to an "
        "authoritative state check when overlay state is uncertain."
    )
    assert not url_shortener_safety_issues(
        saved_round548_redirect_wording,
        require_revocation_completeness=True,
    )
    for unsafe_permanent_wording in (
        "Revocable public links use 301.",
        "Immutable public links use 301 even though they may later be abuse-blocked.",
        "Revocable public links use 301, but never 308.",
    ):
        assert "unsafe_permanent_redirect_for_revocable_link" in (
            url_shortener_safety_issues(
                unsafe_permanent_wording,
                require_revocation_completeness=True,
            )
        ), unsafe_permanent_wording
    unsafe_http_cache = safe_url_design.replace(
        "Cache-Control: no-store",
        "Cache-Control: public, max-age=3600",
    )
    unsafe_http_cache_issues = set(
        url_shortener_safety_issues(
            unsafe_http_cache,
            require_revocation_completeness=True,
        )
    )
    assert "unsafe_revocable_redirect_http_cache_window" in unsafe_http_cache_issues
    assert "missing_revocable_redirect_client_cache_boundary" in unsafe_http_cache_issues
    missing_client_boundary = safe_url_design.replace(
        "Revocable redirects return Cache-Control: no-store so browsers, clients, "
        "intermediaries, and CDNs cannot retain a positive redirect. ",
        "",
    )
    assert "missing_revocable_redirect_client_cache_boundary" in (
        url_shortener_safety_issues(
            missing_client_boundary,
            require_revocation_completeness=True,
        )
    )
    missing_revocation_barrier = (
        "Active mappings use 302 or 307 with Cache-Control: no-store. "
        "Deleted links return 410, expired links return 410, abuse-blocked links return "
        "403, and legal blocks return 451."
    )
    assert "missing_inactive_state_revocation_barrier" in (
        url_shortener_safety_issues(
            missing_revocation_barrier,
            require_revocation_completeness=True,
        )
    )
    live_tombstone_barrier = (
        "Revocable redirects use 302 or 307 with Cache-Control: no-store. Inactive "
        "states synchronously publish a tombstone before acknowledging the transition. "
        "Redirectors enforce that tombstone and fail closed until authoritative state "
        "is confirmed."
    )
    assert not url_shortener_safety_issues(
        live_tombstone_barrier,
        require_revocation_completeness=True,
    )
    exact_worker_boundary = (
        "For public links that can expire, be deleted, blocked, or legally removed, "
        "use 302 or 307 with Cache-Control: no-store. On deletion, synchronously write "
        "a versioned deny overlay before acknowledgement. Every redirect worker checks "
        "the versioned deny overlay before serving any cached active mapping and fails "
        "closed to an authoritative state check or non-redirect response when overlay "
        "or cache state is uncertain."
    )
    assert not url_shortener_safety_issues(
        exact_worker_boundary,
        require_revocation_completeness=True,
    )
    # Exact Q41 live-answer shape: a per-request deny-overlay boundary remains safe
    # when unrelated click analytics are asynchronous in a later sentence.
    live_q41_per_request_boundary = (
        "Revocable redirects use 302 or 307 with Cache-Control: no-store. For "
        "redirects, serve a cached mapping only when it is fresh and not blocked; "
        "otherwise fall back to the authoritative store. The redirect path checks the "
        "versioned deny overlay first, then cache, then authoritative store. Every "
        "redirect worker fails closed to an authoritative state check or non-redirect "
        "response when overlay or cache state is uncertain. Deleted or expired mappings "
        "return 404 or 410; abuse-blocked mappings return 403 or a safe interstitial; "
        "legal blocks return 451. Click analytics are emitted asynchronously."
    )
    assert not url_shortener_safety_issues(
        live_q41_per_request_boundary,
        require_revocation_completeness=True,
    )
    get_path_only_boundary = (
        "Revocable redirects use 302 or 307 with Cache-Control: no-store. GET checks "
        "the versioned deny overlay before cache, and uncertain state fails closed. "
        "Deleted, expired, abuse-blocked, and legally blocked mappings never redirect."
    )
    assert "missing_inactive_state_revocation_barrier" in url_shortener_safety_issues(
        get_path_only_boundary,
        require_revocation_completeness=True,
    )
    fleet_wide_boundary = (
        get_path_only_boundary
        + " Every redirect worker checks the versioned deny overlay before serving any "
        "cached active mapping and fails closed to an authoritative state check or "
        "non-redirect response when overlay or cache state is uncertain."
    )
    assert not url_shortener_safety_issues(
        fleet_wide_boundary,
        require_revocation_completeness=True,
    )
    for unsafe_q41_overlay_boundary in (
        live_q41_per_request_boundary.replace(
            "checks the versioned deny overlay first, then cache",
            "reads cache before checking the versioned deny overlay",
        ),
        live_q41_per_request_boundary.replace(
            "fails closed to an authoritative state check or non-redirect response",
            "fails open to the cached active mapping",
        ),
        f"{live_q41_per_request_boundary} The deny overlay is not enforced.",
        f"{live_q41_per_request_boundary} Redirect workers never enforce the deny overlay.",
        f"{live_q41_per_request_boundary} No redirect worker applies the deny overlay.",
        f"{live_q41_per_request_boundary} Deny overlay enforcement is disabled.",
    ):
        assert "missing_inactive_state_revocation_barrier" in (
            url_shortener_safety_issues(
                unsafe_q41_overlay_boundary,
                require_revocation_completeness=True,
            )
        ), unsafe_q41_overlay_boundary
    for explicitly_enforced_q41_boundary in (
        f"{live_q41_per_request_boundary} The deny overlay is enforced on every redirect.",
        f"{live_q41_per_request_boundary} Deny overlay enforcement is mandatory.",
        f"{live_q41_per_request_boundary} Redirect workers do not skip enforcement of "
        "the deny overlay.",
    ):
        assert not url_shortener_safety_issues(
            explicitly_enforced_q41_boundary,
            require_revocation_completeness=True,
        ), explicitly_enforced_q41_boundary
    unsafe_lifecycle_qualification = exact_worker_boundary.replace(
        "use 302 or 307 with Cache-Control: no-store",
        "use 302 or 307 with Cache-Control: no-store even after they are deleted",
    )
    assert "unsafe_redirect_for_inactive_or_blocked_link" in (
        url_shortener_safety_issues(
            unsafe_lifecycle_qualification,
            require_revocation_completeness=True,
        )
    )
    for unsafe_worker_boundary in (
        "Every redirect worker fails open to cached destinations.",
        "Every redirect worker uses fail-open behavior and serves cached destinations.",
        "Every redirect worker does not check the deny overlay and serves cached destinations.",
        "Every redirect worker eventually checks the deny overlay and serves cached "
        "destinations meanwhile.",
    ):
        assert "missing_inactive_state_revocation_barrier" in (
            url_shortener_safety_issues(
                f"{exact_worker_boundary} {unsafe_worker_boundary}",
                require_revocation_completeness=True,
            )
        ), unsafe_worker_boundary
    negated_live_tombstone_barrier = (
        "Revocable redirects use 302 or 307 with Cache-Control: no-store. For deleted "
        "links, tombstone propagation is asynchronous; do not fail closed until "
        "authoritative state is confirmed."
    )
    assert "missing_inactive_state_revocation_barrier" in (
        url_shortener_safety_issues(
            negated_live_tombstone_barrier,
            require_revocation_completeness=True,
        )
    )
    url_adversarial_cases = (
        (
            "Return HTTP 301 for all active links. Destinations never change. "
            "Cache-Control: no-store. On deletion, synchronously write a tombstone "
            "and purge the redirect cache before acknowledgement.",
            "unsafe_permanent_redirect_for_revocable_link",
        ),
        (
            "Revocable redirects use 302 and Cache-Control: no-store. On deletion, "
            "write the tombstone asynchronously and purge later. Active updates are "
            "synchronous and fail closed.",
            "missing_inactive_state_revocation_barrier",
        ),
        (
            "Blocked links map to the original URL. Cache-Control: no-store. On deletion, "
            "synchronously write a tombstone and purge the redirect cache before acknowledgement.",
            "unsafe_redirect_for_inactive_or_blocked_link",
        ),
        (
            "Revocable redirects use 302. Surrogate-Control: max-age=3600. On deletion, "
            "synchronously write a tombstone and purge the redirect cache before acknowledgement.",
            "unsafe_revocable_redirect_http_cache_window",
        ),
        (
            "Revocable redirects use 302. The CDN cache TTL is one hour. On deletion, "
            "synchronously write a tombstone and purge the redirect cache before acknowledgement.",
            "unsafe_revocable_redirect_http_cache_window",
        ),
        (
            "Revocable redirects use 302. Do not use Cache-Control: no-store; use "
            "Cache-Control: public. On deletion, synchronously write a tombstone and "
            "purge the redirect cache before acknowledgement.",
            "missing_revocable_redirect_client_cache_boundary",
        ),
        (
            "Revocable redirects use 302 and Cache-Control: no-store. Do not write a "
            "tombstone for deletion; deletion propagation is asynchronous. Active "
            "updates are synchronous and purge cache.",
            "missing_inactive_state_revocation_barrier",
        ),
        (
            "Revocable redirects use 302 and Cache-Control: no-store. On deletion, "
            "synchronously do not purge the tombstone or redirect cache before acknowledgement.",
            "missing_inactive_state_revocation_barrier",
        ),
        (
            "Revocable redirects use 302 and Cache-Control: no-store. On deletion, "
            "synchronously publish a tombstone before acknowledging the transition. "
            "Redirectors ignore the tombstone and continue to return cached destinations.",
            "missing_inactive_state_revocation_barrier",
        ),
        (
            "Revocable redirects use 302 and Cache-Control: no-store. On deletion, "
            "synchronously publish a tombstone before acknowledging the transition. "
            "Redirectors consume the tombstone asynchronously and may continue to "
            "return cached destinations.",
            "missing_inactive_state_revocation_barrier",
        ),
        (
            "Revocable redirects use 302 with Cache-Control: no-store. On deletion, "
            "synchronously publish a tombstone before acknowledgement. Redirectors "
            "check the tombstone only during their eventual refresh and may serve the "
            "cached destination until then.",
            "missing_inactive_state_revocation_barrier",
        ),
        (
            "Revocable redirects use 302 with Cache-Control: no-store. On deletion, "
            "synchronously publish a tombstone before acknowledgement. Redirectors "
            "check the tombstone, but a network partition means they fail open to "
            "cached destinations.",
            "missing_inactive_state_revocation_barrier",
        ),
    )
    for value, expected_issue in url_adversarial_cases:
        assert expected_issue in url_shortener_safety_issues(
            value,
            require_revocation_completeness=True,
        ), value
    safe_negated_cache = (
        "Revocable redirects use 302 with Cache-Control: no-store. Never send "
        "Cache-Control: max-age=3600. On deletion, synchronously write a tombstone "
        "and purge the redirect cache before acknowledgement."
    )
    assert not url_shortener_safety_issues(
        safe_negated_cache,
        require_revocation_completeness=True,
    )
    ordinary_update_staleness = (
        "Cache propagation may be eventually consistent for ordinary active target "
        "updates with bounded freshness. Deletion, expiration, and blocking synchronously "
        "purge the cached redirect and commit a safety tombstone before acknowledgment."
    )
    assert not url_shortener_safety_issues(ordinary_update_staleness)
    assert not url_shortener_safety_issues(
        "Allow bounded cache staleness after active-to-active target updates, but never "
        "after deletion, expiration, abuse blocking, or legal blocking."
    )
    assert not url_shortener_safety_issues(
        "Do not allow cache staleness after deletion, expiration, abuse blocking, or "
        "legal blocking; synchronously purge redirects and commit a tombstone."
    )
    for unsafe_inactive_staleness in (
        "Eventual consistency for cache and analytics allows temporary staleness after "
        "updates or deletes. Abusive or expired links must never redirect.",
        "Cache invalidation after deletion is eventual, although deleted links never redirect.",
        "Cache propagation is eventually consistent following deletion, although deleted links never redirect.",
        "After deletion, propagate invalidation asynchronously and serve 410 once it arrives.",
        "Blocked mappings may continue serving a stale cached redirect until the TTL "
        "expires, even though inactive links should never redirect.",
    ):
        assert "unsafe_inactive_state_cache_staleness" in (
            url_shortener_safety_issues(unsafe_inactive_staleness)
        ), unsafe_inactive_staleness
    unsafe_url_design = (
        "For deleted, expired, or blocked links, return 302 or 307 with a tombstone. "
        "Provider timeout after redirect update, reconcile by identifier and do not "
        "assume terminal failure."
    )
    assert set(url_shortener_safety_issues(unsafe_url_design)) == {
        "unsafe_redirect_for_inactive_or_blocked_link",
        "irrelevant_payment_reconciliation_in_url_design",
    }
    assert not url_shortener_safety_issues(
        "Deleted links never return 302; they return 410 and purge cached mappings."
    )
    for safe_url_wording in (
        "Blocked links do not return 307; they return 403 and a safe interstitial.",
        "For an expired mapping, invalidate the cached 302 redirect and return 410.",
        "Public short links use 302 or 307, not 301 or 308, even for immutable targets.",
        "Mutable mappings avoid 301 and 308.",
        "A separately scoped non-revocable immutable alias may use 301 only when it accepts "
        "the browser cache risk and is excluded from deletion and legal revocation.",
        "On a CDN provider timeout, serve the cached active redirect or fall back to origin.",
        "Never use 451 for generic abuse; reserve it for legally unavailable links.",
        "A client idempotency key protects creation of a short-link mapping from retries.",
        "When creating the URL mapping, require an idempotency key for duplicate requests.",
        "Abuse-blocked links use 302 to a safe warning interstitial, never stored destination.",
    ):
        assert not url_shortener_safety_issues(safe_url_wording), safe_url_wording
    unsafe_url_wording = (
        ("Blocked links get a 307 to the stored destination.", "unsafe_redirect_for_inactive_or_blocked_link"),
        ("Redirect expired mappings with 302.", "unsafe_redirect_for_inactive_or_blocked_link"),
        ("Deleted mappings still resolve to their stored destination with HTTP 302.", "unsafe_redirect_for_inactive_or_blocked_link"),
        ("Expired links continue redirecting to the target with 307.", "unsafe_redirect_for_inactive_or_blocked_link"),
        ("Suspended and quarantined links redirect to the original destination.", "unsafe_redirect_for_inactive_or_blocked_link"),
        ("Deleted mappings continue serving the stored destination.", "unsafe_redirect_for_inactive_or_blocked_link"),
        ("Expired links fall back to original URL.", "unsafe_redirect_for_inactive_or_blocked_link"),
        ("Blocked links send the user to stored target.", "unsafe_redirect_for_inactive_or_blocked_link"),
        ("Do not prevent redirects for deleted links.", "unsafe_redirect_for_inactive_or_blocked_link"),
        ("Mutable links return 301 for better caching.", "unsafe_permanent_redirect_for_mutable_link"),
        ("Use a 308 redirect when the mapping is mutable.", "unsafe_permanent_redirect_for_mutable_link"),
        ("Editable mappings use HTTP 301 even though their destination can change.", "unsafe_permanent_redirect_for_mutable_link"),
        ("Users can update destination, but redirects use 301.", "unsafe_permanent_redirect_for_mutable_link"),
        ("Reserve 301 or 308 for explicitly immutable mappings.", "unsafe_permanent_redirect_for_revocable_link"),
        ("Immutable public short links use permanent redirects even though abuse blocks can revoke them.", "unsafe_permanent_redirect_for_revocable_link"),
        ("Targets that never change return 301, but the service may delete or block any link.", "unsafe_permanent_redirect_for_revocable_link"),
        ("Abuse-blocked phishing links return 451.", "unsafe_451_for_generic_abuse_block"),
        ("Policy-blocked links return 451.", "unsafe_451_for_generic_abuse_block"),
        ("Fraudulent links return 451.", "unsafe_451_for_generic_abuse_block"),
        (
            "Provider timeout after redirect update; reconcile by identifier and keep the "
            "outcome UNKNOWN instead of assuming terminal failure.",
            "irrelevant_payment_reconciliation_in_url_design",
        ),
        (
            "Use the payment idempotency key in the URL redirect mapping.",
            "irrelevant_payment_reconciliation_in_url_design",
        ),
        (
            "On a PSP timeout after card capture, move the transaction to UNKNOWN and "
            "poll gateway status.",
            "irrelevant_payment_reconciliation_in_url_design",
        ),
    )
    for value, expected_issue in unsafe_url_wording:
        assert expected_issue in url_shortener_safety_issues(value), value

    shallow_payment = (
        "Move PROCESSING to UNKNOWN, stop retries, query provider status, and use "
        "deduplicated webhooks. Reuse the original key if the command is replayed."
    )
    assert set(payment_timeout_followup_completeness_issues(shallow_payment)) == {
        "missing_authoritative_unknown_terminal_transitions",
        "missing_unresolved_manual_reconciliation_policy",
        "missing_safe_same_operation_replay_condition",
    }
    complete_payment = (
        "UNKNOWN moves to SUCCEEDED or FAILED only from authoritative provider status or webhook "
        "evidence. If unresolved, remain UNKNOWN and escalate to a manual reconciliation "
        "workflow. Only if reconciliation remains inconclusive and the provider guarantees "
        "idempotency do I retry the same operation under a bounded policy with the "
        "original idempotency key."
    )
    assert not payment_timeout_followup_completeness_issues(complete_payment)
    prompt_contract_payment = (
        "I would transition the payment intent from PROCESSING to UNKNOWN and stop "
        "automatic charge retries. Provider status checks by payment ID and webhooks "
        "persisted under a database uniqueness constraint on provider event ID move "
        "UNKNOWN to SUCCEEDED, FAILED, or CANCELED only from authoritative evidence. "
        "Reconcile first. Only if the result remains inconclusive and the provider "
        "contract guarantees idempotent replay may the exact same provider command be "
        "retried under a bounded policy with the original operation's idempotency key, "
        "never a new key. If unresolved, keep it UNKNOWN and escalate to a manual "
        "reconciliation workflow; never release a second charge. The operation key is "
        "not the webhook deduplication key."
    )
    assert not has_unsafe_ambiguous_payment_outcome(prompt_contract_payment)
    assert not payment_operation_semantic_issues(
        prompt_contract_payment,
        require_webhook_event_dedup=False,
        require_complete_idempotency_semantics=False,
        require_same_operation_retry_reuse=True,
    )
    assert not payment_timeout_followup_completeness_issues(prompt_contract_payment)
    split_transition_payment = prompt_contract_payment.replace(
        "Provider status checks by payment ID and webhooks persisted under a database "
        "uniqueness constraint on provider event ID move UNKNOWN to SUCCEEDED, FAILED, "
        "or CANCELED only from authoritative evidence.",
        "UNKNOWN moves only from authoritative provider status or webhook evidence. "
        "A confirmed charge becomes SUCCEEDED. A confirmed no-charge result becomes "
        "FAILED. Webhooks are persisted under a database uniqueness constraint on "
        "provider event ID.",
    )
    assert not payment_timeout_followup_completeness_issues(split_transition_payment)
    timer_payment = prompt_contract_payment.replace(
        "Provider status checks by payment ID and webhooks persisted under a database "
        "uniqueness constraint on provider event ID move UNKNOWN to SUCCEEDED, FAILED, "
        "or CANCELED only from authoritative evidence.",
        "After a timeout, UNKNOWN moves to SUCCEEDED, FAILED, or CANCELED on our timer. "
        "Provider webhooks are persisted under a database uniqueness constraint on "
        "provider event ID.",
    )
    timer_issues = set(payment_timeout_followup_completeness_issues(timer_payment))
    assert "missing_authoritative_unknown_terminal_transitions" in timer_issues
    assert "unsafe_non_authoritative_unknown_terminal_transition" in timer_issues
    unsafe_extra_retry = prompt_contract_payment + (
        " Separately, retry the payment automatically every minute until it succeeds."
    )
    assert has_unsafe_ambiguous_payment_outcome(unsafe_extra_retry)
    active_safe_retry = prompt_contract_payment.replace(
        "Reconcile first. Only if the result remains inconclusive and the provider "
        "contract guarantees idempotent replay may the exact same provider command be "
        "retried under a bounded policy with the original operation's idempotency key, "
        "never a new key.",
        "Reconcile first. Only if the result remains inconclusive and the provider "
        "guarantees idempotency do I retry the payment with the original idempotency "
        "key under a bounded policy, never a new key.",
    )
    assert not has_unsafe_ambiguous_payment_outcome(active_safe_retry)
    saved_round539_q40 = (
        "I would transition the payment intent from PROCESSING to UNKNOWN and stop "
        "automatic charge retries. Then I would reconcile first by checking provider "
        "status using the payment ID or client reference, and by processing webhooks "
        "deduplicated under a database uniqueness constraint on provider event ID. Those "
        "authoritative signals can move UNKNOWN to SUCCEEDED, FAILED, or CANCELED only. "
        "If the result is still inconclusive, and the provider contract guarantees "
        "idempotent replay, I may retry the exact same provider command under a bounded "
        "policy with the original operation's idempotency key, never a new key. If it "
        "remains unresolved, I keep it UNKNOWN and escalate to manual reconciliation, "
        "and I never release a second charge."
    )
    assert not has_unsafe_ambiguous_payment_outcome(saved_round539_q40)
    assert not payment_operation_semantic_issues(
        saved_round539_q40,
        require_webhook_event_dedup=False,
        require_complete_idempotency_semantics=False,
        require_same_operation_retry_reuse=True,
    )
    assert not payment_timeout_followup_completeness_issues(saved_round539_q40)
    for unsafe_terminal_policy in (
        "After the reconciliation deadline, treat UNKNOWN as FAILED.",
        "A timer flips UNKNOWN to FAILED after 24 hours.",
    ):
        assert has_unsafe_ambiguous_payment_outcome(unsafe_terminal_policy), (
            unsafe_terminal_policy
        )
    for safe_terminal_rejection in (
        "UNKNOWN does not become FAILED after 24 hours.",
        "UNKNOWN is not considered FAILED when the deadline expires.",
        "A timer does not convert UNKNOWN into FAILED.",
    ):
        assert not has_unsafe_ambiguous_payment_outcome(safe_terminal_rejection), (
            safe_terminal_rejection
        )
    no_replay_policy = (
        "UNKNOWN moves to SUCCEEDED or FAILED only from authoritative provider status "
        "or webhooks deduplicated by provider event ID. If unresolved, remain UNKNOWN "
        "and escalate to a manual reconciliation workflow. Never retry the charge or "
        "resubmit the provider command."
    )
    assert not payment_operation_semantic_issues(
        no_replay_policy,
        require_webhook_event_dedup=False,
        require_complete_idempotency_semantics=False,
        require_same_operation_retry_reuse=True,
    )
    assert not payment_timeout_followup_completeness_issues(no_replay_policy)
    negated_capability = saved_round539_q40.replace(
        "the provider contract guarantees idempotent replay",
        "the provider contract guarantees that idempotent replay is not supported",
    )
    assert "missing_safe_same_operation_replay_condition" in (
        payment_timeout_followup_completeness_issues(negated_capability)
    )

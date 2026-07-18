"""RAG, feature-store, URL, and leadership answer-safety contracts."""

from __future__ import annotations

import re
from typing import List

from .leadership_contracts import q47_director_alignment_issues
from .payment_contracts import (
    has_explicit_no_provider_command_replay,
    has_safe_payment_same_operation_replay_condition,
)


def has_drift_only_automatic_retraining(text: str) -> bool:
    lower = re.sub(r"\s+", " ", text.casefold())
    pattern = re.compile(
        r"(?:automated|automatic)\s+retrain(?:ing)?|"
        r"automatically\s+(?:trigger\s+)?retrain(?:s|ed|ing)?|"
        r"trigger(?:s|ed|ing)?\s+(?:an?\s+)?automated\s+retraining"
    )
    for match in pattern.finditer(lower):
        window = lower[max(0, match.start() - 220) : match.end() + 260]
        if not re.search(r"(?:drift|distribution shift|threshold)", window):
            continue
        guarded = re.search(
            r"(?:labeled? outcome|ground truth|offline (?:evaluation|validation)|"
            r"holdout (?:evaluation|validation)|human approval|manual approval|"
            r"champion[- ]challenger|canary (?:evaluation|rollout)|shadow evaluation)",
            window,
        )
        if not guarded:
            return True
    return False


REQUIRED_SIGNAL_STEMS = frozenset(
    {
        "anomal",
        "calibrat",
        "clarif",
        "communicat",
        "deprecat",
        "dedup",
        "eval",
        "faithful",
        "hallucin",
        "idempot",
        "imbalanc",
        "observ",
        "quant",
        "reconcil",
        "reliab",
        "retriev",
    }
)


def has_required_signal(text: str, term: str) -> bool:
    """Match curated evidence as a token or intentional stem, not a substring."""
    lower = re.sub(r"\s+", " ", text.casefold().replace("_", " "))
    candidate = re.sub(r"\s+", " ", term.casefold().replace("_", " ").strip())
    if not candidate:
        return False
    if candidate in REQUIRED_SIGNAL_STEMS:
        return bool(re.search(rf"(?<!\w){re.escape(candidate)}\w*", lower))
    if re.fullmatch(r"[a-z0-9]+(?:[ -]+[a-z0-9]+)+", candidate):
        parts = re.findall(r"[a-z0-9]+", candidate)
        pattern = r"[\s/-]+".join(re.escape(part) for part in parts)
        return bool(re.search(rf"(?<!\w){pattern}(?!\w)", lower))
    if re.fullmatch(r"[a-z0-9]+", candidate):
        escaped = re.escape(candidate)
        if len(candidate) <= 3:
            pattern = rf"{escaped}(?:s)?"
        elif candidate.endswith("y"):
            pattern = rf"(?:{escaped}(?:s|ing)?|{re.escape(candidate[:-1])}(?:ies|ied))"
        elif candidate.endswith("e"):
            stem = re.escape(candidate[:-1])
            pattern = rf"(?:{escaped}(?:s|d)?|{stem}(?:ing|able))"
        else:
            pattern = rf"{escaped}(?:s|es|ed|ing)?"
        return bool(re.search(rf"(?<!\w)(?:{pattern})(?!\w)", lower))
    return bool(
        re.search(
            rf"(?<!\w){re.escape(candidate)}(?!\w)",
            lower,
        )
    )


def rag_evaluation_plan_issues(text: str) -> List[str]:
    """Require an actionable launch evaluation, not a keyword-only RAG sketch."""
    lower = re.sub(
        r"\s+",
        " ",
        re.sub(r"[*`~]+", "", text.casefold().replace("’", "'")),
    )
    issues: List[str] = []

    required_signals = (
        (
            "missing_retrieval_recall_or_ranking_metric",
            ("recall@k", "recall at k", "mrr", "mean reciprocal rank", "ndcg"),
        ),
        (
            "missing_citation_correctness_evaluation",
            ("citation correctness", "citation precision", "citation recall"),
        ),
        (
            "missing_no_answer_refusal_evaluation",
            ("no-answer", "no answer", "unanswerable", "refusal", "abstain"),
        ),
        (
            "missing_adversarial_rag_slice",
            ("adversarial", "prompt injection", "jailbreak"),
        ),
        (
            "missing_acl_isolation_rag_slice",
            ("acl", "permission", "tenant leakage", "cross-tenant"),
        ),
        (
            "missing_pii_privacy_rag_slice",
            ("pii", "privacy", "personal data"),
        ),
        (
            "missing_baseline_and_regression_gate",
            ("baseline", "champion", "regression"),
        ),
        (
            "missing_judge_human_calibration",
            ("judge calibration", "calibrate the judge", "inter-rater", "interrater"),
        ),
    )
    for issue, signals in required_signals:
        if not any(signal in lower for signal in signals):
            issues.append(issue)

    explicit_per_slice_gate = bool(
        re.search(
            r"\b(?:per[- ]slice|slice[- ]specific)\b.{0,60}"
            r"\b(?:gate|threshold|criterion|criteria)\w*\b|"
            r"\b(?:gate|threshold|criterion|criteria)\w*\b.{0,60}"
            r"\b(?:per[- ]slice|for\s+(?:each|every)\s+slice)\b|"
            r"\b(?:each|every)\s+slice\b.{0,60}"
            r"\b(?:gate|threshold|criterion|criteria)\w*\b",
            lower,
        )
    )
    champion_on_every_slice = bool(
        re.search(
            r"\b(?:compare|benchmark|evaluate)\w*\b.{0,90}"
            r"\b(?:candidate\b.{0,30})?(?:named\s+)?(?:champion|baseline)\b"
            r".{0,70}\b(?:on|across)\s+(?:each|every)\s+slice\b|"
            r"\b(?:champion|baseline)\b.{0,90}"
            r"\b(?:on|across)\s+(?:each|every)\s+slice\b",
            lower,
        )
    )
    critical_slice_failure_gate = bool(
        re.search(
            r"\b(?:failure|launch|release|rollout)\s+gate\b.{0,90}"
            r"\b(?:any|a)\s+critical[- ]slice\s+regression\b|"
            r"\b(?:fail|block|stop|reject)\w*\b.{0,70}\b(?:launch|release|rollout)\b"
            r".{0,90}\b(?:any|a)\s+critical[- ]slice\s+regression\b|"
            r"\b(?:any|a)\s+critical[- ]slice\s+regression\b.{0,90}"
            r"\b(?:failure|launch|release|rollout)\s+gate\b",
            lower,
        )
    )
    if not (
        explicit_per_slice_gate
        or (champion_on_every_slice and critical_slice_failure_gate)
    ):
        issues.append("missing_per_slice_launch_gates")

    negated_slice_gate = False
    for clause in re.split(r"(?<=[.!?;])\s+", lower):
        noncritical_exception = bool(
            re.search(r"\bnon[- ]?critical\b|\binformational\s+slices?\b", clause)
            and (
                re.search(
                    r"\b(?:critical|high[- ]risk)\b.{0,65}"
                    r"\b(?:mandatory|required|must|block)\w*\b",
                    clause,
                )
                or re.search(
                    r"\b(?:mandatory|required|must|block)\w*\b.{0,65}"
                    r"\b(?:critical|high[- ]risk)\b",
                    clause,
                )
            )
        )
        unsafe_clause = bool(
            re.search(
                r"\b(?:per[- ]slice|slice[- ]specific|for\s+(?:each|every)\s+slice)\b"
                r".{0,70}\b(?:gate|threshold|criterion|criteria)\w*\b.{0,45}"
                r"\b(?:not\s+required|unnecessary|optional|not\s+needed)\b|"
                r"\b(?:do\s+not|don't|never|skip|omit)\b.{0,55}"
                r"\b(?:require|set|define|predeclare|need)\w*\b.{0,55}"
                r"\b(?:per[- ]slice|for\s+(?:each|every)\s+slice)\b|"
                r"\b(?:critical[- ]slice\s+regression|critical\s+slice\s+failure)\b"
                r".{0,55}\b(?:does\s+not|doesn't|will\s+not|won't|need\s+not)\b"
                r".{0,35}\b(?:block|stop|prevent|gate)\w*\b.{0,25}"
                r"\b(?:launch|release|rollout)\b",
                clause,
            )
        )
        if unsafe_clause and not noncritical_exception:
            negated_slice_gate = True
            break
    aggregate_only_gate = bool(
        re.search(
            r"\b(?:aggregate|overall|global)\b.{0,45}\b(?:launch\s+)?"
            r"(?:gate|threshold|score)\b.{0,35}\b(?:is\s+)?(?:enough|sufficient)\b|"
            r"\b(?:only|sole)\b.{0,25}\b(?:aggregate|overall|global)\b"
            r".{0,45}\b(?:gate|threshold|score)\b",
            lower,
        )
        and not re.search(
            r"\b(?:aggregate|overall|global)\b.{0,45}\b(?:gate|threshold|score)\b"
            r".{0,30}\b(?:is\s+)?not\s+(?:enough|sufficient)\b|"
            r"\b(?:do\s+not|don't|never)\s+use\b.{0,30}\bonly\b.{0,20}"
            r"\b(?:aggregate|overall|global)\b",
            lower,
        )
    )
    if negated_slice_gate or aggregate_only_gate:
        issues.append("unsafe_negated_or_aggregate_only_slice_gate")

    rag_safety_signal = (
        r"(?:no[- ]answer|unanswerable|adversarial|prompt\s+injection|acl|permission|"
        r"cross[- ]tenant|pii|privacy|citation(?:\s+correctness)?|refusal)"
    )
    unsafe_omission = False
    for clause in re.split(r"(?<=[.!?;])\s+", lower):
        safely_rejects_omission = bool(
            re.search(
                r"\b(?:do\s+not|don't|never|must\s+not|avoid)\s+"
                r"(?:skip|omit|exclude|ignore)\w*\b.{0,80}"
                rf"\b{rag_safety_signal}\b",
                clause,
            )
        )
        unsafe_clause = bool(
            re.search(
                r"\b(?:do\s+not|don't|never)\s+"
                r"(?:evaluate|test|include|cover|measure|score|check)\w*\b.{0,80}"
                rf"\b{rag_safety_signal}\b|"
                r"\b(?:skip|omit|exclude|ignore)\w*\b.{0,80}"
                rf"\b{rag_safety_signal}\b|"
                rf"\b{rag_safety_signal}\b.{{0,55}}"
                r"\b(?:is|are)\s+(?:unnecessary|not\s+needed|optional)\b",
                clause,
            )
        )
        if unsafe_clause and not safely_rejects_omission:
            unsafe_omission = True
            break
    if unsafe_omission:
        issues.append("unsafe_omitted_rag_safety_slice")

    top_subset_review = bool(
        re.search(
            r"\btop\s+\d+(?:\.\d+)?\s*(?:%|\bpercent\b).{0,100}"
            r"\b(?:(?:human|manual)\s+review|with\s+humans?|reviewed\s+by\s+humans?)\b|"
            r"\b(?:human|manual)\s+review\b.{0,100}"
            r"\btop\s+\d+(?:\.\d+)?\s*(?:%|\bpercent\b)",
            lower,
        )
    )
    explicitly_only_top_subset = bool(
        re.search(
            r"\b(?:review|validate|inspect|score)\w*\b.{0,30}\bonly\b.{0,20}"
            r"\btop\s+\d+(?:\.\d+)?\s*(?:%|\bpercent\b)|"
            r"\b(?:review|validate|inspect|score)\w*\b.{0,30}"
            r"\btop\s+\d+(?:\.\d+)?\s*(?:%|\bpercent\b).{0,30}\bonly\b|"
            r"\bonly\b.{0,20}\btop\s+\d+(?:\.\d+)?\s*(?:%|\bpercent\b)"
            r".{0,80}\b(?:human|manual|review)\b",
            lower,
        )
    )
    review_is_representative = bool(
        re.search(
            r"\b(?:stratified|random|risk[- ]weighted|risk[- ]based|"
            r"all\s+high[- ]risk)\b|"
            r"\brepresentative\s+(?:human\s+)?(?:sample|sampling|review)\b",
            lower,
        )
    )
    safely_rejects_top_only = bool(
        re.search(
            r"\b(?:never|do\s+not|don't|avoid)\b.{0,35}"
            r"\b(?:review|validate|inspect|score)\w*\b.{0,35}"
            r"\b(?:only\s+)?top\s+\d+(?:\.\d+)?\s*(?:%|\bpercent\b)|"
            r"\b(?:never|do\s+not|don't|avoid)\b.{0,35}\bonly\b.{0,25}"
            r"\btop\s+\d+(?:\.\d+)?\s*(?:%|\bpercent\b)",
            lower,
        )
    )
    if not safely_rejects_top_only and (
        explicitly_only_top_subset or (top_subset_review and not review_is_representative)
    ):
        issues.append("unsafe_top_score_only_human_review")

    numeric_targets = bool(
        re.search(
            r"\b(?:target\w*|under|below|less\s+than|at\s+most|no\s+more\s+than)\b"
            r".{0,45}(?:\$?\d+(?:\.\d+)?|\d+\s*(?:ms|seconds?|queries|examples?))|"
            r"\b(?:p\d{2}|latency|cost)\b.{0,35}"
            r"(?:<=|>=|<|>|under|below|at\s+most)\s*\$?\d+(?:\.\d+)?|"
            r"\b(?:golden\s+)?(?:dataset|review\s+set)\b.{0,20}"
            r"\b(?:of|with)\b.{0,10}\d+(?:\s*[–-]\s*\d+)?\b",
            lower,
        )
    )
    labeled_assumption = bool(
        re.search(
            r"\b(?:assumption|illustrative|example\s+target|to\s+be\s+set|"
            r"derive\w*\s+from\s+(?:the\s+)?(?:product\s+)?slo)\b",
            lower,
        )
    )
    if numeric_targets and not labeled_assumption:
        issues.append("unlabeled_numeric_rag_launch_target")
    return issues


def feature_store_consistency_issues(text: str) -> List[str]:
    """Enforce the mechanics that make offline training match online serving."""
    lower = re.sub(
        r"\s+",
        " ",
        re.sub(
            r"[*`~]+",
            "",
            text.casefold()
            .replace("’", "'")
            .replace("_", "-")
            .replace("≤", "<=")
            .replace("≥", ">="),
        ),
    )
    issues: List[str] = []
    decision_time = (
        r"(?:prediction|decision|observation|request|scoring)[- ](?:time|timestamp)|"
        r"(?:prediction|decision|observation|request|scoring)\s+cutoff|"
        r"time\s+(?:of|at)\s+(?:prediction|decision|observation|request|scoring)"
    )
    event_time = r"(?:(?:source|feature)[- ])?event[- ](?:time|timestamp)"
    availability_time = (
        r"(?:availability|ingestion|processing|knowledge)[- ](?:times?|timestamps?)"
    )
    label_time = (
        r"(?:label(?:[- ](?:availability|outcome))?|outcome)[- ](?:time|timestamp)|"
        r"(?:label|outcome)\s+cutoff|time\s+of\s+(?:the\s+)?(?:label|outcome)"
    )

    shared_executable = bool(
        (
            re.search(
                r"\b(?:one|single|same|shared|versioned)\b.{0,55}"
                r"\b(?:executable|compiled|feature\s+code|transformation\s+"
                r"(?:code|definition)|dsl)\b",
                lower,
            )
            or re.search(
                r"\bexecutable\s+(?:feature\s+)?transformations?\b.{0,45}"
                r"\b(?:once|shared|compile\w*)\b",
                lower,
            )
            or re.search(
                r"\bdefine\w*\b.{0,35}\bfeatures?\b.{0,35}\bonce\b"
                r".{0,80}\bcompile\w*\b.{0,40}\b(?:definition|dsl|code)\b|"
                r"\bversioned\s+dsl\b.{0,80}\bcompile\w*\b",
                lower,
            )
        )
        and re.search(r"\b(?:stream|streaming|online|serving)\b", lower)
        and re.search(r"\b(?:batch|offline|training)\b", lower)
    )
    independent_claim = bool(
        re.search(
            r"\b(?:streaming|online)\b.{0,45}\b(?:and|versus|vs\.?|/)\b.{0,20}"
            r"\b(?:batch|offline)\b.{0,60}\b(?:transformations?|code|logic)\b"
            r".{0,45}\b(?:implemented\s+independently|independent|separate|different)\b|"
            r"\b(?:batch|offline)\b.{0,45}\b(?:and|versus|vs\.?|/)\b.{0,20}"
            r"\b(?:streaming|online)\b.{0,60}\b(?:transformations?|code|logic)\b"
            r".{0,45}\b(?:implemented\s+independently|independent|separate|different)\b|"
            r"\bduplicate\w*\b.{0,35}\btransformation\s+logic\b.{0,25}"
            r"\bseparately\b|"
            r"\bregistry\b.{0,45}\bshares?\s+schemas?\s+only\b.{0,80}"
            r"\beach\s+path\b.{0,35}\b(?:its\s+own|separate)\b.{0,25}"
            r"\bimplementation\b|"
            r"\b(?:batch|streaming)\b.{0,35}\b(?:and|versus|vs\.?|/)\b.{0,20}"
            r"\b(?:batch|streaming)\b.{0,45}\buse\w*\b.{0,25}\bdifferent\s+code\b|"
            r"\b(?:do not|don't|never)\b.{0,35}\b(?:compile|share)\w*\b.{0,55}"
            r"\bexecutable\s+transformations?\b.{0,40}\bbetween\s+paths\b|"
            r"\b(?:do\s+not|don't|never|must\s+not)\b.{0,40}"
            r"\b(?:use|share|compile)\w*\b.{0,45}\b(?:same|shared)\b"
            r".{0,45}\b(?:executable\s+(?:feature\s+)?(?:code|transformations?)|"
            r"feature\s+code|transformation\s+code)\b",
            lower,
        )
    )
    safely_rejects_independent = bool(
        re.search(
            r"\b(?:batch|streaming|online|offline)\b.{0,80}"
            r"\b(?:do\s+not|don't|never|must\s+not|should\s+not)\b.{0,35}"
            r"\b(?:use\s+different\s+code|differ|diverge|be\s+implemented\s+"
            r"independently|have\s+independent\s+implementations?)\b|"
            r"\b(?:do\s+not|don't|never|must\s+not|should\s+not)\b.{0,45}"
            r"\b(?:different\s+code|independent\s+(?:transformations?|implementations?))\b",
            lower,
        )
    )
    compiled_from_shared_definition = bool(
        re.search(
            r"\bdifferent\s+(?:code\s+)?artifacts?\b.{0,90}"
            r"\bcompiled\b.{0,45}\b(?:same|one|shared|versioned)\b.{0,30}"
            r"\b(?:dsl|definition|transformation)\b|"
            r"\bcompiled\b.{0,45}\b(?:same|one|shared|versioned)\b.{0,30}"
            r"\b(?:dsl|definition|transformation)\b.{0,90}"
            r"\bdifferent\s+(?:code\s+)?artifacts?\b",
            lower,
        )
    )
    explicitly_rejects_shared_logic = bool(
        re.search(
            r"\b(?:batch|streaming|online|offline)\b.{0,90}"
            r"\b(?:must\s+not|should\s+not|cannot|can't|do\s+not|don't|never)\b"
            r".{0,45}\b(?:use|share|compile)\w*\b.{0,35}"
            r"\b(?:identical|same|shared)\b.{0,35}"
            r"\b(?:feature\s+)?(?:logic|code|transformations?)\b|"
            r"\b(?:must\s+not|should\s+not|cannot|can't|do\s+not|don't|never)\b"
            r".{0,45}\b(?:use|share|compile)\w*\b.{0,35}"
            r"\b(?:identical|same|shared)\b.{0,35}"
            r"\b(?:feature\s+)?(?:logic|code|transformations?)\b.{0,90}"
            r"\b(?:batch|streaming|online|offline)\b",
            lower,
        )
    )
    independent_transformations = bool(
        explicitly_rejects_shared_logic
        or (
            independent_claim
            and not safely_rejects_independent
            and not compiled_from_shared_definition
        )
    )
    if independent_transformations:
        shared_executable = False
        issues.append("unsafe_independent_feature_transformations")
    if not shared_executable:
        issues.append("missing_shared_executable_feature_transformations")

    if not re.search(rf"\b(?:{event_time})\b", lower):
        issues.append("missing_feature_event_time")
    known_by_decision = bool(
        re.search(
            rf"\b(?:known|available|visible)\b.{{0,45}}"
            rf"\b(?:by|at|no\s+later\s+than)\b.{{0,25}}\b(?:{decision_time})\b",
            lower,
        )
    )
    if not (re.search(rf"\b(?:{availability_time})\b", lower) or known_by_decision):
        issues.append("missing_feature_availability_time")
    as_of_join = bool(
        re.search(
            r"\b(?:as[- ]of|temporal|point[- ]in[- ]time|snapshot)\s+join\b|"
            r"\bjoin\b.{0,30}\b(?:as[- ]of|point[- ]in[- ]time)\b",
            lower,
        )
    )
    semantic_sentences = [
        sentence.strip()
        for sentence in re.split(r"(?<=[.!?])\s+", lower)
        if sentence.strip() and not re.fullmatch(r"\d+[.)]?", sentence.strip())
    ]
    anchored_two_sentence_bound = False
    anchored_two_sentence_leak = False
    for index in range(max(0, len(semantic_sentences) - 1)):
        join_sentence = semantic_sentences[index]
        bound_sentence = semantic_sentences[index + 1]
        join_names_decision_time = bool(
            re.search(
                r"\b(?:as[- ]of|temporal|point[- ]in[- ]time|snapshot)\s+join\b|"
                r"\bjoin\b.{0,30}\b(?:as[- ]of|point[- ]in[- ]time)\b",
                join_sentence,
            )
            and re.search(rf"\b(?:{decision_time})\b", join_sentence)
        )
        following_bounds_both_to_anchor = bool(
            (
                re.search(rf"\bboth\b.{{0,50}}\b(?:{event_time})\b", bound_sentence)
                and re.search(rf"\b(?:{availability_time})\b", bound_sentence)
                and re.search(
                    r"\b(?:at\s+or\s+before|no\s+later\s+than|before|"
                    r"not\s+after)\b.{0,30}"
                    r"\b(?:that|this|the\s+same)\s+timestamp\b",
                    bound_sentence,
                )
            )
            or re.search(
                r"\bevent\s+and\s+(?:availability|ingestion|knowledge)\s+"
                r"(?:times?|timestamps?)\b.{0,100}"
                r"\b(?:at\s+or\s+before|no\s+later\s+than|before|not\s+after)\b"
                r".{0,30}\b(?:that|this|the\s+same)\s+timestamp\b",
                bound_sentence,
            )
        )
        if join_names_decision_time and following_bounds_both_to_anchor:
            anchored_two_sentence_bound = True
        if join_names_decision_time and re.search(
            rf"\b(?:{event_time}|{availability_time}|event\s+and\s+"
            r"(?:availability|ingestion|knowledge)\s+(?:times?|timestamps?))\b"
            r".{0,100}\b(?:after|later\s+than|not\s+(?:required\s+to\s+be\s+)?"
            r"(?:at\s+or\s+before|before)|need\s+not\s+be\s+before)\b"
            r".{0,35}\b(?:that|this|the\s+same)\s+timestamp\b",
            bound_sentence,
        ):
            anchored_two_sentence_leak = True
    both_times_bounded = bool(
        anchored_two_sentence_bound
        or re.search(
            rf"\bboth\b.{{0,40}}\b(?:{event_time})\b.{{0,60}}"
            rf"\b(?:{availability_time})\b.{{0,100}}"
            rf"(?:\bat\s+or\s+before\b|\bno\s+later\s+than\b|\bbefore\b|"
            rf"\bprecedes?\b|\bis\s+earlier\s+than\b|\bnot\s+after\b|<=)"
            rf".{{0,50}}\b(?:{decision_time})\b|"
            rf"\b(?:{event_time})\b.{{0,80}}\b(?:and|plus)\b.{{0,40}}"
            rf"\b(?:{availability_time})\b.{{0,100}}"
            rf"(?:\bat\s+or\s+before\b|\bno\s+later\s+than\b|\bbefore\b|"
            rf"\bprecedes?\b|\bis\s+earlier\s+than\b|\bnot\s+after\b|<=)"
            rf".{{0,50}}\b(?:{decision_time})\b|"
            r"\bevent\s+and\s+(?:availability|ingestion|knowledge)\s+"
            rf"(?:times?|timestamps?)\b.{{0,100}}"
            rf"(?:\bat\s+or\s+before\b|\bno\s+later\s+than\b|\bbefore\b|"
            rf"\bprecedes?\b|\bis\s+earlier\s+than\b|\bnot\s+after\b|<=)"
            rf".{{0,50}}\b(?:{decision_time})\b",
            lower,
        )
    )
    event_time_bounded = bool(
        both_times_bounded
        or re.search(
            rf"\b(?:{event_time})\b.{{0,100}}"
            rf"(?:<=|\bat\s+or\s+before\b|\bno\s+later\s+than\b|\bbefore\b|"
            rf"\bprecedes?\b|\bis\s+earlier\s+than\b|\bnot\s+after\b)"
            rf".{{0,80}}\b(?:{decision_time})\b",
            lower,
        )
    )
    availability_time_bounded = bool(
        both_times_bounded
        or re.search(
            rf"\b(?:{availability_time})\b.{{0,100}}"
            rf"(?:<=|\bat\s+or\s+before\b|\bno\s+later\s+than\b|\bbefore\b|"
            rf"\bprecedes?\b|\bis\s+earlier\s+than\b|\bnot\s+after\b)"
            rf".{{0,80}}\b(?:{decision_time})\b",
            lower,
        )
        or known_by_decision
    )
    coordinated_times_unbounded = bool(
        re.search(
            r"\bevent\s+and\s+(?:availability|ingestion|knowledge)\s+"
            r"(?:times?|timestamps?)\b.{0,60}"
            r"(?:\b(?:are\s+)?not\s+(?:required\s+to\s+be\s+|"
            r"necessarily\s+(?:required\s+to\s+be\s+)?)?"
            r"(?:filtered|bounded|checked|enforced|at\s+or\s+before|before|<=)|"
            r"\b(?:need\s+not|(?:do\s+not|don't)\s+need\s+to)\s+be\s+"
            r"(?:filtered|bounded|checked|enforced|at\s+or\s+before|before|<=)|"
            r"\bcan\s+be\s+(?:either\s+)?before\s+or\s+after\b)",
            lower,
        )
        or re.search(
            r"\bboth\b.{0,35}\bevent[- ](?:time|timestamp)\b.{0,55}"
            r"\b(?:and|plus)\b.{0,30}\b(?:availability|ingestion|knowledge)"
            r"[- ](?:time|timestamp)\b.{0,55}\b(?:is|are)\s+not\s+"
            r"(?:at\s+or\s+before|before|no\s+later\s+than|bounded\s+by)\b",
            lower,
        )
    )
    if re.search(
        rf"\b(?:{event_time})\b.{{0,35}}\b(?:is|are)\s+not\s+"
        r"(?:filtered|bounded|checked|enforced)\b",
        lower,
    ):
        event_time_bounded = False
    if re.search(
        rf"\b(?:{availability_time})\b.{{0,35}}"
        r"\b(?:is|are)\s+not\s+(?:filtered|bounded|checked|enforced)\b",
        lower,
    ):
        availability_time_bounded = False

    future_availability = bool(
        anchored_two_sentence_leak
        or re.search(
            r"\btraining\b.{0,45}\b(?:may|can|will)\s+include\w*\b.{0,70}"
            r"\bevent[- ](?:time|timestamp)\b.{0,55}"
            r"\b(?:and|plus)\b.{0,35}\b(?:availability|ingestion|knowledge)"
            r"[- ](?:time|timestamp)\b.{0,55}\b(?:exceed|after|later\s+than)\w*\b"
            rf".{{0,35}}\b(?:{decision_time})\b|"
            rf"\b(?:{availability_time})\b.{{0,25}}"
            r"\b(?:is|remains?|can\s+be)\s+(?:unconstrained|unbounded)\b|"
            rf"\bfuture\s+(?:{availability_time})\b.{{0,20}}"
            r"\b(?:is|are)\s+(?:explicitly\s+)?(?:allowed|admitted|included|used)\b"
            r".{0,45}\btraining\s+rows?\b|"
            rf"\b(?:{availability_time})\b.{{0,35}}"
            r"\b(?:need\s+not|(?:is|are)\s+not\s+required\s+to|"
            r"does\s+not\s+need\s+to)\b.{0,25}"
            r"\b(?:be\s+)?(?:before|precede|no\s+later\s+than)\b.{0,25}"
            rf"\b(?:{decision_time})\b|"
            r"\bfeature\s+values?\b.{0,30}\bmay\s+arrive\s+after\b.{0,20}"
            r"\b(?:prediction|decision|observation|request|scoring)\b.{0,55}"
            r"\b(?:are\s+)?(?:still\s+)?(?:included|eligible|used|admitted)\b|"
            r"\bvalues?\b.{0,25}\bremain\s+eligible\b.{0,25}"
            r"\beven\s+when\s+unavailable\b.{0,20}"
            rf"\b(?:{decision_time})\b|"
            rf"\b(?:{availability_time})\b.{{0,30}}\bmay\s+lag\b.{{0,25}}"
            rf"\b(?:{decision_time})\b.{{0,55}}"
            r"\b(?:still\s+)?(?:include|includes|included|admit|use)\w*\b|"
            r"\bfuture\s+feature\s+values?\b.{0,25}"
            r"\b(?:is|are)\s+(?:explicitly\s+)?(?:allowed|admitted|included|used)\b"
            r".{0,45}\btraining\s+rows?\b|"
            r"\b(?:do\s+not|don't|never)\s+exclude\w*\b.{0,35}"
            r"\bvalues?\b.{0,25}\bunavailable\b.{0,20}"
            rf"\b(?:at|by)\b.{{0,10}}\b(?:{decision_time})\b",
            lower,
        )
    )
    label_cutoff_boundary = False
    for clause in re.split(r"(?<=[.!?;])\s+", lower):
        future_relation = bool(
            re.search(
                rf"\b(?:{availability_time})\b.{{0,65}}"
                rf"(?:\bafter\b|(?<!no\s)\blater\s+than\b)\s*.{{0,20}}"
                rf"\b(?:{decision_time})\b|"
                rf"\b(?:{availability_time})\b\s*>=?\s*"
                rf"\b(?:{decision_time})\b|"
                rf"\b(?:{decision_time})\b.{{0,65}}"
                rf"\bbefore\b\s*.{{0,20}}\b(?:{availability_time})\b|"
                rf"\b(?:{decision_time})\b\s*<=?\s*"
                rf"\b(?:{availability_time})\b|"
                r"\bfeatures?\b.{0,40}\b(?:not\s+)?(?:available|known|ingested)\b"
                rf".{{0,35}}\b(?:until|after)\b.{{0,20}}\b(?:{decision_time})\b",
                clause,
            )
        )
        if future_relation:
            negated_rejection = bool(
                re.search(
                    r"\b(?:do not|don't|never|cannot|can't)\s+"
                    r"(?:reject|exclude|drop|ignore|filter\s+out)\w*\b",
                    clause,
                )
            )
            safely_rejects_future = bool(
                not negated_rejection
                and (
                    re.search(
                        r"\b(?:reject|exclude|drop|ignore|filter\s+out)\w*\b"
                        rf".{{0,95}}\b(?:{availability_time})\b.{{0,50}}"
                        rf"\b(?:after|later\s+than)\b.{{0,20}}"
                        rf"\b(?:{decision_time})\b",
                        clause,
                    )
                    or re.search(
                        rf"\b(?:{availability_time})\b.{{0,50}}"
                        rf"\b(?:after|later\s+than)\b.{{0,20}}"
                        rf"\b(?:{decision_time})\b.{{0,55}}"
                        r"\b(?:is|are|must\s+be|will\s+be)\s+"
                        r"(?:rejected|excluded|dropped|ignored|filtered\s+out)\b",
                        clause,
                    )
                    or re.search(
                        r"\b(?:do not|don't|never|must not|cannot|can't)\s+"
                        r"(?:admit|use|include|join|select)\w*\b.{0,110}"
                        rf"\b(?:{availability_time})\b.{{0,50}}"
                        rf"\b(?:after|later\s+than)\b.{{0,20}}"
                        rf"\b(?:{decision_time})\b",
                        clause,
                    )
                )
            )
            if not safely_rejects_future:
                future_availability = True

        positive_label_boundary = bool(
            re.search(
                rf"\b(?:{event_time}|{availability_time})\b.{{0,100}}"
                rf"(?:<=|\bat\s+or\s+before\b|\bbefore\b|\bnot\s+after\b)"
                rf".{{0,55}}\b(?:{label_time})\b|"
                r"\bevent\s+and\s+(?:availability|ingestion|knowledge)\s+"
                rf"(?:times?|timestamps?)\b.{{0,100}}\b(?:{label_time})\b|"
                rf"\b(?:as[- ]of|temporal|point[- ]in[- ]time)\s+join\b.{{0,100}}"
                rf"\b(?:{label_time})\b",
                clause,
            )
        )
        label_is_explicitly_different = bool(
            re.search(
                rf"\b(?:{label_time})\b.{{0,45}}"
                r"\b(?:(?:is|are)\s+)?(?:not|never)\s+"
                r"(?:exactly\s+|explicitly\s+)?"
                r"(?:the\s+)?(?:same\s+as|identical\s+to|equal\s+to)\b"
                rf".{{0,25}}\b(?:{decision_time})\b|"
                rf"\b(?:{label_time})\b.{{0,45}}"
                r"\b(?:is|are)\s+not\s+(?:the\s+)?"
                rf"(?:{decision_time})\b|"
                rf"\b(?:{label_time})\b.{{0,45}}"
                r"\b(?:differs?\s+from|is\s+(?:unrelated\s+to|approximately))\b"
                rf".{{0,25}}\b(?:{decision_time})\b|"
                rf"\b(?:{decision_time})\b.{{0,45}}"
                r"\bdiffers?\s+from\b.{0,25}"
                rf"\b(?:{label_time})\b",
                clause,
            )
        )
        label_is_decision = bool(
            re.search(
                rf"\b(?:{label_time})\b.{{0,45}}"
                r"\b(?:(?:is|are)\s+)?(?:explicitly\s+)?(?:the\s+)?"
                r"(?:same\s+as|identical\s+to|equal\s+to)\b.{0,25}"
                rf"\b(?:{decision_time})\b|"
                rf"\b(?:{label_time})\b.{{0,45}}\bequals?\b.{{0,25}}"
                rf"\b(?:{decision_time})\b|"
                rf"\b(?:{label_time})\b.{{0,45}}"
                r"\b(?:explicitly\s+defined\s+as|defined\s+to\s+be\s+exactly|"
                r"serves?\s+as|coincides?\s+exactly\s+with)\b.{0,25}"
                rf"\b(?:{decision_time})\b|"
                rf"\b(?:{decision_time})\b.{{0,45}}"
                r"\b(?:same\s+as|identical\s+to|equal\s+to|equals?)\b.{0,25}"
                rf"\b(?:{label_time})\b",
                clause,
            )
            and not label_is_explicitly_different
        )
        rejects_label_boundary = bool(
            re.search(
                r"\b(?:do not|don't|never|must not|cannot|can't|avoid)\b.{0,40}"
                r"\b(?:use|substitute|join|bound|filter)\w*\b.{0,70}"
                rf"\b(?:{label_time})\b",
                clause,
            )
        )
        if positive_label_boundary and label_is_decision:
            if re.search(rf"\b(?:{event_time})\b", clause):
                event_time_bounded = True
            if re.search(rf"\b(?:{availability_time})\b", clause):
                availability_time_bounded = True
            if re.search(
                r"\bevent\s+and\s+(?:availability|ingestion|knowledge)\s+"
                r"(?:times?|timestamps?)\b",
                clause,
            ):
                event_time_bounded = True
                availability_time_bounded = True
        elif positive_label_boundary and not rejects_label_boundary:
            label_cutoff_boundary = True

    if coordinated_times_unbounded:
        event_time_bounded = False
        availability_time_bounded = False
    if future_availability or label_cutoff_boundary:
        availability_time_bounded = False
        issues.append("unsafe_future_feature_availability_or_label_cutoff")
    if not (as_of_join and event_time_bounded and availability_time_bounded):
        issues.append("missing_point_in_time_join_mechanics")

    negation_scan = re.sub(
        r"\b(?:never|do\s+not|don't|avoid)\s+(?:skip|omit)\w*\b.{0,70}"
        r"\b(?:as[- ]of\s+join|event[- ]time|availability[- ]time|"
        r"late\s+events?|skew|parity)\b",
        " ",
        lower,
    )
    if coordinated_times_unbounded or re.search(
        r"\b(?:do\s+not|don't|never)\s+(?:share|persist|store|record|filter|"
        r"correct|recompute|dedup|compare|validate|enforce)\w*\b.{0,90}"
        r"\b(?:executable\s+transform|event[- ]time|availability[- ]time|"
        r"as[- ]of\s+join|late\s+events?|replay|backfill|online|offline|skew|parity)\b|"
        r"\b(?:skip|omit)\w*\b.{0,70}\b(?:as[- ]of\s+join|event[- ]time|"
        r"availability[- ]time|late\s+events?|skew|parity)\b|"
        r"\bnon[- ]idempotent\b.{0,60}\b(?:replay|backfill)\b|"
        r"\b(?:event|availability|ingestion)[- ]time\b.{0,35}"
        r"\b(?:is|are)\s+not\s+(?:filtered|bounded|checked|enforced)\b|"
        r"\b(?:replay|backfill)\b.{0,60}\bnon[- ]idempotent\b",
        negation_scan,
    ):
        issues.append("unsafe_negated_feature_store_correctness")

    if re.search(
        r"\b(?:online[/ -]?offline\s+)?(?:parity|equivalence|skew)\s+checks?\b"
        r".{0,35}\b(?:is|are)\s+(?:unnecessary|not\s+needed|optional)\b|"
        r"\b(?:correcting|recomputing|handling)\s+late\s+events?\b"
        r".{0,35}\b(?:is|are)\s+(?:unnecessary|not\s+needed|optional)\b",
        lower,
    ) and "unsafe_negated_feature_store_correctness" not in issues:
        issues.append("unsafe_negated_feature_store_correctness")

    late_signal = re.search(r"\b(?:late\s+events?|out[- ]of[- ]order|watermark)\b", lower)
    late_policy = re.search(
        r"\b(?:correct|recompute|backfill|drop|quarantine|window|revision|supersed)\w*\b",
        lower,
    )
    if not (late_signal and late_policy):
        issues.append("missing_late_event_correction_policy")

    replay_signal = re.search(r"\b(?:replay|backfill|reprocess)\w*\b", lower)
    replay_safety = re.search(
        r"\b(?:idempot\w*|dedup\w*|event[- ]id|materialization[- ]version)\b",
        lower,
    )
    if not (replay_signal and replay_safety):
        issues.append("missing_idempotent_feature_replay")

    parity_check = bool(
        re.search(r"\b(?:skew|parity|diff|compare|equivalence)\w*\b", lower)
        and re.search(r"\b(?:online|serving|live)\b", lower)
        and re.search(r"\b(?:offline|training|batch)\b", lower)
    )
    if not parity_check:
        issues.append("missing_online_offline_feature_skew_check")

    numeric_target = bool(
        re.search(
            r"\b(?:p\d{2}|latency|throughput|availability|retention|scale)\b"
            r".{0,35}(?:[<>]=?\s*)?\d+(?:\.\d+)?\s*"
            r"(?:ms|milliseconds?|seconds?|qps|rps|events?(?:/|\s+per\s+)"
            r"seconds?|%|percent)\b|"
            r"\b\d+(?:\.\d+)?\s*[km]?\+?\s*events?(?:/|\s+per\s+)seconds?\b",
            lower,
        )
    )
    labeled_assumption = bool(
        re.search(
            r"\b(?:assumption|illustrative|example\s+target|to\s+be\s+set|"
            r"derive\w*\s+from\s+(?:the\s+)?(?:product\s+)?slo)\b",
            lower,
        )
    )
    if numeric_target and not labeled_assumption:
        issues.append("unlabeled_numeric_feature_store_target")
    return issues


def payment_timeout_followup_completeness_issues(text: str) -> List[str]:
    """Require exact, operator-safe resolution for an ambiguous charged timeout."""
    lower = re.sub(
        r"\s+",
        " ",
        re.sub(r"[*`~]+", "", text.casefold().replace("’", "'")),
    )
    issues: List[str] = []
    sentences = [
        sentence.strip()
        for sentence in re.split(r"(?<=[.!?;])\s+", lower)
        if sentence.strip()
    ]
    transition_windows = list(sentences)
    transition_windows.extend(
        f"{sentences[index]} {sentences[index + 1]}"
        for index in range(max(0, len(sentences) - 1))
    )
    transition_sentences = [
        sentence
        for sentence in transition_windows
        if "unknown" in sentence
        and "succeeded" in sentence
        and "failed" in sentence
        and re.search(
            r"\b(?:move|transition|resolve|confirm|set|treat|flip|close|convert)\w*\b",
            sentence,
        )
    ]
    authoritative_transition = any(
        re.search(r"\b(?:provider|processor)\b", sentence)
        and re.search(r"\b(?:status|lookup|query|webhook|evidence|response)\b", sentence)
        and re.search(r"\b(?:authoritative|confirm|definitive)\w*\b", sentence)
        for sentence in transition_sentences
    )
    authoritative_transition = bool(
        authoritative_transition
        or any(
            re.search(r"\b(?:provider|processor)\b", sentence)
            and re.search(r"\b(?:status|lookup|query|webhook|evidence|response)\b", sentence)
            and re.search(
                r"\b(?:those|these|such)\s+authoritative\s+"
                r"(?:signals?|results?|responses?|sources?|evidence)\b",
                sentence,
            )
            for sentence in transition_sentences
        )
    )
    authoritative_boundary = any(
        "unknown" in sentence
        and re.search(r"\b(?:move|transition|resolve|remain|stay)\w*\b", sentence)
        and re.search(r"\b(?:provider|processor)\b", sentence)
        and re.search(r"\b(?:status|lookup|query|webhook|evidence|response)\b", sentence)
        and re.search(r"\b(?:authoritative|confirm|definitive)\w*\b", sentence)
        for sentence in sentences
    )
    succeeded_mapping = any(
        "succeeded" in sentence
        and re.search(r"\b(?:confirm|authoritative|definitive)\w*\b", sentence)
        and re.search(r"\b(?:become|map|move|transition|set|resolve)\w*\b", sentence)
        for sentence in sentences
    )
    failed_mapping = any(
        "failed" in sentence
        and re.search(r"\b(?:confirm|authoritative|definitive)\w*\b", sentence)
        and re.search(r"\b(?:become|map|move|transition|set|resolve)\w*\b", sentence)
        for sentence in sentences
    )
    authoritative_transition = bool(
        authoritative_transition
        or (authoritative_boundary and succeeded_mapping and failed_mapping)
    )
    unsafe_local_transition = any(
        re.search(
            r"\b(?:timer|cron|retry\s+exhaustion|retries\s+(?:end|expire)|"
            r"local\s+timeout|our\s+(?:clock|timer|policy))\b",
            sentence,
        )
        and not (
            re.search(r"\b(?:provider|processor)\b", sentence)
            and re.search(r"\b(?:authoritative|confirm|definitive)\w*\b", sentence)
        )
        for sentence in transition_sentences
    )
    unsafe_local_transition = bool(
        unsafe_local_transition
        or any(
            "unknown" in sentence
            and "failed" in sentence
            and re.search(
                r"\b(?:timer|cron|deadline|scheduled\s+job|retry\s+exhaustion|"
                r"local\s+timeout|our\s+(?:clock|timer|policy))\b",
                sentence,
            )
            and re.search(
                r"\b(?:move|transition|set|treat|flip|close|convert|expire|"
                r"become|consider)\w*\b",
                sentence,
            )
            and not re.search(
                r"\b(?:do\s+not|don't|does\s+not|doesn't|is\s+not|never|"
                r"must\s+not|cannot|can't)\b.{0,55}"
                r"\b(?:move|transition|set|treat|flip|close|convert|expire|"
                r"become|consider)\w*\b",
                sentence,
            )
            for sentence in sentences
        )
    )
    if not authoritative_transition:
        issues.append("missing_authoritative_unknown_terminal_transitions")
    if unsafe_local_transition:
        issues.append("unsafe_non_authoritative_unknown_terminal_transition")

    unresolved_policy = bool(
        re.search(r"\b(?:remain|stay|keep)\w*\b.{0,30}\bunknown\b", lower)
        and re.search(
            r"\b(?:manual|operator|operations|case|escalat|dead[- ]letter)\w*\b"
            r".{0,50}\b(?:reconcil|review|queue|workflow)\w*\b|"
            r"\b(?:reconcil|review)\w*\b.{0,50}"
            r"\b(?:manual|operator|operations|escalat)\w*\b",
            lower,
        )
    )
    if not unresolved_policy:
        issues.append("missing_unresolved_manual_reconciliation_policy")

    safe_replay_condition = has_safe_payment_same_operation_replay_condition(
        lower
    ) or has_explicit_no_provider_command_replay(lower)
    if not safe_replay_condition:
        issues.append("missing_safe_same_operation_replay_condition")
    return issues


def url_shortener_safety_issues(
    text: str, *, require_revocation_completeness: bool = False
) -> List[str]:
    """Reject redirects that can expose stale or abuse-blocked destinations."""
    cleaned = re.sub(r"[*`~]+", "", text.casefold().replace("’", "'"))
    cleaned = re.sub(r"(?:\r?\n)+", ". ", cleaned)
    lower = re.sub(
        r"\s+",
        " ",
        cleaned,
    )
    issues: List[str] = []
    clauses = [
        clause.strip()
        for clause in re.split(
            r"(?<=[.!?;])\s+|,\s+(?=(?:but|while|whereas|although)\b)",
            lower,
        )
        if clause.strip()
    ]
    unsafe_state_redirect = False
    state = (
        r"(?:deleted|expired|blocked|abuse[- ]blocked|disabled|tombstoned|"
        r"revoked|suspended|quarantined|malicious)"
    )
    redirect = r"(?:30[1278]|redirect\w*)"
    for clause in clauses:
        associates_state_with_redirect = bool(
            re.search(
                rf"\b{state}\b.{{0,95}}"
                r"\b(?:gets?|returns?|responds?|serves?|sends?|uses?|issues?|allows?|"
                r"performs?|resolves?|maps?\s+to|mapped\s+to)\b"
                rf".{{0,60}}\b{redirect}\b|"
                rf"\b{state}\b.{{0,95}}\b(?:continue\s+)?redirect\w*\b"
                r".{0,45}\b(?:stored|original|target|destination|30[1278])\b|"
                rf"\b{state}\b.{{0,95}}\b(?:continue\s+)?"
                r"(?:serv(?:e|es|ed|ing)|sends?|routes?|falls?\s+back\s+to)\b.{0,60}"
                r"\b(?:stored|original|target|destination|url)\b|"
                rf"\bredirect\w*\b.{{0,35}}\b{state}\b.{{0,45}}"
                r"\b(?:stored|original|target|destination|30[1278])\b|"
                rf"\bredirect\w*\b.{{0,65}}\b(?:for|on|when|if|to)\b"
                rf".{{0,35}}\b{state}\b|"
                rf"\b(?:30[1278])\b.{{0,65}}\b(?:for|on|when|if)\b"
                rf".{{0,35}}\b{state}\b",
                clause,
            )
            or re.search(
                rf"\b{state}\b.{{0,70}}\b(?:maps?|resolves?|routes?|points?)\b"
                r".{0,45}\b(?:stored|original|target|destination)\b"
                r"(?:\s+url)?\b",
                clause,
            )
        )
        if not associates_state_with_redirect:
            continue
        double_negation = bool(
            re.search(
                r"\b(?:do not|don't|never|must not|should not|cannot|can't)\s+"
                r"(?:prevent|block|forbid|disable)\w*\b.{0,35}\bredirect\w*\b",
                clause,
            )
        )
        revocable_lifecycle_class = bool(
            re.search(
                r"\b(?:public\s+)?(?:links?|mappings?)\b.{0,45}"
                r"\b(?:that|which)\s+(?:can|may|might|could)\b.{0,70}"
                rf"\b{state}\b",
                clause,
            )
            and not re.search(
                rf"\b(?:after|once|when|while|if)\b.{{0,35}}\b{state}\b|"
                rf"\b{state}\b.{{0,35}}\b(?:still|continue\w*|currently|now)\b",
                clause,
            )
        )
        cached_mapping_explicitly_guarded = bool(
            re.search(
                r"\b(?:serve|return|send|issue|allow|perform|resolve|map|redirect)\w*\b"
                r".{0,85}\b(?:only\s+)?(?:when|if|provided)\b.{0,55}"
                r"\b(?:not|isn't|is\s+not)\b.{0,25}"
                rf"\b{state}\b",
                clause,
            )
        )
        safely_rejected = bool(
            not double_negation
            and (
                revocable_lifecycle_class
                # A cache read guarded by an explicit active-state condition is not an
                # inactive redirect.  For example, "serve only when ... not blocked"
                # is the safety boundary, not an instruction to redirect blocked links.
                or cached_mapping_explicitly_guarded
                or re.search(
                    rf"\b{state}\b.{{0,70}}"
                    r"\b(?:do\s+not|don't|never|must\s+not|should\s+not|cannot|can't)\s+"
                    rf"(?:(?:return|serve|send|use|issue|perform)\w*\s+)?\b{redirect}\b",
                    clause,
                )
                or re.search(
                    r"\b(?:do\s+not|don't|never|must\s+not|should\s+not|cannot|can't)\s+"
                    rf"(?:(?:return|serve|send|use|issue|perform)\w*\s+)?\b{redirect}\b"
                    rf".{{0,70}}\b{state}\b",
                    clause,
                )
                or re.search(
                    rf"\b{redirect}\b.{{0,30}}\b(?:is|are|will\s+be|must\s+be)\s+"
                    rf"not\s+(?:used|returned|served|sent|issued)\b.{{0,45}}\b{state}\b",
                    clause,
                )
                or re.search(
                    rf"\b{state}\b.{{0,75}}\b(?:404|410|403|safe\s+interstitial)\b"
                    rf".{{0,45}}\b(?:instead\s+of|rather\s+than|not)\b.{{0,20}}"
                    rf"\b{redirect}\b",
                    clause,
                )
                or re.search(
                    rf"\b(?:purge|invalidate|evict|remove)\w*\b.{{0,40}}"
                    rf"\b(?:cached\s+)?{redirect}\b.{{0,70}}\b(?:404|410|403)\b",
                    clause,
                )
                or re.search(
                    rf"\b{state}\b.{{0,70}}\bredirect\w*\b.{{0,30}}"
                    r"\b(?:safe\s+)?(?:warning|abuse)\s+interstitial\b",
                    clause,
                )
                or re.search(
                    rf"\b{state}\b.{{0,80}}\b(?:302|307)\b.{{0,45}}"
                    r"\b(?:safe\s+)?warning\s+interstitial\b.{0,55}"
                    r"\bnever\b.{0,25}\b(?:stored|original|target|destination)\b",
                    clause,
                )
                or re.search(
                    rf"\b{state}\b.{{0,55}}"
                    r"\b(?:do\s+not|don't|never|must\s+not|cannot|can't)\s+"
                    r"(?:map|resolve|route|point)\w*\b.{0,45}"
                    r"\b(?:stored|original|target|destination)\b",
                    clause,
                )
            )
        )
        if not safely_rejected:
            unsafe_state_redirect = True
            break
    if unsafe_state_redirect:
        issues.append("unsafe_redirect_for_inactive_or_blocked_link")

    mutable_permanent_redirect = False
    mutable = r"(?:mutable|editable|changeable)"
    permanent = r"(?:301|308|permanent\s+redirects?)"
    for clause in clauses:
        association = bool(
            re.search(
                rf"\b{mutable}\b.{{0,80}}\b(?:gets?|returns?|responds?|serves?|"
                rf"sends?|uses?|issues?|allows?|performs?|maps?\s+to|mapped\s+to)\b"
                rf".{{0,30}}\b{permanent}\b|"
                rf"\b{permanent}\b.{{0,70}}\b(?:for|on|when|while)\b.{{0,25}}"
                rf"\b{mutable}\b",
                clause,
            )
        )
        if not association:
            continue
        safely_rejected = bool(
            re.search(
                rf"\b{mutable}\b.{{0,65}}"
                r"\b(?:do\s+not|don't|never|must\s+not|should\s+not|cannot|can't)\s+"
                rf"(?:(?:return|serve|send|use|issue)\w*\s+)?\b{permanent}\b|"
                r"\b(?:do\s+not|don't|never|must\s+not|should\s+not|cannot|can't|reserve)\b"
                rf".{{0,50}}\b{permanent}\b.{{0,60}}\b{mutable}\b|"
                rf"\b{permanent}\b.{{0,30}}\b(?:is|are)\s+not\s+used\b"
                rf".{{0,45}}\b{mutable}\b|"
                rf"\b{mutable}\b.{{0,45}}\b(?:302|307)\b.{{0,25}}\bnot\b.{{0,15}}"
                rf"\b{permanent}\b",
                clause,
            )
        )
        double_negation = bool(
            re.search(
                r"\b(?:do not|don't|never|must not)\s+(?:prevent|block|forbid)\w*\b"
                rf".{{0,35}}\b{permanent}\b",
                clause,
            )
        )
        if not safely_rejected or double_negation:
            mutable_permanent_redirect = True
            break
    if re.search(
        r"\b(?:users?\s+can|allow\w*\s+users?\s+to)\s+"
        r"(?:update|change)\w*\b.{0,30}\bdestination\b.{0,100}"
        r"\bredirects?\b.{0,25}\buses?\b.{0,15}\b(?:http\s+)?(?:301|308)\b",
        lower,
    ):
        mutable_permanent_redirect = True
    if mutable_permanent_redirect:
        issues.append("unsafe_permanent_redirect_for_mutable_link")

    revocable_permanent_redirect = False
    immutable = (
        r"(?:immutable|unchangeable|fixed|never[- ]changing|"
        r"targets?\s+(?:that\s+)?never\s+change|"
        r"destinations?\s+(?:that\s+)?never\s+change)"
    )
    for clause in clauses:
        association = bool(
            re.search(
                rf"\b{immutable}\b.{{0,70}}\b(?:gets?|returns?|serves?|uses?|issues?|"
                rf"allows?|maps?\s+to|mapped\s+to)\b.{{0,35}}\b{permanent}\b|"
                rf"\b(?:reserve|use|issue)\w*\b.{{0,35}}\b{permanent}\b.{{0,65}}"
                rf"\b{immutable}\b|"
                rf"\b{permanent}\b.{{0,65}}\b(?:for|on|when)\b.{{0,30}}"
                rf"\b{immutable}\b",
                clause,
            )
        )
        if not association:
            continue
        safely_rejected = bool(
            re.search(
                r"\b(?:do\s+not|don't|must\s+not|should\s+not|cannot|can't)\b"
                rf".{{0,55}}\b{permanent}\b|"
                r"\bnever\s+(?:use|return|serve|send|issue|allow)\w*\b"
                rf".{{0,35}}\b{permanent}\b|"
                rf"\b{permanent}\b.{{0,55}}\b(?:is|are)\s+not\s+used\b|"
                r"\bnot\s+(?:301|308)\b",
                clause,
            )
        )
        safe_non_revocable_exception = bool(
            re.search(r"\bnon[- ]revocable\b", clause)
            and re.search(
                r"\b(?:browser|client|intermediary)\b.{0,30}\bcach\w*\b.{0,30}\brisk\b|"
                r"\bclient[- ]cache\s+risk\b",
                clause,
            )
            and re.search(
                r"\b(?:exclude\w*\s+from|without|no)\b.{0,55}"
                r"\b(?:delet\w*|expir\w*|moderat\w*|revoc\w*|legal)\b",
                clause,
            )
        )
        if not safely_rejected and not safe_non_revocable_exception:
            revocable_permanent_redirect = True
            break
    if revocable_permanent_redirect:
        issues.append("unsafe_permanent_redirect_for_revocable_link")

    if require_revocation_completeness and not revocable_permanent_redirect:
        for clause in clauses:
            safe_exception = bool(
                re.search(r"\bnon[- ]revocable\b", clause)
                and re.search(r"\b(?:301|308)\b", clause)
                and re.search(
                    r"\b(?:accept|assume)\w*\b.{0,45}\b(?:browser|client|"
                    r"intermediary)\b.{0,25}\bcach\w*\b.{0,25}\brisk\b|"
                    r"\bclient[- ]cache\s+risk\b",
                    clause,
                )
                and re.search(
                    r"\b(?:exclude\w*\s+from|without|no)\b.{0,80}"
                    r"\b(?:delet\w*|expir\w*|moderat\w*|revoc\w*|legal)\b",
                    clause,
                )
            )
            for permanent_code in ("301", "308"):
                positive_permanent = bool(
                    re.search(
                        rf"\b(?:return|serve|send|issue|use|respond\s+with)\w*\b"
                        rf".{{0,35}}\b(?:http\s+)?{permanent_code}\b|"
                        rf"\b{permanent_code}\b.{{0,35}}\b(?:redirect|response)\w*\b",
                        clause,
                    )
                )
                safely_rejected = bool(
                    re.search(
                        r"\b(?:do\s+not|don't|never|must\s+not|should\s+not|"
                        r"cannot|can't)\s+(?:return|serve|send|issue|use)\w*\b"
                        rf".{{0,35}}\b(?:http\s+)?{permanent_code}\b|"
                        rf"\b(?:not|never)\s+(?:http\s+)?{permanent_code}\b|"
                        rf"\b{permanent_code}\b.{{0,30}}\b(?:is|are)\s+not\s+used\b",
                        clause,
                    )
                )
                if positive_permanent and not safely_rejected and not safe_exception:
                    issues.append("unsafe_permanent_redirect_for_revocable_link")
                    break
            if "unsafe_permanent_redirect_for_revocable_link" in issues:
                break

    generic_abuse_451 = False
    for clause in clauses:
        if not (
            re.search(
                r"\b(?:abuse|spam|malware|phishing|malicious|fraudulent|fraud|"
                r"policy[- ]blocked)\b",
                clause,
            )
            and re.search(r"\b451\b", clause)
        ):
            continue
        legally_unavailable = bool(
            re.search(
                r"\b(?:legal|legally|law|court|regulator|regulatory|government|"
                r"statute|jurisdiction|dmca)\b",
                clause,
            )
        )
        rejects_451 = bool(
            re.search(
                r"\b(?:do not|don't|never|must not|cannot|can't)\b.{0,30}\b451\b|"
                r"\b451\b.{0,25}\b(?:is|are)\s+not\s+used\b",
                clause,
            )
        )
        if not (legally_unavailable or rejects_451):
            generic_abuse_451 = True
            break
    if generic_abuse_451:
        issues.append("unsafe_451_for_generic_abuse_block")

    inactive_state = (
        r"(?:delet(?:e|ed|es|ion|ions)|expir(?:e|ed|es|ation)|"
        r"block(?:ed|ing|s)?|inactive|revok(?:e|ed|es)|suspend(?:ed|s)?|"
        r"quarantin(?:e|ed|es)|tombston(?:e|ed|es))"
    )
    inactive_staleness = False
    stale_patterns = (
        rf"\b(?:allow|accept|tolerate)\w*\b[^.!?;]{{0,55}}"
        rf"\b(?:stale|staleness|freshness\s+(?:may|can)\s+lag)\b[^.!?;]{{0,70}}"
        rf"\b(?:after|for|on|following)\b[^.!?;]{{0,45}}\b{inactive_state}\b",
        rf"\b(?:cache\s+)?(?:invalidation|propagation)\b[^.!?;]{{0,70}}"
        rf"\b(?:eventual(?:ly)?|asynchronous(?:ly)?|delayed?)\b[^.!?;]{{0,70}}"
        rf"\b(?:after|for|on|following)\b[^.!?;]{{0,35}}\b{inactive_state}\b",
        rf"\b(?:cache\s+)?(?:invalidation|propagation)\b[^.!?;]{{0,70}}"
        rf"\b(?:after|for|on|following)\b[^.!?;]{{0,35}}\b{inactive_state}\b"
        rf"[^.!?;]{{0,70}}\b(?:eventual(?:ly)?|asynchronous(?:ly)?|delayed?)\b",
        rf"\b(?:after|following)\b[^.!?;]{{0,25}}\b{inactive_state}\b"
        r"[^.!?;]{0,75}\b(?:propagat|invalidat|purge|evict)\w*\b"
        r"[^.!?;]{0,65}\b(?:eventual(?:ly)?|asynchronous(?:ly)?|delayed?)\b",
        rf"\b{inactive_state}\b[^.!?;]{{0,80}}"
        r"\b(?:may|can|will|continue\w*|allow\w*)\b[^.!?;]{0,55}"
        r"\b(?:stale\s+(?:cached\s+)?redirect|cached\s+redirect|"
        r"redirect\w*\s+until\s+(?:the\s+)?ttl|staleness)\b",
    )
    for pattern in stale_patterns:
        for match in re.finditer(pattern, lower):
            window = lower[max(0, match.start() - 90) : match.end() + 130]
            safely_rejected = bool(
                re.search(
                    r"\b(?:do\s+not|don't|never|must\s+not|cannot|can't)\b"
                    r".{0,45}\b(?:allow|accept|tolerate|use)\w*\b.{0,60}"
                    r"\b(?:stale|staleness|eventual)\w*\b",
                    window,
                )
                or re.search(
                    r"\b(?:but|and)\s+(?:do\s+not|don't|never|must\s+not|cannot|can't)"
                    rf"\b.{{0,55}}\b(?:after|for|on)\b.{{0,45}}\b{inactive_state}\b|"
                    r"\b(?:do\s+not|don't|never|must\s+not|cannot|can't)\b"
                    rf".{{0,35}}\b(?:after|for|on)\b.{{0,45}}\b{inactive_state}\b|"
                    rf"\bnot\s+(?:after|for|on|following)\b.{{0,45}}"
                    rf"\b{inactive_state}\b",
                    window,
                )
                or re.search(
                    rf"\b{inactive_state}\b.{{0,80}}"
                    r"\b(?:synchronous(?:ly)?|atomically|before\s+(?:acknowledg|commit))\w*\b"
                    r".{0,80}\b(?:purge|invalidate|evict|remove|tombstone)\w*\b|"
                    rf"\b(?:purge|invalidate|evict|remove)\w*\b.{{0,80}}"
                    rf"\b{inactive_state}\b.{{0,80}}"
                    r"\b(?:before\s+(?:acknowledg|commit)|synchronous(?:ly)?)\w*\b",
                    window,
                )
            )
            if not safely_rejected:
                inactive_staleness = True
                break
        if inactive_staleness:
            break
    if inactive_staleness:
        issues.append("unsafe_inactive_state_cache_staleness")

    unsafe_revocable_http_cache = False
    for clause in clauses:
        positive_cache_window = bool(
            re.search(
                r"\b(?:cache-control|surrogate-control)\s*:\s*[^.!?;\n]{0,100}"
                r"\b(?:s-maxage|max-age)\s*=\s*[1-9]\d*\b|"
                r"\bcache-control\s*:\s*(?:public|private)\b|"
                r"\b(?:browser|client|intermediary|cdn)\b.{0,45}\bcach\w*\b"
                r".{0,65}\b(?:for\s+(?:\d+|one|two|three)\s*"
                r"(?:seconds?|minutes?|hours?|days?)|ttl\s+(?:is|of|=)\s*"
                r"(?:\d+|one|two|three)(?:\s*(?:seconds?|minutes?|hours?|days?))?|"
                r"(?:s-maxage|max-age)\s*=\s*[1-9]\d*)\b|"
                r"\bcdn\s+cache\s+ttl\b.{0,25}\b(?:\d+|one|two|three)\b",
                clause,
            )
        )
        safely_rejected = bool(
            re.search(
                r"\b(?:do\s+not|don't|never|must\s+not|should\s+not|cannot|can't)\b"
                r".{0,40}\b(?:send|set|use|allow|emit)?\w*\b.{0,25}"
                r"\b(?:cache-control|surrogate-control|browser|client|intermediary|cdn)\b"
                r".{0,45}\b(?:s-maxage|max-age|cache|store|ttl)\w*\b|"
                r"\b(?:s-maxage|max-age)\s*=\s*[1-9]\d*\b.{0,35}"
                r"\b(?:forbidden|disallowed|not\s+used)\b",
                clause,
            )
        )
        safe_non_revocable_cache = bool(
            re.search(r"\bnon[- ]revocable\b", clause)
            and re.search(
                r"\b(?:exclude\w*\s+from|without|no)\b.{0,90}"
                r"\b(?:delet\w*|expir\w*|moderat\w*|revoc\w*|legal)\b",
                clause,
            )
            and re.search(r"\b(?:accept|assume)\w*\b.{0,65}\brisk\b", clause)
        )
        if positive_cache_window and not safely_rejected and not safe_non_revocable_cache:
            unsafe_revocable_http_cache = True
            break
    if unsafe_revocable_http_cache:
        issues.append("unsafe_revocable_redirect_http_cache_window")

    if require_revocation_completeness:
        client_cache_boundary = False
        for clause in clauses:
            positive_no_store = bool(re.search(r"\bcache-control\s*:\s*no-store\b", clause))
            negated_no_store = bool(
                re.search(
                    r"\b(?:do\s+not|don't|never|must\s+not|should\s+not|"
                    r"cannot|can't|without)\b.{0,45}\bcache-control\s*:\s*no-store\b",
                    clause,
                )
            )
            natural_boundary = bool(
                re.search(
                    r"\b(?:browser|client|intermediary|cdn)\b.{0,55}"
                    r"\b(?:must\s+not|cannot|can't|never|do\s+not)\b.{0,35}"
                    r"\b(?:cache|store)\w*\b.{0,35}\bredirect\w*\b|"
                    r"\bredirect\w*\b.{0,45}\b(?:must\s+not|cannot|can't|never|"
                    r"do\s+not)\b.{0,35}\b(?:cache|store)\w*\b",
                    clause,
                )
            )
            if (positive_no_store and not negated_no_store) or natural_boundary:
                client_cache_boundary = True
                break
        if not client_cache_boundary:
            issues.append("missing_revocable_redirect_client_cache_boundary")

        overlay_record = r"(?:tombstone|deny\s+overlay|revocation\s+overlay)"
        overlay_enforcement_action = (
            r"(?:(?:read|check|honor|enforce|consult|respect)\w*|"
            r"appl(?:y|ies|ied|ying))"
        )
        explicit_overlay_non_enforcement = any(
            re.search(
                rf"\b{overlay_record}\b[^.!?;]{{0,55}}"
                rf"\b(?:(?:(?:is|are|remains?)\s+(?:not|never)|isn't|aren't)\s+"
                rf"{overlay_enforcement_action}|"
                r"(?:is|are|remains?)\s+(?:unenforced|unchecked|advisory|optional|"
                r"disabled|ignored|bypassed))\b|"
                rf"\b{overlay_record}\s+(?:checks?|enforcement)\b[^.!?;]{{0,35}}"
                r"\b(?:is|are|remains?)\s+(?:disabled|skipped|absent|optional|advisory)\b|"
                r"\b(?:redirectors?|redirect\s+(?:service|path|workers?)|read\s+path)\b"
                rf"[^.!?;]{{0,55}}\b(?:do(?:es)?\s+not|never|fails?\s+to)\s+"
                rf"{overlay_enforcement_action}\b[^.!?;]{{0,45}}\b{overlay_record}\b|"
                r"\bno\s+(?:redirector|redirect\s+(?:service|path|worker)|read\s+path)\b"
                rf"[^.!?;]{{0,45}}\b{overlay_enforcement_action}\b"
                rf"[^.!?;]{{0,45}}\b{overlay_record}\b",
                clause,
            )
            for clause in clauses
        )
        tombstone_not_enforced = explicit_overlay_non_enforcement or bool(
            re.search(
                r"\b(?:redirectors?|redirect\s+(?:service|path|workers?)|read\s+path)\b"
                r".{0,55}\b(?:ignore\w*|bypass\w*|does?\s+not\s+(?:read|check|honor|"
                r"enforce|consult|respect)\w*)\b.{0,45}"
                r"\b(?:tombstone|deny\s+overlay|revocation\s+overlay)\b|"
                r"\b(?:tombstone|deny\s+overlay|revocation\s+overlay)\b.{0,55}"
                r"\b(?:is|are|remains?)\s+(?:ignored|bypassed|unenforced|unchecked)\b|"
                r"\b(?:redirectors?|redirect\s+(?:service|path|workers?))\b.{0,70}"
                r"\b(?:continue\w*|may|can|will)\b.{0,45}"
                r"\b(?:serve|return|use)?\w*\b.{0,25}"
                r"\b(?:cached\s+destinations?|redirect\w*)\b|"
                r"\b(?:redirectors?|redirect\s+(?:service|path|workers?)|read\s+path)\b"
                r".{0,60}\b(?:read|check|honor|enforce|consult|respect)\w*\b"
                r".{0,45}\b(?:tombstone|deny\s+overlay|revocation\s+overlay)\b"
                r"[^.!?;]{0,75}\b(?:only\s+during\b[^.!?;]{0,30}\b(?:eventual|async)\w*|"
                r"eventual(?:ly)?|asynchronous(?:ly)?|later|delayed?)\b|"
                r"\b(?:redirectors?|redirect\s+(?:service|path|workers?)|read\s+path)\b"
                r".{0,65}\b(?:read|check|serve|consult|use)\w*\b.{0,35}"
                r"\b(?:cache|cached\s+(?:active\s+)?mapping|destination)\b"
                r".{0,50}\b(?:before|then)\b.{0,45}"
                r"\b(?:read|check|honor|enforce|consult)\w*\b.{0,45}"
                r"\b(?:tombstone|deny\s+overlay|revocation\s+overlay)\b|"
                r"\b(?:redirectors?|redirect\s+(?:service|path|workers?)|read\s+path|they)\b"
                r".{0,65}\bfail(?:s|ed|ing)?[- ]open\b.{0,55}"
                r"\b(?:cached?\w*|destinations?|redirect\w*)\b|"
                r"\bfail(?:s|ed|ing)?[- ]open\b.{0,55}"
                r"\b(?:cached?\w*|destinations?|redirect\w*)\b|"
                r"\b(?:redirectors?|redirect\s+(?:service|path|workers?)|read\s+path)\b"
                r".{0,55}\b(?:eventual(?:ly)?|asynchronous(?:ly)?|later|delayed?)\b"
                r".{0,45}\b(?:read|check|honor|enforce|consult|respect)\w*\b"
                r".{0,45}\b(?:tombstone|deny\s+overlay|revocation\s+overlay)\b",
                lower,
            )
        )
        tombstone_enforced = bool(
            re.search(
                r"\b(?:redirectors?|redirect\s+(?:service|path|workers?)|read\s+path|cache)\b"
                r".{0,65}\b(?:read|check|honor|enforce|consult|respect)\w*\b"
                r".{0,50}\b(?:tombstone|deny\s+overlay|revocation\s+overlay|"
                r"authoritative\s+state)\b|"
                r"\b(?:tombstone|deny\s+overlay|revocation\s+overlay)\b.{0,55}"
                r"\b(?:block|prevent|suppress|disable|deny)\w*\b.{0,35}"
                r"\bredirect\w*\b",
                lower,
            )
        )
        fail_closed_until_authoritative = bool(
            re.search(
                r"\bfail\s+closed\b.{0,80}\buntil\b.{0,45}"
                r"\bauthoritative\s+state\b.{0,30}\b(?:confirm|check|verif)\w*\b",
                lower,
            )
        )
        overlay_checked_before_cache = bool(
            re.search(
                r"\b(?:redirect\s+(?:path|worker|service)|redirectors?)\b"
                r".{0,70}\b(?:check|read|consult|enforce)\w*\b.{0,35}"
                r"\b(?:versioned\s+)?(?:deny\s+overlay|revocation\s+overlay)\b"
                r".{0,55}\b(?:first|before)\b.{0,55}"
                r"\b(?:cache|cached\s+(?:active\s+)?mapping|destination)\b|"
                r"\b(?:deny\s+overlay|revocation\s+overlay)\b.{0,45}"
                r"\b(?:first|before)\b.{0,65}"
                r"\b(?:cache|cached\s+(?:active\s+)?mapping|destination)\b",
                lower,
            )
        )
        fail_closed_on_overlay_or_cache_uncertainty = bool(
            re.search(
                r"\bfail(?:s|ed|ing)?\s+closed\b.{0,170}"
                r"\b(?:overlay(?:\s+or\s+cache)?|cache)\s+state\b.{0,45}"
                r"\b(?:uncertain|unknown|unavailable)\b|"
                r"\b(?:overlay(?:\s+or\s+cache)?|cache)\s+state\b.{0,115}"
                r"\b(?:uncertain|unknown|unavailable)\b.{0,170}"
                r"\bfail(?:s|ed|ing)?\s+closed\b",
                lower,
            )
        )
        explicit_inactive_non_redirect_outcomes = bool(
            re.search(
                r"\bdeleted\b.{0,55}\bexpired\b.{0,55}\b(?:404|410)\b",
                lower,
            )
            and re.search(
                r"\babuse[- ]blocked\b.{0,55}"
                r"\b(?:403|safe\s+interstitial)\b",
                lower,
            )
            and re.search(r"\blegal\s+block\b.{0,55}\b451\b", lower)
        )
        inactive_revocation_barrier = False
        for clause in clauses:
            has_inactive_state = bool(
                re.search(
                    r"\b(?:delet\w*|expir\w*|abuse[- ]block\w*|legal[- ]block\w*|"
                    r"inactive\s+states?)\b",
                    clause,
                )
            )
            has_barrier_record = bool(
                re.search(r"\b(?:tombstone|deny\s+overlay|revocation\s+overlay)\b", clause)
            )
            has_synchronous_boundary = bool(
                re.search(
                    r"\b(?:synchronous(?:ly)?|atomically|before\s+acknowledg\w*|"
                    r"fail\s+closed|authoritative\s+state\s+check)\b",
                    clause,
                )
            )
            has_redirect_suppression = bool(
                re.search(
                    r"\b(?:purge|invalidate|evict|non[- ]redirect|"
                    r"authoritative\s+state\s+check)\w*\b",
                    clause,
                )
                or tombstone_enforced
                or re.search(
                    r"\bfail\s+closed\b.{0,70}\buntil\b.{0,35}"
                    r"\bauthoritative\s+state\b.{0,25}"
                    r"\b(?:confirm|check|verif)\w*\b",
                    clause,
                )
            )
            negated_barrier = bool(
                re.search(
                    r"\b(?:do\s+not|don't|never|must\s+not|should\s+not|"
                    r"cannot|can't|without)\b.{0,55}"
                    r"\b(?:write|commit|publish|install|retain|purge|invalidate|"
                    r"evict|use)?\w*\b.{0,25}"
                    r"\b(?:tombstone|deny\s+overlay|revocation\s+overlay|"
                    r"redirect\s+cache|cached\s+redirect)\b|"
                    r"\b(?:tombstone|deny\s+overlay|revocation\s+overlay)\b.{0,100}"
                    r"\b(?:asynchronous(?:ly)?|eventual(?:ly)?|later|delayed?)\b|"
                    r"\b(?:asynchronous(?:ly)?|eventual(?:ly)?|later|delayed?)\b"
                    r".{0,100}\b(?:tombstone|deny\s+overlay|revocation\s+overlay)\b",
                    clause,
                )
                or re.search(
                    r"\b(?:do\s+not|don't|never|must\s+not|should\s+not|"
                    r"cannot|can't|without)\b.{0,35}\bfail\s+closed\b",
                    clause,
                )
            )
            if (
                has_inactive_state
                and has_barrier_record
                and has_synchronous_boundary
                and has_redirect_suppression
                and not negated_barrier
                and not tombstone_not_enforced
            ):
                inactive_revocation_barrier = True
                break
        if not inactive_revocation_barrier:
            global_inactive_state = bool(
                re.search(
                    r"\b(?:delet\w*|expir\w*|abuse[- ]block\w*|legal[- ]block\w*|"
                    r"inactive\s+states?)\b",
                    lower,
                )
            )
            global_barrier_record = bool(
                re.search(r"\b(?:tombstone|deny\s+overlay|revocation\s+overlay)\b", lower)
            )
            global_synchronous_boundary = bool(
                re.search(
                    r"\b(?:synchronous(?:ly)?|atomically|before\s+acknowledg\w*|"
                    r"fail\s+closed|authoritative\s+state\s+check)\b",
                    lower,
                )
            )
            global_negated_barrier = bool(
                re.search(
                    r"\b(?:tombstone|deny\s+overlay|revocation\s+overlay)\b[^.!?;]{0,100}"
                    r"\b(?:asynchronous(?:ly)?|eventual(?:ly)?|later|delayed?)\b|"
                    r"\b(?:asynchronous(?:ly)?|eventual(?:ly)?|later|delayed?)\b[^.!?;]{0,100}"
                    r"\b(?:tombstone|deny\s+overlay|revocation\s+overlay)\b|"
                    r"\b(?:do\s+not|don't|never|must\s+not|should\s+not|cannot|can't|"
                    r"without)\b.{0,45}\bfail\s+closed\b",
                    lower,
                )
            )
            inactive_revocation_barrier = bool(
                global_inactive_state
                and global_barrier_record
                and global_synchronous_boundary
                and (tombstone_enforced or fail_closed_until_authoritative)
                and not global_negated_barrier
                and not tombstone_not_enforced
            )
            # A versioned deny-overlay read before cache plus fail-closed handling for
            # overlay/cache uncertainty is an equivalent per-request revocation
            # boundary.  It is safe even when the response does not describe the write
            # acknowledgement sequence, provided it also states non-redirect outcomes
            # for each inactive lifecycle class.
            inactive_revocation_barrier = inactive_revocation_barrier or bool(
                global_inactive_state
                and global_barrier_record
                and overlay_checked_before_cache
                and fail_closed_on_overlay_or_cache_uncertainty
                and explicit_inactive_non_redirect_outcomes
                and not global_negated_barrier
                and not tombstone_not_enforced
            )
        if not inactive_revocation_barrier:
            issues.append("missing_inactive_state_revocation_barrier")

    if re.search(
        r"\breconcil\w*\b.{0,65}\b(?:payment|charge|card|processor|ledger)\b|"
        r"\b(?:payment|charge|card|processor|ledger)\b.{0,65}\breconcil\w*\b|"
        r"\bprovider\s+timeout\b.{0,130}\breconcil\w*\b.{0,90}"
        r"\b(?:terminal\s+(?:failure|state|outcome)|ambiguous\s+outcome|unknown)\b|"
        r"\breconcil\w*\b.{0,90}\b(?:terminal\s+(?:failure|state|outcome)|"
        r"ambiguous\s+outcome|unknown)\b.{0,130}\bprovider\s+timeout\b|"
        r"\bpayment\s+idempotency\s+key\b.{0,80}\b(?:redirect|url|mapping|link)\b|"
        r"\b(?:psp|payment\s+service\s+provider|processor|gateway)\s+timeout\b"
        r".{0,100}\b(?:card|capture|transaction)\w*\b.{0,100}"
        r"\b(?:unknown|poll\w*\s+(?:the\s+)?gateway)\b",
        lower,
    ):
        issues.append("irrelevant_payment_reconciliation_in_url_design")
    return issues

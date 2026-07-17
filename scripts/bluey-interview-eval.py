#!/usr/bin/env python3
"""Run a production-like, 50-question Bluey interview quality evaluation.

The harness logs in with an internal test account, calls Bluey's managed SSE
answer path, records full visible answers and artifacts locally, and produces a
deterministic first-pass scorecard for reliability, latency, human voice,
context retention, and task completeness.

Raw answers and extracted resume/JD context are private test artifacts. Keep
the output directory under ``tmp/`` and do not commit it.
"""

from __future__ import annotations

import argparse
import ast
import hashlib
import json
import os
import re
import statistics
import sys
import time
import uuid
import zipfile
from dataclasses import asdict, dataclass, field
from html import unescape
from pathlib import Path
from typing import Any, Dict, Iterable, List, Optional, Sequence, Tuple
from urllib import error, parse, request

SCRIPT_DIR = Path(__file__).resolve().parent
if str(SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIR))

from bluey_eval.payment_contracts import (  # noqa: E402
    has_exactly_once_processing_overclaim,
    has_explicit_no_provider_command_replay,
    has_mysql_not_valid_portability_claim,
    has_safe_payment_same_operation_replay_condition,
    has_unsafe_ambiguous_payment_outcome,
    large_fk_migration_safety_issues,
    payment_operation_semantic_issues,
    payment_platform_safety_issues,
    self_check_ambiguous_payment_detector,
    self_check_exactly_once_processing_detector,
    self_check_large_fk_migration_safety,
    self_check_payment_operation_semantics,
    self_check_payment_platform_safety_detector,
)
from bluey_eval.system_contracts import (  # noqa: E402
    feature_store_consistency_issues,
    has_drift_only_automatic_retraining,
    has_required_signal,
    payment_timeout_followup_completeness_issues,
    q47_director_alignment_issues,
    rag_evaluation_plan_issues,
    url_shortener_safety_issues,
)


DEFAULT_API_BASE = "https://bluey.sh"
DEFAULT_CREDENTIALS = Path.home() / ".bluey/internal-test-accounts-20260606023943.json"
DEFAULT_OUTPUT = Path("tmp/bluey-interview-eval-20260710")
DEFAULT_DOWNLOADS = Path.home() / "Downloads"
BASE_SYSTEM = (
    "You are Bluey, a fast, accurate desktop work copilot. Give the direct "
    "answer first in natural, speakable language, then the minimum reasoning "
    "needed to make it defensible. Use supplied screen, transcript, document, "
    "and conversation context only when it is relevant to the latest question. "
    "Treat a standalone new topic as new. State important assumptions and never "
    "invent personal experience, project facts, metrics, or missing details. "
    "When code is requested, return a complete runnable fenced implementation; "
    "when existing code changes, return the complete updated implementation."
)

CONTACT_PATTERNS = (
    (re.compile(r"[\w.+-]+@[\w.-]+\.[A-Za-z]{2,}"), "[email removed]"),
    (re.compile(r"(?<!\d)(?:\+?1[-.\s]?)?(?:\(?\d{3}\)?[-.\s]?)\d{3}[-.\s]?\d{4}(?!\d)"), "[phone removed]"),
    (re.compile(r"https?://(?:www\.)?linkedin\.com/\S+", re.I), "[profile link removed]"),
)

ERROR_PHRASES = (
    "could not complete",
    "connection dropped",
    "capacity busy",
    "try again",
    "provider error",
    "insufficient credits",
)
CONTEXT_LOSS_PHRASES = (
    "i don't have the previous",
    "i do not have the previous",
    "i need the rest of the problem",
    "send the full prompt",
    "not enough context",
    "cannot see the prior",
)
META_OPENERS = (
    "sure",
    "here is",
    "here's",
    "you can say",
    "i would say",
    "based on the resume",
    "as an ai",
)

# Some providers do not expose a finish reason through Bluey's billing event.
# An answer that lands exactly on one of these historical/server output caps and
# ends mid-structure is therefore treated as incomplete rather than as a clean
# success. Requested per-case caps are checked separately.
KNOWN_OUTPUT_TOKEN_CAPS = frozenset((256, 512, 1024, 2048, 4096))
MIN_SUBSTANTIVE_ANSWER_WORDS = 30
DEFAULT_MAX_ANSWER_FIRST_TOKEN_P95_MS = 5000.0
DEFAULT_MAX_INTERVENTION_FIRST_TOKEN_P95_MS = 500.0


@dataclass(frozen=True)
class ProfileSpec:
    name: str
    role: str
    resume: Optional[str] = None
    job_description: Optional[str] = None
    extra_document: Optional[str] = None


@dataclass(frozen=True)
class EvalCase:
    id: str
    category: str
    profile: str
    question: str
    origin: str = "curated"
    conversation: Optional[str] = None
    max_tokens: int = 700
    speakable: bool = False
    self_intro: bool = False
    expect_code: bool = False
    expect_design: bool = False
    expect_followup_context: bool = False
    expected_outcome: str = "answer"
    required_groups: Tuple[Tuple[str, ...], ...] = ()


@dataclass
class AttemptResult:
    attempt: int
    ok: bool = False
    status: Optional[int] = None
    request_id: str = ""
    session_id: str = ""
    first_event_ms: Optional[float] = None
    first_token_ms: Optional[float] = None
    total_ms: float = 0.0
    event_count: int = 0
    delta_count: int = 0
    done: bool = False
    error: Optional[str] = None
    error_reason: Optional[str] = None
    error_ref: Optional[str] = None
    visible_answer: str = ""
    streamed_answer: str = ""
    terminal_answer: str = ""
    billing_received: bool = False
    billing_event_count: int = 0
    billing_error: Optional[str] = None
    artifact_type: Optional[str] = None
    artifact_body: Optional[str] = None
    provider: Optional[str] = None
    model: Optional[str] = None
    input_tokens: Optional[int] = None
    output_tokens: Optional[int] = None
    cost_cents: int = 0
    balance_cents_after: Optional[int] = None
    trial_seconds_remaining: Optional[int] = None
    statuses: List[Dict[str, Any]] = field(default_factory=list)
    sources: List[Dict[str, Any]] = field(default_factory=list)


@dataclass
class CaseResult:
    id: str
    category: str
    profile: str
    origin: str
    question: str
    conversation: Optional[str]
    context_sha256: str
    attempts: List[AttemptResult]
    cumulative_elapsed_ms: float
    final_ok: bool
    first_attempt_ok: bool
    accepted_outcome: bool
    first_attempt_accepted: bool
    score: int
    reliability_score: int
    latency_score: int
    human_score: int
    accuracy_score: int
    issues: List[str]


PROFILES: Dict[str, ProfileSpec] = {
    "general": ProfileSpec("general", "Senior software engineer"),
    "sde": ProfileSpec(
        "sde",
        "Senior software engineer focused on distributed backend systems",
        "Ruchi_Resume_SDE.pdf",
    ),
    "de": ProfileSpec(
        "de",
        "Senior data engineer in regulated financial systems",
        "Tharun_DE_Resume (1).pdf",
    ),
    "ds": ProfileSpec(
        "ds",
        "Senior data scientist and GenAI/ML engineer",
        "Mahidhar_Data_Scientist.pdf",
        "hpe_jd.pdf",
    ),
    "amazon_de": ProfileSpec(
        "amazon_de",
        "Senior data engineer interviewing for an Amazon-style role",
        "Tharun_DE_Resume (1).pdf",
        "AMAZON JOB DESCRIPTION.pdf",
        "LPs_loop Interview.docx",
    ),
}


def g(*alternatives: str) -> Tuple[str, ...]:
    return tuple(alternatives)


CASES: Tuple[EvalCase, ...] = (
    EvalCase("Q01", "behavioral", "sde", "Tell me about yourself for this senior software engineering interview.", "resume_pdf", speakable=True, self_intro=True, required_groups=(g("distributed", "backend"), g("100k", "100,000", "50m", "50 million", "10k", "10,000"))),
    EvalCase("Q02", "behavioral", "sde", "Walk me through the most technically challenging backend project you built.", "otter_interview_style", "sde_project", speakable=True, required_groups=(g("payment", "monitoring", "order matching"), g("scale", "100k", "50m", "10k"))),
    EvalCase("Q03", "followup", "sde", "Why did you choose that architecture, and what tradeoff did you accept?", "otter_interview_style", "sde_project", speakable=True, expect_followup_context=True, required_groups=(g("tradeoff", "because", "chose"),)),
    EvalCase("Q04", "behavioral", "sde", "Tell me about a production issue you owned from detection through rollout.", "behavioral_doc", speakable=True, required_groups=(g("monitor", "incident", "production"), g("verify", "test", "rollout", "metric"))),
    EvalCase("Q05", "technical", "sde", "How do you approach API versioning in a production service?", speakable=True, required_groups=(g("v1", "version"), g("backward", "deprecat", "contract"))),
    EvalCase("Q06", "scenario", "sde", "You own code that depends on a flaky third-party API. How do you make the path reliable?", speakable=True, required_groups=(g("timeout", "retry"), g("circuit", "fallback", "idempot"), g("observ", "metric", "trace"))),
    EvalCase("Q07", "technical", "general", "Explain an LRU cache as if an interviewer asked you on a call.", speakable=True, required_groups=(g("least recently used", "lru"), g("hash", "map"), g("doubly linked", "linked list"), g("o(1)",))),
    EvalCase("Q08", "coding", "general", "Implement an LRU cache from first principles in Python. Include comments, explanation, and time and space complexity.", "curated", "lru_code", max_tokens=1200, expect_code=True, required_groups=(g("class lrucache",), g("get",), g("put",), g("time complexity", "complexity"))),
    EvalCase("Q09", "code_followup", "general", "Make that same LRU implementation thread-safe without replacing it with a library cache. Return the complete updated code.", "curated", "lru_code", max_tokens=1400, expect_code=True, expect_followup_context=True, required_groups=(g("lock", "rlock"), g("class lrucache",), g("get",), g("put",))),
    EvalCase("Q10", "scenario", "general", "A junior engineer wants to add a foreign key constraint to a 200 million row production table. What do you tell them?", "otter_interview_style", speakable=True, required_groups=(g("lock", "blocking"), g("not valid", "validate constraint", "online"), g("batch", "backfill"))),
    EvalCase("Q11", "scenario", "sde", "A release improves average latency but makes p99 worse. Would you ship it? Walk me through the decision.", speakable=True, required_groups=(g("p99", "tail"), g("segment", "workload", "trace", "endpoint", "transaction type", "code path", "cohort"), g("rollback", "canary", "slo"))),
    EvalCase("Q12", "behavioral", "sde", "Tell me about a time you disagreed with a product or engineering decision and how you handled it.", "behavioral_doc", speakable=True, required_groups=(g("disagree", "concern", "tradeoff"), g("data", "evidence", "experiment"), g("align", "decision", "commit"))),

    EvalCase("Q13", "behavioral", "de", "Tell me about yourself for a senior data engineering role.", "resume_pdf", speakable=True, self_intro=True, required_groups=(g("data engineer",), g("spark", "kafka"), g("capital one", "fidelity"))),
    EvalCase("Q14", "behavioral", "de", "Walk me through a data pipeline you built that had meaningful scale and business impact.", "otter_interview_style", "de_pipeline", speakable=True, required_groups=(g("spark", "kafka", "glue"), g("terabyte", "scale"), g("capital one", "fidelity"))),
    EvalCase("Q15", "followup", "de", "How did you prove the data was correct before downstream teams trusted it?", "otter_interview_style", "de_pipeline", speakable=True, expect_followup_context=True, required_groups=(g("reconcil", "row count", "control total"), g("quality", "validation", "schema"), g("monitor", "alert"))),
    EvalCase("Q16", "scenario", "de", "Kafka consumer lag suddenly grows while producer traffic is flat. What is your first investigation path?", speakable=True, required_groups=(g("partition", "consumer"), g("latency", "throughput", "processing"), g("offset", "rebalance", "hot"))),
    EvalCase("Q17", "scenario", "de", "One Spark stage is much slower than every other stage and a few tasks hold the job open. Diagnose it.", speakable=True, required_groups=(g("skew",), g("shuffle",), g("salt", "repartition", "adaptive"))),
    EvalCase("Q18", "technical", "de", "How do you handle late and out-of-order events in a streaming pipeline?", speakable=True, required_groups=(g("watermark",), g("event time",), g("dedup", "idempot"))),
    EvalCase("Q19", "technical", "de", "How do you roll out a breaking schema change when producers and consumers deploy independently?", speakable=True, required_groups=(g("backward", "compatible"), g("version",), g("dual", "expand", "contract"))),
    EvalCase("Q20", "scenario", "de", "Snowflake spend doubled this month but row volume grew only ten percent. Where do you start?", speakable=True, required_groups=(g("query", "warehouse"), g("credit", "cost"), g("scan", "partition", "cache"))),
    EvalCase("Q21", "technical", "de", "What does exactly-once really mean in a Kafka-to-warehouse pipeline, and where can it still break?", speakable=True, required_groups=(g("idempot", "transaction"), g("offset",), g("sink", "warehouse"))),
    EvalCase("Q22", "scenario", "de", "Finance says Monday revenue is different in the dashboard and source table. How do you isolate the problem?", speakable=True, required_groups=(g("definition", "timezone", "filter"), g("source", "lineage", "pipeline"), g("reconcil", "row", "aggregate"))),
    EvalCase("Q23", "behavioral", "amazon_de", "Tell me about a time you reduced cloud data-platform cost without hurting reliability.", "leadership_doc", speakable=True, expected_outcome="needs_user_input", required_groups=(g("glue", "spark"), g("70%", "30%", "cost"), g("reliab", "sla", "monitor"))),

    EvalCase("Q24", "behavioral", "ds", "Tell me about yourself for this data science and AI platform role.", "resume_and_jd_pdf", speakable=True, self_intro=True, required_groups=(g("data scientist", "machine learning", "genai"), g("rag", "fraud"), g("hpe", "datacenter", "platform"))),
    EvalCase("Q25", "behavioral", "ds", "Walk me through the secure RAG system you built and the decision you personally owned.", "otter_interview_style", "rag_project", speakable=True, required_groups=(g("2m", "document"), g("98%", "precision"), g("secure", "access"))),
    EvalCase("Q26", "followup", "ds", "Where could that RAG system hallucinate, and what did you put in place to catch it?", "otter_interview_style", "rag_project", speakable=True, expect_followup_context=True, required_groups=(g("retriev", "ground"), g("citation", "source"), g("eval", "threshold", "fallback"))),
    EvalCase("Q27", "technical", "ds", "Design an evaluation plan for a RAG assistant before production launch.", speakable=True, required_groups=(g("recall@k", "recall at k", "mrr", "ndcg"), g("faithful", "ground", "hallucin"), g("citation correctness", "citation precision", "citation recall"), g("no-answer", "unanswerable", "refusal", "abstain"), g("adversarial", "prompt injection"), g("acl", "permission", "tenant leakage"), g("pii", "privacy"), g("latency",), g("cost",), g("human", "golden", "dataset"), g("baseline", "champion"), g("slice", "segment"), g("regression", "launch gate"))),
    EvalCase("Q28", "scenario", "ds", "A fraud model has 99.8 percent accuracy but misses expensive fraud. What is wrong with the evaluation?", speakable=True, required_groups=(g("class imbalance", "imbalanc"), g("precision", "recall"), g("cost", "threshold", "loss"))),
    EvalCase("Q29", "scenario", "ds", "A model was strong offline but degrades two months after launch. How do you determine whether this is drift or a pipeline bug?", speakable=True, required_groups=(g("drift",), g("feature", "pipeline"), g("distribution", "monitor"), g("label", "ground truth"))),
    EvalCase("Q30", "technical", "ds", "How would you reduce p95 latency for a large language model service without silently reducing answer quality?", speakable=True, required_groups=(g("batch", "cache", "quant", "vllm"), g("p95", "latency"), g("quality", "eval"), g("canary", "measure"))),
    EvalCase("Q31", "technical", "ds", "Why can graph features help a fraud model beyond ordinary transaction aggregates?", speakable=True, required_groups=(g("relationship", "network", "graph"), g("ring", "connected"), g("leak", "time"))),
    EvalCase("Q32", "scenario", "ds", "An executive asks why the model rejected a high-value customer. Give the answer you would use in that meeting.", speakable=True, required_groups=(g("factor", "feature", "reason"), g("confidence", "threshold"), g("review", "appeal", "policy"))),
    EvalCase("Q33", "behavioral", "ds", "Why does your background fit a role working on streaming datacenter telemetry, anomaly detection, and search?", "resume_and_jd_pdf", speakable=True, required_groups=(g("stream", "telemetry"), g("anomal", "search"), g("rag", "ml", "platform"))),

    EvalCase("Q34", "system_design", "general", "Design a production messaging app for tens of millions of users. Explain it like a system design interview.", "curated", "messaging_design", max_tokens=1100, speakable=True, expect_design=True, required_groups=(g("websocket", "connection"), g("message", "queue"), g("storage", "database"), g("failure", "retry"))),
    EvalCase("Q35", "design_followup", "general", "How would you preserve per-conversation ordering when users reconnect and servers fail?", "curated", "messaging_design", max_tokens=800, speakable=True, expect_followup_context=True, required_groups=(g("sequence", "offset", "order"), g("idempot", "dedup"), g("reconnect", "replay"))),
    EvalCase("Q36", "system_design", "general", "Design a real-time monitoring platform ingesting 100,000 events per second with alerting and historical queries.", "resume_inspired", "monitoring_design", max_tokens=1100, speakable=True, expect_design=True, required_groups=(g("kafka", "queue", "stream"), g("time series", "storage"), g("alert",), g("partition", "scale"))),
    EvalCase("Q37", "design_followup", "general", "One tenant becomes a hot partition. Change the design without breaking ordering for that tenant.", "curated", "monitoring_design", max_tokens=850, speakable=True, expect_followup_context=True, required_groups=(g("partition", "shard"), g("order", "sequence"), g("tenant",))),
    EvalCase("Q38", "system_design", "ds", "Design an online feature store that serves low-latency features and keeps training data consistent with serving.", max_tokens=1100, speakable=True, expect_design=True, required_groups=(g("offline", "batch training"), g("online", "live serving"), g("event time", "event timestamp", "source timestamp"), g("availability time", "availability timestamp", "ingestion time", "knowledge time", "known by"), g("as-of", "as of", "temporal join", "point-in-time join", "snapshot join"), g("executable transformation", "executable transformations", "compiled feature definition", "shared feature code", "shared transformation definition", "same executable feature definition", "same executable feature definitions", "versioned transformation code", "versioned dsl"), g("late event", "out-of-order", "watermark"), g("idempot", "dedup"), g("skew", "parity", "equivalence"), g("fresh", "stream"))),
    EvalCase("Q39", "system_design", "general", "Design a payment processing platform that safely handles retries and duplicate requests.", "curated", "payment_design", max_tokens=1100, speakable=True, expect_design=True, required_groups=(g("idempot",), g("ledger",), g("webhook", "processor"), g("reconcil",))),
    EvalCase("Q40", "design_followup", "general", "The provider times out after charging the card. What exact state transition and retry behavior do you use?", "curated", "payment_design", max_tokens=850, speakable=True, expect_followup_context=True, required_groups=(g("unknown", "pending", "reconcil"), g("idempot",), g("webhook", "query"))),
    EvalCase("Q41", "system_design", "general", "Design a URL shortener and make the main scale and consistency tradeoff explicit.", max_tokens=950, speakable=True, expect_design=True, required_groups=(g("key", "id"), g("cache",), g("redirect",), g("consistency", "collision"))),
    EvalCase("Q42", "system_design", "ds", "Design a multi-tenant enterprise RAG platform with document permissions, citations, and cost controls.", max_tokens=1200, speakable=True, expect_design=True, required_groups=(g("tenant", "permission", "acl"), g("chunk", "embedding"), g("citation",), g("cost", "quota"))),

    EvalCase("Q43", "behavioral", "amazon_de", "Tell me about a time the requirements were ambiguous and you still moved the work forward safely.", "behavioral_doc", speakable=True, expected_outcome="needs_user_input", required_groups=(g("clarif", "stakeholder", "requirement"), g("assumption", "scope", "prototype"), g("result", "outcome"))),
    EvalCase("Q44", "behavioral", "amazon_de", "Tell me about a time you challenged a decision with data and then committed to the final direction.", "leadership_doc", speakable=True, expected_outcome="needs_user_input", required_groups=(g("data", "evidence"), g("disagree", "challenge"), g("commit", "align"))),
    EvalCase("Q45", "behavioral", "amazon_de", "Tell me about a failure. What did you change so the same class of failure would not repeat?", "behavioral_doc", speakable=True, expected_outcome="needs_user_input", required_groups=(g("fail", "mistake"), g("root cause", "learn"), g("guardrail", "test", "monitor", "process"))),
    EvalCase("Q46", "behavioral", "amazon_de", "Give me an example of ownership beyond your assigned task.", "leadership_doc", speakable=True, expected_outcome="needs_user_input", required_groups=(g("ownership", "took"), g("customer", "team", "impact"), g("result", "reduced", "improved"))),
    EvalCase("Q47", "behavioral", "amazon_de", "Two urgent requests arrive from different directors and both claim top priority. What do you do?", "behavioral_doc", speakable=True, required_groups=(g("impact", "severity", "customer"), g("align", "stakeholder", "tradeoff to both directors", "visible to both directors", "both directors together"), g("communicat", "tradeoff", "lay out"))),
    EvalCase("Q48", "behavioral", "sde", "A junior engineer keeps making the same code review mistake. How do you coach them without taking over the work?", speakable=True, required_groups=(g("coach", "explain"), g("example", "pair", "checklist"), g("follow", "ownership"))),
    EvalCase("Q49", "scenario", "ds", "Two cameras and two sensors overlap, so the same vehicle can be detected multiple times. How would you prevent double counting?", "otter_visible_scenario", speakable=True, required_groups=(g("track", "identity"), g("calibrat", "time", "spatial"), g("dedup", "fusion", "association"))),
    EvalCase("Q50", "behavioral", "ds", "Why this role, and what would you focus on in your first ninety days?", "resume_and_jd_pdf", speakable=True, required_groups=(g("hpe", "datacenter", "telemetry"), g("first", "90", "ninety"), g("stakeholder", "baseline", "production"))),
)


def normalize_base(value: str) -> str:
    parsed = parse.urlsplit(value.strip())
    if parsed.scheme not in ("http", "https") or not parsed.netloc:
        raise ValueError("API base must be an http(s) URL")
    return parse.urlunsplit((parsed.scheme, parsed.netloc, parsed.path.rstrip("/"), "", ""))


def api_url(base: str, path: str) -> str:
    return f"{base.rstrip('/')}{path}"


def json_request(
    base: str,
    path: str,
    method: str = "GET",
    token: Optional[str] = None,
    payload: Optional[Dict[str, Any]] = None,
    timeout: float = 30.0,
) -> Tuple[int, Dict[str, Any]]:
    data = json.dumps(payload).encode() if payload is not None else None
    headers = {"Accept": "application/json", "User-Agent": "bluey-interview-eval/1.0"}
    if data is not None:
        headers["Content-Type"] = "application/json"
    if token:
        headers["Authorization"] = f"Bearer {token}"
    req = request.Request(api_url(base, path), data=data, headers=headers, method=method)
    with request.urlopen(req, timeout=timeout) as response:
        body = response.read()
        parsed = json.loads(body.decode("utf-8", "replace")) if body else {}
        return response.status, parsed


def load_test_account(path: Path, purpose_contains: str) -> Tuple[str, str]:
    data = json.loads(path.read_text())
    needle = purpose_contains.casefold()
    for account in data.get("accounts", []):
        purpose = str(account.get("purpose", "")).casefold()
        if needle in purpose:
            return str(account["email"]), str(account["password"])
    raise RuntimeError(f"No internal test account matched purpose: {purpose_contains}")


def extract_pdf(path: Path) -> str:
    try:
        from pypdf import PdfReader  # type: ignore
    except ImportError as exc:
        raise RuntimeError(
            "PDF extraction needs pypdf. Run with Bluey's bundled workspace Python."
        ) from exc
    reader = PdfReader(str(path))
    return "\n".join((page.extract_text() or "") for page in reader.pages)


def extract_docx(path: Path) -> str:
    with zipfile.ZipFile(path) as archive:
        xml = archive.read("word/document.xml").decode("utf-8", "replace")
    xml = re.sub(r"</w:p>", "\n", xml)
    xml = re.sub(r"</w:tr>", "\n", xml)
    return unescape(re.sub(r"<[^>]+>", "", xml))


def extract_document(path: Path) -> str:
    suffix = path.suffix.lower()
    if suffix == ".pdf":
        text = extract_pdf(path)
    elif suffix == ".docx":
        text = extract_docx(path)
    else:
        text = path.read_text(errors="replace")
    text = text.replace("\x00", "")
    for pattern, replacement in CONTACT_PATTERNS:
        text = pattern.sub(replacement, text)
    return re.sub(r"[ \t]+", " ", text).strip()


def compact(text: str, limit: int) -> str:
    value = text.strip()
    if len(value) <= limit:
        return value
    return value[:limit].rstrip() + "\n...[compacted for evaluation]"


def profile_context(
    profile: ProfileSpec, downloads: Path
) -> Tuple[List[str], List[Dict[str, Any]]]:
    sources: List[str] = []
    typed: List[Dict[str, Any]] = []
    for label, filename, limit, role, sensitivity in (
        ("Resume", profile.resume, 6000, "candidate_resume", "confidential"),
        (
            "Job description",
            profile.job_description,
            6000,
            "job_description",
            "internal",
        ),
        (
            "Interview preparation document",
            profile.extra_document,
            5000,
            "interview_preparation",
            "confidential",
        ),
    ):
        if not filename:
            continue
        path = downloads / filename
        if not path.is_file():
            raise FileNotFoundError(path)
        content = compact(extract_document(path), limit)
        sources.append(path.name)
        typed.append(
            {
                "kind": "document",
                "content": content,
                "title": label,
                "source": path.name,
                "sensitivity": sensitivity,
                "role": role,
            }
        )
    typed.append(
        {
            "kind": "user_note",
            "content": profile.role,
            "title": "Role target from evaluation",
            "source": "curated evaluation profile",
            "sensitivity": "internal",
            "role": "other",
        }
    )
    return sources, typed


def build_user_prompt(
    case: EvalCase,
    answer_context: Sequence[Dict[str, Any]],
) -> str:
    """Mirror Bluey's current flattened user envelope alongside typed context."""
    blocks = [
        f"[{item['title']} from {item['source']}]\n{item['content']}"
        for item in answer_context
    ]
    context = "\n\n".join(blocks)
    if len(context) > 32_000:
        context = context[:32_000].rstrip()
        context += "\n\n[older context compacted to stay within the active model window]"
    if not context:
        return case.question
    return f"Question:\n{case.question}\n\nSession context:\n{context}"


def build_typed_answer_context(
    case: EvalCase,
    profile_context: Sequence[Dict[str, Any]],
    prior_results: Dict[str, CaseResult],
) -> List[Dict[str, Any]]:
    """Build the exact v1 provenance envelope exercised by current clients."""
    contexts = [dict(item) for item in profile_context]
    # Deliberate truth-gap fixtures have story-shaped preparation material but
    # no matching user-confirmed STAR story. Do not let a generic resume bullet
    # create fabricate-or-fail pressure for these lived-story questions.
    if case.expected_outcome == "needs_user_input":
        contexts = [
            item for item in contexts if item.get("role") != "candidate_resume"
        ]
    if case.conversation:
        prior = [
            result
            for result in prior_results.values()
            if result.conversation == case.conversation and result.final_ok
        ]
        for result in prior[-2:]:
            attempt = result.attempts[-1]
            prior_text = attempt.visible_answer
            if attempt.artifact_body:
                prior_text += "\n\n[Previous workbench artifact]\n" + attempt.artifact_body
            contexts.append(
                {
                    "kind": "meeting_memory",
                    "content": (
                        f"Previous question: {result.question}\n"
                        f"Previous Bluey answer: {compact(prior_text, 9000)}"
                    ),
                    "title": "Retained conversation context",
                    "source": "prior evaluation turn",
                    "sensitivity": "internal",
                    "role": "other",
                }
            )
    return contexts


def validate_typed_answer_context(contexts: Sequence[Dict[str, Any]]) -> None:
    """Fail locally if the evaluator no longer matches Bluey's v1 envelope."""
    valid_kinds = {
        "transcript",
        "meeting_memory",
        "screenshot",
        "document",
        "user_note",
        "system",
        "other",
    }
    valid_roles = {
        "candidate_resume",
        "job_description",
        "interview_preparation",
        "user_confirmed_story",
        "other",
    }
    valid_sensitivity = {"public", "internal", "confidential", "restricted"}
    if len(contexts) > 64:
        raise ValueError("typed context exceeds Bluey's 64-item limit")
    combined_bytes = 0
    for index, item in enumerate(contexts):
        if item.get("kind") not in valid_kinds:
            raise ValueError(f"context[{index}] has invalid kind")
        if item.get("role") not in valid_roles:
            raise ValueError(f"context[{index}] has invalid role")
        if item.get("sensitivity") not in valid_sensitivity:
            raise ValueError(f"context[{index}] has invalid sensitivity")
        content = item.get("content")
        title = item.get("title")
        source = item.get("source")
        if not isinstance(content, str) or not content.strip():
            raise ValueError(f"context[{index}] has empty content")
        if not isinstance(title, str) or not isinstance(source, str):
            raise ValueError(f"context[{index}] has invalid provenance")
        content_bytes = len(content.encode())
        title_bytes = len(title.encode())
        source_bytes = len(source.encode())
        if content_bytes > 32 * 1024:
            raise ValueError(f"context[{index}] content exceeds 32 KiB")
        if title_bytes > 1024:
            raise ValueError(f"context[{index}] title exceeds 1 KiB")
        if source_bytes > 4096:
            raise ValueError(f"context[{index}] source exceeds 4 KiB")
        combined_bytes += content_bytes + title_bytes + source_bytes
    if combined_bytes > 256 * 1024:
        raise ValueError("typed context exceeds Bluey's 256 KiB combined limit")


def context_provenance_manifest(
    contexts: Sequence[Dict[str, Any]], aggregate_sha256: str
) -> Dict[str, Any]:
    """Record exact roles and hashes without duplicating private source text."""
    return {
        "context_schema_version": 1,
        "aggregate_sha256": aggregate_sha256,
        "items": [
            {
                "kind": item["kind"],
                "role": item["role"],
                "sensitivity": item["sensitivity"],
                "title": item["title"],
                "source": item["source"],
                "content_bytes": len(item["content"].encode()),
                "content_sha256": hashlib.sha256(item["content"].encode()).hexdigest(),
            }
            for item in contexts
        ],
    }


def self_check_typed_answer_context() -> None:
    truth_gap_cases = [
        case for case in CASES if case.expected_outcome == "needs_user_input"
    ]
    fixture = [
        {
            "kind": "document",
            "content": "verified resume facts",
            "title": "Resume",
            "source": "resume.pdf",
            "sensitivity": "confidential",
            "role": "candidate_resume",
        },
        {
            "kind": "document",
            "content": "style guidance with an incomplete example",
            "title": "Interview preparation document",
            "source": "prep.docx",
            "sensitivity": "confidential",
            "role": "interview_preparation",
        },
    ]
    for case in truth_gap_cases:
        truth_gap_context = build_typed_answer_context(case, fixture, {})
        validate_typed_answer_context(truth_gap_context)
        assert all(item["role"] != "candidate_resume" for item in truth_gap_context)
        assert any(
            item["role"] == "interview_preparation" for item in truth_gap_context
        )
        assert all(
            item["role"] != "user_confirmed_story" for item in truth_gap_context
        )


def iter_sse(response: Any) -> Iterable[Tuple[str, str]]:
    event_name = "message"
    data_lines: List[str] = []
    while True:
        raw = response.readline()
        if raw == b"":
            break
        line = raw.decode("utf-8", "replace").rstrip("\r\n")
        if not line:
            if data_lines or event_name != "message":
                yield event_name, "\n".join(data_lines)
            event_name, data_lines = "message", []
            continue
        if line.startswith(":"):
            continue
        field_name, sep, value = line.partition(":")
        if not sep:
            continue
        value = value[1:] if value.startswith(" ") else value
        if field_name == "event":
            event_name = value or "message"
        elif field_name == "data":
            data_lines.append(value)
    if data_lines or event_name != "message":
        yield event_name, "\n".join(data_lines)


def delta_text(data: str) -> str:
    try:
        value = json.loads(data)
    except json.JSONDecodeError:
        return ""
    if not isinstance(value, dict):
        return ""
    choices = value.get("choices")
    if isinstance(choices, list):
        chunks: List[str] = []
        for choice in choices:
            if not isinstance(choice, dict):
                continue
            delta = choice.get("delta")
            if isinstance(delta, dict) and isinstance(delta.get("content"), str):
                chunks.append(delta["content"])
            elif isinstance(choice.get("text"), str):
                chunks.append(choice["text"])
        return "".join(chunks)
    for key in ("delta", "content"):
        if isinstance(value.get(key), str):
            return value[key]
    return ""


def parse_error_payload(data: str) -> Tuple[str, Optional[str], Optional[str]]:
    try:
        value = json.loads(data)
    except json.JSONDecodeError:
        return data[:500], None, None
    if not isinstance(value, dict):
        return str(value)[:500], None, None
    message = str(value.get("error") or value.get("message") or value.get("reason") or "stream error")
    reason = value.get("reason")
    ref = value.get("request_ref") or value.get("ref") or value.get("trace_ref")
    return message[:500], str(reason) if reason else None, str(ref) if ref else None


def finalize_attempt_answers(result: AttemptResult, streamed_chunks: Sequence[str]) -> None:
    """Preserve both SSE text surfaces and score exactly what the customer saw."""
    result.streamed_answer = "".join(streamed_chunks)
    result.visible_answer = result.streamed_answer or result.terminal_answer


def validate_billing_payload(data: str) -> Tuple[Optional[Dict[str, Any]], Optional[str]]:
    """Validate the terminal CompleteResponse before it can prove billing success."""
    try:
        value = json.loads(data)
    except json.JSONDecodeError:
        return None, "malformed_billing_event:invalid_json"
    if not isinstance(value, dict):
        return None, "malformed_billing_event:not_an_object"

    for key in ("text", "provider", "model"):
        field_value = value.get(key)
        if not isinstance(field_value, str) or not field_value.strip():
            return None, f"malformed_billing_event:invalid_{key}"

    integer_fields = (
        "input_tokens",
        "output_tokens",
        "cost_cents",
        "balance_cents_after",
        "trial_seconds_remaining",
    )
    for key in integer_fields:
        field_value = value.get(key)
        if (
            not isinstance(field_value, int)
            or isinstance(field_value, bool)
            or field_value < 0
        ):
            return None, f"malformed_billing_event:invalid_{key}"

    for key in ("artifact_type", "artifact_body"):
        if key in value and value[key] is not None and not isinstance(value[key], str):
            return None, f"malformed_billing_event:invalid_{key}"
    if "sources" in value and (
        not isinstance(value["sources"], list)
        or not all(isinstance(source, dict) for source in value["sources"])
    ):
        return None, "malformed_billing_event:invalid_sources"
    return value, None


def record_billing_event(result: AttemptResult, data: str) -> None:
    """Accept exactly one schema-valid billing event without overwriting evidence."""
    result.billing_event_count += 1
    if result.billing_event_count > 1:
        result.billing_received = False
        result.billing_error = "duplicate_billing_event"
        return

    value, validation_error = validate_billing_payload(data)
    if value is None:
        result.billing_received = False
        result.billing_error = validation_error or "malformed_billing_event"
        return

    result.terminal_answer = value["text"]
    result.artifact_type = value.get("artifact_type")
    result.artifact_body = value.get("artifact_body")
    result.provider = value["provider"]
    result.model = value["model"]
    result.input_tokens = value["input_tokens"]
    result.output_tokens = value["output_tokens"]
    result.cost_cents = value["cost_cents"]
    result.balance_cents_after = value["balance_cents_after"]
    result.trial_seconds_remaining = value["trial_seconds_remaining"]
    if isinstance(value.get("sources"), list):
        result.sources = value["sources"]
    result.billing_error = None
    result.billing_received = True


def self_check_billing_event_validation() -> None:
    valid = {
        "text": "A complete terminal answer.",
        "provider": "test-provider",
        "model": "test-model",
        "input_tokens": 12,
        "output_tokens": 7,
        "cost_cents": 1,
        "balance_cents_after": 499,
        "trial_seconds_remaining": 0,
    }
    accepted = AttemptResult(attempt=1)
    record_billing_event(accepted, json.dumps(valid))
    assert accepted.billing_received
    assert accepted.billing_event_count == 1
    assert accepted.billing_error is None
    assert accepted.terminal_answer == valid["text"]

    malformed_payloads = (
        "not-json",
        json.dumps({**valid, "provider": ""}),
        json.dumps({**valid, "input_tokens": "12"}),
        json.dumps({key: value for key, value in valid.items() if key != "balance_cents_after"}),
        json.dumps({**valid, "text": "   "}),
    )
    for payload in malformed_payloads:
        rejected = AttemptResult(attempt=1)
        record_billing_event(rejected, payload)
        assert not rejected.billing_received, payload
        assert rejected.billing_error and rejected.billing_error.startswith(
            "malformed_billing_event:"
        )
        assert not rejected.terminal_answer

    duplicate = AttemptResult(attempt=1)
    record_billing_event(duplicate, json.dumps(valid))
    record_billing_event(
        duplicate,
        json.dumps({**valid, "text": "A conflicting second terminal answer."}),
    )
    assert duplicate.billing_event_count == 2
    assert not duplicate.billing_received
    assert duplicate.billing_error == "duplicate_billing_event"
    assert duplicate.terminal_answer == valid["text"]


def run_attempt(
    base: str,
    token: str,
    case: EvalCase,
    user_prompt: str,
    answer_context: Sequence[Dict[str, Any]],
    session_id: str,
    attempt_number: int,
    timeout: float,
) -> AttemptResult:
    request_id = f"interview-eval-{case.id.lower()}-{uuid.uuid4()}"
    payload = {
        "request_id": request_id,
        "session_id": session_id,
        "system": BASE_SYSTEM,
        "user": user_prompt,
        "lane": "balanced",
        "max_tokens": case.max_tokens,
        "temperature": 0.35,
        "context_schema_version": 1,
        "context": list(answer_context),
    }
    result = AttemptResult(attempt=attempt_number, request_id=request_id, session_id=session_id)
    encoded = json.dumps(payload).encode()
    req = request.Request(
        api_url(base, "/router/complete/stream"),
        data=encoded,
        headers={
            "Accept": "text/event-stream",
            "Content-Type": "application/json",
            "Authorization": f"Bearer {token}",
            "User-Agent": "bluey-interview-eval/1.0",
        },
        method="POST",
    )
    started = time.perf_counter()
    streamed_text: List[str] = []
    try:
        with request.urlopen(req, timeout=timeout) as response:
            result.status = response.status
            for event_name, data in iter_sse(response):
                elapsed_ms = (time.perf_counter() - started) * 1000
                result.event_count += 1
                if result.first_event_ms is None:
                    result.first_event_ms = elapsed_ms
                if data.strip() == "[DONE]":
                    result.done = True
                    break
                if event_name == "status":
                    try:
                        value = json.loads(data)
                        if isinstance(value, dict):
                            result.statuses.append(value)
                    except json.JSONDecodeError:
                        result.statuses.append({"message": data[:500]})
                    continue
                if event_name == "sources":
                    try:
                        value = json.loads(data)
                        if isinstance(value, dict) and isinstance(value.get("sources"), list):
                            result.sources.extend(value["sources"])
                    except json.JSONDecodeError:
                        pass
                    continue
                if event_name == "error":
                    result.error, result.error_reason, result.error_ref = parse_error_payload(data)
                    continue
                if event_name == "billing":
                    record_billing_event(result, data)
                    continue
                chunk = delta_text(data)
                if chunk:
                    if result.first_token_ms is None:
                        result.first_token_ms = elapsed_ms
                    result.delta_count += 1
                    streamed_text.append(chunk)
    except error.HTTPError as exc:
        result.status = exc.code
        body = exc.read(8192).decode("utf-8", "replace")
        result.error, result.error_reason, result.error_ref = parse_error_payload(body)
    except Exception as exc:  # network and stream failures must be recorded
        result.error = f"{type(exc).__name__}: {exc}"[:500]
    result.total_ms = (time.perf_counter() - started) * 1000
    finalize_attempt_answers(result, streamed_text)
    result.ok = bool(
        result.status is not None
        and 200 <= result.status < 300
        and result.done
        and result.billing_received
        and not result.billing_error
        and result.visible_answer.strip()
        and not result.error
    )
    return result


def has_first_person(text: str) -> bool:
    return bool(re.search(r"\b(?:I|I'm|I've|I'd|my|me)\b", text, re.I))


def answer_evidence_text(attempt: AttemptResult) -> str:
    """Return all customer-visible answer material used by quality gates."""
    return (attempt.visible_answer + "\n" + (attempt.artifact_body or "")).strip()


def stream_terminal_integrity_issues(attempt: AttemptResult) -> List[str]:
    """Report when persisted terminal text differs from what the customer saw."""
    streamed = attempt.streamed_answer.replace("\r\n", "\n").replace("\r", "\n").strip()
    terminal = attempt.terminal_answer.replace("\r\n", "\n").replace("\r", "\n").strip()
    visible = attempt.visible_answer.replace("\r\n", "\n").replace("\r", "\n").strip()
    issues: List[str] = []
    if streamed and visible != streamed:
        issues.append("streamed_answer_not_preserved")
    if (
        attempt.billing_received
        and streamed != terminal
        and attempt.artifact_type != "code"
    ):
        issues.append("stream_terminal_answer_mismatch")
    return issues


def stream_terminal_audit_issues(attempt: AttemptResult) -> List[str]:
    """Retain intentional code-shape divergence as nonblocking audit evidence."""
    streamed = attempt.streamed_answer.replace("\r\n", "\n").replace("\r", "\n").strip()
    terminal = attempt.terminal_answer.replace("\r\n", "\n").replace("\r", "\n").strip()
    if (
        attempt.billing_received
        and attempt.artifact_type == "code"
        and streamed != terminal
    ):
        return ["code_stream_terminal_shape_mismatch"]
    return []


def looks_structurally_incomplete(text: str) -> bool:
    """Detect strong static evidence that an answer stopped mid-structure."""
    stripped = text.rstrip()
    if not stripped:
        return True
    if stripped.count("```") % 2:
        return True

    last_line = next(
        (line.strip() for line in reversed(stripped.splitlines()) if line.strip()),
        "",
    )
    if re.fullmatch(r"#{1,6}\s+.+", last_line):
        return True
    if stripped[-1] in ":,;/\\([{":
        return True
    # At an exact output cap, prose ending on a bare word is materially
    # different from a completed sentence or a closed code/data structure.
    if stripped[-1].isalnum():
        return True
    return bool(
        re.search(
            r"\b(?:and|or|but|because|including|such as|for example|the|a|an|to|with)\s*$",
            stripped,
            re.I,
        )
    )




Q46_GENERIC_PLACEHOLDER = re.compile(
    r"\[(?:company(?:/project)?(?:\s+and\s+what\s+happened)?|project|situation|"
    r"task|what\s+you\s+were\s+responsible\s+for|(?:2-3\s+)?actions?|"
    r"2-3\s+actions\s+you\s+personally\s+took|result|(?:verified\s+)?outcome|"
    r"user-confirmed\s+qualitative\s+or\s+quantitative\s+outcome|problem|constraint)\]",
    re.I,
)


def q46_non_placeholder_prose(text: str) -> str:
    """Remove only recognized fill-in grammar while retaining every factual claim."""
    prose = text
    placeholder = Q46_GENERIC_PLACEHOLDER.pattern
    safe_template_clauses = (
        rf"\b(?:i\s+was|we\s+were)\s+responsible\s+for\s*{placeholder}",
        rf"\b(?:my|our)\s+(?:task|responsibility|goal)\s+was\s*{placeholder}",
        rf"\b(?:the\s+)?result\s+was\s*{placeholder}",
        rf"\b(?:i|we)\s*{placeholder}",
    )
    for pattern in safe_template_clauses:
        prose = re.sub(pattern, " ", prose, flags=re.I)
    prose = Q46_GENERIC_PLACEHOLDER.sub(" ", prose)
    return re.sub(
        r"\s+",
        " ",
        re.sub(r"[*_`~]+", "", prose.casefold().replace("’", "'")),
    ).strip()


def has_q46_unsupported_lived_claim(text: str) -> bool:
    """Detect first-person lived claims using grammar plus irregular actions."""
    lower = q46_non_placeholder_prose(text)
    regular_past = r"[a-z][a-z-]{2,}ed"
    irregular_past = (
        r"built|brought|chose|chosen|cut|did|done|drove|driven|found|grew|grown|"
        r"had|kept|led|made|overcame|overseen|oversaw|put|ran|run|rose|seen|set|"
        r"shown|sold|spent|taught|taken|took|went|won|written|wrote"
    )
    past_action = rf"(?:{regular_past}|{irregular_past})"
    first_person_action = re.search(
        rf"\b(?:i|we)\s+(?:(?:personally|directly|successfully)\s+)?"
        rf"(?:(?:have|had)\s+(?:(?:personally|directly)\s+)?{past_action}|"
        rf"{past_action})\b",
        lower,
    )
    present_story_action = (
        r"own|lead|manage|coordinate|build|run|operate|maintain|support|fix|restore|"
        r"improve|reduce|increase|drive|deliver|design|develop|implement|launch|ship|"
        r"create|resolve|handle|oversee|optimize|stabilize|automate|migrate|scale|"
        r"mentor|partner|champion|spearhead|debug|deploy|architect"
    )
    first_person_present_action = re.search(
        rf"\b(?:i|we)\s+(?:(?:personally|directly|currently|regularly|successfully)\s+)?"
        rf"(?:{present_story_action})(?:s|es|ing)?\b",
        lower,
    )
    concrete_locative_identity = re.search(
        r"\b(?:at|for|within|during)\s+"
        r"(?:(?:a|the)\s+(?:previous|former|current)\s+(?:company|team|project)|"
        r"[a-z0-9&][a-z0-9&.'-]*(?:\s+[a-z0-9&][a-z0-9&.'-]*){0,4})"
        r"\s*,\s*(?:i|we)\b",
        lower,
    )
    first_person_role = re.search(
        r"\b(?:i\s+(?:was|have\s+been|had\s+been)|"
        r"we\s+(?:were|have\s+been|had\s+been))\s+"
        r"(?:(?:personally|directly|solely|only)\s+)?"
        r"(?:responsible|accountable|the\s+owner|in\s+charge)\b",
        lower,
    )
    possessive_story = re.search(
        r"\b(?:my|our)\s+(?:role|task|responsibility|goal|project|initiative)\s+"
        r"(?:was|became|included|involved)\b(?!.{0,20}\[[^]]+\])",
        lower,
    )
    return bool(
        first_person_action
        or first_person_present_action
        or concrete_locative_identity
        or first_person_role
        or possessive_story
    )


def has_q46_unsupported_passive_story_claim(text: str) -> bool:
    """Detect passive or outcome-shaped factual stories without enumerating verbs."""
    lower = q46_non_placeholder_prose(text)
    regular_participle = r"[a-z][a-z-]{2,}ed"
    irregular_participle = (
        r"built|brought|cut|done|driven|found|grown|kept|made|overseen|put|"
        r"rebuilt|redone|rewritten|run|set|shown|taken|won|written"
    )
    story_participle = rf"(?:{regular_participle}|{irregular_participle})"
    story_subject = (
        r"(?:(?:my|our|the)\s+team|(?:the\s+)?(?:engineers?|pipeline|system|"
        r"service|process|delivery|project|initiative|awards?|customer\s+impact|"
        r"costs?|latency|errors?|incident|outcome|result))"
    )
    return bool(
        re.search(
            rf"\b{story_subject}\s+(?:was|were)\s+(?:successfully\s+)?"
            rf"{story_participle}\b",
            lower,
        )
        or re.search(
            rf"\b{story_subject}\s+(?:fell|grew|rose|dropped|ran|became|"
            rf"{story_participle})\b",
            lower,
        )
        or re.search(
            r"\b(?:result|outcome|impact)\s+(?:was|included|became)\s+"
            r"(?:an?\s+|the\s+)?[a-z0-9]",
            lower,
        )
        or re.search(r"\bas a result,?\s+[a-z0-9]", lower)
    )


def has_q46_concrete_filled_template_claim(text: str) -> bool:
    """Reject bracketed facts masquerading as generic template placeholders."""
    if not re.search(r"\b(?:fill[- ]in\s+)?template\b", text, re.I):
        return False
    bracketed = re.findall(r"\[[^\]\n]{2,100}\]", text)
    concrete = [value for value in bracketed if not Q46_GENERIC_PLACEHOLDER.fullmatch(value)]
    return len(concrete) >= 2


def is_q46_truth_gap_or_fill_in_template(text: str) -> bool:
    """Accept a useful facts request or unmistakably non-factual STAR template."""
    lower = re.sub(
        r"\s+",
        " ",
        re.sub(r"[*_`~]+", "", text.casefold().replace("’", "'")),
    )
    has_all_star_fields = all(
        re.search(rf"\b{field}\b", lower)
        for field in ("situation", "task", "action", "result")
    )
    truth_gap = bool(
        re.search(
            r"\b(?:do\s+not|don't|cannot|can't)\s+have\b.{0,120}"
            r"\b(?:verified|confirmed|factual|real|complete|end[- ]to[- ]end|"
            r"story|example|facts?|details?)\b",
            lower,
        )
        or re.search(
            r"\b(?:need|missing|send|provide|share)\b.{0,120}"
            r"\b(?:verified|confirmed|factual|real|facts?|details?|situation|"
            r"task|action|result)\b",
            lower,
        )
    )
    placeholders = Q46_GENERIC_PLACEHOLDER.findall(text)
    template_match = re.search(r"\b(?:fill[- ]in\s+)?template\b", lower)
    labeled_template = bool(template_match and len(placeholders) >= 3)
    if not has_all_star_fields or not (truth_gap or labeled_template):
        return False
    # Scan all remaining prose, including text after the template. Only the
    # recognized placeholders and their fill-in grammar are exempt.
    return not (
        has_q46_unsupported_lived_claim(text)
        or has_q46_unsupported_passive_story_claim(text)
        or has_q46_concrete_filled_template_claim(text)
    )


def q46_story_grounding_issues(text: str) -> List[str]:
    """Enforce the Q46 fixture's cross-source, incomplete-story truth boundary."""
    safe_fallback = is_q46_truth_gap_or_fill_in_template(text)
    lived_claim = has_q46_unsupported_lived_claim(text)
    passive_claim = has_q46_unsupported_passive_story_claim(text)
    filled_template_claim = has_q46_concrete_filled_template_claim(text)
    prose = q46_non_placeholder_prose(text)
    foreign_story_anchor = bool(
        re.search(
            r"\b(?:marriott|loyalty|free\s+night\s+awards?|fnas?)\b",
            prose,
            re.I,
        )
    )

    issues: List[str] = []
    if not safe_fallback:
        issues.append("q46_missing_verified_story_truth_gap_or_template")
    if lived_claim and not safe_fallback:
        issues.append("q46_unsupported_first_person_story")
    if passive_claim and not safe_fallback:
        issues.append("q46_unsupported_passive_story")
    if filled_template_claim and not safe_fallback:
        issues.append("q46_unsupported_filled_template_story")
    if (
        lived_claim or passive_claim or filled_template_claim
    ) and foreign_story_anchor and not safe_fallback:
        issues.append("q46_cross_source_identity_story")
    return issues


def self_check_q46_story_grounding_detector() -> None:
    safe = (
        "I don't have one verified end-to-end story I can safely put in your voice "
        "yet. Send these four facts: Situation (company/project and problem), Task "
        "(what you personally owned), Action (two or three actions you took), and "
        "Result (a verified outcome; qualitative is fine). Fill-in template (not a "
        "factual answer): At [company/project], [situation]. I was responsible for "
        "[task]. I [actions]. As a result, [verified outcome].",
        "Verified story gap: I need your factual Situation, Task, Action, and Result. "
        "Fill-in template, replace the brackets: At [company], [situation]. My task "
        "was [task]. I [action]. The result was [outcome].",
    )
    unsafe = (
        (
            "At Marriott, I was only responsible for part of the loyalty ETL flow, "
            "but I coordinated across teams and restored Free Night Awards.",
            {
                "q46_missing_verified_story_truth_gap_or_template",
                "q46_unsupported_first_person_story",
                "q46_cross_source_identity_story",
            },
        ),
        (
            "At a previous company, I led an incident response and improved the "
            "customer outcome.",
            {
                "q46_missing_verified_story_truth_gap_or_template",
                "q46_unsupported_first_person_story",
            },
        ),
        (
            "At a previous company, I championed a reliability initiative and the "
            "service stabilized afterward.",
            {
                "q46_missing_verified_story_truth_gap_or_template",
                "q46_unsupported_first_person_story",
            },
        ),
        (
            "At a previous company, I ran the recovery program and cut customer "
            "latency in half.",
            {
                "q46_missing_verified_story_truth_gap_or_template",
                "q46_unsupported_first_person_story",
            },
        ),
        (
            "At a previous company, I have overseen the recovery program and the "
            "service was rebuilt for reliability.",
            {
                "q46_missing_verified_story_truth_gap_or_template",
                "q46_unsupported_first_person_story",
                "q46_unsupported_passive_story",
            },
        ),
        (
            "My role was to coordinate the incident. The platform was hardened and "
            "the customer impact was eliminated.",
            {
                "q46_missing_verified_story_truth_gap_or_template",
                "q46_unsupported_first_person_story",
                "q46_unsupported_passive_story",
            },
        ),
        (
            "I would frame the answer around ownership and customer impact.",
            {"q46_missing_verified_story_truth_gap_or_template"},
        ),
        (
            "Fill-in template: At Marriott, I worked on loyalty data and fixed the "
            "FNA issue.",
            {
                "q46_missing_verified_story_truth_gap_or_template",
                "q46_unsupported_first_person_story",
                "q46_cross_source_identity_story",
            },
        ),
        (
            "At Marriott, I led the loyalty fix and restored FNAs. Fill-in template: "
            "At [company], [situation]. My task was [task]. I [action]. The result "
            "was [outcome].",
            {
                "q46_missing_verified_story_truth_gap_or_template",
                "q46_unsupported_first_person_story",
                "q46_cross_source_identity_story",
            },
        ),
        (
            "Fill-in template: At [company], [situation]. My task was [task]. I "
            "[action]. The result was [outcome]. At Marriott, the loyalty issue was "
            "resolved and Free Night Awards were restored.",
            {
                "q46_missing_verified_story_truth_gap_or_template",
                "q46_unsupported_passive_story",
                "q46_cross_source_identity_story",
            },
        ),
        (
            "Fill-in template: At [Marriott], [loyalty issue]. My task was [restore "
            "FNAs]. I [fixed the pipeline]. The result was [97 percent restored].",
            {
                "q46_missing_verified_story_truth_gap_or_template",
                "q46_unsupported_filled_template_story",
                "q46_cross_source_identity_story",
            },
        ),
        (
            "Fill-in template: At [company], [situation]. My task was [task]. I "
            "[action]. The result was [outcome]. At Marriott, I fixed the loyalty "
            "pipeline and restored FNAs.",
            {
                "q46_missing_verified_story_truth_gap_or_template",
                "q46_unsupported_first_person_story",
                "q46_cross_source_identity_story",
            },
        ),
    )
    assert all(not q46_story_grounding_issues(text) for text in safe)
    for text, expected in unsafe:
        issues = set(q46_story_grounding_issues(text))
        assert expected.issubset(issues), (text, issues)




def self_check_production_answer_contracts() -> None:
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
    q38_case = next(case for case in CASES if case.id == "Q38")
    assert not missing_required_group_issues(q38_case, saved_round539_q38_mechanics)
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


def missing_required_group_issues(case: EvalCase, text: str) -> List[str]:
    return [
        "missing_signal:" + "|".join(group)
        for group in case.required_groups
        if not any(has_required_signal(text, term) for term in group)
    ]




def has_complete_code_artifact_body(body: str) -> bool:
    stripped = body.strip()
    if len(stripped) < 80:
        return False
    python_shape = bool(
        re.search(
            r"(?:^|\n)\s*(?:class\s+[A-Za-z_]\w*(?:\([^\n)]*\))?\s*:|"
            r"(?:async\s+)?def\s+[A-Za-z_]\w*\s*\([^\n)]*\)\s*:)",
            stripped,
        )
    )
    fenced_code = stripped.count("```") >= 2 and python_shape
    return fenced_code or python_shape


def python_source_from_code_artifact(body: str) -> str:
    """Extract Python/untagged fenced blocks without executing them."""
    normalized = body.replace("\r\n", "\n").replace("\r", "\n").strip()
    # Bluey's persisted code artifact is a sectioned workbench document. Isolate
    # CODE before scanning fences so an example inside NOTES cannot replace it.
    sectioned = re.match(r"(?is)^CODE\s*\n-+\s*\n(.*)$", normalized)
    if sectioned:
        normalized = re.split(
            r"(?m)^\s*(?:LINE NOTES|COMPLEXITY|NOTES)\s*\n-+\s*$",
            sectioned.group(1),
            maxsplit=1,
        )[0].strip()
    blocks = re.findall(
        r"```[ \t]*([^\n`]*)\n(.*?)```",
        normalized,
        re.S,
    )
    candidates = [
        code.strip()
        for language, code in blocks
        if language.strip().casefold() in ("", "py", "python", "python3")
        and code.strip()
    ]
    if candidates:
        return "\n\n".join(candidates)
    return normalized


def _attribute_tokens(node: ast.AST) -> set[str]:
    tokens: set[str] = set()
    for candidate in ast.walk(node):
        if isinstance(candidate, ast.Attribute):
            tokens.add(candidate.attr.casefold())
        elif isinstance(candidate, ast.Name):
            tokens.add(candidate.id.casefold())
    return tokens


def _reachable_class_methods(
    start: ast.AST,
    methods: Dict[str, ast.AST],
) -> List[ast.AST]:
    """Follow self.method() calls so helpers count only when the answer uses them."""
    reachable: List[ast.AST] = []
    pending = [start]
    visited: set[str] = set()
    while pending:
        node = pending.pop()
        name = getattr(node, "name", "")
        if name in visited:
            continue
        visited.add(name)
        reachable.append(node)
        for candidate in ast.walk(node):
            if not isinstance(candidate, ast.Call) or not isinstance(
                candidate.func, ast.Attribute
            ):
                continue
            owner = candidate.func.value
            if isinstance(owner, ast.Name) and owner.id == "self":
                called = methods.get(candidate.func.attr)
                if called is not None and candidate.func.attr not in visited:
                    pending.append(called)
    return reachable


def _mutated_link_attributes(nodes: Sequence[ast.AST]) -> set[str]:
    mutated: set[str] = set()

    def collect(target: ast.AST) -> None:
        if isinstance(target, ast.Attribute):
            if target.attr.casefold() in {"prev", "next", "head", "tail"}:
                mutated.add(target.attr.casefold())
        elif isinstance(target, (ast.Tuple, ast.List)):
            for item in target.elts:
                collect(item)

    for node in nodes:
        for candidate in ast.walk(node):
            if isinstance(candidate, (ast.Assign, ast.AnnAssign, ast.AugAssign)):
                targets = (
                    candidate.targets
                    if isinstance(candidate, ast.Assign)
                    else [candidate.target]
                )
                for target in targets:
                    collect(target)
    return mutated


def _initialized_lock_attributes(class_node: ast.ClassDef) -> set[str]:
    initialized: set[str] = set()

    def is_lock_call(value: ast.AST) -> bool:
        if not isinstance(value, ast.Call):
            return False
        function = value.func
        if isinstance(function, ast.Attribute):
            name = function.attr
        elif isinstance(function, ast.Name):
            name = function.id
        else:
            return False
        return name.casefold() in {"lock", "rlock"}

    for candidate in ast.walk(class_node):
        if isinstance(candidate, ast.Assign):
            targets = candidate.targets
            value = candidate.value
        elif isinstance(candidate, ast.AnnAssign):
            targets = [candidate.target]
            value = candidate.value
        else:
            continue
        if value is None or not is_lock_call(value):
            continue
        for target in targets:
            if (
                isinstance(target, ast.Attribute)
                and isinstance(target.value, ast.Name)
                and target.value.id == "self"
            ):
                initialized.add(target.attr)
            elif isinstance(target, ast.Name):
                initialized.add(target.id)
    return initialized


def _used_lock_attributes(nodes: Sequence[ast.AST]) -> set[str]:
    used: set[str] = set()
    for node in nodes:
        for candidate in ast.walk(node):
            if isinstance(candidate, (ast.With, ast.AsyncWith)):
                for item in candidate.items:
                    expression = item.context_expr
                    if (
                        isinstance(expression, ast.Attribute)
                        and isinstance(expression.value, ast.Name)
                        and expression.value.id == "self"
                    ):
                        used.add(expression.attr)
            if not isinstance(candidate, ast.Call) or not isinstance(
                candidate.func, ast.Attribute
            ):
                continue
            if candidate.func.attr not in {"acquire", "release"}:
                continue
            lock = candidate.func.value
            if (
                isinstance(lock, ast.Attribute)
                and isinstance(lock.value, ast.Name)
                and lock.value.id == "self"
            ):
                used.add(lock.attr)
    return used


def lru_code_semantic_issues(case: EvalCase, body: str) -> List[str]:
    """Validate runnable LRU behavior and Q09's lock in the returned Python AST."""
    source = python_source_from_code_artifact(body)
    try:
        tree = ast.parse(source)
        compile(tree, "<bluey-code-artifact>", "exec")
    except (SyntaxError, ValueError, TypeError, MemoryError):
        return ["invalid_python_code_artifact"]

    lru_class = next(
        (
            node
            for node in ast.walk(tree)
            if isinstance(node, ast.ClassDef)
            and re.sub(r"[^a-z0-9]", "", node.name.casefold()) == "lrucache"
        ),
        None,
    )
    if lru_class is None:
        return ["missing_lru_cache_class"]

    method_types = (ast.FunctionDef, ast.AsyncFunctionDef)
    methods: Dict[str, ast.AST] = {
        node.name: node for node in lru_class.body if isinstance(node, method_types)
    }
    get_method = methods.get("get")
    put_method = methods.get("put")
    if get_method is None or put_method is None:
        return ["missing_lru_get_or_put_implementation"]

    issues: List[str] = []
    class_tokens = _attribute_tokens(tree)
    has_linked_recency = (
        {"prev", "next"}.issubset(class_tokens)
        and bool({"head", "tail"} & class_tokens)
        and not bool({"ordereddict", "functools.lru_cache"} & class_tokens)
    )
    if not has_linked_recency:
        issues.append("missing_lru_linked_recency_structure")

    get_scope = _reachable_class_methods(get_method, methods)
    put_scope = _reachable_class_methods(put_method, methods)
    get_mutations = _mutated_link_attributes(get_scope)
    if not get_mutations:
        issues.append("missing_lru_recency_update_in_get")

    put_tokens = set().union(*(_attribute_tokens(node) for node in put_scope))
    has_capacity_guard = any(
        isinstance(candidate, (ast.If, ast.While))
        and "capacity" in _attribute_tokens(candidate.test)
        and bool({"len", "size", "cache", "nodes", "map"} & _attribute_tokens(candidate.test))
        for node in put_scope
        for candidate in ast.walk(node)
    )
    has_destructive_eviction = any(
        isinstance(candidate, ast.Delete)
        or (
            isinstance(candidate, ast.Call)
            and isinstance(candidate.func, ast.Attribute)
            and candidate.func.attr.casefold() in {"pop", "popitem"}
        )
        or (
            isinstance(candidate, ast.Call)
            and isinstance(candidate.func, ast.Attribute)
            and isinstance(candidate.func.value, ast.Name)
            and candidate.func.value.id == "self"
            and "evict" in candidate.func.attr.casefold()
        )
        for node in put_scope
        for candidate in ast.walk(node)
    )
    has_lru_eviction_target = bool({"head", "tail", "prev", "next", "lru", "least"} & put_tokens)
    if not (has_capacity_guard and has_destructive_eviction and has_lru_eviction_target):
        issues.append("missing_lru_capacity_eviction")

    if case.id == "Q09":
        initialized_locks = _initialized_lock_attributes(lru_class)
        get_locks = _used_lock_attributes(get_scope) & initialized_locks
        put_locks = _used_lock_attributes(put_scope) & initialized_locks
        if not initialized_locks:
            issues.append("missing_shared_lock_initialization")
        if not get_locks:
            issues.append("missing_lock_usage_in_get")
        if not put_locks:
            issues.append("missing_lock_usage_in_put")
        if get_locks and put_locks and not (get_locks & put_locks):
            issues.append("get_and_put_use_different_locks")
    return issues


def code_complexity_issues(text: str) -> List[str]:
    lower = text.casefold()
    has_time = "time complexity" in lower or bool(
        re.search(r"(?:^|\n)\s*(?:[-*]\s*)?(?:\*\*)?time(?:\*\*)?\s*:", text, re.I)
    ) or bool(re.search(r"\bO\([^\n)]*\)\s+time\b", text, re.I))
    has_space = "space complexity" in lower or bool(
        re.search(r"(?:^|\n)\s*(?:[-*]\s*)?(?:\*\*)?space(?:\*\*)?\s*:", text, re.I)
    )
    complexity_section = bool(
        re.search(r"(?:^|\n)\s*(?:#{1,6}\s*)?(?:\*\*)?complexity(?:\*\*)?\s*$", text, re.I | re.M)
    )
    operation_bounds = all(
        re.search(
            rf"`?\b{operation}\s*\([^\n)]*\)`?\s*:\s*O\([^\n)]+\)",
            text,
            re.I,
        )
        for operation in ("get", "put")
    )
    has_time = has_time or (complexity_section and operation_bounds)
    return [] if has_time and has_space else ["missing_complexity"]


def has_valid_needs_story_facts_artifact(body: Optional[str]) -> bool:
    if not body:
        return False
    try:
        value = json.loads(body)
    except json.JSONDecodeError:
        return False
    if not isinstance(value, dict) or value.get("state") != "needs_story_facts":
        return False
    expected = {"Situation", "Task", "Action", "Result"}
    all_fields = value.get("all_fields")
    required_fields = value.get("required_fields")
    return bool(
        isinstance(all_fields, list)
        and len(all_fields) == 4
        and all(isinstance(field, str) for field in all_fields)
        and len(set(all_fields)) == 4
        and set(all_fields) == expected
        and isinstance(required_fields, list)
        and 1 <= len(required_fields) <= 4
        and all(isinstance(field, str) for field in required_fields)
        and set(required_fields).issubset(expected)
        and len(required_fields) == len(set(required_fields))
    )


def is_safe_needs_user_input_outcome(case: EvalCase, attempt: AttemptResult) -> bool:
    """Recognize a configured truth-gap intervention as the safe customer outcome."""
    if (
        case.expected_outcome != "needs_user_input"
        or attempt.artifact_type != "needs_story_facts"
    ):
        return False
    return bool(
        has_valid_needs_story_facts_artifact(attempt.artifact_body)
        and is_q46_truth_gap_or_fill_in_template(attempt.visible_answer)
        and not q46_story_grounding_issues(attempt.visible_answer)
    )


def mandatory_answer_shape_issues(case: EvalCase, attempt: AttemptResult) -> List[str]:
    """Return evidence and artifact defects that must block final success."""
    if is_safe_needs_user_input_outcome(case, attempt):
        return []

    combined = answer_evidence_text(attempt)
    issues = missing_required_group_issues(case, combined)
    if case.expect_code:
        if attempt.artifact_type != "code":
            issues.append("missing_code_artifact")
        elif not has_complete_code_artifact_body(attempt.artifact_body or ""):
            issues.append("incomplete_code_body")
        else:
            if case.id in ("Q08", "Q09"):
                issues.extend(lru_code_semantic_issues(case, attempt.artifact_body or ""))
            artifact_group_hits = sum(
                1
                for group in case.required_groups
                if any(
                    has_required_signal(attempt.artifact_body or "", term)
                    for term in group
                )
            )
            if artifact_group_hits < min(3, len(case.required_groups)):
                issues.append("code_artifact_not_grounded")
        issues.extend(code_complexity_issues(combined))
        visible_group_hits = sum(
            1
            for group in case.required_groups
            if any(has_required_signal(attempt.visible_answer, term) for term in group)
        )
        visible_words = len(re.findall(r"\b[\w'’+-]+\b", attempt.visible_answer))
        visible_has_code = bool(
            re.search(
                r"(?:^|\n)\s*(?:```|class\s+\w+|def\s+\w+)",
                attempt.visible_answer,
            )
        )
        if visible_words < 12 or (visible_group_hits < 2 and not visible_has_code):
            issues.append("code_visible_answer_not_grounded")
    if case.expect_design:
        if attempt.artifact_type not in ("system_design", "diagram"):
            issues.append("missing_design_artifact")
        else:
            artifact_body = (attempt.artifact_body or "").strip()
            if len(artifact_body) < 40 or len(re.findall(r"\b\w+\b", artifact_body)) < 6:
                issues.append("incomplete_design_artifact")
            else:
                artifact_group_hits = sum(
                    1
                    for group in case.required_groups
                    if any(has_required_signal(artifact_body, term) for term in group)
                )
                if artifact_group_hits < min(2, len(case.required_groups)):
                    issues.append("design_artifact_not_grounded")
    return issues


def blocking_answer_issues(case: EvalCase, attempt: AttemptResult) -> List[str]:
    """Return deterministic defects that prevent an answer from being success."""
    issues = stream_terminal_integrity_issues(attempt)
    if attempt.billing_error:
        issues.append(attempt.billing_error)
    if attempt.artifact_type == "needs_story_facts":
        issues.append("needs_user_input")
    if not attempt.ok:
        return issues
    combined = answer_evidence_text(attempt)
    safe_needs_user_input = is_safe_needs_user_input_outcome(case, attempt)
    word_count = len(re.findall(r"\b[\w'’+-]+\b", combined))
    if word_count < MIN_SUBSTANTIVE_ANSWER_WORDS and not safe_needs_user_input:
        issues.append("answer_too_short")

    exact_cap = attempt.output_tokens is not None and (
        attempt.output_tokens == case.max_tokens
        or attempt.output_tokens in KNOWN_OUTPUT_TOKEN_CAPS
    )
    if exact_cap and looks_structurally_incomplete(combined):
        issues.append("visibly_truncated_at_token_cap")

    issues.extend(mandatory_answer_shape_issues(case, attempt))

    if case.id == "Q10" and has_mysql_not_valid_portability_claim(combined):
        issues.append("unsafe_mysql_not_valid_portability_claim")
    if case.id == "Q10":
        issues.extend(large_fk_migration_safety_issues(combined))
    if case.id == "Q27":
        issues.extend(rag_evaluation_plan_issues(combined))
    if case.id == "Q39" and has_exactly_once_processing_overclaim(combined):
        issues.append("unsafe_exactly_once_processing_claim")
    if case.id == "Q39":
        issues.extend(payment_platform_safety_issues(combined))
        issues.extend(
            payment_operation_semantic_issues(
                combined,
                require_webhook_event_dedup=True,
            )
        )
        core_idempotency_issues = {
            "unsafe_new_idempotency_key_on_retry",
            "unsafe_missing_idempotency_key_on_retry",
            "unsafe_constant_idempotency_key",
            "unsafe_shared_idempotency_key_across_payment_operations",
            "missing_stable_idempotency_key_per_operation",
            "missing_distinct_authorize_capture_refund_keys",
            "missing_same_operation_idempotency_key_reuse",
        }
        for surface_name, surface_text in (
            ("spoken", attempt.visible_answer),
            ("canvas", attempt.artifact_body or ""),
        ):
            for surface_issue in payment_operation_semantic_issues(
                surface_text,
                require_webhook_event_dedup=False,
            ):
                if surface_issue in core_idempotency_issues:
                    issues.append(f"q39_{surface_name}_{surface_issue}")
    if case.id == "Q40":
        if has_unsafe_ambiguous_payment_outcome(combined):
            issues.append("unsafe_ambiguous_payment_retry_or_terminal_failure")
        issues.extend(
            payment_operation_semantic_issues(
                combined,
                require_webhook_event_dedup=False,
                require_complete_idempotency_semantics=False,
                require_same_operation_retry_reuse=True,
            )
        )
        issues.extend(payment_timeout_followup_completeness_issues(combined))
    if case.id == "Q41":
        issues.extend(
            url_shortener_safety_issues(
                combined,
                require_revocation_completeness=True,
            )
        )
    if case.id == "Q46":
        q46_visible = attempt.visible_answer
        issues.extend(q46_story_grounding_issues(q46_visible))
        safe_truth_gap = is_q46_truth_gap_or_fill_in_template(q46_visible)
        if safe_truth_gap and attempt.artifact_type != "needs_story_facts":
            issues.append("q46_truth_gap_missing_needs_story_facts_artifact")
        if attempt.artifact_type == "needs_story_facts" and not (
            safe_truth_gap and has_valid_needs_story_facts_artifact(attempt.artifact_body)
        ):
            issues.append("q46_invalid_needs_story_facts_artifact")
    if case.id == "Q47":
        issues.extend(q47_director_alignment_issues(combined))
    if case.id in ("Q29", "Q38") and has_drift_only_automatic_retraining(combined):
        issues.append("unsafe_drift_only_automatic_retraining")
    if case.id == "Q38":
        issues.extend(feature_store_consistency_issues(combined))
    return issues


def answer_is_success(case: EvalCase, attempt: AttemptResult) -> bool:
    return attempt.ok and not blocking_answer_issues(case, attempt)


def expected_outcome_is_accepted(case: EvalCase, attempt: AttemptResult) -> bool:
    """Judge the configured customer outcome without relabeling interventions."""
    if case.expected_outcome == "answer":
        return answer_is_success(case, attempt)
    if case.expected_outcome == "needs_user_input":
        return attempt.ok and is_safe_needs_user_input_outcome(case, attempt)
    raise ValueError(f"unsupported expected outcome for {case.id}: {case.expected_outcome}")


def self_check_attempt_integrity_guards() -> None:
    assert has_required_signal("def get(self, key):", "get")
    assert has_required_signal("point in time training-serving data", "point-in-time")
    assert not has_required_signal("We worked together on the output.", "get")
    assert not has_required_signal("The database stores rows.", "data")
    assert not has_required_signal("Identity is generated.", "id")
    q47 = next(case for case in CASES if case.id == "Q47")
    assert not missing_required_group_issues(
        q47,
        "I would compare customer impact and severity, escalate to leadership to "
        "decide if needed, communicate the tradeoff to both directors, and document "
        "the sequence.",
    )
    assert missing_required_group_issues(
        q47,
        "I would show leadership by selecting the request with the greatest customer "
        "impact, then communicate my tradeoff and final decision.",
    ) == [
        "missing_signal:align|stakeholder|tradeoff to both directors|visible to both directors|both directors together"
    ]
    assert not q47_director_alignment_issues(
        "I compare customer impact, explain the tradeoff to both directors, and "
        "ask them to align on the order before I communicate the decision."
    )
    adversarial_q47 = (
        "I compare customer impact, but I do not speak with both directors or seek "
        "their agreement. I communicate the tradeoff only to my team and make a "
        "unilateral decision."
    )
    assert "unsafe_negated_or_unilateral_director_alignment" in (
        q47_director_alignment_issues(adversarial_q47)
    )
    adversarial_attempt = AttemptResult(
        attempt=1,
        ok=True,
        visible_answer=adversarial_q47,
        streamed_answer=adversarial_q47,
        terminal_answer=adversarial_q47,
        billing_received=True,
    )
    assert not answer_is_success(q47, adversarial_attempt)
    for unsafe_director_answer in (
        "I compare customer impact and severity, but avoid explaining the tradeoff "
        "to both directors. I choose privately, then communicate my order to the team.",
        "I compare customer impact and explain the tradeoff to both directors, but "
        "ignore their input, choose privately, and communicate my final order.",
        "I ask both directors for input and show the tradeoff, then decide the priority "
        "myself and communicate my final order to both.",
    ):
        unsafe_attempt = AttemptResult(
            attempt=1,
            ok=True,
            visible_answer=unsafe_director_answer,
            streamed_answer=unsafe_director_answer,
            terminal_answer=unsafe_director_answer,
            billing_received=True,
        )
        assert not answer_is_success(q47, unsafe_attempt)
    for unsafe_alignment in (
        "I explain the same tradeoff to both directors, then personally make the priority call.",
        "I consult both directors and independently decide which request wins.",
        "I consult both directors but conceal each tradeoff from the other and choose the priority.",
    ):
        assert "unsafe_negated_or_unilateral_director_alignment" in (
            q47_director_alignment_issues(unsafe_alignment)
        ), unsafe_alignment
    for safe_alignment in (
        "I do not decide alone; I compare impact and align both directors on one shared priority.",
        "I avoid making a unilateral decision and instead ask both directors to agree "
        "on the tradeoff and order.",
        "I never choose privately; I explain the same tradeoff to both directors and "
        "seek one shared priority.",
        "Without making a unilateral call, I present the comparison to both directors "
        "and escalate to their common owner if they cannot agree.",
        "I email both directors the same tradeoff matrix and ask for one shared priority.",
        "I make one comparison visible to both directors using impact, urgency, effort, "
        "dependencies, and reversibility. Then I ask them to agree on the order; if "
        "they cannot, I escalate to their common accountable owner.",
    ):
        assert not q47_director_alignment_issues(safe_alignment), safe_alignment
    live_q47 = (
        "I make one comparison visible to both directors using the same criteria: "
        "impact, deadline urgency, effort, dependencies, and reversibility. Then I ask "
        "them to agree on the order or a shared rule. If they still disagree, I "
        "escalate the unresolved decision to their common accountable owner. I make "
        "the tradeoff explicit and communicate the final order."
    )
    assert not missing_required_group_issues(q47, live_q47)
    assert not q47_director_alignment_issues(live_q47)
    side_by_side_q47 = (
        "I go to both directors together and lay out a single, objective comparison "
        "of the two requests side by side using customer impact, deadline urgency, "
        "effort, dependencies, and reversibility. I ask them to agree on the order. "
        "If they cannot agree, I escalate the decision to their common accountable "
        "owner and notify both directors."
    )
    assert not missing_required_group_issues(q47, side_by_side_q47)
    assert not q47_director_alignment_issues(side_by_side_q47)
    unsafe_visible_comparison = (
        "I make one comparison visible to both directors, but do not ask them to agree; "
        "I choose the priority myself and communicate my tradeoff."
    )
    unsafe_visible_issues = set(q47_director_alignment_issues(unsafe_visible_comparison))
    assert "missing_affirmative_director_alignment" in unsafe_visible_issues
    assert "unsafe_negated_or_unilateral_director_alignment" in unsafe_visible_issues
    unsafe_accuracy_agreement = (
        "I make one comparison visible to both directors and ask them to agree it is "
        "accurate. I then pick the request to do first based on my personal preference "
        "and communicate it."
    )
    unsafe_accuracy_issues = set(q47_director_alignment_issues(unsafe_accuracy_agreement))
    assert "missing_affirmative_director_alignment" in unsafe_accuracy_issues
    assert "unsafe_negated_or_unilateral_director_alignment" in unsafe_accuracy_issues
    for unsafe_post_alignment in (
        "I make one comparison visible to both directors, then ask them to agree on "
        "the order. After listening, I decide based on personal preference.",
        "I make one comparison visible to both directors and ask them to align on the "
        "priority. If they cannot agree, I decide which one wins.",
        "I make one comparison visible to both directors using customer impact, then "
        "ask them to agree on the priority. If they cannot agree, I resolve the conflict "
        "myself. I communicate the tradeoff.",
    ):
        assert "unsafe_negated_or_unilateral_director_alignment" in (
            q47_director_alignment_issues(unsafe_post_alignment)
        ), unsafe_post_alignment

    streamed = "This is the complete customer-streamed answer with enough words to evaluate."
    matching = AttemptResult(
        attempt=1,
        ok=True,
        visible_answer=streamed,
        streamed_answer=streamed,
        terminal_answer=streamed,
        billing_received=True,
    )
    assert not stream_terminal_integrity_issues(matching)

    preserved = AttemptResult(
        attempt=1,
        terminal_answer="Billing tried to replace the answer.",
        billing_received=True,
    )
    finalize_attempt_answers(preserved, ["Customer ", "streamed answer."])
    assert preserved.streamed_answer == "Customer streamed answer."
    assert preserved.visible_answer == "Customer streamed answer."
    assert preserved.terminal_answer == "Billing tried to replace the answer."

    mismatched = AttemptResult(
        attempt=1,
        ok=True,
        visible_answer="The terminal answer incorrectly replaced what the customer saw.",
        streamed_answer="Unsafe text that the customer actually saw while streaming.",
        terminal_answer="Different sanitized terminal text persisted by billing.",
        billing_received=True,
    )
    mismatch_issues = set(stream_terminal_integrity_issues(mismatched))
    assert mismatch_issues == {
        "streamed_answer_not_preserved",
        "stream_terminal_answer_mismatch",
    }

    code_shape = AttemptResult(
        attempt=1,
        ok=True,
        visible_answer="Streamed explanation plus complete code.",
        streamed_answer="Streamed explanation plus complete code.",
        terminal_answer="Streamed explanation only.",
        billing_received=True,
        artifact_type="code",
    )
    assert not stream_terminal_integrity_issues(code_shape)
    assert stream_terminal_audit_issues(code_shape) == [
        "code_stream_terminal_shape_mismatch"
    ]
    q05 = next(case for case in CASES if case.id == "Q05")
    assert "stream_terminal_answer_mismatch" not in blocking_answer_issues(
        q05, code_shape
    )

    canvas_shape = AttemptResult(
        attempt=1,
        ok=True,
        visible_answer="Streamed spoken answer plus canvas detail.",
        streamed_answer="Streamed spoken answer plus canvas detail.",
        terminal_answer="Streamed spoken answer only.",
        billing_received=True,
        artifact_type="system_design",
    )
    assert stream_terminal_integrity_issues(canvas_shape) == [
        "stream_terminal_answer_mismatch"
    ]
    assert "stream_terminal_answer_mismatch" in blocking_answer_issues(
        q05, canvas_shape
    )

    needs_facts_text = (
        "I don't have one verified story yet. Send Situation, Task, Action, and Result. "
        "Fill-in template: At [company], [situation]. My task was [task]. I [action]. "
        "The result was [outcome]."
    )
    story_facts_artifact = json.dumps(
        {
            "state": "needs_story_facts",
            "required_fields": ["Situation", "Task", "Action", "Result"],
            "all_fields": ["Situation", "Task", "Action", "Result"],
        }
    )
    assert has_valid_needs_story_facts_artifact(story_facts_artifact)
    malformed_story_artifacts = (
        '{"state":"needs_story_facts","all_fields":[{}],"required_fields":["Task"]}',
        '{"state":"needs_story_facts","all_fields":["Situation","Task","Action","Action"],"required_fields":["Task"]}',
        '{"state":"needs_story_facts","all_fields":["Situation","Task","Action","Result"],"required_fields":[]}',
        '{"state":"needs_story_facts","all_fields":["Situation","Task","Action","Result"],"required_fields":[{}]}',
    )
    assert all(
        not has_valid_needs_story_facts_artifact(body)
        for body in malformed_story_artifacts
    )
    needs_facts = AttemptResult(
        attempt=1,
        ok=True,
        visible_answer=needs_facts_text,
        streamed_answer=needs_facts_text,
        terminal_answer=needs_facts_text,
        billing_received=True,
        artifact_type="needs_story_facts",
        artifact_body=story_facts_artifact,
    )
    truth_gap_cases = [
        case for case in CASES if case.expected_outcome == "needs_user_input"
    ]
    q46 = next(case for case in truth_gap_cases if case.id == "Q46")
    for case in truth_gap_cases:
        assert blocking_answer_issues(case, needs_facts) == ["needs_user_input"]
        assert is_safe_needs_user_input_outcome(case, needs_facts)
        assert not answer_is_success(case, needs_facts)
        assert expected_outcome_is_accepted(case, needs_facts)
        _, _, _, safe_accuracy, safe_issues = quality_scores(case, needs_facts)
        assert safe_accuracy == 25
        assert "answer_quality_gate_failed" not in safe_issues
        assert "not_first_person_speakable" not in safe_issues

    unlabeled_needs_facts = AttemptResult(
        attempt=1,
        ok=True,
        visible_answer=needs_facts_text,
        streamed_answer=needs_facts_text,
        terminal_answer=needs_facts_text,
        billing_received=True,
    )
    assert "q46_truth_gap_missing_needs_story_facts_artifact" in blocking_answer_issues(
        q46, unlabeled_needs_facts
    )
    assert not is_safe_needs_user_input_outcome(q46, unlabeled_needs_facts)
    assert not answer_is_success(q46, unlabeled_needs_facts)

    fabricated_story = (
        "At a previous company, I championed the recovery and cut latency in half. "
        "The customer impact was eliminated after the platform was hardened."
    )
    mislabeled_fabrication = AttemptResult(
        attempt=1,
        ok=True,
        visible_answer=fabricated_story,
        streamed_answer=fabricated_story,
        terminal_answer=fabricated_story,
        billing_received=True,
        artifact_type="needs_story_facts",
        artifact_body=story_facts_artifact,
    )
    fabricated_issues = set(blocking_answer_issues(q46, mislabeled_fabrication))
    assert "q46_unsupported_first_person_story" in fabricated_issues
    assert "q46_invalid_needs_story_facts_artifact" in fabricated_issues
    assert not is_safe_needs_user_input_outcome(q46, mislabeled_fabrication)

    appended_present_fabrication_text = (
        needs_facts_text
        + " At Marriott, I own the loyalty pipeline, coordinate incident response, "
        "restore Free Night Awards, and improve customer outcomes every quarter."
    )
    appended_present_fabrication = AttemptResult(
        attempt=1,
        ok=True,
        visible_answer=appended_present_fabrication_text,
        streamed_answer=appended_present_fabrication_text,
        terminal_answer=appended_present_fabrication_text,
        billing_received=True,
        artifact_type="needs_story_facts",
        artifact_body=story_facts_artifact,
    )
    appended_issues = set(blocking_answer_issues(q46, appended_present_fabrication))
    assert "q46_unsupported_first_person_story" in appended_issues
    assert "q46_invalid_needs_story_facts_artifact" in appended_issues
    assert not is_safe_needs_user_input_outcome(q46, appended_present_fabrication)

    q08 = next(case for case in CASES if case.id == "Q08")
    code_visible = (
        "The LRUCache class keeps get and put operations constant time with a hash "
        "map and linked ordering. Time complexity is O(1) per operation, while space "
        "complexity is O(capacity). The complete implementation is in the code artifact."
    )
    code_body = (
        "```python\n"
        "class Node:\n"
        "    def __init__(self, key, value):\n"
        "        self.key = key\n"
        "        self.value = value\n"
        "        self.prev = None\n"
        "        self.next = None\n\n"
        "class LRUCache:\n"
        "    def __init__(self, capacity):\n"
        "        self.capacity = capacity\n"
        "        self.cache = {}\n"
        "        self.head = Node(None, None)\n"
        "        self.tail = Node(None, None)\n"
        "        self.head.next = self.tail\n"
        "        self.tail.prev = self.head\n\n"
        "    def _remove(self, node):\n"
        "        node.prev.next = node.next\n"
        "        node.next.prev = node.prev\n\n"
        "    def _add_recent(self, node):\n"
        "        node.prev = self.head\n"
        "        node.next = self.head.next\n"
        "        self.head.next.prev = node\n"
        "        self.head.next = node\n\n"
        "    def get(self, key):\n"
        "        if key not in self.cache:\n"
        "            return -1\n"
        "        node = self.cache[key]\n"
        "        self._remove(node)\n"
        "        self._add_recent(node)\n"
        "        return node.value\n\n"
        "    def put(self, key, value):\n"
        "        if key in self.cache:\n"
        "            self._remove(self.cache[key])\n"
        "        node = Node(key, value)\n"
        "        self.cache[key] = node\n"
        "        self._add_recent(node)\n"
        "        if len(self.cache) > self.capacity:\n"
        "            lru = self.tail.prev\n"
        "            self._remove(lru)\n"
        "            del self.cache[lru.key]\n"
        "```"
    )
    canonical_code_body = (
        "CODE\n----\nclass Example:\n    pass\n\n"
        "LINE NOTES\n----------\n1: class declaration\n\n"
        "COMPLEXITY\n----------\n- Time: O(1)\n\n"
        "NOTES\n-----\n```python\nprint('notes example')\n```\n"
    )
    assert python_source_from_code_artifact(canonical_code_body) == (
        "class Example:\n    pass"
    )
    ast.parse(python_source_from_code_artifact(canonical_code_body))
    valid_code = AttemptResult(
        attempt=1,
        ok=True,
        visible_answer=code_visible,
        streamed_answer=code_visible,
        terminal_answer="A shorter terminal code summary.",
        billing_received=True,
        artifact_type="code",
        artifact_body=code_body,
    )
    assert not mandatory_answer_shape_issues(q08, valid_code)
    assert not blocking_answer_issues(q08, valid_code)
    assert answer_is_success(q08, valid_code)
    _, _, _, _, code_quality_issues = quality_scores(q08, valid_code)
    assert "code_stream_terminal_shape_mismatch" in code_quality_issues

    invalid_python_body = (
        "```python\n"
        "class LRUCache:\n"
        "    def get(self, key):\n"
        "        return -1\n"
        "    def put(self, key, value):\n"
        "        # Missing executable body must not pass the release gate.\n"
        "```"
    )
    invalid_python = AttemptResult(
        attempt=1,
        ok=True,
        visible_answer=code_visible,
        streamed_answer=code_visible,
        terminal_answer=code_visible,
        billing_received=True,
        artifact_type="code",
        artifact_body=invalid_python_body,
    )
    assert "invalid_python_code_artifact" in mandatory_answer_shape_issues(
        q08, invalid_python
    )
    assert not answer_is_success(q08, invalid_python)

    fake_lru_body = (
        "```python\n"
        "class LRUCache:\n"
        "    def __init__(self, capacity):\n"
        "        self.capacity, self.cache = capacity, {}\n"
        "    def get(self, key):\n"
        "        return self.cache.get(key, -1)\n"
        "    def put(self, key, value):\n"
        "        self.cache[key] = value\n"
        "        if len(self.cache) > self.capacity:\n"
        "            return  # Never evicts the least-recently-used entry.\n"
        "```"
    )
    fake_lru = AttemptResult(
        attempt=1,
        ok=True,
        visible_answer=code_visible,
        streamed_answer=code_visible,
        terminal_answer=code_visible,
        billing_received=True,
        artifact_type="code",
        artifact_body=fake_lru_body,
    )
    fake_lru_issues = set(mandatory_answer_shape_issues(q08, fake_lru))
    assert "missing_lru_linked_recency_structure" in fake_lru_issues
    assert "missing_lru_recency_update_in_get" in fake_lru_issues
    assert "missing_lru_capacity_eviction" in fake_lru_issues
    assert not answer_is_success(q08, fake_lru)

    q09 = next(case for case in CASES if case.id == "Q09")
    q09_visible = (
        "The complete LRUCache keeps get and put at O(1) and uses one shared RLock "
        "around both operations. Time complexity is O(1) per call and space "
        "complexity is O(capacity). The artifact contains the complete updated code."
    )
    unlocked_q09 = AttemptResult(
        attempt=1,
        ok=True,
        visible_answer=q09_visible,
        streamed_answer=q09_visible,
        terminal_answer=q09_visible,
        billing_received=True,
        artifact_type="code",
        artifact_body=code_body,
    )
    unlocked_issues = set(mandatory_answer_shape_issues(q09, unlocked_q09))
    assert "missing_shared_lock_initialization" in unlocked_issues
    assert "missing_lock_usage_in_get" in unlocked_issues
    assert "missing_lock_usage_in_put" in unlocked_issues
    assert not answer_is_success(q09, unlocked_q09)

    locked_code_body = (
        "```python\n"
        "import threading\n\n"
        "class Node:\n"
        "    def __init__(self, key, value):\n"
        "        self.key, self.value = key, value\n"
        "        self.prev = self.next = None\n\n"
        "class LRUCache:\n"
        "    def __init__(self, capacity):\n"
        "        self.capacity = capacity\n"
        "        self.cache = {}\n"
        "        self._lock = threading.RLock()\n"
        "        self.head, self.tail = Node(None, None), Node(None, None)\n"
        "        self.head.next, self.tail.prev = self.tail, self.head\n\n"
        "    def _remove(self, node):\n"
        "        node.prev.next, node.next.prev = node.next, node.prev\n\n"
        "    def _add_recent(self, node):\n"
        "        node.prev, node.next = self.head, self.head.next\n"
        "        self.head.next.prev = node\n"
        "        self.head.next = node\n\n"
        "    def get(self, key):\n"
        "        with self._lock:\n"
        "            if key not in self.cache:\n"
        "                return -1\n"
        "            node = self.cache[key]\n"
        "            self._remove(node)\n"
        "            self._add_recent(node)\n"
        "            return node.value\n\n"
        "    def put(self, key, value):\n"
        "        with self._lock:\n"
        "            if key in self.cache:\n"
        "                self._remove(self.cache[key])\n"
        "            node = Node(key, value)\n"
        "            self.cache[key] = node\n"
        "            self._add_recent(node)\n"
        "            if len(self.cache) > self.capacity:\n"
        "                lru = self.tail.prev\n"
        "                self._remove(lru)\n"
        "                del self.cache[lru.key]\n"
        "```"
    )
    locked_q09 = AttemptResult(
        attempt=1,
        ok=True,
        visible_answer=q09_visible,
        streamed_answer=q09_visible,
        terminal_answer=q09_visible,
        billing_received=True,
        artifact_type="code",
        artifact_body=locked_code_body,
    )
    assert not mandatory_answer_shape_issues(q09, locked_q09)
    assert answer_is_success(q09, locked_q09)

    unrelated_code = AttemptResult(
        attempt=1,
        ok=True,
        visible_answer=code_visible,
        streamed_answer=code_visible,
        terminal_answer="A different terminal summary.",
        billing_received=True,
        artifact_type="code",
        artifact_body=(
            "```python\nclass Unrelated:\n    def alpha(self, value):\n"
            "        return value + 1\n    def beta(self, value):\n"
            "        return value * 2\n```"
        ),
    )
    assert "code_artifact_not_grounded" in mandatory_answer_shape_issues(
        q08, unrelated_code
    )
    assert not answer_is_success(q08, unrelated_code)

    for case_id, prose in (
        (
            "Q08",
            "The class LRUCache would expose get and put methods and use a dictionary "
            "plus a linked list. Time complexity is O(1), and space complexity is "
            "O(capacity), but this response intentionally contains no implementation.",
        ),
        (
            "Q09",
            "The class LRUCache would wrap get and put with an RLock to make access "
            "thread safe. Time complexity remains O(1), and space complexity remains "
            "O(capacity), but this response intentionally contains no updated code.",
        ),
    ):
        code_case = next(case for case in CASES if case.id == case_id)
        prose_only = AttemptResult(
            attempt=1,
            ok=True,
            visible_answer=prose,
            streamed_answer=prose,
            terminal_answer=prose,
            billing_received=True,
        )
        assert "missing_code_artifact" in blocking_answer_issues(code_case, prose_only)
        assert not answer_is_success(code_case, prose_only)

    q34 = next(case for case in CASES if case.id == "Q34")
    design_prose = (
        "Clients maintain WebSocket connections through a connection tier. Messages "
        "flow through a durable queue into database storage, with retry handling for "
        "failure recovery. This prose explains the messaging design in detail but "
        "intentionally supplies no diagram or system design artifact."
    )
    design_without_artifact = AttemptResult(
        attempt=1,
        ok=True,
        visible_answer=design_prose,
        streamed_answer=design_prose,
        terminal_answer=design_prose,
        billing_received=True,
    )
    assert "missing_design_artifact" in blocking_answer_issues(
        q34, design_without_artifact
    )
    assert not answer_is_success(q34, design_without_artifact)

    valid_design = AttemptResult(
        attempt=1,
        ok=True,
        visible_answer=design_prose.replace(
            "intentionally supplies no diagram or system design artifact",
            "is paired with the complete system design artifact",
        ),
        billing_received=True,
        artifact_type="system_design",
        artifact_body=(
            "Clients -> WebSocket connection tier -> message queue -> database storage\n"
            "Failure -> retry queue -> delivery worker -> client acknowledgment"
        ),
    )
    valid_design.streamed_answer = valid_design.visible_answer
    valid_design.terminal_answer = valid_design.visible_answer
    assert not mandatory_answer_shape_issues(q34, valid_design)
    assert answer_is_success(q34, valid_design)

    q39 = next(case for case in CASES if case.id == "Q39")
    incomplete_payment_contract = (
        "Persist the payment intent before calling the processor and record confirmed "
        "movements in a durable double-entry ledger. Use idempotency for retries. "
        "Reconcile UNKNOWN outcomes through provider status and deduplicate webhooks "
        "by provider event ID. Redis is only a cache, not the correctness boundary."
    )
    payment_attempt = AttemptResult(
        attempt=1,
        ok=True,
        visible_answer=incomplete_payment_contract,
        streamed_answer=incomplete_payment_contract,
        terminal_answer=incomplete_payment_contract,
        billing_received=True,
        artifact_type="system_design",
        artifact_body=(
            "Client -> durable payment intent -> transactional outbox -> provider\n"
            "Provider status and webhook -> reconciliation worker -> immutable ledger"
        ),
    )
    payment_issues = set(blocking_answer_issues(q39, payment_attempt))
    assert "missing_stable_idempotency_key_per_operation" in payment_issues
    assert "missing_distinct_authorize_capture_refund_keys" in payment_issues
    assert "missing_same_operation_idempotency_key_reuse" in payment_issues
    assert not answer_is_success(q39, payment_attempt)

    complete_payment_surface = (
        "Authorize, capture, and refund each use a distinct operation-scoped "
        "idempotency key. Retries of the same logical operation reuse its original "
        "stable key. Persist the payment intent and durable double-entry ledger before "
        "calling the provider. Deduplicate webhooks by provider event ID and reconcile "
        "UNKNOWN outcomes through authoritative provider status."
    )
    weak_payment_surface = (
        "Client requests flow through durable ledger storage to the provider, and "
        "provider webhooks feed a reconciliation worker for uncertain outcomes."
    )
    complete_both_surfaces = AttemptResult(
        attempt=1,
        ok=True,
        visible_answer=complete_payment_surface,
        streamed_answer=complete_payment_surface,
        terminal_answer=complete_payment_surface,
        billing_received=True,
        artifact_type="system_design",
        artifact_body=complete_payment_surface,
    )
    complete_surface_issues = set(
        blocking_answer_issues(q39, complete_both_surfaces)
    )
    assert not {
        issue for issue in complete_surface_issues if issue.startswith("q39_")
    }, complete_surface_issues
    spoken_omission = AttemptResult(
        attempt=1,
        ok=True,
        visible_answer=weak_payment_surface,
        streamed_answer=weak_payment_surface,
        terminal_answer=weak_payment_surface,
        billing_received=True,
        artifact_type="system_design",
        artifact_body=complete_payment_surface,
    )
    spoken_issues = set(blocking_answer_issues(q39, spoken_omission))
    assert "q39_spoken_missing_distinct_authorize_capture_refund_keys" in spoken_issues
    assert not any(issue.startswith("q39_canvas_missing_") for issue in spoken_issues)
    canvas_omission = AttemptResult(
        attempt=1,
        ok=True,
        visible_answer=complete_payment_surface,
        streamed_answer=complete_payment_surface,
        terminal_answer=complete_payment_surface,
        billing_received=True,
        artifact_type="system_design",
        artifact_body=weak_payment_surface,
    )
    canvas_issues = set(blocking_answer_issues(q39, canvas_omission))
    assert "q39_canvas_missing_distinct_authorize_capture_refund_keys" in canvas_issues
    assert not any(issue.startswith("q39_spoken_missing_") for issue in canvas_issues)

    q40 = next(case for case in CASES if case.id == "Q40")
    incomplete_timeout_contract = (
        "I would move the payment to UNKNOWN and PENDING_RECONCILIATION, stop the "
        "automatic charge retry, and query provider status. A provider webhook can "
        "confirm the terminal outcome. Idempotency protects payment commands, but "
        "this intentionally omits the exact operation-scoped key contract."
    )
    timeout_attempt = AttemptResult(
        attempt=1,
        ok=True,
        visible_answer=incomplete_timeout_contract,
        streamed_answer=incomplete_timeout_contract,
        terminal_answer=incomplete_timeout_contract,
        billing_received=True,
    )
    timeout_issues = set(blocking_answer_issues(q40, timeout_attempt))
    assert "missing_distinct_authorize_capture_refund_keys" not in timeout_issues
    assert "missing_stable_idempotency_key_per_operation" not in timeout_issues
    assert "missing_same_operation_idempotency_key_reuse" in timeout_issues
    assert not answer_is_success(q40, timeout_attempt)

    reconciled_new_purchase = (
        "Authorize, capture, and refund each use distinct operation-scoped "
        "idempotency keys, and replaying the same logical operation reuses its same "
        "stable key. A timeout leaves the original payment UNKNOWN in "
        "PENDING_RECONCILIATION and blocks that charge. Query provider status and "
        "deduplicate webhooks by provider event ID while reconciling the original "
        "payment. Authoritative provider evidence moves UNKNOWN to SUCCEEDED or "
        "FAILED. Only if reconciliation is inconclusive and the provider guarantees "
        "idempotency do we retry the same operation under a bounded policy with its "
        "original idempotency key. "
        "If still unresolved, keep it UNKNOWN and escalate to manual reconciliation. "
        "After the provider status lookup confirms it FAILED with no charge, the "
        "customer explicitly authorizes a distinct later purchase, so create a new "
        "payment intent with its own new operation-scoped key."
    )
    reconciled_attempt = AttemptResult(
        attempt=1,
        ok=True,
        visible_answer=reconciled_new_purchase,
        streamed_answer=reconciled_new_purchase,
        terminal_answer=reconciled_new_purchase,
        billing_received=True,
    )
    assert not blocking_answer_issues(q40, reconciled_attempt)
    assert answer_is_success(q40, reconciled_attempt)


def quality_scores(case: EvalCase, attempt: AttemptResult) -> Tuple[int, int, int, int, List[str]]:
    issues: List[str] = stream_terminal_audit_issues(attempt)
    reliability = 35 if attempt.ok else 0
    if not attempt.ok:
        issues.append("request_failed")
        if attempt.first_token_ms is not None or attempt.delta_count > 0:
            issues.append("request_failed_after_partial_stream")
    if attempt.first_token_ms is not None:
        if attempt.first_token_ms <= 2000:
            latency = 15
        elif attempt.first_token_ms <= 3500:
            latency = 12
        elif attempt.first_token_ms <= 5000:
            latency = 8
            issues.append("first_token_watch")
        elif attempt.first_token_ms <= 8000:
            latency = 4
            issues.append("first_token_slow")
        else:
            latency = 0
            issues.append("first_token_very_slow")
    else:
        latency = 0
        issues.append("no_first_token")

    combined = answer_evidence_text(attempt)
    lower = combined.casefold()
    visible_lower = attempt.visible_answer.strip().casefold()
    human = 25
    if any(visible_lower.startswith(opener) for opener in META_OPENERS):
        human -= 7
        issues.append("assistant_or_meta_opener")
    if (
        case.speakable
        and case.expected_outcome == "answer"
        and not has_first_person(attempt.visible_answer)
    ):
        human -= 7
        issues.append("not_first_person_speakable")
    if case.self_intro and not re.match(r"\s*(?:i['’]?m|my name is)\b", attempt.visible_answer, re.I):
        human -= 8
        issues.append("self_intro_wrong_opener")
    if "—" in attempt.visible_answer:
        human -= 3
        issues.append("em_dash_in_chat")
    if any(phrase in visible_lower for phrase in ERROR_PHRASES):
        human -= 5
        issues.append("error_copy_in_answer")
    if len(attempt.visible_answer) > 4500 and case.category not in ("system_design", "coding"):
        human -= 4
        issues.append("chat_too_long")
    human = max(0, human)
    if not attempt.ok:
        human = 0
        issues.append("incomplete_response_not_human_scored")

    accuracy = 25
    if case.expect_followup_context and any(phrase in lower for phrase in CONTEXT_LOSS_PHRASES):
        accuracy -= 8
        issues.append("followup_context_lost")
    blocking_issues = blocking_answer_issues(case, attempt)
    if (
        case.expected_outcome == "needs_user_input"
        and is_safe_needs_user_input_outcome(case, attempt)
    ):
        blocking_issues = [
            issue for issue in blocking_issues if issue != "needs_user_input"
        ]
    if not attempt.ok:
        accuracy = 0
        issues.append("answer_quality_gate_failed")
    elif blocking_issues:
        accuracy = 0
        issues.append("answer_quality_gate_failed")
        issues.extend(blocking_issues)
    accuracy = max(0, accuracy)
    return reliability, latency, human, accuracy, issues


def prior_attempt_audit_issues(attempts: Sequence[AttemptResult]) -> List[str]:
    """Surface failures hidden by a later successful retry."""
    prior_failures = [attempt for attempt in attempts[:-1] if not attempt.ok]
    if not prior_failures:
        return []
    issues = ["recovered_after_retry"]
    if any(
        attempt.first_token_ms is not None or attempt.delta_count > 0
        for attempt in prior_failures
    ):
        issues.append("request_failed_after_partial_stream")
    return issues


def self_check_failed_attempt_scoring() -> None:
    case = next(case for case in CASES if case.id == "Q41")
    partial = AttemptResult(
        attempt=1,
        ok=False,
        first_token_ms=100.0,
        delta_count=1,
        visible_answer="A fluent but incomplete partial response.",
        streamed_answer="A fluent but incomplete partial response.",
    )
    reliability, latency, human, accuracy, issues = quality_scores(case, partial)
    assert (reliability, latency, human, accuracy) == (0, 15, 0, 0)
    assert "request_failed_after_partial_stream" in issues
    assert "incomplete_response_not_human_scored" in issues

    recovered = AttemptResult(attempt=2, ok=True)
    assert prior_attempt_audit_issues([partial, recovered]) == [
        "recovered_after_retry",
        "request_failed_after_partial_stream",
    ]


def percentile(values: Sequence[float], percent: float) -> Optional[float]:
    if not values:
        return None
    ordered = sorted(values)
    index = (len(ordered) - 1) * percent
    low = int(index)
    high = min(low + 1, len(ordered) - 1)
    fraction = index - low
    return ordered[low] + (ordered[high] - ordered[low]) * fraction


def release_exit_code(
    *,
    results_run: int,
    selected_count: int,
    accepted_outcomes: int,
    first_attempt_accepted_outcomes: int,
    cases_with_retries: int,
    failed_partial_stream_attempts: int,
    first_attempt_latency_gate_passed: bool,
    minimum_reliability_percent: float,
) -> int:
    """Fail closed unless every case is fast and accepted on its first attempt."""
    if minimum_reliability_percent != 100.0:
        raise ValueError("the release reliability threshold is fixed at 100 percent")
    if results_run != selected_count or selected_count <= 0:
        return 2
    return (
        0
        if accepted_outcomes == selected_count
        and first_attempt_accepted_outcomes == selected_count
        and cases_with_retries == 0
        and failed_partial_stream_attempts == 0
        and first_attempt_latency_gate_passed
        else 2
    )


def self_check_release_exit_gate() -> None:
    assert release_exit_code(
        results_run=12,
        selected_count=12,
        accepted_outcomes=12,
        first_attempt_accepted_outcomes=12,
        cases_with_retries=0,
        failed_partial_stream_attempts=0,
        first_attempt_latency_gate_passed=True,
        minimum_reliability_percent=100.0,
    ) == 0
    assert release_exit_code(
        results_run=12,
        selected_count=12,
        accepted_outcomes=11,
        first_attempt_accepted_outcomes=11,
        cases_with_retries=0,
        failed_partial_stream_attempts=0,
        first_attempt_latency_gate_passed=True,
        minimum_reliability_percent=100.0,
    ) == 2
    assert release_exit_code(
        results_run=11,
        selected_count=12,
        accepted_outcomes=11,
        first_attempt_accepted_outcomes=11,
        cases_with_retries=0,
        failed_partial_stream_attempts=0,
        first_attempt_latency_gate_passed=True,
        minimum_reliability_percent=100.0,
    ) == 2
    for overrides in (
        {"first_attempt_accepted_outcomes": 11},
        {"cases_with_retries": 1},
        {"failed_partial_stream_attempts": 1},
        {"first_attempt_latency_gate_passed": False},
    ):
        gate = {
            "results_run": 12,
            "selected_count": 12,
            "accepted_outcomes": 12,
            "first_attempt_accepted_outcomes": 12,
            "cases_with_retries": 0,
            "failed_partial_stream_attempts": 0,
            "first_attempt_latency_gate_passed": True,
            "minimum_reliability_percent": 100.0,
        }
        gate.update(overrides)
        assert release_exit_code(**gate) == 2, overrides
    try:
        release_exit_code(
            results_run=12,
            selected_count=12,
            accepted_outcomes=11,
            first_attempt_accepted_outcomes=11,
            cases_with_retries=0,
            failed_partial_stream_attempts=0,
            first_attempt_latency_gate_passed=True,
            minimum_reliability_percent=90.0,
        )
    except ValueError as exc:
        assert "fixed at 100" in str(exc)
    else:
        raise AssertionError("release gate accepted a threshold below 100 percent")


def write_json(path: Path, value: Any) -> None:
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n")


def build_summary(
    results: Sequence[CaseResult],
    started_at: str,
    finished_at: str,
    account_before: Dict[str, Any],
    account_after: Dict[str, Any],
    max_answer_first_token_p95_ms: float = DEFAULT_MAX_ANSWER_FIRST_TOKEN_P95_MS,
    max_intervention_first_token_p95_ms: float = (
        DEFAULT_MAX_INTERVENTION_FIRST_TOKEN_P95_MS
    ),
) -> Dict[str, Any]:
    case_by_id = {case.id: case for case in CASES}
    answer_results = [
        result
        for result in results
        if case_by_id[result.id].expected_outcome == "answer"
    ]
    intervention_results = [
        result
        for result in results
        if case_by_id[result.id].expected_outcome == "needs_user_input"
    ]
    final_answers = [result for result in answer_results if result.final_ok]
    accepted = [result for result in results if result.accepted_outcome]
    needs_user_input = [
        result
        for result in results
        if result.id in case_by_id
        and is_safe_needs_user_input_outcome(
            case_by_id[result.id], result.attempts[-1]
        )
    ]
    first_tokens = [
        result.attempts[-1].first_token_ms
        for result in results
        if result.attempts[-1].first_token_ms is not None
    ]
    answer_first_tokens = [
        result.attempts[-1].first_token_ms
        for result in answer_results
        if result.attempts[-1].first_token_ms is not None
    ]
    intervention_first_tokens = [
        result.attempts[-1].first_token_ms
        for result in intervention_results
        if result.attempts[-1].first_token_ms is not None
    ]
    first_attempt_first_tokens = [
        result.attempts[0].first_token_ms
        for result in results
        if result.attempts[0].first_token_ms is not None
    ]
    first_attempt_answer_first_tokens = [
        result.attempts[0].first_token_ms
        for result in answer_results
        if result.attempts[0].first_token_ms is not None
    ]
    first_attempt_intervention_first_tokens = [
        result.attempts[0].first_token_ms
        for result in intervention_results
        if result.attempts[0].first_token_ms is not None
    ]
    first_attempt_totals = [result.attempts[0].total_ms for result in results]
    cumulative_case_elapsed = [result.cumulative_elapsed_ms for result in results]
    attempt_counts = [len(result.attempts) for result in results]
    first_attempt_answer_p95 = percentile(first_attempt_answer_first_tokens, 0.95)
    first_attempt_intervention_p95 = percentile(
        first_attempt_intervention_first_tokens, 0.95
    )
    all_first_attempt_tokens_measured = len(first_attempt_first_tokens) == len(results)
    answer_first_attempt_latency_passed = not answer_results or bool(
        len(first_attempt_answer_first_tokens) == len(answer_results)
        and first_attempt_answer_p95 is not None
        and first_attempt_answer_p95 <= max_answer_first_token_p95_ms
    )
    intervention_first_attempt_latency_passed = not intervention_results or bool(
        len(first_attempt_intervention_first_tokens) == len(intervention_results)
        and first_attempt_intervention_p95 is not None
        and first_attempt_intervention_p95 <= max_intervention_first_token_p95_ms
    )
    first_attempt_latency_gate_passed = bool(
        all_first_attempt_tokens_measured
        and answer_first_attempt_latency_passed
        and intervention_first_attempt_latency_passed
    )
    totals = [result.attempts[-1].total_ms for result in accepted]
    provider_counts: Dict[str, int] = {}
    model_counts: Dict[str, int] = {}
    issue_counts: Dict[str, int] = {}
    category_scores: Dict[str, List[int]] = {}
    total_cost = 0
    for result in results:
        attempt = result.attempts[-1]
        provider_counts[attempt.provider or "unknown"] = provider_counts.get(attempt.provider or "unknown", 0) + 1
        model_counts[attempt.model or "unknown"] = model_counts.get(attempt.model or "unknown", 0) + 1
        category_scores.setdefault(result.category, []).append(result.score)
        total_cost += sum(item.cost_cents for item in result.attempts)
        for issue in result.issues:
            issue_counts[issue] = issue_counts.get(issue, 0) + 1
    return {
        "started_at": started_at,
        "finished_at": finished_at,
        "questions_planned": len(results),
        "questions_run": len(results),
        "answer_cases": len(answer_results),
        "final_answer_successes": len(final_answers),
        "expected_intervention_cases": len(intervention_results),
        "successful_expected_interventions": sum(
            1 for result in intervention_results if result.accepted_outcome
        ),
        "accepted_outcomes": len(accepted),
        "needs_user_input_outcomes": len(needs_user_input),
        "first_attempt_answer_successes": sum(
            1 for result in answer_results if result.first_attempt_ok
        ),
        "first_attempt_accepted_outcomes": sum(
            1 for result in results if result.first_attempt_accepted
        ),
        "first_attempt_outcome_acceptance_percent": (
            round(
                100
                * sum(1 for result in results if result.first_attempt_accepted)
                / len(results),
                1,
            )
            if results
            else 0
        ),
        "cases_with_retries": sum(1 for result in results if len(result.attempts) > 1),
        "failed_partial_stream_attempts": sum(
            1
            for result in results
            for attempt in result.attempts
            if not attempt.ok
            and (attempt.first_token_ms is not None or attempt.delta_count > 0)
        ),
        "answer_reliability_percent": (
            round(100 * len(final_answers) / len(answer_results), 1)
            if answer_results
            else 100.0
        ),
        "first_attempt_answer_reliability_percent": (
            round(
                100
                * sum(1 for result in answer_results if result.first_attempt_ok)
                / len(answer_results),
                1,
            )
            if answer_results
            else 100.0
        ),
        "outcome_acceptance_percent": (
            round(100 * len(accepted) / len(results), 1) if results else 0
        ),
        "average_score": round(statistics.mean(result.score for result in results), 1) if results else 0,
        "first_token_ms": {
            "median": round(statistics.median(first_tokens), 1) if first_tokens else None,
            "p90": round(percentile(first_tokens, 0.9) or 0, 1) if first_tokens else None,
            "p95": round(percentile(first_tokens, 0.95) or 0, 1) if first_tokens else None,
            "max": round(max(first_tokens), 1) if first_tokens else None,
            "measured": len(first_tokens),
            "missing": len(results) - len(first_tokens),
            "total_cases": len(results),
        },
        "answer_first_token_ms": {
            "median": round(statistics.median(answer_first_tokens), 1) if answer_first_tokens else None,
            "p90": round(percentile(answer_first_tokens, 0.9) or 0, 1) if answer_first_tokens else None,
            "p95": round(percentile(answer_first_tokens, 0.95) or 0, 1) if answer_first_tokens else None,
            "max": round(max(answer_first_tokens), 1) if answer_first_tokens else None,
            "measured": len(answer_first_tokens),
            "missing": len(answer_results) - len(answer_first_tokens),
            "total_cases": len(answer_results),
        },
        "intervention_first_token_ms": {
            "median": round(statistics.median(intervention_first_tokens), 1) if intervention_first_tokens else None,
            "p95": round(percentile(intervention_first_tokens, 0.95) or 0, 1) if intervention_first_tokens else None,
            "max": round(max(intervention_first_tokens), 1) if intervention_first_tokens else None,
            "measured": len(intervention_first_tokens),
            "missing": len(intervention_results) - len(intervention_first_tokens),
            "total_cases": len(intervention_results),
        },
        "first_attempt_first_token_ms": {
            "median": (
                round(statistics.median(first_attempt_first_tokens), 1)
                if first_attempt_first_tokens
                else None
            ),
            "p90": (
                round(percentile(first_attempt_first_tokens, 0.9) or 0, 1)
                if first_attempt_first_tokens
                else None
            ),
            "p95": (
                round(percentile(first_attempt_first_tokens, 0.95) or 0, 1)
                if first_attempt_first_tokens
                else None
            ),
            "max": (
                round(max(first_attempt_first_tokens), 1)
                if first_attempt_first_tokens
                else None
            ),
            "measured": len(first_attempt_first_tokens),
            "missing": len(results) - len(first_attempt_first_tokens),
            "total_cases": len(results),
        },
        "first_attempt_answer_first_token_ms": {
            "median": (
                round(statistics.median(first_attempt_answer_first_tokens), 1)
                if first_attempt_answer_first_tokens
                else None
            ),
            "p90": (
                round(percentile(first_attempt_answer_first_tokens, 0.9) or 0, 1)
                if first_attempt_answer_first_tokens
                else None
            ),
            "p95": (
                round(first_attempt_answer_p95, 1)
                if first_attempt_answer_p95 is not None
                else None
            ),
            "max": (
                round(max(first_attempt_answer_first_tokens), 1)
                if first_attempt_answer_first_tokens
                else None
            ),
            "measured": len(first_attempt_answer_first_tokens),
            "missing": len(answer_results) - len(first_attempt_answer_first_tokens),
            "total_cases": len(answer_results),
        },
        "first_attempt_intervention_first_token_ms": {
            "median": (
                round(statistics.median(first_attempt_intervention_first_tokens), 1)
                if first_attempt_intervention_first_tokens
                else None
            ),
            "p95": (
                round(first_attempt_intervention_p95, 1)
                if first_attempt_intervention_p95 is not None
                else None
            ),
            "max": (
                round(max(first_attempt_intervention_first_tokens), 1)
                if first_attempt_intervention_first_tokens
                else None
            ),
            "measured": len(first_attempt_intervention_first_tokens),
            "missing": (
                len(intervention_results)
                - len(first_attempt_intervention_first_tokens)
            ),
            "total_cases": len(intervention_results),
        },
        "first_attempt_latency_gate": {
            "passed": first_attempt_latency_gate_passed,
            "all_first_tokens_measured": all_first_attempt_tokens_measured,
            "answer_p95_limit_ms": max_answer_first_token_p95_ms,
            "answer_p95_passed": answer_first_attempt_latency_passed,
            "intervention_p95_limit_ms": max_intervention_first_token_p95_ms,
            "intervention_p95_passed": intervention_first_attempt_latency_passed,
        },
        "first_attempt_total_ms": {
            "median": (
                round(statistics.median(first_attempt_totals), 1)
                if first_attempt_totals
                else None
            ),
            "p90": (
                round(percentile(first_attempt_totals, 0.9) or 0, 1)
                if first_attempt_totals
                else None
            ),
            "p95": (
                round(percentile(first_attempt_totals, 0.95) or 0, 1)
                if first_attempt_totals
                else None
            ),
            "max": round(max(first_attempt_totals), 1) if first_attempt_totals else None,
        },
        "cumulative_case_elapsed_ms": {
            "median": (
                round(statistics.median(cumulative_case_elapsed), 1)
                if cumulative_case_elapsed
                else None
            ),
            "p90": (
                round(percentile(cumulative_case_elapsed, 0.9) or 0, 1)
                if cumulative_case_elapsed
                else None
            ),
            "p95": (
                round(percentile(cumulative_case_elapsed, 0.95) or 0, 1)
                if cumulative_case_elapsed
                else None
            ),
            "max": (
                round(max(cumulative_case_elapsed), 1)
                if cumulative_case_elapsed
                else None
            ),
        },
        "attempt_count": {
            "median": (
                round(statistics.median(attempt_counts), 1) if attempt_counts else None
            ),
            "p95": (
                round(percentile(attempt_counts, 0.95) or 0, 1)
                if attempt_counts
                else None
            ),
            "max": max(attempt_counts) if attempt_counts else None,
            "total_attempts": sum(attempt_counts),
        },
        "total_ms": {
            "median": round(statistics.median(totals), 1) if totals else None,
            "p90": round(percentile(totals, 0.9) or 0, 1) if totals else None,
            "max": round(max(totals), 1) if totals else None,
        },
        "customer_cost_cents": total_cost,
        "balance_cents_before": account_before.get("balance_cents"),
        "balance_cents_after": account_after.get("balance_cents"),
        "trial_seconds_before": account_before.get("trial_seconds_remaining"),
        "trial_seconds_after": account_after.get("trial_seconds_remaining"),
        "provider_counts": dict(sorted(provider_counts.items())),
        "model_counts": dict(sorted(model_counts.items())),
        "category_average_scores": {
            key: round(statistics.mean(values), 1) for key, values in sorted(category_scores.items())
        },
        "issue_counts": dict(sorted(issue_counts.items(), key=lambda item: (-item[1], item[0]))),
    }


def render_report(summary: Dict[str, Any], results: Sequence[CaseResult], sources: Dict[str, List[str]]) -> str:
    ft = summary["first_token_ms"]
    answer_ft = summary["answer_first_token_ms"]
    intervention_ft = summary["intervention_first_token_ms"]
    first_attempt_ft = summary["first_attempt_first_token_ms"]
    first_attempt_answer_ft = summary["first_attempt_answer_first_token_ms"]
    first_attempt_intervention_ft = summary[
        "first_attempt_intervention_first_token_ms"
    ]
    latency_gate = summary["first_attempt_latency_gate"]
    cumulative = summary["cumulative_case_elapsed_ms"]
    attempt_count = summary["attempt_count"]
    lines = [
        "# Bluey 50-Question Interview Evaluation",
        "",
        "Raw answers and artifacts remain in this local eval directory.",
        "",
        "## Summary",
        "",
        f"- Accepted outcomes: {summary['accepted_outcomes']}/{summary['questions_run']} ({summary['outcome_acceptance_percent']}%)",
        f"- Normal answers: {summary['final_answer_successes']}/{summary['answer_cases']} ({summary['answer_reliability_percent']}%)",
        f"- Expected safe interventions: {summary['successful_expected_interventions']}/{summary['expected_intervention_cases']}",
        f"- First-attempt normal answers: {summary['first_attempt_answer_successes']}/{summary['answer_cases']} ({summary['first_attempt_answer_reliability_percent']}%)",
        f"- First-attempt accepted outcomes: {summary['first_attempt_accepted_outcomes']}/{summary['questions_run']} ({summary['first_attempt_outcome_acceptance_percent']}%)",
        f"- Cases requiring retries: {summary['cases_with_retries']}; failed partial-stream attempts: {summary['failed_partial_stream_attempts']}",
        f"- First-attempt latency gate: {'PASS' if latency_gate['passed'] else 'FAIL'}; all first tokens measured: {latency_gate['all_first_tokens_measured']}",
        f"- First-attempt token ({first_attempt_ft['measured']}/{first_attempt_ft['total_cases']} measured; {first_attempt_ft['missing']} missing): median {first_attempt_ft['median']} ms, p90 {first_attempt_ft['p90']} ms, p95 {first_attempt_ft['p95']} ms, max {first_attempt_ft['max']} ms",
        f"- First-attempt normal-answer token ({first_attempt_answer_ft['measured']}/{first_attempt_answer_ft['total_cases']} measured; limit {latency_gate['answer_p95_limit_ms']} ms): median {first_attempt_answer_ft['median']} ms, p90 {first_attempt_answer_ft['p90']} ms, p95 {first_attempt_answer_ft['p95']} ms, max {first_attempt_answer_ft['max']} ms",
        f"- First-attempt intervention token ({first_attempt_intervention_ft['measured']}/{first_attempt_intervention_ft['total_cases']} measured; limit {latency_gate['intervention_p95_limit_ms']} ms): median {first_attempt_intervention_ft['median']} ms, p95 {first_attempt_intervention_ft['p95']} ms, max {first_attempt_intervention_ft['max']} ms",
        f"- Cumulative case elapsed: median {cumulative['median']} ms, p90 {cumulative['p90']} ms, p95 {cumulative['p95']} ms, max {cumulative['max']} ms",
        f"- Attempts per case: median {attempt_count['median']}, p95 {attempt_count['p95']}, max {attempt_count['max']}; {attempt_count['total_attempts']} total attempts",
        f"- Average deterministic score: {summary['average_score']}/100",
        f"- First token ({ft['measured']}/{ft['total_cases']} measured; {ft['missing']} missing): median {ft['median']} ms, p90 {ft['p90']} ms, p95 {ft['p95']} ms, max {ft['max']} ms",
        f"- Normal-answer first token ({answer_ft['measured']}/{answer_ft['total_cases']} measured; {answer_ft['missing']} missing): median {answer_ft['median']} ms, p90 {answer_ft['p90']} ms, p95 {answer_ft['p95']} ms, max {answer_ft['max']} ms",
        f"- Intervention first token ({intervention_ft['measured']}/{intervention_ft['total_cases']} measured; {intervention_ft['missing']} missing): median {intervention_ft['median']} ms, p95 {intervention_ft['p95']} ms, max {intervention_ft['max']} ms",
        f"- Customer charge recorded by Bluey: {summary['customer_cost_cents']} cents",
        f"- Balance: {summary['balance_cents_before']} -> {summary['balance_cents_after']} cents",
        f"- Trial seconds: {summary['trial_seconds_before']} -> {summary['trial_seconds_after']}",
        "",
        "## Providers",
        "",
    ]
    lines.extend(f"- {name}: {count}" for name, count in summary["provider_counts"].items())
    lines.extend(["", "## Source Coverage", ""])
    for profile, files in sorted(sources.items()):
        lines.append(f"- {profile}: {', '.join(files) if files else 'role prompt only'}")
    lines.extend(["", "## Frequent Issues", ""])
    issue_counts = summary["issue_counts"]
    if issue_counts:
        lines.extend(f"- {issue}: {count}" for issue, count in list(issue_counts.items())[:25])
    else:
        lines.append("- None detected by the deterministic checks.")
    lines.extend(["", "## Case Results", "", "| ID | Category | First-attempt token | Attempts | Cumulative | Provider/model | Score | Result |", "|---|---|---:|---:|---:|---|---:|---|"])
    for result in results:
        attempt = result.attempts[-1]
        first_attempt = result.attempts[0]
        first = f"{first_attempt.first_token_ms:.0f} ms" if first_attempt.first_token_ms is not None else "n/a"
        route = f"{attempt.provider or 'unknown'}/{attempt.model or 'unknown'}"
        if result.accepted_outcome and result.final_ok:
            outcome = "ok"
        elif result.accepted_outcome and is_safe_needs_user_input_outcome(
            next(case for case in CASES if case.id == result.id), attempt
        ):
            outcome = "needs_user_input (expected)"
        else:
            outcome = attempt.error_reason or attempt.error or attempt.billing_error or "failed"
        lines.append(f"| {result.id} | {result.category} | {first} | {len(result.attempts)} | {result.cumulative_elapsed_ms:.0f} ms | {route} | {result.score} | {outcome[:80]} |")
    lines.extend(["", "## Review Queue", ""])
    review = sorted(results, key=lambda item: (item.score, item.id))
    for result in review[:20]:
        attempt = result.attempts[-1]
        lines.append(f"### {result.id}: {result.question}")
        lines.append("")
        lines.append(f"Score {result.score}. Issues: {', '.join(result.issues) if result.issues else 'none'}." )
        lines.append("")
        preview = re.sub(r"\s+", " ", attempt.visible_answer).strip()[:500]
        lines.append(f"> {preview or '[no answer]'}")
        lines.append("")
    return "\n".join(lines).rstrip() + "\n"


def reliability_percent_arg(value: str) -> float:
    try:
        parsed = float(value)
    except ValueError as exc:
        raise argparse.ArgumentTypeError("must be a number from 0 through 100") from exc
    if parsed != 100.0:
        raise argparse.ArgumentTypeError("the release gate is fixed at 100 percent")
    return parsed


def select_eval_cases(only: Optional[str], limit: int) -> List[EvalCase]:
    selected = list(CASES)
    if only is not None:
        wanted = {
            value.strip().upper() for value in only.split(",") if value.strip()
        }
        if not wanted:
            raise ValueError("--only must name at least one case ID")
        known = {case.id for case in CASES}
        unknown = sorted(wanted - known)
        if unknown:
            raise ValueError(f"unknown --only case IDs: {', '.join(unknown)}")
        selected = [case for case in selected if case.id in wanted]
    return selected[: max(0, limit)]


def self_check_case_selection() -> None:
    assert [case.id for case in select_eval_cases("q38,Q41", len(CASES))] == [
        "Q38",
        "Q41",
    ]
    for invalid in ("Q38,Q99", " , "):
        try:
            select_eval_cases(invalid, len(CASES))
        except ValueError:
            pass
        else:
            raise AssertionError(f"invalid --only selection was accepted: {invalid!r}")


def parse_args(argv: Sequence[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--api-base", default=os.environ.get("BLUEY_API_BASE", DEFAULT_API_BASE))
    parser.add_argument("--credentials-file", type=Path, default=DEFAULT_CREDENTIALS)
    parser.add_argument("--account-purpose", default="normal managed answer")
    parser.add_argument("--downloads", type=Path, default=DEFAULT_DOWNLOADS)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--limit", type=int, default=len(CASES))
    parser.add_argument("--only", help="Comma-separated case IDs")
    parser.add_argument("--timeout", type=float, default=90.0)
    parser.add_argument("--pause-ms", type=int, default=350)
    parser.add_argument("--capacity-retries", type=int, default=1)
    parser.add_argument("--max-customer-cost-cents", type=int, default=900)
    parser.add_argument(
        "--minimum-reliability-percent",
        type=reliability_percent_arg,
        default=100.0,
        help="release success threshold; fixed at 100",
    )
    parser.add_argument("--keep-login", action="store_true")
    parser.add_argument("--dry-run", action="store_true")
    return parser.parse_args(argv)


def main(argv: Sequence[str]) -> int:
    args = parse_args(argv)
    self_check_billing_event_validation()
    self_check_ambiguous_payment_detector()
    self_check_large_fk_migration_safety()
    self_check_exactly_once_processing_detector()
    self_check_payment_operation_semantics()
    self_check_payment_platform_safety_detector()
    self_check_q46_story_grounding_detector()
    self_check_production_answer_contracts()
    self_check_attempt_integrity_guards()
    self_check_failed_attempt_scoring()
    self_check_release_exit_gate()
    self_check_case_selection()
    self_check_typed_answer_context()
    if args.minimum_reliability_percent != 100.0:
        raise ValueError("--minimum-reliability-percent is fixed at 100")
    base = normalize_base(args.api_base)
    selected = select_eval_cases(args.only, args.limit)
    if len(CASES) != 50:
        raise AssertionError(f"Expected exactly 50 built-in cases, found {len(CASES)}")
    invalid_outcomes = [
        case.id
        for case in selected
        if case.expected_outcome not in {"answer", "needs_user_input"}
    ]
    if invalid_outcomes:
        raise ValueError(f"unsupported expected outcomes: {', '.join(invalid_outcomes)}")

    sources: Dict[str, List[str]] = {}
    typed_contexts: Dict[str, List[Dict[str, Any]]] = {}
    for profile_name in sorted({case.profile for case in selected}):
        (
            sources[profile_name],
            typed_contexts[profile_name],
        ) = profile_context(PROFILES[profile_name], args.downloads)
        validate_typed_answer_context(typed_contexts[profile_name])
    print(f"Prepared {len(selected)} cases from {sum(len(v) for v in sources.values())} local source files.")
    print("Raw evaluation output will stay local under:", args.output)
    if args.dry_run:
        for case in selected:
            print(case.id, case.category, case.profile, case.origin)
        return 0

    email, password = load_test_account(args.credentials_file, args.account_purpose)
    _, auth = json_request(base, "/auth/login", "POST", payload={"email": email, "password": password})
    token = str(auth["access_token"])
    refresh_token = str(auth["refresh_token"])
    _, account_before = json_request(base, "/account/me", token=token)
    args.output.mkdir(parents=True, exist_ok=True)
    raw_path = args.output / "results.jsonl"
    transcript_path = args.output / "answers.md"
    raw_path.write_text("")
    transcript_path.write_text("# Bluey Interview Eval Answers\n\n")
    started_at = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    results: List[CaseResult] = []
    result_by_id: Dict[str, CaseResult] = {}
    case_context_manifest: Dict[str, Dict[str, Any]] = {}
    conversation_sessions: Dict[str, str] = {}
    spent = 0
    try:
        for index, case in enumerate(selected, start=1):
            if spent >= args.max_customer_cost_cents:
                print(f"Stopping before {case.id}: spend guard reached {spent} cents.")
                break
            session_key = case.conversation or case.id
            session_id = conversation_sessions.setdefault(
                session_key, f"interview-eval-{session_key}-{uuid.uuid4()}"
            )
            answer_context = build_typed_answer_context(
                case, typed_contexts[case.profile], result_by_id
            )
            validate_typed_answer_context(answer_context)
            user_prompt = build_user_prompt(case, answer_context)
            context_hash = hashlib.sha256(
                json.dumps(
                    answer_context,
                    ensure_ascii=False,
                    sort_keys=True,
                    separators=(",", ":"),
                ).encode()
            ).hexdigest()
            case_context_manifest[case.id] = context_provenance_manifest(
                answer_context, context_hash
            )
            attempts: List[AttemptResult] = []
            case_started = time.perf_counter()
            max_attempts = 1 + max(0, args.capacity_retries)
            for attempt_number in range(1, max_attempts + 1):
                attempt = run_attempt(
                    base,
                    token,
                    case,
                    user_prompt,
                    answer_context,
                    session_id,
                    attempt_number,
                    args.timeout,
                )
                attempts.append(attempt)
                spent += attempt.cost_cents
                if attempt.ok:
                    break
                capacity = (attempt.error_reason or "").casefold() in {
                    "provider_capacity",
                    "provider_key_cooling_down",
                    "provider_busy",
                    "account_llm_busy",
                } or "capacity" in (attempt.error or "").casefold()
                if attempt_number < max_attempts and capacity:
                    time.sleep(max(1.0, attempt_number * 1.5))
                    continue
                break
            cumulative_elapsed_ms = (time.perf_counter() - case_started) * 1000
            final_attempt = attempts[-1]
            reliability, latency, human, accuracy, issues = quality_scores(case, final_attempt)
            for audit_issue in prior_attempt_audit_issues(attempts):
                if audit_issue not in issues:
                    issues.append(audit_issue)
            result = CaseResult(
                id=case.id,
                category=case.category,
                profile=case.profile,
                origin=case.origin,
                question=case.question,
                conversation=case.conversation,
                context_sha256=context_hash,
                attempts=attempts,
                cumulative_elapsed_ms=cumulative_elapsed_ms,
                final_ok=answer_is_success(case, final_attempt),
                first_attempt_ok=answer_is_success(case, attempts[0]),
                accepted_outcome=expected_outcome_is_accepted(case, final_attempt),
                first_attempt_accepted=expected_outcome_is_accepted(case, attempts[0]),
                score=reliability + latency + human + accuracy,
                reliability_score=reliability,
                latency_score=latency,
                human_score=human,
                accuracy_score=accuracy,
                issues=issues,
            )
            results.append(result)
            result_by_id[result.id] = result
            with raw_path.open("a") as handle:
                handle.write(json.dumps(asdict(result), ensure_ascii=False) + "\n")
            with transcript_path.open("a") as handle:
                handle.write(f"## {case.id}: {case.question}\n\n")
                handle.write(f"Profile: {case.profile}; category: {case.category}; score: {result.score}\n\n")
                handle.write((final_attempt.visible_answer or "[no visible answer]") + "\n\n")
                if final_attempt.artifact_body:
                    handle.write("### Workbench artifact\n\n" + final_attempt.artifact_body + "\n\n")
                if issues:
                    handle.write("Issues: " + ", ".join(issues) + "\n\n")
            route = f"{final_attempt.provider or 'unknown'}/{final_attempt.model or 'unknown'}"
            first_attempt = attempts[0]
            ft = f"{first_attempt.first_token_ms:.0f}ms" if first_attempt.first_token_ms is not None else "n/a"
            print(
                f"[{index:02d}/{len(selected)}] {case.id} "
                f"{'OK' if result.accepted_outcome else 'FAIL'} score={result.score} "
                f"first_attempt={'OK' if result.first_attempt_accepted else 'FAIL'} "
                f"first={ft} attempts={len(attempts)} "
                f"cumulative={cumulative_elapsed_ms:.0f}ms route={route}"
            )
            time.sleep(max(0, args.pause_ms) / 1000)
        _, account_after = json_request(base, "/account/me", token=token)
        finished_at = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
        summary = build_summary(
            results,
            started_at,
            finished_at,
            account_before,
            account_after,
        )
        write_json(args.output / "summary.json", summary)
        (args.output / "report.md").write_text(render_report(summary, results, sources))
        write_json(
            args.output / "source-manifest.json",
            {
                "profiles": sources,
                "cases": case_context_manifest,
            },
        )
        print(json.dumps(summary, indent=2))
        return release_exit_code(
            results_run=len(results),
            selected_count=len(selected),
            accepted_outcomes=summary["accepted_outcomes"],
            first_attempt_accepted_outcomes=summary[
                "first_attempt_accepted_outcomes"
            ],
            cases_with_retries=summary["cases_with_retries"],
            failed_partial_stream_attempts=summary[
                "failed_partial_stream_attempts"
            ],
            first_attempt_latency_gate_passed=summary[
                "first_attempt_latency_gate"
            ]["passed"],
            minimum_reliability_percent=args.minimum_reliability_percent,
        )
    finally:
        if not args.keep_login:
            try:
                json_request(
                    base,
                    "/auth/logout",
                    "POST",
                    token=token,
                    payload={"refresh_token": refresh_token, "revoke_all": False},
                )
            except Exception:
                pass


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))

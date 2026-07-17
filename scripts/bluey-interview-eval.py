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
    final_ok: bool
    first_attempt_ok: bool
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
    EvalCase("Q11", "scenario", "sde", "A release improves average latency but makes p99 worse. Would you ship it? Walk me through the decision.", speakable=True, required_groups=(g("p99", "tail"), g("segment", "workload", "trace"), g("rollback", "canary", "slo"))),
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
    EvalCase("Q23", "behavioral", "amazon_de", "Tell me about a time you reduced cloud data-platform cost without hurting reliability.", "leadership_doc", speakable=True, required_groups=(g("glue", "spark"), g("70%", "30%", "cost"), g("reliab", "sla", "monitor"))),

    EvalCase("Q24", "behavioral", "ds", "Tell me about yourself for this data science and AI platform role.", "resume_and_jd_pdf", speakable=True, self_intro=True, required_groups=(g("data scientist", "machine learning", "genai"), g("rag", "fraud"), g("hpe", "datacenter", "platform"))),
    EvalCase("Q25", "behavioral", "ds", "Walk me through the secure RAG system you built and the decision you personally owned.", "otter_interview_style", "rag_project", speakable=True, required_groups=(g("2m", "document"), g("98%", "precision"), g("secure", "access"))),
    EvalCase("Q26", "followup", "ds", "Where could that RAG system hallucinate, and what did you put in place to catch it?", "otter_interview_style", "rag_project", speakable=True, expect_followup_context=True, required_groups=(g("retriev", "ground"), g("citation", "source"), g("eval", "threshold", "fallback"))),
    EvalCase("Q27", "technical", "ds", "Design an evaluation plan for a RAG assistant before production launch.", speakable=True, required_groups=(g("retrieval",), g("faithful", "ground", "hallucin"), g("latency", "cost"), g("human", "golden", "dataset"))),
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
    EvalCase("Q38", "system_design", "ds", "Design an online feature store that serves low-latency features and keeps training data consistent with serving.", max_tokens=1100, speakable=True, expect_design=True, required_groups=(g("offline",), g("online",), g("point-in-time", "training-serving"), g("fresh", "stream"))),
    EvalCase("Q39", "system_design", "general", "Design a payment processing platform that safely handles retries and duplicate requests.", "curated", "payment_design", max_tokens=1100, speakable=True, expect_design=True, required_groups=(g("idempot",), g("ledger",), g("webhook", "processor"), g("reconcil",))),
    EvalCase("Q40", "design_followup", "general", "The provider times out after charging the card. What exact state transition and retry behavior do you use?", "curated", "payment_design", max_tokens=850, speakable=True, expect_followup_context=True, required_groups=(g("unknown", "pending", "reconcil"), g("idempot",), g("webhook", "query"))),
    EvalCase("Q41", "system_design", "general", "Design a URL shortener and make the main scale and consistency tradeoff explicit.", max_tokens=950, speakable=True, expect_design=True, required_groups=(g("key", "id"), g("cache",), g("redirect",), g("consistency", "collision"))),
    EvalCase("Q42", "system_design", "ds", "Design a multi-tenant enterprise RAG platform with document permissions, citations, and cost controls.", max_tokens=1200, speakable=True, expect_design=True, required_groups=(g("tenant", "permission", "acl"), g("chunk", "embedding"), g("citation",), g("cost", "quota"))),

    EvalCase("Q43", "behavioral", "amazon_de", "Tell me about a time the requirements were ambiguous and you still moved the work forward safely.", "behavioral_doc", speakable=True, required_groups=(g("clarif", "stakeholder", "requirement"), g("assumption", "scope", "prototype"), g("result", "outcome"))),
    EvalCase("Q44", "behavioral", "amazon_de", "Tell me about a time you challenged a decision with data and then committed to the final direction.", "leadership_doc", speakable=True, required_groups=(g("data", "evidence"), g("disagree", "challenge"), g("commit", "align"))),
    EvalCase("Q45", "behavioral", "amazon_de", "Tell me about a failure. What did you change so the same class of failure would not repeat?", "behavioral_doc", speakable=True, required_groups=(g("fail", "mistake"), g("root cause", "learn"), g("guardrail", "test", "monitor", "process"))),
    EvalCase("Q46", "behavioral", "amazon_de", "Give me an example of ownership beyond your assigned task.", "leadership_doc", speakable=True, required_groups=(g("ownership", "took"), g("customer", "team", "impact"), g("result", "reduced", "improved"))),
    EvalCase("Q47", "behavioral", "amazon_de", "Two urgent requests arrive from different directors and both claim top priority. What do you do?", "behavioral_doc", speakable=True, required_groups=(g("impact", "severity", "customer"), g("align", "stakeholder"), g("communicat", "tradeoff"))),
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


def profile_context(profile: ProfileSpec, downloads: Path) -> Tuple[str, List[str]]:
    blocks: List[str] = []
    sources: List[str] = []
    for label, filename, limit in (
        ("Resume", profile.resume, 6000),
        ("Job description", profile.job_description, 6000),
        ("Interview preparation document", profile.extra_document, 5000),
    ):
        if not filename:
            continue
        path = downloads / filename
        if not path.is_file():
            raise FileNotFoundError(path)
        sources.append(path.name)
        blocks.append(f"[{label} from {path.name}]\n{compact(extract_document(path), limit)}")
    blocks.append(f"[Role target from evaluation]\n{profile.role}")
    return "\n\n".join(blocks), sources


def build_user_prompt(
    case: EvalCase,
    context: str,
    prior_results: Dict[str, CaseResult],
) -> str:
    blocks = [context] if context else []
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
            blocks.append(
                "[Retained conversation context]\n"
                f"Previous question: {result.question}\n"
                f"Previous Bluey answer: {compact(prior_text, 9000)}"
            )
    if not blocks:
        return case.question
    return f"Question:\n{case.question}\n\nSession context:\n" + "\n\n".join(blocks)


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


def run_attempt(
    base: str,
    token: str,
    case: EvalCase,
    user_prompt: str,
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
                    try:
                        value = json.loads(data)
                    except json.JSONDecodeError:
                        value = {}
                    if isinstance(value, dict):
                        result.visible_answer = str(value.get("text") or "")
                        result.artifact_type = value.get("artifact_type")
                        result.artifact_body = value.get("artifact_body")
                        result.provider = value.get("provider")
                        result.model = value.get("model")
                        result.input_tokens = value.get("input_tokens")
                        result.output_tokens = value.get("output_tokens")
                        result.cost_cents = int(value.get("cost_cents") or 0)
                        result.balance_cents_after = value.get("balance_cents_after")
                        result.trial_seconds_remaining = value.get("trial_seconds_remaining")
                        if isinstance(value.get("sources"), list):
                            result.sources = value["sources"]
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
    if not result.visible_answer:
        result.visible_answer = "".join(streamed_text).strip()
    result.ok = bool(
        result.status is not None
        and 200 <= result.status < 300
        and result.done
        and result.visible_answer.strip()
        and not result.error
    )
    return result


def has_first_person(text: str) -> bool:
    return bool(re.search(r"\b(?:I|I'm|I've|I'd|my|me)\b", text, re.I))


def answer_evidence_text(attempt: AttemptResult) -> str:
    """Return all customer-visible answer material used by quality gates."""
    return (attempt.visible_answer + "\n" + (attempt.artifact_body or "")).strip()


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


def has_mysql_not_valid_portability_claim(text: str) -> bool:
    lower = re.sub(r"\s+", " ", text.casefold())
    if "mysql" not in lower or "not valid" not in lower:
        return False
    if re.search(
        r"mysql.{0,100}(?:does not|doesn't|doesn’t|cannot|can't|can’t|lacks).{0,80}(?:support|have).{0,60}not valid",
        lower,
    ) or re.search(
        r"not valid.{0,120}(?:is not|isn't|isn’t|not).{0,60}(?:supported|available).{0,50}mysql",
        lower,
    ):
        return False
    return bool(
        re.search(
            r"not valid.{0,240}(?:works?|supported|available|introduced).{0,100}mysql",
            lower,
        )
        or re.search(
            r"mysql.{0,160}(?:supports?|has|offers|allows).{0,100}not valid",
            lower,
        )
        or re.search(r"works?.{0,100}(?:recent|current|newer).{0,40}mysql", lower)
    )


def has_exactly_once_processing_overclaim(text: str) -> bool:
    for sentence in re.split(r"(?<=[.!?])\s+|\n+", text.casefold()):
        if not re.search(r"exactly[- ]once", sentence):
            continue
        caveated = bool(
            re.search(
                r"(?:cannot|can't|can’t|not possible|not truly|no true|"
                r"effectively[- ]once|exactly[- ]once effect|limited to|only within)",
                sentence,
            )
        )
        if caveated:
            continue
        if re.search(
            r"(?:guarantee|guarantees|guaranteed|ensure|ensures|achieve|achieves)"
            r".{0,80}exactly[- ]once|exactly[- ]once.{0,50}processing semantics",
            sentence,
        ):
            return True
    return False


def has_unsafe_ambiguous_payment_outcome(text: str) -> bool:
    lower = re.sub(r"\s+", " ", re.sub(r"[*_`~]+", "", text.casefold()))
    action_words = r"mark(?:ed)?|move(?:d)?|transition(?:ed)?|set"
    terminal_failure = False
    for outcome in re.finditer(
        r"\b(?:timeout|timed out|unknown|ambiguous|no record|maximum retries|max retries)\b",
        lower,
    ):
        window = lower[outcome.start() : outcome.end() + 320]
        actions = re.finditer(
            rf"\b(?:{action_words})\b"
            rf"(?:(?!\b(?:{action_words})\b).){{0,80}}\bfailed\b",
            window,
        )
        for action in actions:
            action_start = outcome.start() + action.start()
            prefix = lower[max(0, action_start - 60) : action_start]
            if re.search(
                r"(?:do not|don't|don’t|never|must not|should not|cannot|can't|can’t)"
                r"(?:\s+(?:ever|be))?\s*$",
                prefix,
            ) or re.search(r"\bnot\b.{0,20}\bfailed\b", action.group()):
                continue
            terminal_failure = True
            break
        if terminal_failure:
            break
    charge_retry = re.search(
        r"\b(?:retry|retries|retrying|resubmit|resubmits|resubmitting|re-submit|re-submits)"
        r"\s+(?:the\s+|a\s+)?(?:charge|payment|gateway call|charge submission|payment submission)\b",
        lower,
    )
    if charge_retry:
        prefix = lower[max(0, charge_retry.start() - 45) : charge_retry.start()]
        if re.search(r"(?:do not|don't|don’t|never|must not|cannot|can't|can’t)\s*$", prefix):
            charge_retry = None
    return bool(terminal_failure or charge_retry)


def self_check_ambiguous_payment_detector() -> None:
    safe = (
        "A timeout leaves the outcome UNKNOWN, not failed. "
        "Move it to PENDING_RECONCILIATION; do not mark it FAILED.",
        "After a timeout, never retry the charge; retry only the status lookup.",
    )
    unsafe = (
        "After a timeout, mark the payment FAILED and retry the charge.",
        "The outcome is unknown. After maximum retries, transition it to FAILED.",
    )
    assert not any(has_unsafe_ambiguous_payment_outcome(text) for text in safe)
    assert all(has_unsafe_ambiguous_payment_outcome(text) for text in unsafe)


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


def blocking_answer_issues(case: EvalCase, attempt: AttemptResult) -> List[str]:
    """Return deterministic defects that prevent an answer from being success."""
    if not attempt.ok:
        return []
    combined = answer_evidence_text(attempt)
    issues: List[str] = []
    word_count = len(re.findall(r"\b[\w'’+-]+\b", combined))
    if word_count < MIN_SUBSTANTIVE_ANSWER_WORDS:
        issues.append("answer_too_short")

    exact_cap = attempt.output_tokens is not None and (
        attempt.output_tokens == case.max_tokens
        or attempt.output_tokens in KNOWN_OUTPUT_TOKEN_CAPS
    )
    if exact_cap and looks_structurally_incomplete(combined):
        issues.append("visibly_truncated_at_token_cap")

    if case.id == "Q10" and has_mysql_not_valid_portability_claim(combined):
        issues.append("unsafe_mysql_not_valid_portability_claim")
    if case.id == "Q39" and has_exactly_once_processing_overclaim(combined):
        issues.append("unsafe_exactly_once_processing_claim")
    if case.id == "Q40" and has_unsafe_ambiguous_payment_outcome(combined):
        issues.append("unsafe_ambiguous_payment_retry_or_terminal_failure")
    if case.id == "Q38" and has_drift_only_automatic_retraining(combined):
        issues.append("unsafe_drift_only_automatic_retraining")
    return issues


def answer_is_success(case: EvalCase, attempt: AttemptResult) -> bool:
    return attempt.ok and not blocking_answer_issues(case, attempt)


def quality_scores(case: EvalCase, attempt: AttemptResult) -> Tuple[int, int, int, int, List[str]]:
    issues: List[str] = []
    reliability = 35 if attempt.ok else 0
    if not attempt.ok:
        issues.append("request_failed")
    if attempt.ok and attempt.first_token_ms is not None:
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
    if case.speakable and not has_first_person(attempt.visible_answer):
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

    accuracy = 25
    for group in case.required_groups:
        if not any(term.casefold() in lower for term in group):
            accuracy -= 4
            issues.append("missing_signal:" + "|".join(group))
    if case.expect_followup_context and any(phrase in lower for phrase in CONTEXT_LOSS_PHRASES):
        accuracy -= 8
        issues.append("followup_context_lost")
    if case.expect_code:
        if attempt.artifact_type != "code":
            accuracy -= 8
            issues.append("missing_code_artifact")
        artifact = attempt.artifact_body or ""
        if "```" not in artifact and not re.search(r"\b(class|def|func)\b", artifact):
            accuracy -= 5
            issues.append("incomplete_code_body")
        has_time_complexity = "time complexity" in lower or bool(
            re.search(r"(?:^|\n)\s*(?:[-*]\s*)?(?:\*\*)?time(?:\*\*)?\s*:", combined, re.I)
        ) or bool(
            re.search(r"\bO\([^\n)]*\)\s+time\b", combined, re.I)
        )
        has_space_complexity = "space complexity" in lower or bool(
            re.search(r"(?:^|\n)\s*(?:[-*]\s*)?(?:\*\*)?space(?:\*\*)?\s*:", combined, re.I)
        )
        if not has_time_complexity or not has_space_complexity:
            accuracy -= 5
            issues.append("missing_complexity")
    if case.expect_design and attempt.artifact_type not in ("system_design", "diagram"):
        accuracy -= 7
        issues.append("missing_design_artifact")
    blocking_issues = blocking_answer_issues(case, attempt)
    if blocking_issues:
        accuracy = 0
        issues.append("answer_quality_gate_failed")
        issues.extend(blocking_issues)
    accuracy = max(0, accuracy)
    return reliability, latency, human, accuracy, issues


def percentile(values: Sequence[float], percent: float) -> Optional[float]:
    if not values:
        return None
    ordered = sorted(values)
    index = (len(ordered) - 1) * percent
    low = int(index)
    high = min(low + 1, len(ordered) - 1)
    fraction = index - low
    return ordered[low] + (ordered[high] - ordered[low]) * fraction


def write_json(path: Path, value: Any) -> None:
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n")


def build_summary(
    results: Sequence[CaseResult],
    started_at: str,
    finished_at: str,
    account_before: Dict[str, Any],
    account_after: Dict[str, Any],
) -> Dict[str, Any]:
    final_ok = [result for result in results if result.final_ok]
    first_tokens = [
        result.attempts[-1].first_token_ms
        for result in final_ok
        if result.attempts[-1].first_token_ms is not None
    ]
    totals = [result.attempts[-1].total_ms for result in final_ok]
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
        "final_successes": len(final_ok),
        "first_attempt_successes": sum(1 for result in results if result.first_attempt_ok),
        "reliability_percent": round(100 * len(final_ok) / len(results), 1) if results else 0,
        "first_attempt_reliability_percent": round(100 * sum(1 for result in results if result.first_attempt_ok) / len(results), 1) if results else 0,
        "average_score": round(statistics.mean(result.score for result in results), 1) if results else 0,
        "first_token_ms": {
            "median": round(statistics.median(first_tokens), 1) if first_tokens else None,
            "p90": round(percentile(first_tokens, 0.9) or 0, 1) if first_tokens else None,
            "p95": round(percentile(first_tokens, 0.95) or 0, 1) if first_tokens else None,
            "max": round(max(first_tokens), 1) if first_tokens else None,
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
    lines = [
        "# Bluey 50-Question Interview Evaluation",
        "",
        "Raw answers and artifacts remain in this local eval directory.",
        "",
        "## Summary",
        "",
        f"- Final reliability: {summary['final_successes']}/{summary['questions_run']} ({summary['reliability_percent']}%)",
        f"- First-attempt reliability: {summary['first_attempt_successes']}/{summary['questions_run']} ({summary['first_attempt_reliability_percent']}%)",
        f"- Average deterministic score: {summary['average_score']}/100",
        f"- First token: median {ft['median']} ms, p90 {ft['p90']} ms, p95 {ft['p95']} ms, max {ft['max']} ms",
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
    lines.extend(["", "## Case Results", "", "| ID | Category | First token | Total | Provider/model | Score | Result |", "|---|---|---:|---:|---|---:|---|"])
    for result in results:
        attempt = result.attempts[-1]
        first = f"{attempt.first_token_ms:.0f} ms" if attempt.first_token_ms is not None else "n/a"
        route = f"{attempt.provider or 'unknown'}/{attempt.model or 'unknown'}"
        outcome = "ok" if result.final_ok else (attempt.error_reason or attempt.error or "failed")
        lines.append(f"| {result.id} | {result.category} | {first} | {attempt.total_ms:.0f} ms | {route} | {result.score} | {outcome[:80]} |")
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
    parser.add_argument("--keep-login", action="store_true")
    parser.add_argument("--dry-run", action="store_true")
    return parser.parse_args(argv)


def main(argv: Sequence[str]) -> int:
    args = parse_args(argv)
    self_check_ambiguous_payment_detector()
    base = normalize_base(args.api_base)
    selected = list(CASES)
    if args.only:
        wanted = {value.strip().upper() for value in args.only.split(",") if value.strip()}
        selected = [case for case in selected if case.id in wanted]
    selected = selected[: max(0, args.limit)]
    if len(CASES) != 50:
        raise AssertionError(f"Expected exactly 50 built-in cases, found {len(CASES)}")

    contexts: Dict[str, str] = {}
    sources: Dict[str, List[str]] = {}
    for profile_name in sorted({case.profile for case in selected}):
        contexts[profile_name], sources[profile_name] = profile_context(PROFILES[profile_name], args.downloads)
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
            user_prompt = build_user_prompt(case, contexts[case.profile], result_by_id)
            context_hash = hashlib.sha256(contexts[case.profile].encode()).hexdigest()
            attempts: List[AttemptResult] = []
            max_attempts = 1 + max(0, args.capacity_retries)
            for attempt_number in range(1, max_attempts + 1):
                attempt = run_attempt(
                    base, token, case, user_prompt, session_id, attempt_number, args.timeout
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
            final_attempt = attempts[-1]
            reliability, latency, human, accuracy, issues = quality_scores(case, final_attempt)
            result = CaseResult(
                id=case.id,
                category=case.category,
                profile=case.profile,
                origin=case.origin,
                question=case.question,
                conversation=case.conversation,
                context_sha256=context_hash,
                attempts=attempts,
                final_ok=answer_is_success(case, final_attempt),
                first_attempt_ok=answer_is_success(case, attempts[0]),
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
            ft = f"{final_attempt.first_token_ms:.0f}ms" if final_attempt.first_token_ms is not None else "n/a"
            print(
                f"[{index:02d}/{len(selected)}] {case.id} "
                f"{'OK' if result.final_ok else 'FAIL'} score={result.score} "
                f"first={ft} total={final_attempt.total_ms:.0f}ms route={route}"
            )
            time.sleep(max(0, args.pause_ms) / 1000)
        _, account_after = json_request(base, "/account/me", token=token)
        finished_at = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
        summary = build_summary(results, started_at, finished_at, account_before, account_after)
        write_json(args.output / "summary.json", summary)
        (args.output / "report.md").write_text(render_report(summary, results, sources))
        write_json(args.output / "source-manifest.json", {"profiles": sources})
        print(json.dumps(summary, indent=2))
        return 0 if len(results) == len(selected) and summary["reliability_percent"] >= 90 else 2
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
